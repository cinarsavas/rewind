//! Optional add-ons: install, remove and discover voice packs and themes.
//! Files live under `<config dir>/packs/`; nothing here is bundled in the installer.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use eframe::egui;
use rewind_core::config::EffectStyle;
use rewind_core::packs::{index_url, EffectFile, EffectPack, PackIndex, ThemePack, VoicePack};
use rewind_core::theme::{Palette, ThemeFile, ThemeLayout};

use crate::download::{download_verified, fetch_text, unzip, Progress};

/// Where add-ons come from. CI sets `REWIND_REPO=owner/name` so a release build points at its
/// own repository; the same variable in the environment overrides it at run time, and a plain
/// local build falls back to the upstream project so add-ons still work without CI.
pub fn repo() -> String {
    let usable = |r: String| (!r.is_empty() && r != "OWNER/rewind").then_some(r);
    std::env::var("REWIND_REPO")
        .ok()
        .and_then(usable)
        .or_else(|| {
            option_env!("REWIND_REPO")
                .map(str::to_owned)
                .and_then(usable)
        })
        .unwrap_or_else(|| "cinarsavas/rewind".to_string())
}

pub const KEY_RUNTIME: &str = "voice-runtime";
pub const KEY_PREVIEW: &str = "preview";
pub const KEY_WHISPER: &str = "voice-whisper";

/// Vosk models come in two layouts: the current one with `am/`, `conf/` and `graph/`
/// folders, and the older flat one used by the small Turkish model (`final.mdl`,
/// `HCLr.fst`, `mfcc.conf`, `ivector/`). Vosk loads both, so accept both when installing
/// and when checking whether a language is ready.
fn is_voice_model(dir: &Path) -> bool {
    (dir.join("am").is_dir() && dir.join("conf").is_dir())
        || (dir.join("final.mdl").is_file() && dir.join("HCLr.fst").is_file())
}

#[cfg(test)]
mod tests {
    use super::is_voice_model;

