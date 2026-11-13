//! Optional add-ons: install, remove and discover voice packs and themes.
//! Files live under `<config dir>/packs/`; nothing here is bundled in the installer.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use eframe::egui;
use rewind_core::packs::{index_url, PackIndex, ThemePack, VoicePack};
use rewind_core::theme::{Palette, ThemeFile};

use crate::download::{download_verified, fetch_text, unzip, Progress};

/// Set by CI (`REWIND_REPO=owner/name`) so release builds fetch add-ons from their own repo.
pub const REPO: &str = match option_env!("REWIND_REPO") {
    Some(r) => r,
    None => "OWNER/rewind",
};

pub const KEY_RUNTIME: &str = "voice-runtime";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexState {
    Idle,
    Loading,
    Ready,
    Failed(String),
}

#[derive(Clone)]
pub struct InstalledTheme {
    pub id: String,
    pub name: String,
    pub author: String,
    pub palette: Palette,
}

enum Msg {
    Index(Result<PackIndex, String>),
    Done { key: String, result: Result<(), String> },
}

pub enum PackEvent {
    Installed(String),
    Failed(String, String),
}

pub struct PackManager {
    root: PathBuf,
    ctx: egui::Context,
    pub index: Option<PackIndex>,
    pub index_state: IndexState,
    pub jobs: HashMap<String, Arc<Progress>>,
    pub themes: Vec<InstalledTheme>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl PackManager {
    pub fn new(root: PathBuf, ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        let mut m = Self { root, ctx, index: None, index_state: IndexState::Idle, jobs: HashMap::new(), themes: Vec::new(), tx, rx };
        m.scan_themes();
        m
    }

    pub fn voice_runtime_dir(&self) -> PathBuf {
        self.root.join(KEY_RUNTIME)
    }

    pub fn voice_model_dir(&self, id: &str) -> PathBuf {
        self.root.join(format!("voice-{id}"))
    }

    pub fn runtime_installed(&self) -> bool {
        self.voice_runtime_dir().join("libvosk.dll").is_file()
    }

    pub fn model_installed(&self, id: &str) -> bool {
        let d = self.voice_model_dir(id);
        d.join("am").is_dir() || d.join("conf").is_dir()
    }

    fn themes_dir(&self) -> PathBuf {
        self.root.join("themes")
    }

    pub fn request_index(&mut self) {
        if self.index_state == IndexState::Loading {
            return;
        }
        if REPO == "OWNER/rewind" {
            self.index_state = IndexState::Failed("This build isn't linked to a GitHub repository, so there's nothing to download from. Release builds from GitHub are.".into());
            return;
        }
        self.index_state = IndexState::Loading;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let result = fetch_text(&index_url(REPO)).and_then(|t| PackIndex::from_toml(&t));
            let _ = tx.send(Msg::Index(result));
            ctx.request_repaint();
        });
    }

    fn spawn_job(&mut self, key: &str, job: impl FnOnce(Arc<Progress>) -> Result<(), String> + Send + 'static) {
        if self.jobs.contains_key(key) {
            return;
        }
        let progress = Arc::new(Progress::default());
        self.jobs.insert(key.to_string(), progress.clone());
        let (tx, ctx, key) = (self.tx.clone(), self.ctx.clone(), key.to_string());
        std::thread::spawn(move || {
            let result = job(progress);
            let _ = tx.send(Msg::Done { key, result });
            ctx.request_repaint();
        });
    }

    pub fn install_runtime(&mut self) {
        let Some(file) = self.index.as_ref().and_then(|i| i.voice_runtime.clone()) else { return };
        let (root, dest) = (self.root.clone(), self.voice_runtime_dir());
        self.spawn_job(KEY_RUNTIME, move |p| install_zip(&root, "voice-runtime.zip", &file.url, &file.sha256, &dest, &p, |d| d.join("libvosk.dll").is_file()));
    }

    pub fn install_voice(&mut self, pack: &VoicePack) {
        let (root, dest, file) = (self.root.clone(), self.voice_model_dir(&pack.id), pack.file.clone());
        let key = format!("voice-{}", pack.id);
        let zip_name = format!("{key}.zip");
        self.spawn_job(&key, move |p| install_zip(&root, &zip_name, &file.url, &file.sha256, &dest, &p, |d| d.join("am").is_dir() || d.join("conf").is_dir()));
    }

    pub fn install_theme(&mut self, pack: &ThemePack) {
        let (dir, file, id) = (self.themes_dir(), pack.file.clone(), pack.id.clone());
        self.spawn_job(&format!("theme-{id}"), move |p| {
            let tmp = dir.join(format!("{id}.toml.download"));
            download_verified(&file.url, &tmp, &file.sha256, &p)?;
            let text = fs::read_to_string(&tmp).map_err(|e| e.to_string())?;
            let check = ThemeFile::from_toml(&text).and_then(|t| t.to_palette().map(|_| ()));
            if let Err(e) = check {
                let _ = fs::remove_file(&tmp);
                return Err(format!("the theme isn't usable: {e}"));
            }
            fs::rename(&tmp, dir.join(format!("{id}.toml"))).map_err(|e| e.to_string())
        });
    }

    pub fn remove_runtime(&mut self) {
        let _ = fs::remove_dir_all(self.voice_runtime_dir());
    }

    pub fn remove_voice(&mut self, id: &str) {
        let _ = fs::remove_dir_all(self.voice_model_dir(id));
    }

    pub fn remove_theme(&mut self, id: &str) {
        let _ = fs::remove_file(self.themes_dir().join(format!("{id}.toml")));
        self.scan_themes();
    }

    pub fn scan_themes(&mut self) {
        self.themes.clear();
        let Ok(rd) = fs::read_dir(self.themes_dir()) else { return };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(id) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else { continue };
            if !rewind_core::packs::valid_id(&id) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else { continue };
            let Ok(file) = ThemeFile::from_toml(&text) else { continue };
            let Ok(palette) = file.to_palette() else { continue };
            self.themes.push(InstalledTheme { id, name: file.name, author: file.author, palette });
        }
        self.themes.sort_by_key(|t| t.name.to_lowercase());
    }

    /// Palette for a theme id; falls back to the built-in default if it's gone or broken.
    pub fn palette_for(&self, id: &str) -> Palette {
        self.themes.iter().find(|t| t.id == id).map(|t| t.palette).unwrap_or_else(Palette::default_dark)
    }

    pub fn poll(&mut self) -> Vec<PackEvent> {
        let mut events = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Index(Ok(idx)) => {
                    self.index = Some(idx);
                    self.index_state = IndexState::Ready;
                }
                Msg::Index(Err(e)) => self.index_state = IndexState::Failed(e),
                Msg::Done { key, result } => {
                    self.jobs.remove(&key);
                    match result {
                        Ok(()) => {
                            if key.starts_with("theme-") {
                                self.scan_themes();
                            }
                            events.push(PackEvent::Installed(key));
                        }
                        Err(e) => events.push(PackEvent::Failed(key, e)),
                    }
                }
            }
        }
        events
    }
}

/// Download a zip, verify it, unpack it beside the final folder and swap it in.
fn install_zip(root: &Path, zip_name: &str, url: &str, sha256: &str, dest: &Path, progress: &Arc<Progress>, valid: impl Fn(&Path) -> bool) -> Result<(), String> {
    let zip_path = root.join(".downloads").join(zip_name);
    download_verified(url, &zip_path, sha256, progress)?;
    let staging = dest.with_extension("new");
    let _ = fs::remove_dir_all(&staging);
    let result = unzip(&zip_path, &staging).and_then(|_| if valid(&staging) { Ok(()) } else { Err("the download doesn't contain the expected files".to_string()) });
    let _ = fs::remove_file(&zip_path);
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let _ = fs::remove_dir_all(dest);
    fs::rename(&staging, dest).map_err(|e| format!("installing: {e}"))
}
