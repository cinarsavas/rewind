//! Custom "clip saved" sounds: your own .mp3 or .wav files, kept in `<config>/sounds`.
//!
//! Files are added by dragging them onto Rewind (or copying them into the folder). They play
//! through Windows' MCI on a small dedicated thread, so nothing blocks and no audio library is
//! added to the app. The built-in chime stays the default and the fallback.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

/// How many custom sounds are kept when the limit is on (Add-ons → Custom sounds).
pub const LIMIT: usize = 5;
/// Big files are almost certainly songs, not notification sounds.
const MAX_BYTES: u64 = 10 * 1024 * 1024;

fn is_sound(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mp3") || e.eq_ignore_ascii_case("wav"))
        .unwrap_or(false)
}

/// File names of the custom sounds in `dir`, sorted.
pub fn list(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.path())
                .filter(|p| is_sound(p))
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|n| n.to_lowercase());
    out
}

/// True for files [`import`] would accept, so a drop can be told apart from other files.
pub fn accepts(path: &Path) -> bool {
    is_sound(path)
}

/// Copy `src` into the sounds folder. Returns the stored file name.
pub fn import(dir: &Path, src: &Path, limit: Option<usize>) -> Result<String, String> {
    if !is_sound(src) {
        return Err("only .mp3 and .wav files can be used".into());
    }
    let size = std::fs::metadata(src)
        .map_err(|e| format!("can't read the file: {e}"))?
        .len();
    if size > MAX_BYTES {
        return Err("that file is over 10 MB; pick a short sound".into());
    }
    if let Some(limit) = limit {
        if list(dir).len() >= limit {
            return Err(format!(
                "you already have {limit} sounds; remove one, or turn off the limit in Add-ons"
            ));
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("creating the sounds folder: {e}"))?;
    let stem = src
        .file_stem()
        .map(|s| rewind_core::library::sanitize_file_name(&s.to_string_lossy()))
        .unwrap_or_else(|| "sound".into());
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "mp3".into());
    let dest = rewind_core::library::unique_path(dir, &stem, &ext);
    std::fs::copy(src, &dest).map_err(|e| format!("copying the sound: {e}"))?;
    Ok(dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default())
}

pub fn remove(dir: &Path, name: &str) {
    // Names come from `list`, but never let one reach outside the folder.
    if name.contains(['/', '\\']) || name.contains("..") {
        return;
    }
    let _ = std::fs::remove_file(dir.join(name));
}

/// Play the clip-saved sound: the chosen custom file if it exists, otherwise the built-in
/// chime. Never blocks.
pub fn play_clip_saved(dir: &Path, chosen: &str) {
    if !chosen.is_empty() && !chosen.contains(['/', '\\']) {
        let path = dir.join(chosen);
        if path.is_file() && send(Cmd::Play(path)) {
            return;
        }
    }
    crate::platform::clip_saved_sound();
}

/// Open the chosen custom sound ahead of time, so the first save plays it without delay.
pub fn prepare(dir: &Path, chosen: &str) {
    if !chosen.is_empty() && !chosen.contains(['/', '\\']) {
        let path = dir.join(chosen);
        if path.is_file() {
            send(Cmd::Prepare(path));
        }
    }
}

enum Cmd {
    Prepare(PathBuf),
    Play(PathBuf),
}

/// Queue a command on the player thread. False if the player couldn't start.
fn send(cmd: Cmd) -> bool {
    static PLAYER: OnceLock<Option<Sender<Cmd>>> = OnceLock::new();
    let player = PLAYER.get_or_init(|| {
        let (tx, rx) = channel::<Cmd>();
        std::thread::Builder::new()
            .name("rewind-sound".into())
            .spawn(move || {
                // The file that's open right now; replaying it skips decoding it again.
                let mut open: Option<PathBuf> = None;
                while let Ok(mut cmd) = rx.recv() {
                    // Several saves in a row: only the latest sound matters.
                    while let Ok(newer) = rx.try_recv() {
                        cmd = newer;
                    }
                    match cmd {
                        Cmd::Prepare(path) => {
                            if open.as_ref() != Some(&path) {
                                open = mci::open(&path).then_some(path);
                            }
                        }
                        Cmd::Play(path) => {
                            if open.as_ref() != Some(&path) {
                                open = mci::open(&path).then_some(path);
                            }
                            if open.is_some() {
                                mci::play_from_start();
                            } else {
                                // Not playable (odd codec): the chime rather than silence.
                                crate::platform::clip_saved_sound();
                            }
                        }
                    }
                }
                mci::close();
            })
            .ok()
            .map(|_| tx)
    });
    player
        .as_ref()
        .map(|tx| tx.send(cmd).is_ok())
        .unwrap_or(false)
}

#[cfg(windows)]
mod mci {
    use std::path::Path;
    use windows_sys::Win32::Media::Multimedia::mciSendStringW;

    fn send(cmd: &str) -> u32 {
        let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { mciSendStringW(wide.as_ptr(), std::ptr::null_mut(), 0, std::ptr::null_mut()) }
    }

    pub fn close() {
        send("close rewindclipsound");
    }

    /// Open (and decode the header of) a sound file. True when it can be played.
    pub fn open(path: &Path) -> bool {
        close();
        let open = format!(
            "open \"{}\" type mpegvideo alias rewindclipsound",
            path.display()
        );
        send(&open) == 0
    }

    pub fn play_from_start() {
        send("play rewindclipsound from 0");
    }
}

#[cfg(not(windows))]
mod mci {
    pub fn close() {}
    pub fn open(_path: &std::path::Path) -> bool {
        false
    }
    pub fn play_from_start() {}
}
