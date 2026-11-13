//! On-screen notifications drawn above games and other apps, ShadowPlay-style.
//!
//! A native layered Win32 window on its own thread, so it keeps working while the main window
//! is hidden in the tray. It is click-through, never takes focus, and is excluded from screen
//! capture, so the toast never ends up inside your clips.
//!
//! Limit: a game in *exclusive* fullscreen owns the display and Windows won't draw other
//! windows over it. Borderless / "fullscreen windowed" games show the toast fine.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use rewind_core::config::Corner;
use rewind_core::toast::{alpha_at, place, rounded_rect_coverage, slide_remaining, Rect, ToastKind, ToastSpec};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

enum Cmd {
    Show { spec: ToastSpec, corner: Corner, secs: f32 },
    Quit,
}

pub struct Overlay {
    tx: Sender<Cmd>,
}

impl Overlay {
    pub fn spawn() -> Overlay {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("rewind-overlay".into())
            .spawn(move || run(rx))
            .ok();
        Overlay { tx }
    }

    pub fn show(&self, spec: ToastSpec, corner: Corner, secs: u32) {
        let _ = self.tx.send(Cmd::Show { spec, corner, secs: secs as f32 });
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize {
    DefWindowProcW(hwnd, msg, w, l)
}

const WDA_EXCLUDEFROMCAPTURE: u32 = 0x0000_0011;

// Logical (96-dpi) layout, scaled by the monitor DPI at draw time.
const CARD_W: f32 = 372.0;
const CARD_H: f32 = 84.0;
const RADIUS: f32 = 16.0;

struct Active {
    started: Instant,
    total: f32,
    x: i32,
    y: i32,
    dir_y: i32, // +1 slides down into place (top corners), -1 slides up (bottom corners)
    w: i32,
    h: i32,
    mem_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
}

fn run(rx: std::sync::mpsc::Receiver<Cmd>) {
    unsafe {
        let hinstance = GetModuleHandleW(null());
        let class_name = wide("RewindOverlayWindow");
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = hinstance;
        wc.lpszClassName = class_name.as_ptr();
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class_name.as_ptr(),
            wide("Rewind overlay").as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        // Keep the toast out of recordings and screenshots (Windows 10 2004+).
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);

        let screen_dc = GetDC(null_mut());
        let mut active: Option<Active> = None;

        loop {
            let mut msg: MSG = zeroed();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Wake 60 times a second only while a toast is animating; when idle, sleep long
            // so this thread costs nothing on slow PCs.
            let wait = if active.is_some() { 16 } else { 400 };
            match rx.recv_timeout(Duration::from_millis(wait)) {
                Ok(Cmd::Show { spec, corner, secs }) => {
                    if let Some(old) = active.take() {
                        free(old);
                    }
                    active = start_toast(hwnd, screen_dc, &spec, corner, secs);
                }
                Ok(Cmd::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }

            if let Some(a) = &active {
                let t = a.started.elapsed().as_secs_f32();
                if t >= a.total {
                    ShowWindow(hwnd, SW_HIDE);
                    free(active.take().unwrap());
                } else {
                    let alpha = (alpha_at(t, a.total) * 255.0).round() as u8;
                    let slide = -((slide_remaining(t) * 14.0) as i32 * a.dir_y);
                    let dst = POINT { x: a.x, y: a.y + slide };
                    let size = SIZE { cx: a.w, cy: a.h };
                    let src = POINT { x: 0, y: 0 };
                    let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: alpha, AlphaFormat: AC_SRC_ALPHA as u8 };
                    UpdateLayeredWindow(hwnd, screen_dc, &dst, &size, a.mem_dc, &src, 0, &blend, ULW_ALPHA);
                }
            }
        }
        if let Some(a) = active.take() {
            free(a);
        }
        ReleaseDC(null_mut(), screen_dc);
        DestroyWindow(hwnd);
    }
}

unsafe fn free(a: Active) {
    SelectObject(a.mem_dc, a.old_bitmap);
    DeleteObject(a.bitmap);
    DeleteDC(a.mem_dc);
}

unsafe fn target_monitor() -> (Rect, f32) {
    let fg = GetForegroundWindow();
    let hmon = MonitorFromWindow(if fg.is_null() { GetDesktopWindow() } else { fg }, MONITOR_DEFAULTTOPRIMARY);
    let mut mi: MONITORINFO = zeroed();
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    let rect = if GetMonitorInfoW(hmon, &mut mi) != 0 {
        let r = mi.rcMonitor;
        Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top }
    } else {
        Rect { x: 0, y: 0, w: 1920, h: 1080 }
    };
    let (mut dx, mut dy) = (96u32, 96u32);
    if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) != 0 || dx == 0 {
        dx = 96;
    }
    (rect, dx as f32 / 96.0)
}