    #[test]
    fn accepts_both_vosk_model_layouts() {
        let root = std::env::temp_dir().join(format!("rewind-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let nested = root.join("nested");
        std::fs::create_dir_all(nested.join("am")).unwrap();
        std::fs::create_dir_all(nested.join("conf")).unwrap();
        assert!(is_voice_model(&nested));

        // The Turkish small model ships flat: final.mdl + HCLr.fst at the root.
        let flat = root.join("flat");
        std::fs::create_dir_all(&flat).unwrap();
        std::fs::write(flat.join("final.mdl"), b"x").unwrap();
        std::fs::write(flat.join("HCLr.fst"), b"x").unwrap();
        assert!(is_voice_model(&flat));

        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(!is_voice_model(&empty));

        let _ = std::fs::remove_dir_all(&root);
    }
}

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
    /// Layout and background overrides from the theme file.
    pub layout: ThemeLayout,
    /// Absolute path of the background image, if the theme ships one next to its TOML.
    pub background_path: Option<PathBuf>,
}

/// A downloaded celebration pack. The painters live in the app; the pack says which ones
/// are unlocked, so a fresh install has effects switched off until you download this.
#[derive(Clone)]
pub struct InstalledEffects {
    pub id: String,
    pub name: String,
    pub author: String,
    pub styles: Vec<EffectStyle>,
}

enum Msg {
    Index(Result<PackIndex, String>),
    Done {
        key: String,
        result: Result<(), String>,
    },
}

pub enum PackEvent {
    Installed(String),
    Failed(String, String),
    /// A theme preview finished downloading and is ready to show (its display name).
    Previewing(String),
}

/// Prefix for the id of a theme being previewed, so it can never clash with an installed one.
pub const PREVIEW_PREFIX: &str = "preview:";

pub struct PackManager {
    root: PathBuf,
    ctx: egui::Context,
    pub index: Option<PackIndex>,
    pub index_state: IndexState,
    pub jobs: HashMap<String, Arc<Progress>>,
    pub themes: Vec<InstalledTheme>,
    pub effects: Vec<InstalledEffects>,
    /// A theme shown without installing it. Its file is deleted as soon as it's loaded, so
    /// nothing stays on disk; ending the preview forgets it entirely.
    pub preview: Option<(InstalledTheme, ThemePack)>,
    preview_pending: Option<ThemePack>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl PackManager {
    pub fn new(root: PathBuf, ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        let mut m = Self {
            root,
            ctx,
            index: None,
            index_state: IndexState::Idle,
            jobs: HashMap::new(),
            themes: Vec::new(),
            effects: Vec::new(),
            preview: None,
            preview_pending: None,
            tx,
            rx,
        };
        m.scan_themes();
        m.scan_effects();
        m
    }

    pub fn voice_runtime_dir(&self) -> PathBuf {
        self.root.join(KEY_RUNTIME)
    }

    pub fn voice_model_dir(&self, id: &str) -> PathBuf {
        self.root.join(format!("voice-{id}"))
    }

    fn preview_dir(&self) -> PathBuf {
        self.root.join(KEY_PREVIEW)
    }

    /// ffmpeg.exe from the hover-preview add-on, if it is installed.
    pub fn preview_ffmpeg(&self) -> Option<PathBuf> {
        let exe = self.preview_dir().join("ffmpeg.exe");
        exe.is_file().then_some(exe)
    }

    pub fn install_preview(&mut self) {
        let Some(file) = self.index.as_ref().and_then(|i| i.preview.clone()) else {
            return;
        };
        let (root, dest) = (self.root.clone(), self.preview_dir());
        self.spawn_job(KEY_PREVIEW, move |p| {
            install_zip(
                &root,
                "preview-ffmpeg.zip",
                &file.url,
                &file.sha256,
                &dest,
                &p,
                |d| d.join("ffmpeg.exe").is_file(),
            )
        });
    }

    pub fn remove_preview(&mut self) {
        let _ = fs::remove_dir_all(self.preview_dir());
    }

    pub fn whisper_dir(&self) -> PathBuf {
        self.root.join(KEY_WHISPER)
    }

    pub fn whisper_installed(&self) -> bool {
        crate::whisper::is_installed(&self.whisper_dir())
    }

    pub fn install_whisper(&mut self) {
        let Some(file) = self.index.as_ref().and_then(|i| i.whisper.clone()) else {
            return;
        };
        let (root, dest) = (self.root.clone(), self.whisper_dir());
        self.spawn_job(KEY_WHISPER, move |p| {
            install_zip(
                &root,
                "voice-whisper.zip",
                &file.url,
                &file.sha256,
                &dest,
                &p,
                crate::whisper::is_installed,
            )
        });
    }

    pub fn remove_whisper(&mut self) {
        let _ = fs::remove_dir_all(self.whisper_dir());
    }

    pub fn runtime_installed(&self) -> bool {
        self.voice_runtime_dir().join("libvosk.dll").is_file()
    }

    pub fn model_installed(&self, id: &str) -> bool {
        is_voice_model(&self.voice_model_dir(id))
    }

    fn themes_dir(&self) -> PathBuf {
        self.root.join("themes")
    }

    fn effects_dir(&self) -> PathBuf {
        self.root.join("effects")
    }

    pub fn effects_installed(&self) -> bool {
        !self.effects.is_empty()
    }

    /// Every celebration unlocked by the downloaded packs, in pack order.
    pub fn available_effects(&self) -> Vec<EffectStyle> {
        let mut out = Vec::new();
        for pack in &self.effects {
            for style in &pack.styles {
                if !out.contains(style) {
                    out.push(*style);
                }
            }
        }
        out
    }

    pub fn request_index(&mut self) {
        if self.index_state == IndexState::Loading {
            return;
        }
        self.index_state = IndexState::Loading;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let repo = repo();
        std::thread::spawn(move || {
            let result = fetch_text(&index_url(&repo)).and_then(|t| PackIndex::from_toml(&t));
            let _ = tx.send(Msg::Index(result));
            ctx.request_repaint();
        });
    }

    fn spawn_job(
        &mut self,
        key: &str,
        job: impl FnOnce(Arc<Progress>) -> Result<(), String> + Send + 'static,
    ) {
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
        let Some(file) = self.index.as_ref().and_then(|i| i.voice_runtime.clone()) else {
            return;
        };
        let (root, dest) = (self.root.clone(), self.voice_runtime_dir());
        self.spawn_job(KEY_RUNTIME, move |p| {
            install_zip(
                &root,
                "voice-runtime.zip",
                &file.url,
                &file.sha256,
                &dest,
                &p,
                |d| d.join("libvosk.dll").is_file(),
            )
        });
    }

    pub fn install_voice(&mut self, pack: &VoicePack) {
        let (root, dest, file) = (
            self.root.clone(),
            self.voice_model_dir(&pack.id),
            pack.file.clone(),
        );
        let key = format!("voice-{}", pack.id);
        let zip_name = format!("{key}.zip");
        self.spawn_job(&key, move |p| {
            install_zip(&root, &zip_name, &file.url, &file.sha256, &dest, &p, |d| {
                is_voice_model(d)
            })
        });
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

    fn previews_dir(&self) -> PathBuf {
        self.root.join(".theme-previews")
    }

    /// Download a theme just to look at it. It is applied without being installed, and the
    /// downloaded file is removed again right after it has been read.
    pub fn preview_theme(&mut self, pack: &ThemePack) {
        let (dir, file, id) = (self.previews_dir(), pack.file.clone(), pack.id.clone());
        self.preview_pending = Some(pack.clone());
        self.spawn_job(&format!("preview-theme-{id}"), move |p| {
            let _ = fs::create_dir_all(&dir);
            let tmp = dir.join(format!("{id}.toml"));
            download_verified(&file.url, &tmp, &file.sha256, &p)
        });
    }

    /// Stop previewing and forget the theme.
    pub fn end_preview(&mut self) {
        self.preview = None;
        self.preview_pending = None;
        let _ = fs::remove_dir_all(self.previews_dir());
    }

    /// Turn a downloaded preview file into an in-memory theme and delete the file.
    fn load_preview(&mut self, id: &str) -> Result<String, String> {
        let path = self.previews_dir().join(format!("{id}.toml"));
        let text = fs::read_to_string(&path).map_err(|e| e.to_string());
        let _ = fs::remove_dir_all(self.previews_dir());
        let file = ThemeFile::from_toml(&text?).map_err(|e| e.to_string())?;
        let palette = file.to_palette().map_err(|e| e.to_string())?;
        let pack = self
            .preview_pending
            .take()
            .filter(|p| p.id == id)
            .ok_or("the preview was cancelled")?;
        let name = file.name.clone();
        self.preview = Some((
            InstalledTheme {
                id: format!("{PREVIEW_PREFIX}{id}"),
                name: file.name,
                author: file.author,
                palette,
                layout: file.layout,
                // Preview downloads are single files; a background image can't come along.
                background_path: None,
            },
            pack,
        ));
        Ok(name)
    }

    fn find_theme(&self, id: &str) -> Option<&InstalledTheme> {
        if let Some((t, _)) = &self.preview {
            if t.id == id {
                return Some(t);
            }
        }
        self.themes.iter().find(|t| t.id == id)
    }

    pub fn remove_theme(&mut self, id: &str) {
        let _ = fs::remove_file(self.themes_dir().join(format!("{id}.toml")));
        self.scan_themes();
    }

    pub fn install_effects(&mut self, pack: &EffectPack) {
        let (dir, file, id) = (self.effects_dir(), pack.file.clone(), pack.id.clone());
        self.spawn_job(&format!("effects-{id}"), move |p| {
            let tmp = dir.join(format!("{id}.toml.download"));
            download_verified(&file.url, &tmp, &file.sha256, &p)?;
            let text = fs::read_to_string(&tmp).map_err(|e| e.to_string())?;
            let file = match EffectFile::from_toml(&text) {
                Ok(file) => file,
                Err(e) => {
                    let _ = fs::remove_file(&tmp);
                    return Err(format!("the effect pack isn't usable: {e}"));
                }
            };
            if file.known_styles().is_empty() {
                let _ = fs::remove_file(&tmp);
                return Err("the effect pack doesn't list any effects".into());
            }
            fs::rename(&tmp, dir.join(format!("{id}.toml"))).map_err(|e| e.to_string())
        });
    }

    pub fn remove_effects(&mut self, id: &str) {
        let _ = fs::remove_file(self.effects_dir().join(format!("{id}.toml")));
        self.scan_effects();
    }

    pub fn scan_effects(&mut self) {
        self.effects.clear();
        let Ok(rd) = fs::read_dir(self.effects_dir()) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(id) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if !rewind_core::packs::valid_id(&id) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(file) = EffectFile::from_toml(&text) else {
                continue;
            };
            let styles = file.known_styles();
            if styles.is_empty() {
                continue;
            }
            let index = self.index.as_ref().and_then(|i| {
                i.effects
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| (e.name.clone(), e.author.clone()))
            });
            self.effects.push(InstalledEffects {
                id: id.clone(),
                name: index
                    .as_ref()
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| "Celebration effects".into()),
                author: index.map(|(_, a)| a).unwrap_or_default(),
                styles,
            });
        }
        self.effects.sort_by_key(|e| e.name.to_lowercase());
    }

