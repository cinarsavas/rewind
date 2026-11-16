//! Hover previews for the clip list (optional add-on).
//!
//! When the pointer rests on a clip's thumbnail, the last 20 seconds of that clip play as a
//! small looping flip-book. Frames come from the add-on's ffmpeg.exe, once per clip, and are
//! cached on disk as small JPEGs.
//!
//! To make hovering feel instant:
//! - while Rewind's window is in front, the clips you can see are prepared in the background,
//!   one at a time, at idle priority on one thread, so they're usually ready before you hover;
//! - a hover jumps the queue and stops any background job, and runs at normal priority with
//!   all threads, skipping frames that nothing depends on;
//! - frames are kept as small JPEGs in memory and only the one on screen is decoded, so a
//!   hovered clip costs well under a megabyte instead of tens of megabytes;
//! - frames are pulled at the size the card is actually shown at (small cards get small
//!   frames), so big libraries and small cards stay cheap.
//!
//! Nothing runs while you're in a game (Rewind isn't in front then), and the UI uploads one
//! small texture at a time, only while hovering.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;

/// Seconds from the end of the clip that the preview covers.
const SPAN_SECS: u32 = 20;
/// Frames pulled per second of clip, and the playback rate (2x speed: a 10 s loop).
const EXTRACT_FPS: u32 = 4;
const PLAY_FPS: f32 = 8.0;
/// Frame widths: cards shown smaller than about 300 px get the small size.
const SMALL_WIDTH: u32 = 240;
const LARGE_WIDTH: u32 = 400;
/// Previews kept on disk before the oldest are removed.
const MAX_CACHED: usize = 150;
/// Decoded clips kept in memory for quick back-and-forth hovering.
const IN_MEMORY: usize = 8;

/// Clip path, modification time, frame width.
type Key = (PathBuf, SystemTime, u32);

/// Frame width for a thumbnail shown `display_px` physical pixels wide.
fn width_for(display_px: f32) -> u32 {
    if display_px > 300.0 {
        LARGE_WIDTH
    } else {
        SMALL_WIDTH
    }
}

enum Cmd {
    /// The pointer is on this clip: do it now and send the frames back.
    Hover(Key, PathBuf),
    /// Prepare this clip's frames on disk in the background.
    Prefetch(Key, PathBuf),
}

enum Done {
    Ready(Key, Vec<Vec<u8>>),
    Failed(Key, String),
}

enum State {
    Loading,
    /// JPEG bytes of every frame; only the one on screen is decoded.
    Ready(Vec<Vec<u8>>),
    Failed(String),
}