unsafe fn start_toast(hwnd: HWND, screen_dc: HDC, spec: &ToastSpec, corner: Corner, secs: f32) -> Option<Active> {
    let (monitor, scale) = target_monitor();
    let w = (CARD_W * scale).round() as i32;
    let h = (CARD_H * scale).round() as i32;
    let margin = (24.0 * scale).round() as i32;
    let (x, y) = place(corner, monitor, w, h, margin);

    let mem_dc = CreateCompatibleDC(screen_dc);
    let mut bmi: BITMAPINFO = zeroed();
    bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h; // top-down
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut c_void = null_mut();
    let bitmap = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() || bits.is_null() {
        DeleteDC(mem_dc);
        return None;
    }
    let old_bitmap = SelectObject(mem_dc, bitmap);
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
    render(mem_dc, pixels, w, h, scale, spec);

    // Park the window (invisible until the first UpdateLayeredWindow tick) at its spot.
    SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    let dir_y = if matches!(corner, Corner::TopLeft | Corner::TopRight) { 1 } else { -1 };
    Some(Active { started: Instant::now(), total: secs.max(1.0), x, y, dir_y, w, h, mem_dc, bitmap, old_bitmap })
}

// ---------- software rendering ----------

fn rgb(hex: u32) -> (f32, f32, f32) {
    (((hex >> 16) & 0xff) as f32, ((hex >> 8) & 0xff) as f32, (hex & 0xff) as f32)
}

fn put(px: &mut u32, c: (f32, f32, f32), a: f32) {
    let (r, g, b) = ((*px >> 16 & 0xff) as f32, (*px >> 8 & 0xff) as f32, (*px & 0xff) as f32);
    let m = |d: f32, s: f32| (d + (s - d) * a).round().clamp(0.0, 255.0) as u32;
    *px = 0xff00_0000 | (m(r, c.0) << 16) | (m(g, c.1) << 8) | m(b, c.2);
}

fn fill_circle(px: &mut [u32], w: i32, cx: f32, cy: f32, r: f32, c: (f32, f32, f32)) {
    let (x0, x1) = ((cx - r - 2.0).max(0.0) as i32, (cx + r + 2.0) as i32);
    let (y0, y1) = ((cy - r - 2.0).max(0.0) as i32, (cy + r + 2.0) as i32);
    for y in y0..y1 {
        for x in x0..x1.min(w) {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let cov = (r + 0.5 - d).clamp(0.0, 1.0);
            if cov > 0.0 {
                if let Some(p) = px.get_mut((y * w + x) as usize) {
                    put(p, c, cov);
                }
            }
        }
    }
}