    pub fn scan_themes(&mut self) {
        self.themes.clear();
        let Ok(rd) = fs::read_dir(self.themes_dir()) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(id) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if !rewind_core::packs::valid_id(&id) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(file) = ThemeFile::from_toml(&text) else {
                continue;
            };
            let Ok(palette) = file.to_palette() else {
                continue;
            };
            // A background image is resolved relative to the theme file. It is optional, and
            // a missing file simply leaves the theme without one.
            let background_path = {
                let rel = file.layout.background.trim();
                if rel.is_empty() {
                    None
                } else {
                    let p = path.parent().unwrap_or(Path::new(".")).join(rel);
                    p.is_file().then_some(p)
                }
            };
            self.themes.push(InstalledTheme {
                id,
                name: file.name,
                author: file.author,
                palette,
                layout: file.layout,
                background_path,
            });
        }
        self.themes.sort_by_key(|t| t.name.to_lowercase());
    }

    /// Palette for a theme id; falls back to the built-in default if it's gone or broken.
    pub fn palette_for(&self, id: &str) -> Palette {
        self.find_theme(id)
            .map(|t| t.palette)
            .unwrap_or_else(Palette::default_dark)
    }

    /// Layout overrides for a theme id, or the built-in layout.
    pub fn layout_for(&self, id: &str) -> ThemeLayout {
        self.find_theme(id)
            .map(|t| t.layout.clone())
            .unwrap_or_default()
    }

    /// Background image shipped with a theme, if any.
    pub fn background_for(&self, id: &str) -> Option<PathBuf> {
        self.find_theme(id).and_then(|t| t.background_path.clone())
    }

    pub fn poll(&mut self) -> Vec<PackEvent> {
        let mut events = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Index(Ok(idx)) => {
                    self.index = Some(idx);
                    self.index_state = IndexState::Ready;
                    // Installed packs were scanned before the index arrived; re-read them
                    // now so their display names and authors come from packs.toml.
                    self.scan_effects();
                }
                Msg::Index(Err(e)) => self.index_state = IndexState::Failed(e),
                Msg::Done { key, result } => {
                    self.jobs.remove(&key);
                    match result {
                        Ok(()) => {
                            if let Some(id) = key.strip_prefix("preview-theme-") {
                                match self.load_preview(id) {
                                    Ok(name) => events.push(PackEvent::Previewing(name)),
                                    Err(e) => events.push(PackEvent::Failed(key.clone(), e)),
                                }
                                continue;
                            }
                            if key.starts_with("theme-") {
                                self.scan_themes();
                            }
                            if key.starts_with("effects-") {
                                self.scan_effects();
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
fn install_zip(
    root: &Path,
    zip_name: &str,
    url: &str,
    sha256: &str,
    dest: &Path,
    progress: &Arc<Progress>,
    valid: impl Fn(&Path) -> bool,
) -> Result<(), String> {
    let zip_path = root.join(".downloads").join(zip_name);
    download_verified(url, &zip_path, sha256, progress)?;
    let staging = dest.with_extension("new");
    let _ = fs::remove_dir_all(&staging);
    let result = unzip(&zip_path, &staging).and_then(|_| {
        if valid(&staging) {
            Ok(())
        } else {
            Err("the download doesn't contain the expected files".to_string())
        }
    });
    let _ = fs::remove_file(&zip_path);
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let _ = fs::remove_dir_all(dest);
    fs::rename(&staging, dest).map_err(|e| format!("installing: {e}"))
}
