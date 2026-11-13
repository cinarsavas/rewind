//! Video thumbnails from the Windows shell, the same ones File Explorer shows. They come out of
//! Windows' own thumbnail cache, so there is no ffmpeg to ship and repeat views are nearly free.
//! One worker thread, lazy requests, a small texture cache: gentle on slow PCs.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::SystemTime;

use eframe::egui;
use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_THUMBNAILONLY};

const THUMB_W: i32 = 320;
const THUMB_H: i32 = 180;
const MAX_TEXTURES: usize = 96;

struct Raw {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

type Key = (PathBuf, Option<SystemTime>);

pub struct Thumbs {
    tx: Sender<Key>,
    rx: Receiver<(Key, Option<Raw>)>,
    textures: HashMap<Key, Option<egui::TextureHandle>>,
    requested: HashSet<Key>,
}

impl Thumbs {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, req_rx) = channel::<Key>();
        let (res_tx, rx) = channel();
        std::thread::Builder::new()
            .name("rewind-thumbs".into())
            .spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                }
                while let Ok(key) = req_rx.recv() {
                    let raw = shell_thumbnail(&key.0);
                    if res_tx.send((key, raw)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
                unsafe {
                    CoUninitialize();
                }
            })
            .ok();
        Self { tx, rx, textures: HashMap::new(), requested: HashSet::new() }
    }

    /// Texture for this clip if its thumbnail is ready; otherwise queue it (once) and return None.
    pub fn get(&mut self, ctx: &egui::Context, path: &Path, modified: SystemTime) -> Option<egui::TextureHandle> {
        self.drain(ctx);
        let key: Key = (path.to_path_buf(), Some(modified));
        if let Some(t) = self.textures.get(&key) {
            return t.clone();
        }
        if self.requested.insert(key.clone()) {
            let _ = self.tx.send(key);
        }
        None
    }

    fn drain(&mut self, ctx: &egui::Context) {
        while let Ok((key, raw)) = self.rx.try_recv() {
            if self.textures.len() >= MAX_TEXTURES {
                self.textures.clear();
                self.requested.clear();
            }
            let tex = raw.map(|r| {
                let image = egui::ColorImage::from_rgba_unmultiplied([r.w, r.h], &r.rgba);
                ctx.load_texture(key.0.to_string_lossy(), image, egui::TextureOptions::LINEAR)
            });
            self.textures.insert(key, tex);
        }
    }

    /// Forget everything, for example after the thumbnails setting is switched off.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.requested.clear();
    }
}

fn shell_thumbnail(path: &Path) -> Option<Raw> {
    unsafe {
        let wide: Vec<u16> = path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
        let hbmp = factory.GetImage(SIZE { cx: THUMB_W, cy: THUMB_H }, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let obj = HGDIOBJ(hbmp.0);

        let mut bm: BITMAP = zeroed();
        if GetObjectW(obj, size_of::<BITMAP>() as i32, Some(&mut bm as *mut BITMAP as *mut c_void)) == 0 {
            let _ = DeleteObject(obj);
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
        if w <= 0 || h <= 0 {
            let _ = DeleteObject(obj);
            return None;
        }
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;
        let mut px = vec![0u8; (w * h * 4) as usize];
        let hdc = GetDC(None);
        let lines = GetDIBits(hdc, hbmp, 0, h as u32, Some(px.as_mut_ptr() as *mut c_void), &mut bmi, DIB_RGB_COLORS);
        ReleaseDC(None, hdc);
        let _ = DeleteObject(obj);
        if lines == 0 {
            return None;
        }
        // BGRA -> RGBA; video thumbnails are opaque.
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
            p[3] = 255;
        }
        Some(Raw { w: w as usize, h: h as usize, rgba: px })
    }
}
