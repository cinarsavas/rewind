//! Hover previews for the clip list (optional add-on).
//!
//! When the pointer rests on a clip's thumbnail, the last 20 seconds of that clip play as a
//! small looping flip-book. Frames come from the add-on's ffmpeg.exe, once per clip, and are
//! cached on disk as one small file of JPEGs.
//!
//! To make hovering feel instant:
//! - frames are streamed: ffmpeg writes JPEGs to a pipe and each one is shown the moment it
//!   arrives, so playback starts after the first frame (a fraction of a second) instead of
//!   after the whole clip has been decoded;
//! - while Rewind's window is in front, the clips you can see are prepared in the background,
//!   one at a time, at idle priority on one thread, so they're usually ready before you
//!   hover. A hover interrupts that work, and the interrupted clip is queued again;
//! - a hover runs at normal priority with every core, software decoding (no slow GPU
//!   start-up), minimal probing, and skips frames nothing depends on;
//! - frames stay compressed in memory and only the one on screen is decoded;
//! - frames are pulled at the size the card is actually shown at.
//!
//! Nothing runs while you're in a game (Rewind isn't in front then), and the UI uploads one
//! small texture at a time, only while hovering.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;

/// Seconds from the end of the clip that the preview covers.
const SPAN_SECS: u32 = 20;
/// Frames pulled per second of clip, and the playback rate (2x speed: a 10 s loop).
const EXTRACT_FPS: u32 = 6;
const PLAY_FPS: f32 = 12.0;
/// Frame widths: cards shown smaller than about 300 px get the small size.
const SMALL_WIDTH: u32 = 240;
const LARGE_WIDTH: u32 = 400;
/// Previews kept on disk before the oldest are removed.
const MAX_CACHED: usize = 200;
/// Clips kept in memory for quick back-and-forth hovering.
const IN_MEMORY: usize = 10;
/// Cache file format version; bump to ignore old caches.
const CACHE_VERSION: u32 = 2;

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
    /// The pointer is on this clip: do it now and stream the frames back.
    Hover(Key, PathBuf),
    /// Prepare this clip's frames on disk in the background.
    Prefetch(Key, PathBuf),
}

enum Done {
    /// One more frame of a hovered clip.
    Frame(Key, Vec<u8>),
    /// Every frame of a hovered clip has been sent.
    Finished(Key),
    Failed(Key, String),
}