/// What the clip row should draw for the hovered thumbnail.
pub enum Frame<'a> {
    Loading,
    Ready(&'a egui::TextureHandle, f32),
    Failed(&'a str),
}

pub struct Previews {
    tx: Sender<Cmd>,
    rx: Receiver<Done>,
    /// Recently hovered clips, oldest first.
    states: Vec<(Key, State)>,
    prefetched: HashSet<Key>,
    texture: Option<egui::TextureHandle>,
    shown: Option<(Key, usize)>,
    hover_started: Instant,
    hovered: Option<Key>,
}

impl Previews {
    pub fn new(cache_dir: PathBuf, ctx: egui::Context) -> Self {
        let (tx, cmds) = channel::<Cmd>();
        let (done_tx, rx) = channel::<Done>();
        std::thread::Builder::new()
            .name("rewind-preview".into())
            .spawn(move || worker(&cache_dir, &cmds, &done_tx, &ctx))
            .ok();
        Self {
            tx,
            rx,
            states: Vec::new(),
            prefetched: HashSet::new(),
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

    /// Ask for this clip to be prepared in the background (visible rows, window in front).
    pub fn prefetch(&mut self, path: &Path, modified: SystemTime, ffmpeg: &Path, display_px: f32) {
        let key: Key = (path.to_path_buf(), modified, width_for(display_px));
        if self.prefetched.insert(key.clone()) {
            let _ = self.tx.send(Cmd::Prefetch(key, ffmpeg.to_path_buf()));
        }
    }

    /// Called by the hovered thumbnail. Returns what to draw over it.
    pub fn hover(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        modified: SystemTime,
        ffmpeg: &Path,
        display_px: f32,
    ) -> Frame<'_> {
        self.drain();
        let key: Key = (path.to_path_buf(), modified, width_for(display_px));
        let new_hover = self.hovered.as_ref() != Some(&key);
        if new_hover {
            self.hovered = Some(key.clone());
            self.hover_started = Instant::now();
        }
        match self.states.iter().position(|(k, _)| *k == key) {
            Some(i) => {
                // Most recently used goes last; a failed one is retried on a new hover.
                let entry = self.states.remove(i);
                let retry = matches!(entry.1, State::Failed(_));
                self.states.push(entry);
                if retry && new_hover {
                    self.states.last_mut().unwrap().1 = State::Loading;
                    let _ = self.tx.send(Cmd::Hover(key.clone(), ffmpeg.to_path_buf()));
                }
            }
            None => {
                if self.states.len() >= IN_MEMORY {
                    self.states.remove(0);
                }
                self.states.push((key.clone(), State::Loading));
                let _ = self.tx.send(Cmd::Hover(key.clone(), ffmpeg.to_path_buf()));
            }
        }
        let Some((_, state)) = self.states.last() else {
            return Frame::Loading;
        };
        match state {
            State::Loading => {
                ctx.request_repaint_after(Duration::from_millis(60));
                Frame::Loading
            }
            State::Failed(e) => Frame::Failed(e),
            State::Ready(frames) => {
                let elapsed = self.hover_started.elapsed().as_secs_f32();
                let n = frames.len();
                let i = ((elapsed * PLAY_FPS) as usize) % n;
                if self.shown.as_ref() != Some(&(key.clone(), i)) || self.texture.is_none() {
                    // Decode just this frame (a few hundred microseconds for a small JPEG).
                    let Some(image) = decode(&frames[i]) else {
                        return Frame::Loading;
                    };
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
                ctx.request_repaint_after(Duration::from_secs_f32(1.0 / PLAY_FPS));
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
        self.shown = None;
    }
}

/// The preview thread: hovers first, background preparation when nothing else is asked for.
fn worker(cache_dir: &Path, cmds: &Receiver<Cmd>, done: &Sender<Done>, ctx: &egui::Context) {
    let mut hover: Option<(Key, PathBuf)> = None;
    let mut background: VecDeque<(Key, PathBuf)> = VecDeque::new();
    loop {
        // Collect everything waiting; only the newest hover counts.
        let first = if hover.is_none() && background.is_empty() {
            match cmds.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            }
        } else {
            None
        };
        for cmd in first
            .into_iter()
            .chain(std::iter::from_fn(|| cmds.try_recv().ok()))
        {
            match cmd {
                Cmd::Hover(k, f) => hover = Some((k, f)),
                Cmd::Prefetch(k, f) => background.push_back((k, f)),
            }
        }

        if let Some((key, ffmpeg)) = hover.take() {
            let msg = match frames_for(cache_dir, &key, &ffmpeg, true, None) {
                Ok(frames) => Done::Ready(key, frames),
                Err(e) => Done::Failed(key, e),
            };
            if done.send(msg).is_err() {
                return;
            }
            ctx.request_repaint();
            continue;
        }

        if let Some((key, ffmpeg)) = background.pop_front() {
            // Stop if a hover arrives meanwhile; it is handled next, and this clip is
            // prepared again later if still needed.
            let mut interrupted = None;
            let _ = extract(
                cache_dir,
                &key,
                &ffmpeg,
                false,
                Some((cmds, &mut interrupted)),
            );
            if let Some(cmd) = interrupted {
                match cmd {
                    Cmd::Hover(k, f) => hover = Some((k, f)),
                    Cmd::Prefetch(k, f) => background.push_back((k, f)),
                }
            }
        }
    }
}

fn cache_dir_for(cache_dir: &Path, key: &Key) -> PathBuf {
    let mut h = DefaultHasher::new();
    key.0.hash(&mut h);
    key.1.hash(&mut h);
    key.2.hash(&mut h);
    cache_dir.join(format!("{:016x}", h.finish()))
}

fn decode(jpeg: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

/// Make sure the frames are on disk, then decode them.
fn frames_for(
    cache_dir: &Path,
    key: &Key,
    ffmpeg: &Path,
    foreground: bool,
    interrupt: Option<(&Receiver<Cmd>, &mut Option<Cmd>)>,
) -> Result<Vec<Vec<u8>>, String> {
    let dir = extract(cache_dir, key, ffmpeg, foreground, interrupt)?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("cache folder: {e}"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false))
        .collect();
    files.sort();
    // Keep the compressed bytes; decoding happens one frame at a time while shown.
    Ok(files
        .iter()
        .filter_map(|f| std::fs::read(f).ok())
        .filter(|b| !b.is_empty())
        .collect())
}

/// Run ffmpeg for one clip unless its frames are already cached. Returns the frame folder.
/// With `interrupt`, a new command arriving stops ffmpeg; it is handed back to the caller.
fn extract(
    cache_dir: &Path,
    key: &Key,
    ffmpeg: &Path,
    foreground: bool,
    mut interrupt: Option<(&Receiver<Cmd>, &mut Option<Cmd>)>,
) -> Result<PathBuf, String> {
    let dir = cache_dir_for(cache_dir, key);
    let done = dir.join("done");
    if done.is_file() {
        return Ok(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cache folder: {e}"))?;
    prune(cache_dir, &dir);

    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        // A hover gets every core; background work stays on one.
        .args(["-threads", if foreground { "0" } else { "1" }])
        .args(["-hwaccel", "auto"])
        // Frames nothing else depends on can be skipped; at 5 frames a second they're
        // never missed, and decoding gets much cheaper.
        .args(["-skip_frame", "nonref"])
        .args(["-sseof", &format!("-{SPAN_SECS}")])
        .arg("-i")
        .arg(&key.0)
        .args([
            "-an",
            "-vf",
            &format!("fps={EXTRACT_FPS},scale={}:-2:flags=fast_bilinear", key.2),
            "-q:v",
            "7",
        ])
        .arg(dir.join("f%03d.jpg"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW, plus IDLE_PRIORITY_CLASS for background work so anything else
        // on the PC always comes first.
        let priority = if foreground { 0 } else { 0x0000_0040 };
        cmd.creation_flags(0x0800_0000 | priority);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if let Some((cmds, slot)) = interrupt.as_mut() {
            match cmds.recv_timeout(Duration::from_millis(40)) {
                Ok(cmd) => {
                    **slot = Some(cmd);
                    stop(&mut child);
                    let _ = std::fs::remove_dir_all(&dir);
                    return Err("interrupted".into());
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    stop(&mut child);
                    return Err("closing".into());
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            use std::io::Read;
            let _ = e.read_to_string(&mut err);
        }
        let _ = std::fs::remove_dir_all(&dir);
        return Err(err.lines().last().unwrap_or("ffmpeg failed").to_string());
    }
    let _ = std::fs::write(&done, b"");
    Ok(dir)
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
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
