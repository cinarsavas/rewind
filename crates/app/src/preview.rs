//! Hover previews for the clip list (optional add-on).
//!
//! When the pointer rests on a clip's thumbnail, the last 20 seconds of that clip play as a
//! small looping flip-book. Frames come from the add-on's ffmpeg.exe, once per clip, and are
//! cached on disk as small JPEGs, so the second hover is instant.
//!
//! Kept deliberately gentle so nothing stutters:
//! - nothing runs until you hover a clip; only the newest request is worked on;
//! - ffmpeg runs at below-normal priority, with 2 threads and GPU decoding when available;
//! - only the hovered clip's frames are held in memory, and the UI uploads one small frame
//!   texture at a time (about 10 per second, only while hovering).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::{Instant, SystemTime};

use eframe::egui;

/// Seconds from the end of the clip that the preview covers.
const SPAN_SECS: u32 = 20;
/// Frames pulled per second of clip, and the playback rate (2x speed: a 10 s loop).
const EXTRACT_FPS: u32 = 5;
const PLAY_FPS: f32 = 10.0;
const FRAME_WIDTH: u32 = 320;
/// Previews kept on disk before the oldest are removed.
const MAX_CACHED: usize = 150;

type Key = (PathBuf, SystemTime);

struct Job {
    key: Key,
    ffmpeg: PathBuf,
}

enum Done {
    Ready(Key, Vec<egui::ColorImage>),
    Failed(Key, String),
}

enum State {
    Loading,
    Ready(Vec<egui::ColorImage>),
    Failed(String),
}

/// What the clip row should draw for the hovered thumbnail.
pub enum Frame<'a> {
    Loading,
    Ready(&'a egui::TextureHandle, f32),
    Failed(&'a str),
}

pub struct Previews {
    tx: Sender<Job>,
    rx: Receiver<Done>,
    /// The hovered clip (and at most one previous one, for quick back-and-forth).
    states: Vec<(Key, State)>,
    texture: Option<egui::TextureHandle>,
    shown: Option<(Key, usize)>,
    hover_started: Instant,
    hovered: Option<Key>,
}

impl Previews {
    pub fn new(cache_dir: PathBuf, ctx: egui::Context) -> Self {
        let (tx, jobs) = channel::<Job>();
        let (done_tx, rx) = channel::<Done>();
        std::thread::Builder::new()
            .name("rewind-preview".into())
            .spawn(move || {
                while let Ok(mut job) = jobs.recv() {
                    // Only the latest hover matters; skip anything the pointer already left.
                    loop {
                        match jobs.try_recv() {
                            Ok(newer) => job = newer,
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }
                    let msg = match build(&cache_dir, &job) {
                        Ok(frames) => Done::Ready(job.key, frames),
                        Err(e) => Done::Failed(job.key, e),
                    };
                    if done_tx.send(msg).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            })
            .ok();
        Self {
            tx,
            rx,
            states: Vec::new(),
            texture: None,
            shown: None,
            hover_started: Instant::now(),
            hovered: None,
        }
    }

    fn drain(&mut self) {
        while let Ok(done) = self.rx.try_recv() {
            let (key, state) = match done {
                Done::Ready(k, frames) if !frames.is_empty() => (k, State::Ready(frames)),
                Done::Ready(k, _) => (k, State::Failed("the clip is too short".into())),
                Done::Failed(k, e) => (k, State::Failed(e)),
            };
            if let Some(slot) = self.states.iter_mut().find(|(k, _)| *k == key) {
                slot.1 = state;
            }
        }
    }

    /// Call once per frame before drawing rows: forgets the hover if no row claimed it.
    pub fn begin_frame(&mut self) {
        self.drain();
    }

    /// Called by the hovered thumbnail. Returns what to draw over it.
    pub fn hover(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        modified: SystemTime,
        ffmpeg: &Path,
    ) -> Frame<'_> {
        let key: Key = (path.to_path_buf(), modified);
        if self.hovered.as_ref() != Some(&key) {
            self.hovered = Some(key.clone());
            self.hover_started = Instant::now();
        }
        if !self.states.iter().any(|(k, _)| *k == key) {
            // Keep the previous clip too, drop anything older.
            if self.states.len() >= 2 {
                self.states.remove(0);
            }
            self.states.push((key.clone(), State::Loading));
            let _ = self.tx.send(Job {
                key: key.clone(),
                ffmpeg: ffmpeg.to_path_buf(),
            });
        }
        let Some((_, state)) = self.states.iter().find(|(k, _)| *k == key) else {
            return Frame::Loading;
        };
        match state {
            State::Loading => {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                Frame::Loading
            }
            State::Failed(e) => Frame::Failed(e),
            State::Ready(frames) => {
                let elapsed = self.hover_started.elapsed().as_secs_f32();
                let n = frames.len();
                let i = ((elapsed * PLAY_FPS) as usize) % n;
                if self.shown.as_ref() != Some(&(key.clone(), i)) || self.texture.is_none() {
                    let image = frames[i].clone();
                    match &mut self.texture {
                        Some(tex) => tex.set(image, egui::TextureOptions::LINEAR),
                        None => {
                            self.texture = Some(ctx.load_texture(
                                "rewind-hover-preview",
                                image,
                                egui::TextureOptions::LINEAR,
                            ))
                        }
                    }
                    self.shown = Some((key, i));
                }
                ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / PLAY_FPS));
                match &self.texture {
                    Some(tex) => Frame::Ready(tex, (i + 1) as f32 / n as f32),
                    None => Frame::Loading,
                }
            }
        }
    }

    /// The pointer left every thumbnail: restart the loop next time.
    pub fn unhover(&mut self) {
        self.hovered = None;
    }
}

