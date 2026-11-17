//! Saving a clip the instant it's asked for.
//!
//! The save hotkey, the voice trigger and the tray all call [`QuickSave::trigger`] directly,
//! on whatever thread noticed the request. It plays the clip-saved sound right away (the
//! OBS save itself takes a second or two to finish writing the file) and starts the save on
//! a worker thread. Before, the request first had to wake the app's window, which in a game
//! can lag noticeably, and the sound only played once the file was finished.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rewind_core::library;

use crate::engine::Saver;
use crate::platform;

/// What happens when a save finishes (success or not). Runs on the worker thread.
pub type Done = Box<dyn Fn(Result<PathBuf, String>) + Send + Sync>;

#[derive(Clone, Default, PartialEq)]
pub struct Settings {
    pub root: PathBuf,
    pub per_game: bool,
    /// `None`: no sound. `Some("")`: the built-in chime. Otherwise a custom sound's file name.
    pub sound: Option<String>,
    pub sounds_dir: PathBuf,
}

pub struct QuickSave {
    saver: Mutex<Option<Saver>>,
    settings: Mutex<Settings>,
    saving: AtomicBool,
    done: Done,
}

impl QuickSave {
    pub fn new(done: Done) -> Arc<Self> {
        Arc::new(Self {
            saver: Mutex::new(None),
            settings: Mutex::new(Settings::default()),
            saving: AtomicBool::new(false),
            done,
        })
    }

    /// Ready to save (`Some`) or not (`None`: engine not running or Instant Replay off).
    pub fn arm(&self, saver: Option<Saver>, settings: Settings) {
        if let Some(sound) = settings.sound.as_deref().filter(|s| !s.is_empty()) {
            // Open a custom sound now so the first save doesn't wait for the decoder.
            crate::sounds::prepare(&settings.sounds_dir, sound);
        }
        if let Ok(mut s) = self.saver.lock() {
            *s = saver;
        }
        if let Ok(mut s) = self.settings.lock() {
            *s = settings;
        }
    }

    pub fn is_saving(&self) -> bool {
        self.saving.load(Ordering::Relaxed)
    }

    /// Save a clip now. False when there's nothing to save (Instant Replay off) or a save is
    /// already running.
    pub fn trigger(self: &Arc<Self>) -> bool {
        let Some(saver) = self.saver.lock().ok().and_then(|s| s.clone()) else {
            return false;
        };
        if self.saving.swap(true, Ordering::SeqCst) {
            return false;
        }
        let settings = self.settings.lock().map(|s| s.clone()).unwrap_or_default();
        // The sound first: this is the "got it" moment.
        if let Some(sound) = &settings.sound {
            crate::sounds::play_clip_saved(&settings.sounds_dir, sound);
        }
        // Note what's in front now, while the game still is (the save takes a moment).
        let game = if settings.per_game {
            platform::foreground_app_name().and_then(|n| library::game_folder_name(&n))
        } else {
            None
        };
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("rewind-save".into())
            .spawn(move || {
                let result = saver
                    .save()
                    .map(|path| move_to_game_folder(&settings.root, path, game.as_deref()));
                if result.is_err() && settings.sound.is_some() {
                    // The sound already said "saved": make the failure audible too.
                    platform::error_sound();
                }
                this.saving.store(false, Ordering::SeqCst);
                (this.done)(result);
            });
        if spawned.is_err() {
            self.saving.store(false, Ordering::SeqCst);
            return false;
        }
        true
    }
}

/// Move a saved clip into its game's folder (per-game folders).
pub fn move_to_game_folder(root: &Path, path: PathBuf, game: Option<&str>) -> PathBuf {
    let Some(game) = game else { return path };
    let dir = root.join(game);
    if std::fs::create_dir_all(&dir).is_err() {
        return path;
    }
    let Some(name) = path.file_name() else {
        return path;
    };
    let mut target = dir.join(name);
    if target.exists() {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "clip".into());
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mp4".into());
        target = library::unique_path(&dir, &stem, &ext);
    }
    // OBS may still hold the file for a moment after reporting it saved.
    for _ in 0..10 {
        if std::fs::rename(&path, &target).is_ok() {
            return target;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    path
}