fn stroke(px: &mut [u32], w: i32, a: (f32, f32), b: (f32, f32), t: f32, c: (f32, f32, f32)) {
    let pad = t + 2.0;
    let (x0, x1) = ((a.0.min(b.0) - pad).max(0.0) as i32, (a.0.max(b.0) + pad) as i32);
    let (y0, y1) = ((a.1.min(b.1) - pad).max(0.0) as i32, (a.1.max(b.1) + pad) as i32);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = (dx * dx + dy * dy).max(1e-6);
    for y in y0..y1 {
        for x in x0..x1.min(w) {
            let (qx, qy) = (x as f32 + 0.5 - a.0, y as f32 + 0.5 - a.1);
            let k = ((qx * dx + qy * dy) / len2).clamp(0.0, 1.0);
            let d = ((qx - k * dx).powi(2) + (qy - k * dy).powi(2)).sqrt();
            let cov = (t / 2.0 + 0.5 - d).clamp(0.0, 1.0);
            if cov > 0.0 {
                if let Some(p) = px.get_mut((y * w + x) as usize) {
                    put(p, c, cov);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn text(dc: HDC, s: &str, x: i32, y: i32, w: i32, h: i32, px_height: i32, weight: i32, color: u32) {
    let face = wide("Segoe UI");
    let font = CreateFontW(-px_height, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, CLEARTYPE_QUALITY as u32, 0, face.as_ptr());
    let old = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    // COLORREF is 0x00BBGGRR.
    SetTextColor(dc, ((color & 0xff) << 16) | (color & 0xff00) | ((color >> 16) & 0xff));
    let mut rc = RECT { left: x, top: y, right: x + w, bottom: y + h };
    let t = wide(s);
    DrawTextW(dc, t.as_ptr(), -1, &mut rc, DT_LEFT | DT_TOP | DT_NOPREFIX | DT_SINGLELINE | DT_END_ELLIPSIS);
    SelectObject(dc, old);
    DeleteObject(font);
}

unsafe fn render(dc: HDC, px: &mut [u32], w: i32, h: i32, s: f32, spec: &ToastSpec) {
    const CARD: u32 = 0x1a1e28;
    const BORDER: u32 = 0x343c4f;
    let accent = match spec.kind {
        ToastKind::Success => 0x4ad6a7,
        ToastKind::Info => 0x7aa2ff,
        ToastKind::Error => 0xff5d6c,
    };
    for p in px.iter_mut() {
        *p = 0xff00_0000 | CARD;
    }

    // Icon badge: a tinted disc with a glyph.
    let (icx, icy, ir) = (30.0 * s, h as f32 / 2.0, 20.0 * s);
    let acc = rgb(accent);
    let dark = rgb(CARD);
    fill_circle(px, w, icx, icy, ir, (acc.0 * 0.22 + dark.0 * 0.78, acc.1 * 0.22 + dark.1 * 0.78, acc.2 * 0.22 + dark.2 * 0.78));
    let th = 2.6 * s;
    match spec.kind {
        ToastKind::Success => {
            stroke(px, w, (icx - 7.0 * s, icy + 0.5 * s), (icx - 2.0 * s, icy + 6.0 * s), th, acc);
            stroke(px, w, (icx - 2.0 * s, icy + 6.0 * s), (icx + 8.0 * s, icy - 6.0 * s), th, acc);
        }
        ToastKind::Info => {
            fill_circle(px, w, icx, icy - 7.0 * s, 1.9 * s, acc);
            stroke(px, w, (icx, icy - 2.0 * s), (icx, icy + 8.0 * s), th, acc);
        }
        ToastKind::Error => {
            stroke(px, w, (icx, icy - 8.0 * s), (icx, icy + 2.0 * s), th, acc);
            fill_circle(px, w, icx, icy + 7.5 * s, 1.9 * s, acc);
        }
    }

    // Text (GDI) — title and subtitle.
    let tx = (62.0 * s) as i32;
    let tw = w - tx - (16.0 * s) as i32;
    text(dc, &spec.title, tx, (h as f32 * 0.5 - 24.0 * s) as i32, tw, (24.0 * s) as i32, (17.0 * s) as i32, 600, 0xe6e9f0);
    if !spec.subtitle.is_empty() {
        text(dc, &spec.subtitle, tx, (h as f32 * 0.5 + 2.0 * s) as i32, tw, (20.0 * s) as i32, (13.0 * s) as i32, 400, 0x8a93a6);
    }

    // Rounded corners + 1px border, premultiplied alpha for UpdateLayeredWindow.
    let (fw, fh, fr) = (w as f32, h as f32, RADIUS * s);
    let border = rgb(BORDER);
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let outer = rounded_rect_coverage(cx, cy, fw, fh, fr);
            let inner = rounded_rect_coverage(cx - 1.0, cy - 1.0, fw - 2.0, fh - 2.0, (fr - 1.0).max(0.0));
            let p = &mut px[(y * w + x) as usize];
            let (r, g, b) = ((*p >> 16 & 0xff) as f32, (*p >> 8 & 0xff) as f32, (*p & 0xff) as f32);
            let mix = |c: f32, bd: f32| c * inner + bd * (1.0 - inner);
            let (r, g, b) = (mix(r, border.0), mix(g, border.1), mix(b, border.2));
            let a = outer;
            *p = (((a * 255.0).round() as u32) << 24)
                | (((r * a).round() as u32) << 16)
                | (((g * a).round() as u32) << 8)
                | ((b * a).round() as u32);
        }
    }
}