struct Entry {
    key: Key,
    /// JPEG bytes of every frame received so far; only the one on screen is decoded.
    frames: Vec<Vec<u8>>,
    complete: bool,
    failed: Option<String>,
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
    entries: Vec<Entry>,
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
            entries: Vec::new(),
            prefetched: HashSet::new(),
            texture: None,
            shown: None,
            hover_started: Instant::now(),
            hovered: None,
        }
    }

    fn drain(&mut self) {
        while let Ok(done) = self.rx.try_recv() {
            match done {
                Done::Frame(k, bytes) => {
                    if let Some(e) = self.entries.iter_mut().find(|e| e.key == k) {
                        if !e.complete {
                            e.frames.push(bytes);
                        }
                    }
                }
                Done::Finished(k) => {
                    if let Some(e) = self.entries.iter_mut().find(|e| e.key == k) {
                        e.complete = true;
                        if e.frames.is_empty() {
                            e.failed = Some("the clip is too short".into());
                        }
                    }
                }
                Done::Failed(k, err) => {
                    if let Some(e) = self.entries.iter_mut().find(|e| e.key == k) {
                        // A preview that already shows frames keeps them.
                        e.complete = !e.frames.is_empty();
                        if e.frames.is_empty() {
                            e.failed = Some(err);
                        }
                    }
                }
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
        match self.entries.iter().position(|e| e.key == key) {
            Some(i) => {
                // Most recently used goes last. A failed or unfinished one (its stream was
                // stopped when the pointer moved on) starts again on a new hover.
                let mut entry = self.entries.remove(i);
                if new_hover && (entry.failed.is_some() || !entry.complete) {
                    entry.frames.clear();
                    entry.failed = None;
                    entry.complete = false;
                    let _ = self.tx.send(Cmd::Hover(key.clone(), ffmpeg.to_path_buf()));
                }
                self.entries.push(entry);
            }
            None => {
                if self.entries.len() >= IN_MEMORY {
                    self.entries.remove(0);
                }
                self.entries.push(Entry {
                    key: key.clone(),
                    frames: Vec::new(),
                    complete: false,
                    failed: None,
                });
                let _ = self.tx.send(Cmd::Hover(key.clone(), ffmpeg.to_path_buf()));
            }
        }
        let Some(entry) = self.entries.last() else {
            return Frame::Loading;
        };
        if let Some(e) = &entry.failed {
            return Frame::Failed(e);
        }
        let n = entry.frames.len();
        if n == 0 {
            ctx.request_repaint_after(Duration::from_millis(30));
            return Frame::Loading;
        }
        let elapsed = self.hover_started.elapsed().as_secs_f32();
        let step = (elapsed * PLAY_FPS) as usize;
        let i = if entry.complete {
            step % n
        } else {
            // Still streaming: play along as frames arrive, waiting on the newest one.
            step.min(n - 1)
        };
        if self.shown.as_ref() != Some(&(key.clone(), i)) || self.texture.is_none() {
            // Decode just this frame (a few hundred microseconds for a small JPEG).
            if let Some(image) = decode(&entry.frames[i]) {
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
        }
        ctx.request_repaint_after(Duration::from_secs_f32(1.0 / PLAY_FPS));
        let progress = if entry.complete {
            (i + 1) as f32 / n as f32
        } else {
            // Unknown total while streaming; the expected count is a close guess.
            ((i + 1) as f32 / (SPAN_SECS * EXTRACT_FPS) as f32).min(1.0)
        };
        match &self.texture {
            Some(tex) if self.shown.is_some() => Frame::Ready(tex, progress),
            _ => Frame::Loading,
        }
    }

    /// The pointer left every thumbnail: restart the loop next time.
    pub fn unhover(&mut self) {
        if self.hovered.take().is_some() {
            // Stop a stream nobody is watching; background preparation carries on.
            let _ = self.tx.send(Cmd::Hover(
                (PathBuf::new(), SystemTime::UNIX_EPOCH, 0),
                PathBuf::new(),
            ));
        }
        self.shown = None;
    }
}

/// Why a running ffmpeg was stopped.
enum Stopped {
    /// A command arrived that needs the worker; it's handed back.
    By(Cmd),
    Closing,
}

/// The preview thread: hovers first, background preparation when nothing else is asked for.
fn worker(cache_dir: &Path, cmds: &Receiver<Cmd>, done: &Sender<Done>, ctx: &egui::Context) {
    let mut hover: Option<(Key, PathBuf)> = None;
    let mut background: VecDeque<(Key, PathBuf)> = VecDeque::new();
    let take = |cmd: Cmd, hover: &mut Option<(Key, PathBuf)>, bg: &mut VecDeque<(Key, PathBuf)>| {
        match cmd {
            Cmd::Hover(k, f) => *hover = (!f.as_os_str().is_empty()).then_some((k, f)),
            Cmd::Prefetch(k, f) => bg.push_back((k, f)),
        }
    };
    loop {
        if hover.is_none() && background.is_empty() {
            match cmds.recv() {
                Ok(c) => take(c, &mut hover, &mut background),
                Err(_) => return,
            }
        }
        // Collect everything waiting; only the newest hover counts.
        loop {
            match cmds.try_recv() {
                Ok(c) => take(c, &mut hover, &mut background),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if let Some((key, ffmpeg)) = hover.take() {
            let mut sink = |bytes: Vec<u8>| {
                let _ = done.send(Done::Frame(key.clone(), bytes));
                ctx.request_repaint();
            };
            match run(
                cache_dir,
                &key,
                &ffmpeg,
                true,
                cmds,
                Some(&mut sink),
                &mut background,
            ) {
                Ok(()) => {
                    let _ = done.send(Done::Finished(key));
                }
                Err(RunErr::Stopped(Stopped::By(cmd))) => {
                    // The pointer moved on: finish this clip in the background so it's
                    // instant next time, and handle the new hover.
                    background.push_back((key, ffmpeg));
                    take(cmd, &mut hover, &mut background);
                }
                Err(RunErr::Stopped(Stopped::Closing)) => return,
                Err(RunErr::Failed(e)) => {
                    let _ = done.send(Done::Failed(key, e));
                }
            }
            ctx.request_repaint();
            continue;
        }

        if let Some((key, ffmpeg)) = background.pop_front() {
            let mut more = VecDeque::new();
            let result = run(cache_dir, &key, &ffmpeg, false, cmds, None, &mut more);
            background.extend(more);
            match result {
                Ok(()) | Err(RunErr::Failed(_)) => {}
                Err(RunErr::Stopped(Stopped::Closing)) => return,
                Err(RunErr::Stopped(Stopped::By(cmd))) => {
                    // A hover (or more work) arrived: finish this clip later instead of
                    // forgetting it.
                    background.push_back((key, ffmpeg));
                    take(cmd, &mut hover, &mut background);
                }
            }
        }
    }
}

enum RunErr {
    Stopped(Stopped),
    Failed(String),
}

fn cache_file(cache_dir: &Path, key: &Key) -> PathBuf {
    let mut h = DefaultHasher::new();
    CACHE_VERSION.hash(&mut h);
    key.0.hash(&mut h);
    key.1.hash(&mut h);
    key.2.hash(&mut h);
    cache_dir.join(format!("{:016x}.mjpg", h.finish()))
}

fn decode(jpeg: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// Splits a stream of back-to-back JPEGs (as ffmpeg's mjpeg encoder writes them) into
/// frames. ffmpeg's frames carry no embedded thumbnails or restart markers, so a frame
/// ends at the first end-of-image marker after its start-of-image marker.
#[derive(Default)]
struct JpegSplitter {
    buf: Vec<u8>,
    /// Where the search for the end marker resumes.
    scan: usize,
}

impl JpegSplitter {
    fn push(&mut self, data: &[u8], mut out: impl FnMut(Vec<u8>)) {
        self.buf.extend_from_slice(data);
        loop {
            // Drop anything before a start-of-image marker.
            if self.buf.len() >= 2 && self.buf[..2] != [0xFF, 0xD8] {
                match self.buf.windows(2).position(|w| w == [0xFF, 0xD8]) {
                    Some(p) => {
                        self.buf.drain(..p);
                        self.scan = 0;
                    }
                    None => {
                        let keep = usize::from(self.buf.last() == Some(&0xFF));
                        let cut = self.buf.len() - keep;
                        self.buf.drain(..cut);
                        self.scan = 0;
                        return;
                    }
                }
            }
            let from = self.scan.max(2);
            if self.buf.len() < from + 2 {
                return;
            }
            match self.buf[from..].windows(2).position(|w| w == [0xFF, 0xD9]) {
                Some(p) => {
                    let end = from + p + 2;
                    let frame: Vec<u8> = self.buf.drain(..end).collect();
                    self.scan = 0;
                    out(frame);
                }
                None => {
                    self.scan = self.buf.len() - 1;
                    return;
                }
            }
        }
    }
}

fn split_all(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut s = JpegSplitter::default();
    let mut frames = Vec::new();
    s.push(bytes, |f| frames.push(f));
    frames
}

/// Produce one clip's frames: from the cache when it's there, otherwise by running ffmpeg,
/// which also fills the cache. With a `sink`, every frame is handed over as soon as it
/// exists. A command arriving meanwhile stops ffmpeg and is handed back, except that a
/// hover keeps going through background requests (they're put in `queue`).
fn run(
    cache_dir: &Path,
    key: &Key,
    ffmpeg: &Path,
    foreground: bool,
    cmds: &Receiver<Cmd>,
    mut sink: Option<&mut dyn FnMut(Vec<u8>)>,
    queue: &mut VecDeque<(Key, PathBuf)>,
) -> Result<(), RunErr> {
    let file = cache_file(cache_dir, key);
    if let Ok(bytes) = std::fs::read(&file) {
        let frames = split_all(&bytes);
        if !frames.is_empty() {
            if let Some(sink) = sink.as_mut() {
                for f in frames {
                    sink(f);
                }
            }
            return Ok(());
        }
    }
    std::fs::create_dir_all(cache_dir).map_err(|e| RunErr::Failed(format!("cache folder: {e}")))?;
    prune(cache_dir, &file);

    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        // Clip files are local and well-formed: don't spend time probing them.
        .args(["-probesize", "1M", "-analyzeduration", "0"])
        // A hover gets every core; background work stays on one.
        .args(["-threads", if foreground { "0" } else { "1" }])
        // Frames nothing else depends on can be skipped; at 6 frames a second they're
        // never missed, and decoding gets much cheaper.
        .args(["-skip_frame", "noref"])
        .args(["-sseof", &format!("-{SPAN_SECS}")])
        .arg("-i")
        .arg(&key.0)
        .args([
            "-an",
            "-sn",
            "-vf",
            &format!(
                "fps={EXTRACT_FPS},scale={}:-2:flags=fast_bilinear",
                key.2
            ),
            "-f",
            "image2pipe",
            "-c:v",
            "mjpeg",
            "-q:v",
            "7",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
        .map_err(|e| RunErr::Failed(format!("couldn't run ffmpeg: {e}")))?;

    // Read the pipe on its own thread so this one can watch for new commands.
    let (chunk_tx, chunks) = channel::<Vec<u8>>();
    let mut stdout = child.stdout.take().expect("piped stdout");
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    if chunk_tx.send(buf[..n].to_vec()).is_err() {
                        return;
                    }
                }
            }
        }
    });
    let mut stderr = child.stderr.take();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = stderr.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });

    let mut splitter = JpegSplitter::default();
    let mut all: Vec<u8> = Vec::new();
    let mut count = 0usize;
    loop {
        match chunks.recv_timeout(Duration::from_millis(30)) {
            Ok(data) => {
                all.extend_from_slice(&data);
                splitter.push(&data, |f| {
                    count += 1;
                    if let Some(sink) = sink.as_mut() {
                        sink(f);
                    }
                });
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        match cmds.try_recv() {
            // Background requests just wait their turn; only a hover stops ffmpeg.
            Ok(Cmd::Prefetch(k, f)) => queue.push_back((k, f)),
            // "Stopped hovering" only matters to a hover stream.
            Ok(Cmd::Hover(_, f)) if !foreground && f.as_os_str().is_empty() => {}
            Ok(c) => {
                stop(&mut child);
                return Err(RunErr::Stopped(Stopped::By(c)));
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                stop(&mut child);
                return Err(RunErr::Stopped(Stopped::Closing));
            }
        }
    }
    let status = child.wait().map_err(|e| RunErr::Failed(e.to_string()))?;
    let err = err_reader.join().unwrap_or_default();
    if !status.success() && count == 0 {
        return Err(RunErr::Failed(
            err.lines().last().unwrap_or("ffmpeg failed").to_string(),
        ));
    }
    if count > 0 {
        // Write whole, then rename, so a half-written cache is never read.
        let tmp = file.with_extension("part");
        if std::fs::write(&tmp, &all).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }
    Ok(())
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Keep the cache bounded: remove the oldest previews beyond [`MAX_CACHED`]. Also clears
/// out the folders older versions of Rewind used.
fn prune(cache_dir: &Path, keep: &Path) {
    let Ok(rd) = std::fs::read_dir(cache_dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(&p);
            continue;
        }
        if p == keep || p.extension().map(|x| x != "mjpg").unwrap_or(true) {
            continue;
        }
        let t = e
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        files.push((t, p));
    }
    if files.len() < MAX_CACHED {
        return;
    }
    files.sort();
    for (_, p) in files.iter().take(files.len() + 1 - MAX_CACHED) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(n: u8) -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, n, 0xFF, 0x00, n, 0xFF, 0xD9]
    }

    #[test]
    fn splits_back_to_back_jpegs_in_any_chunking() {
        let stream: Vec<u8> = (0..5).flat_map(jpeg).collect();
        for chunk in 1..stream.len() {
            let mut s = JpegSplitter::default();
            let mut got = Vec::new();
            for c in stream.chunks(chunk) {
                s.push(c, |f| got.push(f));
            }
            assert_eq!(got, (0..5).map(jpeg).collect::<Vec<_>>(), "chunk {chunk}");
        }
    }

    #[test]
    fn skips_junk_before_a_frame() {
        let mut stream = vec![1, 2, 3, 0xFF];
        stream.extend(jpeg(9));
        assert_eq!(split_all(&stream), vec![jpeg(9)]);
    }
}