fn cache_key(path: &Path, modified: SystemTime) -> String {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    modified.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Extract (or reuse) the frames for one clip and decode them.
fn build(cache_dir: &Path, job: &Job) -> Result<Vec<egui::ColorImage>, String> {
    let (path, modified) = &job.key;
    let dir = cache_dir.join(cache_key(path, *modified));
    let done = dir.join("done");
    if !done.is_file() {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cache folder: {e}"))?;
        prune(cache_dir, &dir);
        let mut cmd = std::process::Command::new(&job.ffmpeg);
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-threads",
            "2",
        ])
        .args(["-hwaccel", "auto"])
        .args(["-sseof", &format!("-{SPAN_SECS}")])
        .arg("-i")
        .arg(path)
        .args([
            "-an",
            "-vf",
            &format!("fps={EXTRACT_FPS},scale={FRAME_WIDTH}:-2"),
            "-q:v",
            "6",
        ])
        .arg(dir.join("f%03d.jpg"));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS: no console, and games come first.
            cmd.creation_flags(0x0800_0000 | 0x0000_4000);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(err.lines().last().unwrap_or("ffmpeg failed").to_string());
        }
        let _ = std::fs::write(&done, b"");
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("cache folder: {e}"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false))
        .collect();
    files.sort();
    let mut frames = Vec::with_capacity(files.len());
    for f in files {
        let Ok(img) = image::open(&f) else { continue };
        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        frames.push(egui::ColorImage::from_rgba_unmultiplied(
            size,
            rgba.as_raw(),
        ));
    }
    Ok(frames)
}

/// Keep the cache bounded: remove the oldest previews beyond [`MAX_CACHED`].
fn prune(cache_dir: &Path, keep: &Path) {
    let Ok(rd) = std::fs::read_dir(cache_dir) else {
        return;
    };
    let mut dirs: Vec<(SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| {
            let t = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (t, e.path())
        })
        .filter(|(_, p)| p != keep)
        .collect();
    if dirs.len() < MAX_CACHED {
        return;
    }
    dirs.sort();
    for (_, p) in dirs.iter().take(dirs.len() + 1 - MAX_CACHED) {
        let _ = std::fs::remove_dir_all(p);
    }
}
