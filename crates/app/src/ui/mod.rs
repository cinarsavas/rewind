mod effects;
pub(crate) mod i18n;
mod theme;
mod views;
mod widgets;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use rewind_core::config::{Config, EffectStyle, Preset, VoiceConfig};
use rewind_core::library::{self, human_size, Clip};
use rewind_core::toast::{ToastKind, ToastSpec};
use rewind_core::trim::{self, TrimJob};

use crate::engine::{self, Engine, MonitorInfo};
use crate::hotkeys::{Action, Hotkeys};
use crate::overlay::Overlay;
use crate::packs::{PackEvent, PackManager};
use crate::perf::Sampler;
use crate::platform;
use crate::runtime;
use crate::thumbs::Thumbs;
use crate::tray::{Tray, TrayAction};
use crate::voice::{self, VoiceHandle, VoiceStatus};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Replay,
    Clips,
    Settings,
    Addons,
    About,
}

impl Page {
    /// Nav order, used to pick the direction of the page transition.
    pub fn rank(self) -> u8 {
        match self {
            Page::Replay => 0,
            Page::Clips => 1,
            Page::Settings => 2,
            Page::Addons => 3,
            Page::About => 4,
        }
    }
}

pub enum Phase {
    Installing,
    Starting { frames: u8 },
    Ready,
    Failed(String),
}

pub enum Msg {
    Installed(Result<(), String>),
    Saved(Result<PathBuf, String>),
    Trimmed(Result<PathBuf, String>),
    Scanned(Vec<Clip>),
    Update(Result<Option<crate::updates::UpdateInfo>, String>),
    /// The update is downloaded and the installer is waiting for Rewind to quit.
    UpdateReady(Result<(), String>),
    /// A smaller copy for sharing is ready (or failed).
    Shared(Result<PathBuf, String>),
}

/// What the About page knows about newer releases.
#[derive(Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(crate::updates::UpdateInfo),
    /// Downloading and verifying the new version; progress is shared with the worker.
    Installing(
        crate::updates::UpdateInfo,
        std::sync::Arc<crate::download::Progress>,
    ),
    /// Installed copy is about to restart into the new version.
    Restarting(String),
    /// Installing failed; the release page is still offered.
    InstallFailed(crate::updates::UpdateInfo, String),
    Failed(String),
}

/// How long an in-app toast stays up, including its fade-out.
pub const TOAST_TOTAL: f32 = 5.35;

/// Clip rows fade in one after another. This is the total time the stagger can take.
pub const ROW_REVEAL_SPAN: f32 = 0.75;

/// Reveal progress for row `index`, or 1.0 when animation is off. Capped at 12 rows so a
/// long list doesn't keep animating for seconds.
pub fn row_reveal_at(shown: Instant, index: usize, animate: bool) -> f32 {
    if !animate {
        return 1.0;
    }
    let delay = index.min(12) as f32 * 0.032;
    let t = ((shown.elapsed().as_secs_f32() - delay) / 0.34).clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub struct Toast {
    pub text: String,
    pub is_error: bool,
    pub at: Instant,
}

/// A clip plus the display strings for it. Rebuilt at most every few seconds, never per frame.
pub struct ClipRow {
    pub clip: Clip,
    pub meta: String,
}

fn clip_meta(now: SystemTime, clip: &Clip) -> String {
    let group = if clip.group.is_empty() {
        String::new()
    } else {
        format!("{} · ", clip.group)
    };
    format!(
        "{group}{} · {}",
        human_size(clip.size_bytes),
        i18n::age(now, clip.modified)
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrimMode {
    LastSeconds,
    Range,
}

pub struct TrimState {
    pub clip: Clip,
    pub mode: TrimMode,
    pub last_secs: f64,
    pub start: f64,
    pub end: f64,
    pub running: bool,
}

/// The rename dialog's working state. `stem` is the file name without its extension, so
/// typing a new name never accidentally drops the `.mp4`.
pub struct RenameState {
    pub clip: Clip,
    pub stem: String,
    pub error: Option<String>,
    /// Focus the text field on the first frame, then leave the keyboard alone.
    pub focus: bool,
}

pub struct RewindApp {
    pub cfg: Config,
    pub draft: Config,
    cfg_path: PathBuf,
    pub phase: Phase,
    pub engine: Option<Engine>,
    hotkeys: Option<Hotkeys>,
    tray: Option<Tray>,
    overlay: Overlay,
    pub hotkey_errors: Vec<String>,
    /// Which hotkey field is waiting for a key press (0 = save, 1 = toggle).
    pub capturing_hotkey: Option<usize>,
    pub hotkey_capture_error: Option<String>,
    pub page: Page,
    pub toast: Option<Toast>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub clips: Vec<Clip>,
    pub clips_total_bytes: u64,
    /// Cached display rows and the filtered index into them, so drawing never clones the library.
    pub clip_rows: Vec<ClipRow>,
    pub filter_idx: Vec<usize>,
    pub clip_filter: String,
    /// Clips page shows only starred clips.
    pub favorites_only: bool,
    clips_rev: u64,
    clip_rows_rev: u64,
    clip_rows_bucket: u64,
    filter_rev: u64,
    filter_dirty: bool,
    clips_scanned: Option<Instant>,
    scan_tx: Option<Sender<PathBuf>>,
    scan_inflight: bool,
    scan_again: bool,
    pub monitors: Vec<MonitorInfo>,
    pub saving: bool,
    pub last_saved: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    /// ffmpeg.exe from the hover-preview add-on, checked at start and after installs.
    preview_exe: Option<PathBuf>,
    pub trim: Option<TrimState>,
    pub rename: Option<RenameState>,
    pub confirm_delete: Option<Clip>,
    pub buffer_started_at: Option<Instant>,
    /// Cached replay-buffer state. Asking OBS is a round trip to its actor thread, and the
    /// UI needs the answer several times per frame, so it is refreshed at most once a second
    /// (and immediately whenever Rewind itself starts, stops or rebuilds the buffer).
    buffer_on: bool,
    buffer_checked: Option<Instant>,
    /// Whether the voice runtime and the current language's model are on disk, re-checked
    /// every few seconds instead of hitting the filesystem every frame.
    voice_files: Option<(String, bool, bool, Instant)>,
    /// Whether the running listener was started with the Whisper check.
    voice_running_whisper: bool,
    quitting: bool,
    window_hidden: bool,
    focused: bool,
    tray_hint_shown: bool,
    ctx: egui::Context,
    pub perf: Sampler,
    pub thumbs: Thumbs,
    pub previews: crate::preview::Previews,
    pub packs: PackManager,
    voice: Option<VoiceHandle>,
    voice_running_cfg: Option<VoiceConfig>,
    /// When an errored listener should be tried again.
    voice_retry_at: Option<Instant>,
    pub voice_error: Option<String>,
    /// While set and in the future, the voice test panel is open (live meter, last heard).
    pub voice_test_until: Option<Instant>,
    page_changed: Instant,
    page_dir: f32,
    applied_theme: String,
    /// Eased scroll position for the main content area.
    pub scroll_pos: f32,
    /// Where the eased scroll is heading.
    pub scroll_target: f32,
    /// Last known maximum scroll offset, measured after the content is laid out.
    pub scroll_max: f32,
    /// When the current clip list appeared; drives the staggered row reveal.
    pub list_shown: Instant,
    /// Height of one clip row including spacing, measured while drawing. Lets the Clips
    /// page skip rows that are scrolled out of view.
    pub clip_row_h: f32,
    /// Live celebrations (toggle effects), painted above the UI.
    fx: Vec<effects::FxEvent>,
    fx_seed: u32,
    /// Audio endpoints for the settings dropdowns; refreshed on demand.
    pub output_devices: Vec<crate::audio::AudioDevice>,
    pub input_devices: Vec<crate::audio::AudioDevice>,
    pub update_state: UpdateState,
    update_auto_checked: bool,
    /// Name of the clip a small copy is being made of, while it runs.
    pub sharing: Option<String>,
    /// The last few error messages, newest last, for "Copy diagnostics".
    pub recent_errors: std::collections::VecDeque<(SystemTime, String)>,
}

impl RewindApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start_hidden: bool) -> Self {
        let cfg_path =
            Config::default_path().unwrap_or_else(|| PathBuf::from("rewind-config.toml"));
        let first_run = !cfg_path.exists();
        let mut cfg = Config::load_from(&cfg_path);
        if first_run {
            // No config yet: pick a light preset for clearly older machines instead of
            // starting at 1080p60 and letting the first recording stutter. The friend's
            // Arrandale i5 reports 4 logical cores, so it lands on Low-end.
            let cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            if cores <= 4 {
                cfg.apply_preset(Preset::LowEnd);
            }
            // Persist the detected preset so later launches don't re-detect.
            let _ = cfg.save_to(&cfg_path);
        }
        let packs = PackManager::new(directories_dir().join("packs"), cc.egui_ctx.clone());
        // Effects are a downloadable pack now, so a config that still names one must fall
        // back to Off until the pack is installed.
        if cfg.effects != EffectStyle::None && !packs.available_effects().contains(&cfg.effects) {
            cfg.effects = EffectStyle::None;
        }
        i18n::set_lang(cfg.language);
        theme::install_fonts(&cc.egui_ctx);
        theme::set_animations(&cc.egui_ctx, cfg.animations);
        theme::set_extra_motion(cfg.extra_motion && !cfg.low_power_ui);
        theme::set_palette(&packs.palette_for(&cfg.theme));
        theme::set_layout(theme::resolve_layout(
            &packs.layout_for(&cfg.theme),
            packs.background_for(&cfg.theme).as_deref(),
            &cc.egui_ctx,
        ));
        theme::apply_visuals(&cc.egui_ctx);
        let (tx, rx) = channel();
        // One long-lived worker scans the clips folder off the UI thread on request.
        let (scan_tx, scan_rx) = channel::<PathBuf>();
        let scan_tx = {
            let (msgs, ctx) = (tx.clone(), cc.egui_ctx.clone());
            std::thread::Builder::new()
                .name("rewind-scan".into())
                .spawn(move || {
                    while let Ok(dir) = scan_rx.recv() {
                        let clips = library::scan(&dir);
                        if msgs.send(Msg::Scanned(clips)).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                })
                .ok();
            scan_tx
        };
        let phase = if runtime::is_installed() {
            Phase::Starting { frames: 0 }
        } else {
            let tx2 = tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let _ = tx2.send(Msg::Installed(runtime::install_blocking()));
                ctx.request_repaint();
            });
            Phase::Installing
        };
        let app_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let hotkeys = Hotkeys::new(cc.egui_ctx.clone()).ok();
        let tray = Tray::new(cc.egui_ctx.clone(), cfg.start_buffer_on_launch)
            .map_err(|e| log::warn!("{e}"))
            .ok();
        let hide_now = (start_hidden || cfg.start_minimized) && tray.is_some();
        let cfg_theme = cfg.theme.clone();
        if hide_now {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        let just_updated = std::env::args().any(|a| a == "--updated");
        let packs_ffmpeg = packs.preview_ffmpeg();
        Self {
            draft: cfg.clone(),
            cfg,
            cfg_path,
            phase,
            engine: None,
            hotkeys,
            tray,
            overlay: Overlay::spawn(),
            hotkey_errors: Vec::new(),
            capturing_hotkey: None,
            hotkey_capture_error: None,
            page: Page::Replay,
            toast: just_updated.then(|| Toast {
                text: i18n::tf(
                    "Rewind updated to version {v}",
                    &[("v", env!("CARGO_PKG_VERSION"))],
                ),
                is_error: false,
                at: Instant::now(),
            }),
            tx,
            rx,
            clips: Vec::new(),
            clips_total_bytes: 0,
            clip_rows: Vec::new(),
            filter_idx: Vec::new(),
            clip_filter: String::new(),
            favorites_only: false,
            clips_rev: 0,
            clip_rows_rev: u64::MAX,
            clip_rows_bucket: u64::MAX,
            filter_rev: u64::MAX,
            filter_dirty: true,
            clips_scanned: None,
            scan_tx: Some(scan_tx),
            scan_inflight: false,
            scan_again: false,
            monitors: engine::list_monitors(),
            saving: false,
            last_saved: None,
            ffmpeg: trim::find_ffmpeg(app_dir.as_deref()).or_else(|| packs_ffmpeg.clone()),
            preview_exe: packs_ffmpeg,
            trim: None,
            rename: None,
            confirm_delete: None,
            buffer_started_at: None,
            buffer_on: false,
            buffer_checked: None,
            voice_files: None,
            voice_running_whisper: false,
            quitting: false,
            window_hidden: hide_now,
            focused: !hide_now,
            tray_hint_shown: false,
            ctx: cc.egui_ctx.clone(),
            perf: Sampler::new(),
            thumbs: Thumbs::new(cc.egui_ctx.clone()),
            previews: crate::preview::Previews::new(
                directories_dir().join("previews"),
                cc.egui_ctx.clone(),
            ),
            packs,
            voice: None,
            voice_running_cfg: None,
            voice_retry_at: None,
            voice_error: None,
            voice_test_until: None,
            page_changed: Instant::now(),
            page_dir: 1.0,
            applied_theme: cfg_theme,
            scroll_pos: 0.0,
            scroll_target: 0.0,
            scroll_max: 0.0,
            list_shown: Instant::now(),
            clip_row_h: 0.0,
            fx: Vec::new(),
            fx_seed: 0x9E37_79B9,
            output_devices: crate::audio::output_devices(),
            input_devices: crate::audio::input_devices(),
            update_state: UpdateState::Idle,
            update_auto_checked: false,
            recent_errors: std::collections::VecDeque::new(),
            sharing: None,
        }
    }

    /// ffmpeg for hover previews, when the add-on is installed and previews are switched on.
    pub fn preview_ffmpeg(&self) -> Option<PathBuf> {
        if !self.cfg.hover_preview || !self.cfg.thumbnails {
            return None;
        }
        self.preview_exe.clone()
    }

    /// Re-check the hover-preview add-on; it also gives trimming an ffmpeg if none was found.
    pub fn refresh_preview_exe(&mut self) {
        self.preview_exe = self.packs.preview_ffmpeg();
        if self.ffmpeg.is_none() {
            self.ffmpeg = self.preview_exe.clone();
        } else if self.ffmpeg.as_ref().map(|p| !p.is_file()).unwrap_or(false) {
            // The add-on was removed: fall back to whatever else is on the system.
            let app_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            self.ffmpeg = trim::find_ffmpeg(app_dir.as_deref()).or(self.preview_exe.clone());
        }
    }

    /// Add sounds dropped onto the window. Other files are ignored.
    fn take_dropped_sounds(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| crate::sounds::accepts(p))
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        let limit = self.cfg.limit_custom_sounds.then_some(crate::sounds::LIMIT);
        for path in dropped {
            match crate::sounds::import(&sounds_dir(), &path, limit) {
                Ok(name) => {
                    self.notify(i18n::tf("Added sound: {name}", &[("name", &name)]), false);
                    // A first custom sound becomes the one that plays, without needing Apply.
                    if self.cfg.clip_sound.is_empty() {
                        self.cfg.clip_sound = name.clone();
                        self.draft.clip_sound = name;
                        self.save_quietly();
                    }
                }
                Err(e) => self.notify(i18n::tf("Couldn't add the sound: {e}", &[("e", &e)]), true),
            }
        }
    }

    /// Remove a custom sound; if it was the active one, go back to the chime.
    pub fn remove_sound(&mut self, name: &str) {
        crate::sounds::remove(&sounds_dir(), name);
        if self.cfg.clip_sound == name {
            self.cfg.clip_sound.clear();
            self.save_quietly();
        }
        if self.draft.clip_sound == name {
            self.draft.clip_sound.clear();
        }
    }

    /// Persist a setting changed outside the Apply flow (like the Clips page zoom).
    pub fn save_quietly(&self) {
        let _ = self.cfg.save_to(&self.cfg_path);
    }

    /// Re-read the Windows audio endpoints. Cheap enough to call from a button.
    pub fn refresh_audio_devices(&mut self) {
        self.output_devices = crate::audio::output_devices();
        self.input_devices = crate::audio::input_devices();
    }

    /// Switch to the theme in the draft: colours cross-fade, layout and background swap at
    /// once. Called when the theme picker changes, so previews are immediate.
    /// The theme on screen: a preview while one is shown, otherwise the chosen theme.
    pub fn effective_theme(&self) -> String {
        self.packs
            .preview
            .as_ref()
            .map(|(t, _)| t.id.clone())
            .unwrap_or_else(|| self.draft.theme.clone())
    }

    /// Stop showing a previewed theme; nothing of it stays installed.
    pub fn end_theme_preview(&mut self) {
        if self.packs.preview.is_some() {
            self.packs.end_preview();
        }
    }

    /// Install the previewed theme for real and switch to it.
    pub fn keep_theme_preview(&mut self) {
        if let Some((_, pack)) = self.packs.preview.clone() {
            self.packs.install_theme(&pack);
            self.draft.theme = pack.id.clone();
            self.packs.end_preview();
        }
    }

    pub fn apply_theme(&mut self, ctx: &egui::Context) {
        let id = self.effective_theme();
        let palette = self.packs.palette_for(&id);
        let layout = theme::resolve_layout(
            &self.packs.layout_for(&id),
            self.packs.background_for(&id).as_deref(),
            ctx,
        );
        theme::fade_to(ctx, &palette);
        theme::set_layout(layout);
        theme::apply_visuals(ctx);
        self.applied_theme = id;
    }

    /// Start a GitHub release check in the background.
    pub fn check_updates(&mut self) {
        if matches!(
            self.update_state,
            UpdateState::Checking | UpdateState::Installing(..) | UpdateState::Restarting(_)
        ) {
            return;
        }
        self.update_state = UpdateState::Checking;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let repo = crate::packs::repo();
        std::thread::Builder::new()
            .name("rewind-update".into())
            .spawn(move || {
                let result = crate::updates::check(&repo);
                let _ = tx.send(Msg::Update(result));
                ctx.request_repaint();
            })
            .ok();
    }

    /// Download, verify and install `info`, then quit so the installer can replace the app.
    pub fn install_update(&mut self, info: crate::updates::UpdateInfo) {
        if matches!(
            self.update_state,
            UpdateState::Installing(..) | UpdateState::Restarting(_)
        ) {
            return;
        }
        if !crate::updates::can_install(&info) {
            // Nothing installable in this release: fall back to the page.
            self.update_state = UpdateState::Available(info);
            return;
        }
        let progress = std::sync::Arc::new(crate::download::Progress::default());
        self.update_state = UpdateState::Installing(info.clone(), progress.clone());
        self.show_overlay(
            ToastKind::Info,
            "Updating Rewind",
            &i18n::tf("Downloading version {v}…", &[("v", &info.version)]),
            false,
        );
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("rewind-update-install".into())
            .spawn(move || {
                let result = crate::updates::prepare(&info, &progress);
                let _ = tx.send(Msg::UpdateReady(result));
                ctx.request_repaint();
            })
            .ok();
    }

    /// Switch page, restarting the page-enter transition.
    pub fn set_page(&mut self, page: Page) {
        if self.page != page {
            self.page_dir = if page.rank() >= self.page.rank() {
                1.0
            } else {
                -1.0
            };
            self.page = page;
            self.page_changed = Instant::now();
            // Start each page at the top, without the old scroll position leaking across.
            self.scroll_pos = 0.0;
            self.scroll_target = 0.0;
            self.scroll_max = 0.0;
            self.list_shown = Instant::now();
            // A theme preview only lives while you're looking at it on the Add-ons page.
            if self.page != Page::Addons {
                self.end_theme_preview();
            }
            if page == Page::Addons {
                self.packs.scan_themes();
                if self.packs.index.is_none() {
                    self.packs.request_index();
                }
            }
        }
    }

    /// 0..=1 progress of the page-enter animation (always 1 when animations are off).
    pub fn page_anim(&self) -> f32 {
        if !theme::animations_on() || self.cfg.low_power_ui {
            return 1.0;
        }
        let t = (self.page_changed.elapsed().as_secs_f32() / 0.30).clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    /// True while the eased scroll is still gliding toward its target.
    pub fn scroll_animating(&self) -> bool {
        (self.scroll_target - self.scroll_pos).abs() > 0.12
    }

    /// True while clip rows are still fading/sliding into place.
    pub fn list_animating(&self) -> bool {
        theme::animations_on()
            && !self.cfg.low_power_ui
            && self.list_shown.elapsed().as_secs_f32() < ROW_REVEAL_SPAN
    }

    /// 0..=1 reveal progress for clip row `index`, staggered down the list.
    pub fn row_reveal(&self, index: usize) -> f32 {
        row_reveal_at(
            self.list_shown,
            index,
            theme::animations_on() && !self.cfg.low_power_ui,
        )
    }

    /// Which way the current page transition slides: 1 = entered from the right.
    pub fn page_dir(&self) -> f32 {
        self.page_dir
    }

    /// Opacity of the in-app toast, 0..=1. Drives both the fade and the repaint cadence.
    pub fn toast_alpha(&self) -> f32 {
        match &self.toast {
            Some(t) => rewind_core::toast::alpha_at(t.at.elapsed().as_secs_f32(), TOAST_TOTAL),
            None => 1.0,
        }
    }

    /// Move celebrations queued during the last frame into the live list. Ignored entirely
    /// while animations are off or the low-power UI is on, so slow PCs never pay for them.
    fn absorb_fx(&mut self, ctx: &egui::Context) {
        let enabled = self.draft.animations && !self.draft.low_power_ui;
        let queued = effects::take_queued();
        if !enabled {
            // Settings turned effects off mid-flight: stop painting them immediately.
            self.fx.clear();
            return;
        }
        for (style, at) in queued {
            if style == EffectStyle::None {
                continue;
            }
            // Keep the celebration fully visible even for a switch near the window edge.
            let at = effects::clamp_to_screen(ctx, at);
            if self.fx.len() >= 8 {
                self.fx.remove(0);
            }
            self.fx_seed = self
                .fx_seed
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            self.fx.push(effects::FxEvent::new(style, at, self.fx_seed));
        }
        self.fx.retain(|e| !e.expired());
    }

    /// Refresh the cached clip rows (and their display strings) when the library or the
    /// relative-age bucket changed. Cheap to call every frame; does nothing most of the time.
    pub fn sync_clip_rows(&mut self) {
        let bucket = now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() / 5)
            .unwrap_or(0);
        if self.clip_rows_rev == self.clips_rev && self.clip_rows_bucket == bucket {
            return;
        }
        let now = now();
        if self.clip_rows_rev == self.clips_rev && self.clip_rows.len() == self.clips.len() {
            // Same library, only the "5 min ago" strings aged: refresh just the text.
            for (row, clip) in self.clip_rows.iter_mut().zip(self.clips.iter()) {
                row.meta = clip_meta(now, clip);
            }
        } else {
            self.clip_rows = self
                .clips
                .iter()
                .map(|c| ClipRow {
                    meta: clip_meta(now, c),
                    clip: c.clone(),
                })
                .collect();
            self.filter_dirty = true;
        }
        self.clip_rows_rev = self.clips_rev;
        self.clip_rows_bucket = bucket;
    }

    /// Re-filter the cached rows. Only runs when the search box or the library changed.
    pub fn sync_filter(&mut self) {
        if !self.filter_dirty && self.filter_rev == self.clips_rev {
            return;
        }
        let needle = self.clip_filter.trim().to_lowercase();
        let favorites: Option<std::collections::HashSet<&str>> = self
            .favorites_only
            .then(|| self.cfg.favorites.iter().map(String::as_str).collect());
        let root = self.cfg.output_dir.clone();
        self.filter_idx = self
            .clip_rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                (needle.is_empty()
                    || r.clip.name.to_lowercase().contains(&needle)
                    || r.clip.group.to_lowercase().contains(&needle))
                    && favorites
                        .as_ref()
                        .map(|f| f.contains(library::relative_key(&root, &r.clip.path).as_str()))
                        .unwrap_or(true)
            })
            .map(|(i, _)| i)
            .collect();
        self.filter_dirty = false;
        self.filter_rev = self.clips_rev;
    }

    /// Called by the search box so filtering re-runs only on real edits.
    pub fn mark_filter_dirty(&mut self) {
        self.filter_dirty = true;
        self.list_shown = Instant::now();
    }

    /// True when the voice runtime and the configured model are installed. Cached for a few
    /// seconds; [`Self::invalidate_voice_files`] forces a fresh look after installs.
    fn voice_files_ready(&mut self) -> bool {
        self.voice_files_cached().0
    }

    /// (voice runtime + model installed, Whisper add-on installed), cached for 3 seconds.
    fn voice_files_cached(&mut self) -> (bool, bool) {
        let lang = &self.cfg.voice.language;
        if let Some((cached_lang, ok, whisper, at)) = &self.voice_files {
            if cached_lang == lang && at.elapsed() < Duration::from_secs(3) {
                return (*ok, *whisper);
            }
        }
        let ok = self.packs.runtime_installed() && self.packs.model_installed(lang);
        let whisper = self.packs.whisper_installed();
        self.voice_files = Some((lang.clone(), ok, whisper, Instant::now()));
        (ok, whisper)
    }

    fn invalidate_voice_files(&mut self) {
        self.voice_files = None;
    }

    /// Start, restart or stop the voice listener so it matches the settings and what's installed.
    pub fn sync_voice(&mut self) {
        let want = self.cfg.voice.enabled && self.engine.is_some() && self.voice_files_ready();
        if !want {
            self.voice = None;
            self.voice_running_cfg = None;
            self.voice_retry_at = None;
            return;
        }
        // A listener that stopped with an error (mic unplugged, device busy) is retried after
        // a short pause instead of staying dead until the next Apply.
        let errored = self
            .voice
            .as_ref()
            .map(|v| matches!(v.shared.status(), VoiceStatus::Error(_)))
            .unwrap_or(false);
        if errored {
            self.voice = None;
            self.voice_running_cfg = None;
            let retry_at = *self
                .voice_retry_at
                .get_or_insert_with(|| Instant::now() + std::time::Duration::from_secs(5));
            if Instant::now() < retry_at {
                self.ctx
                    .request_repaint_after(std::time::Duration::from_millis(500));
                return;
            }
            self.voice_retry_at = None;
        }
        let whisper = self.cfg.voice.use_whisper && self.voice_files_cached().1;
        if self.voice.is_some()
            && self.voice_running_cfg.as_ref() == Some(&self.cfg.voice)
            && self.voice_running_whisper == whisper
        {
            return;
        }
        self.voice = None;
        self.voice_error = None;
        let handle = voice::start(
            &self.cfg.voice,
            self.packs.voice_runtime_dir(),
            self.packs.voice_model_dir(&self.cfg.voice.language),
            whisper.then(|| self.packs.whisper_dir()),
            self.cfg.audio.mic_device.clone(),
            self.ctx.clone(),
        );
        self.voice_running_whisper = whisper;
        self.voice = Some(handle);
        self.voice_running_cfg = Some(self.cfg.voice.clone());
        self.voice_retry_at = None;
    }

    pub fn voice_handle(&self) -> Option<&VoiceHandle> {
        self.voice.as_ref()
    }

    /// In-app toast only.
    pub fn notify(&mut self, text: impl Into<String>, is_error: bool) {
        let text = i18n::tp(&text.into());
        if is_error {
            if self.recent_errors.len() >= 20 {
                self.recent_errors.pop_front();
            }
            self.recent_errors
                .push_back((SystemTime::now(), text.clone()));
        }
        self.toast = Some(Toast {
            text,
            is_error,
            at: Instant::now(),
        });
    }

    /// Put `text` on the clipboard and say so.
    pub fn copy_to_clipboard(&mut self, text: String) {
        self.ctx.copy_text(text);
        self.notify(i18n::t("Copied to clipboard"), false);
    }

    /// Everything useful for a bug report, as plain text. Built only when asked for.
    pub fn diagnostics(&self) -> String {
        use std::fmt::Write;
        let c = &self.cfg;
        let mut out = String::new();
        let _ = writeln!(out, "Rewind {}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(
            out,
            "Install: {}",
            if crate::updates::is_installed_copy() {
                "installer"
            } else {
                "portable"
            }
        );
        let _ = writeln!(
            out,
            "CPU: {} ({} threads)",
            std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".into()),
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(0)
        );
        let _ = writeln!(
            out,
            "This app: {:.0}% CPU, {} MB",
            self.perf.cpu_pct, self.perf.mem_mb
        );
        let phase = match &self.phase {
            Phase::Installing => "installing runtime".to_string(),
            Phase::Starting { .. } => "starting".to_string(),
            Phase::Ready => "ready".to_string(),
            Phase::Failed(e) => format!("failed: {e}"),
        };
        let _ = writeln!(out, "State: {phase}");
        if let Some(e) = &self.engine {
            let _ = writeln!(
                out,
                "Encoder: {} (cpu: {})",
                e.encoder_label, e.cpu_encoding
            );
            for n in &e.notes {
                let _ = writeln!(out, "Note: {n}");
            }
        }
        let _ = writeln!(
            out,
            "Replay buffer: {}",
            if self.buffer_active() { "on" } else { "off" }
        );
        let _ = writeln!(
            out,
            "Video: {:?} {:?} {:?} {:?} {} fps {} kbps, {:?}, monitor {} of {}",
            c.capture_mode,
            c.display_method,
            c.codec,
            c.quality,
            c.fps,
            c.bitrate_kbps,
            c.resolution,
            c.monitor_index + 1,
            self.monitors.len()
        );
        let _ = writeln!(
            out,
            "Buffer: {} s, {} MB max, {:?}",
            c.buffer_secs, c.buffer_max_mb, c.container
        );
        let _ = writeln!(
            out,
            "Audio: desktop {} ({}), mic {} ({})",
            c.audio.desktop_enabled,
            if c.audio.desktop_device.is_empty() {
                "default"
            } else {
                "chosen device"
            },
            c.audio.mic_enabled,
            if c.audio.mic_device.is_empty() {
                "default"
            } else {
                "chosen device"
            },
        );
        let _ = writeln!(
            out,
            "UI: lang {:?}, theme {}, low power {}, animations {}",
            c.language, c.theme, c.low_power_ui, c.animations
        );
        let voice = self
            .voice
            .as_ref()
            .map(|v| format!("{:?}", v.shared.status()))
            .unwrap_or_else(|| "not running".into());
        let _ = writeln!(
            out,
            "Voice: enabled {}, lang {}, phrase \"{}\", {voice}",
            c.voice.enabled, c.voice.language, c.voice.phrase
        );
        if let Some(h) = self.voice.as_ref().and_then(|v| v.shared.last_heard()) {
            let _ = writeln!(
                out,
                "Voice last heard: \"{}\" {:?} {:?}",
                h.text, h.conf, h.verdict
            );
        }
        let _ = writeln!(
            out,
            "Add-ons: voice runtime {}, effects {}, themes {}",
            self.packs.runtime_installed(),
            self.packs.effects.len(),
            self.packs.themes.len()
        );
        for e in &self.hotkey_errors {
            let _ = writeln!(out, "Hotkey: {e}");
        }
        let now = SystemTime::now();
        for (at, e) in &self.recent_errors {
            let ago = now.duration_since(*at).map(|d| d.as_secs()).unwrap_or(0);
            let _ = writeln!(out, "Error ({ago} s ago): {e}");
        }
        out
    }

    /// In-app toast plus the over-the-game overlay (skipped while Rewind itself is focused).
    pub fn announce(&mut self, kind: ToastKind, title: &str, subtitle: &str) {
        let (title, subtitle) = (i18n::tp(title), i18n::tp(subtitle));
        let (title, subtitle) = (title.as_str(), subtitle.as_str());
        self.notify(
            if subtitle.is_empty() {
                title.to_string()
            } else {
                format!("{title} — {subtitle}")
            },
            kind == ToastKind::Error,
        );
        self.show_overlay(kind, title, subtitle, false);
    }

    pub fn show_overlay(&self, kind: ToastKind, title: &str, subtitle: &str, force: bool) {
        if self.cfg.overlay_enabled && (force || !self.focused) {
            self.overlay.show(
                ToastSpec {
                    kind,
                    title: i18n::tp(title),
                    subtitle: i18n::tp(subtitle),
                },
                self.cfg.overlay_corner,
                self.cfg.overlay_secs,
                theme::toast_colors(),
            );
        }
    }

    pub fn preview_overlay(&self) {
        // Use the draft values so the user sees what they're about to apply.
        self.overlay.show(
            ToastSpec {
                kind: ToastKind::Success,
                title: i18n::t("Replay saved").to_string(),
                subtitle: i18n::t("This is how notifications look").to_string(),
            },
            self.draft.overlay_corner,
            self.draft.overlay_secs,
            theme::toast_colors(),
        );
    }

    fn module_dir() -> PathBuf {
        directories_dir().join("obs-modules")
    }

    fn start_engine(&mut self) {
        match Engine::start(&self.cfg, &Self::module_dir()) {
            Ok(engine) => {
                for note in &engine.notes {
                    log::warn!("{note}");
                }
                self.engine = Some(engine);
                self.register_hotkeys();
                self.sync_voice();
                if self.cfg.start_buffer_on_launch {
                    self.set_buffer(true);
                }
                self.phase = Phase::Ready;
            }
            Err(e) => self.phase = Phase::Failed(e),
        }
    }

    fn register_hotkeys(&mut self) {
        if let Some(hk) = &mut self.hotkeys {
            self.hotkey_errors = hk.set(&[
                (Action::SaveReplay, &self.cfg.save_hotkey),
                (Action::ToggleBuffer, &self.cfg.toggle_hotkey),
            ]);
        } else {
            self.hotkey_errors = vec!["Global hotkeys are unavailable on this system.".into()];
        }
    }

    pub fn buffer_active(&self) -> bool {
        self.engine.is_some() && self.buffer_on
    }

    /// Ask OBS for the real buffer state. `force` skips the once-a-second throttle.
    fn refresh_buffer_state(&mut self, force: bool) {
        if !force
            && self
                .buffer_checked
                .map(|t| t.elapsed() < Duration::from_secs(1))
                .unwrap_or(false)
        {
            return;
        }
        let on = self
            .engine
            .as_ref()
            .map(|e| e.is_buffer_active())
            .unwrap_or(false);
        self.buffer_on = on;
        self.buffer_checked = Some(Instant::now());
    }

    pub fn set_buffer(&mut self, on: bool) {
        let Some(engine) = &self.engine else { return };
        match engine.set_buffer_active(on) {
            Ok(()) => {
                self.buffer_started_at = if on { Some(Instant::now()) } else { None };
                // OBS just accepted the change; trust it rather than asking again while the
                // output may still be spinning up.
                self.buffer_on = on;
                self.buffer_checked = Some(Instant::now());
                let (title, sub) = if on {
                    (
                        "Instant Replay on",
                        i18n::tf(
                            "Keeping the last {secs} s",
                            &[("secs", &self.cfg.buffer_secs.to_string())],
                        ),
                    )
                } else {
                    ("Instant Replay off", String::new())
                };
                self.announce(ToastKind::Info, title, &sub);
            }
            Err(e) => self.announce(ToastKind::Error, "Couldn't change Instant Replay", &e),
        }
    }

    pub fn save_replay(&mut self, ctx: &egui::Context) {
        if self.saving {
            return;
        }
        let Some(saver) = self.engine.as_ref().and_then(|e| e.saver()) else {
            return;
        };
        self.saving = true;
        // Note what's in front now, while the game still is (the save takes a moment).
        let game = if self.cfg.per_game_folders {
            platform::foreground_app_name().and_then(|n| library::game_folder_name(&n))
        } else {
            None
        };
        let root = self.cfg.output_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = saver
                .save()
                .map(|path| move_to_game_folder(&root, path, game.as_deref()));
            let _ = tx.send(Msg::Saved(result));
            ctx.request_repaint();
        });
    }

    /// Put the clip file on the clipboard.
    pub fn share_copy(&mut self, clip: &Clip) {
        match crate::share::copy_file(&clip.path) {
            Ok(()) => self.notify(
                i18n::t("Clip copied. Paste it into Discord, WhatsApp or a folder."),
                false,
            ),
            Err(e) => self.notify(i18n::tf("Couldn't copy: {e}", &[("e", &e)]), true),
        }
    }

    /// Make a small (under 10 MB) 720p copy in the background, then put it on the clipboard.
    pub fn share_small(&mut self, clip: &Clip) {
        if self.sharing.is_some() {
            return;
        }
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            self.notify(
                i18n::t("Needs ffmpeg: install Clip previews from Add-ons."),
                true,
            );
            return;
        };
        self.sharing = Some(clip.name.clone());
        self.notify(i18n::t("Making a smaller copy…"), false);
        let (tx, ctx, src) = (self.tx.clone(), self.ctx.clone(), clip.path.clone());
        std::thread::Builder::new()
            .name("rewind-share".into())
            .spawn(move || {
                let _ = tx.send(Msg::Shared(crate::share::make_small_copy(&ffmpeg, &src)));
                ctx.request_repaint();
            })
            .ok();
    }

    /// Whether a clip is starred.
    pub fn is_favorite(&self, clip: &Clip) -> bool {
        let key = library::relative_key(&self.cfg.output_dir, &clip.path);
        self.cfg.favorites.iter().any(|f| *f == key)
    }

    /// Star or unstar a clip. Saved right away; no Apply needed.
    pub fn toggle_favorite(&mut self, clip: &Clip) {
        let key = library::relative_key(&self.cfg.output_dir, &clip.path);
        if let Some(i) = self.cfg.favorites.iter().position(|f| *f == key) {
            self.cfg.favorites.remove(i);
        } else {
            self.cfg.favorites.push(key);
        }
        self.draft.favorites = self.cfg.favorites.clone();
        self.save_quietly();
        self.filter_dirty = true;
    }

    /// Keep a favourite when its file is renamed or deleted.
    pub fn favorite_moved(&mut self, from: &std::path::Path, to: Option<&std::path::Path>) {
        let old = library::relative_key(&self.cfg.output_dir, from);
        let Some(i) = self.cfg.favorites.iter().position(|f| *f == old) else {
            return;
        };
        match to {
            Some(to) => {
                self.cfg.favorites[i] = library::relative_key(&self.cfg.output_dir, to);
            }
            None => {
                self.cfg.favorites.remove(i);
            }
        }
        self.draft.favorites = self.cfg.favorites.clone();
        self.save_quietly();
    }

    /// Delete the oldest clips (never favourites) until the folder fits the storage limit.
    fn enforce_storage_limit(&mut self) {
        if self.cfg.storage_limit_gb == 0 {
            return;
        }
        let limit = self.cfg.storage_limit_gb as u64 * 1_000_000_000;
        let doomed: Vec<std::path::PathBuf> =
            library::over_limit(&self.clips, limit, |c| self.is_favorite(c))
                .into_iter()
                .map(|c| c.path.clone())
                .collect();
        if doomed.is_empty() {
            return;
        }
        let removed = doomed
            .iter()
            .filter(|p| std::fs::remove_file(p).is_ok())
            .count();
        if removed > 0 {
            self.notify(
                i18n::tf(
                    "Removed {n} old clips to stay under {gb} GB",
                    &[
                        ("n", &removed.to_string()),
                        ("gb", &self.cfg.storage_limit_gb.to_string()),
                    ],
                ),
                false,
            );
            self.refresh_clips(true);
        }
    }

    /// Apply edited settings: persist, rebuild what changed, re-register hotkeys.
    pub fn apply_settings(&mut self) {
        let mut next = self.draft.clone();
        next.sanitize();
        let was_running = self.buffer_active();
        let rebuild = self.cfg.requires_engine_rebuild(&next);
        let mut note = None;
        if let Some(engine) = &mut self.engine {
            if rebuild {
                if let Err(e) = engine.apply(&next, was_running) {
                    // Roll back to the last known-good settings so the buffer keeps working.
                    let _ = engine.apply(&self.cfg, was_running);
                    self.draft = self.cfg.clone();
                    self.notify(
                        i18n::tf("Couldn't apply settings: {e}", &[("e", &e.to_string())]),
                        true,
                    );
                    return;
                }
            } else {
                engine.set_volumes(&next.audio);
            }
        }
        if !next.thumbnails {
            self.thumbs.clear();
        }
        if next.launch_at_login != self.cfg.launch_at_login {
            let exe = std::env::current_exe().unwrap_or_default();
            if let Err(e) = platform::set_autostart(next.launch_at_login, &exe) {
                next.launch_at_login = self.cfg.launch_at_login;
                note = Some(i18n::tf(
                    "Start with Windows: {e}",
                    &[("e", &e.to_string())],
                ));
            }
        }
        self.refresh_buffer_state(true);
        let language_changed = next.language != self.cfg.language;
        self.cfg = next;
        i18n::set_lang(self.cfg.language);
        if language_changed {
            if let Some(tray) = &mut self.tray {
                tray.retranslate();
            }
        }
        self.invalidate_voice_files();
        self.draft = self.cfg.clone();
        if let Err(e) = self.cfg.save_to(&self.cfg_path) {
            self.notify(
                i18n::tf(
                    "Settings applied but couldn't be saved: {e}",
                    &[("e", &e.to_string())],
                ),
                true,
            );
        } else {
            self.notify(
                note.unwrap_or_else(|| i18n::t("Settings applied").to_string()),
                false,
            );
        }
        self.register_hotkeys();
        self.sync_voice();
        self.refresh_clips(true);
    }

    /// Ask the scan worker for a fresh clip list. Never blocks the UI thread; the results
    /// arrive through [`Msg::Scanned`] a frame or two later.
    pub fn refresh_clips(&mut self, force: bool) {
        let stale = self
            .clips_scanned
            .map(|t| t.elapsed() > Duration::from_secs(5))
            .unwrap_or(true);
        if !force && !stale {
            return;
        }
        if self.scan_inflight {
            // A scan is already running; make sure a fresh one follows it so we never
            // miss a clip that was saved while it was in flight.
            self.scan_again = true;
            return;
        }
        let dir = self.cfg.output_dir.clone();
        if self
            .scan_tx
            .as_ref()
            .map(|tx| tx.send(dir.clone()).is_ok())
            .unwrap_or(false)
        {
            self.scan_inflight = true;
            self.clips_scanned = Some(Instant::now());
        } else {
            // Worker unavailable: fall back to a blocking scan rather than showing no clips.
            self.scan_tx = None;
            self.accept_scan(library::scan(&dir));
        }
    }

    fn accept_scan(&mut self, clips: Vec<Clip>) {
        self.clips_total_bytes = clips.iter().map(|c| c.size_bytes).sum();
        // A scan that found the same clips shouldn't invalidate the cached rows or restart
        // the reveal animation; this happens on every 5-second refresh.
        let changed = clips != self.clips;
        self.clips = clips;
        if changed {
            self.clips_rev = self.clips_rev.wrapping_add(1);
            self.list_shown = Instant::now();
        }
        self.clips_scanned = Some(Instant::now());
        self.scan_inflight = false;
        if std::mem::take(&mut self.scan_again) {
            self.refresh_clips(true);
        } else if changed {
            self.enforce_storage_limit();
        }
    }

    pub fn run_trim(&mut self, ctx: &egui::Context) {
        let (Some(ts), Some(ffmpeg)) = (&mut self.trim, self.ffmpeg.clone()) else {
            return;
        };
        let job = TrimJob {
            input: ts.clip.path.clone(),
            output: trim::default_output(&ts.clip.path),
            start_secs: ts.start,
            end_secs: ts.end,
            last_secs: (ts.mode == TrimMode::LastSeconds).then_some(ts.last_secs),
            keep_audio: vec![],
        };
        if let Err(e) = job.validate() {
            let msg = e.to_string();
            self.notify(msg, true);
            return;
        }
        ts.running = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut cmd = std::process::Command::new(ffmpeg);
            cmd.args(job.ffmpeg_args());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
            let res = match cmd.output() {
                Ok(o) if o.status.success() => Ok(job.output.clone()),
                Ok(o) => Err(String::from_utf8_lossy(&o.stderr)
                    .lines()
                    .last()
                    .unwrap_or("ffmpeg failed")
                    .to_string()),
                Err(e) => Err(format!("couldn't run ffmpeg: {e}")),
            };
            let _ = tx.send(Msg::Trimmed(res));
            ctx.request_repaint();
        });
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Installed(Ok(())) => self.phase = Phase::Starting { frames: 0 },
                Msg::Installed(Err(e)) => self.phase = Phase::Failed(e),
                Msg::Saved(Ok(path)) => {
                    self.saving = false;
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.last_saved = Some(path);
                    if self.cfg.sound_on_save {
                        crate::sounds::play_clip_saved(&sounds_dir(), &self.cfg.clip_sound);
                    }
                    self.announce(ToastKind::Success, "Replay saved", &name);
                    self.refresh_clips(true);
                }
                Msg::Saved(Err(e)) => {
                    self.saving = false;
                    self.announce(ToastKind::Error, "Couldn't save replay", &e);
                }
                Msg::Trimmed(Ok(path)) => {
                    self.trim = None;
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.notify(
                        i18n::tf("Trimmed copy saved: {name}", &[("name", &name)]),
                        false,
                    );
                    self.refresh_clips(true);
                }
                Msg::Trimmed(Err(e)) => {
                    if let Some(t) = &mut self.trim {
                        t.running = false;
                    }
                    self.notify(i18n::tf("Trim failed: {e}", &[("e", &e.to_string())]), true);
                }
                Msg::Scanned(clips) => self.accept_scan(clips),
                Msg::Update(Ok(Some(info))) => {
                    if self.cfg.auto_install_updates && crate::updates::can_install(&info) {
                        self.install_update(info);
                    } else {
                        self.update_state = UpdateState::Available(info);
                    }
                }
                Msg::UpdateReady(Ok(())) => {
                    let version = match &self.update_state {
                        UpdateState::Installing(info, _) => info.version.clone(),
                        _ => String::new(),
                    };
                    self.announce(
                        ToastKind::Info,
                        "Updating Rewind",
                        &i18n::tf("Restarting into version {v}…", &[("v", &version)]),
                    );
                    self.update_state = UpdateState::Restarting(version);
                    // Quit for real (not to the tray) so the installer can replace the files.
                    self.quitting = true;
                    self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Msg::UpdateReady(Err(e)) => {
                    let info = match std::mem::replace(&mut self.update_state, UpdateState::Idle) {
                        UpdateState::Installing(info, _) => Some(info),
                        _ => None,
                    };
                    self.notify(i18n::tf("Couldn't update: {e}", &[("e", &e)]), true);
                    self.update_state = match info {
                        Some(info) => UpdateState::InstallFailed(info, e),
                        None => UpdateState::Failed(e),
                    };
                }
                Msg::Shared(result) => {
                    self.sharing = None;
                    match result.and_then(|p| crate::share::copy_file(&p).map(|_| p)) {
                        Ok(_) => self.notify(
                            i18n::t("Small copy ready (under 10 MB). Paste it into Discord."),
                            false,
                        ),
                        Err(e) => self.notify(
                            i18n::tf("Couldn't make a small copy: {e}", &[("e", &e)]),
                            true,
                        ),
                    }
                }
                Msg::Update(Ok(None)) => self.update_state = UpdateState::UpToDate,
                Msg::Update(Err(e)) => self.update_state = UpdateState::Failed(e),
            }
        }
    }

    fn handle_hotkeys(&mut self, ctx: &egui::Context) {
        let actions = self.hotkeys.as_ref().map(|h| h.poll()).unwrap_or_default();
        for a in actions {
            if !matches!(self.phase, Phase::Ready) {
                continue;
            }
            match a {
                Action::SaveReplay => self.save_replay(ctx),
                Action::ToggleBuffer => {
                    let on = !self.buffer_active();
                    self.set_buffer(on);
                }
            }
        }
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        let actions = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for a in actions {
            match a {
                TrayAction::Show => self.show_window(ctx),
                TrayAction::SaveReplay if matches!(self.phase, Phase::Ready) => {
                    self.save_replay(ctx)
                }
                TrayAction::ToggleBuffer if matches!(self.phase, Phase::Ready) => {
                    let on = !self.buffer_active();
                    self.set_buffer(on);
                }
                TrayAction::OpenFolder => {
                    let _ = std::fs::create_dir_all(&self.cfg.output_dir);
                    let _ = opener::open(&self.cfg.output_dir);
                }
                TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
        }
        let on = self.buffer_active();
        if let Some(t) = &mut self.tray {
            t.sync(on);
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.window_hidden = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        self.window_hidden = true;
        self.focused = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        if !self.tray_hint_shown {
            self.tray_hint_shown = true;
            self.show_overlay(
                ToastKind::Info,
                "Rewind is still running",
                "Find it in the system tray",
                true,
            );
        }
    }
}

/// Move a just-saved clip into `root/<game>/`. Any problem leaves it where it is.
fn move_to_game_folder(root: &std::path::Path, path: PathBuf, game: Option<&str>) -> PathBuf {
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

/// Custom clip-saved sounds (.mp3 / .wav).
pub fn sounds_dir() -> PathBuf {
    directories_dir().join("sounds")
}

pub fn themes_dir() -> PathBuf {
    directories_dir().join("packs").join("themes")
}

fn directories_dir() -> PathBuf {
    Config::default_path()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

impl eframe::App for RewindApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The language follows the applied config, so a change only takes effect on Apply.
        i18n::set_lang(self.cfg.language);
        self.drain_messages();
        self.take_dropped_sounds(ctx);
        if self.engine.is_some() {
            self.refresh_buffer_state(false);
        }
        self.handle_hotkeys(ctx);
        self.handle_tray(ctx);

        for ev in self.packs.poll() {
            match ev {
                PackEvent::Previewing(name) => {
                    self.notify(i18n::tf("Previewing {name}", &[("name", &name)]), false);
                }
                PackEvent::Installed(key) => {
                    self.notify(i18n::tf("Installed {key}", &[("key", &key)]), false);
                    if key.starts_with("theme-") {
                        // The chosen theme may have just arrived: draw it again.
                        self.applied_theme.clear();
                    }
                    self.invalidate_voice_files();
                    self.refresh_preview_exe();
                    if !self.packs.available_effects().contains(&self.draft.effects) {
                        self.draft.effects = EffectStyle::None;
                    }
                    self.sync_voice();
                }
                PackEvent::Failed(key, e) => self.notify(
                    i18n::tf("Couldn't install {key}: {e}", &[("key", &key), ("e", &e)]),
                    true,
                ),
            }
        }
        // Keeps a crashed/errored listener on its retry schedule.
        self.sync_voice();
        // One quiet update check per launch, only once the engine is up.
        if self.cfg.check_updates && !self.update_auto_checked && matches!(self.phase, Phase::Ready)
        {
            self.update_auto_checked = true;
            self.check_updates();
        }
        // Live theme / animation preview while editing settings.
        if self.applied_theme != self.effective_theme() {
            self.apply_theme(ctx);
        }
        if theme::animations_on() != self.draft.animations {
            theme::set_animations(ctx, self.draft.animations);
        }
        theme::set_extra_motion(self.draft.extra_motion && !self.draft.low_power_ui);
        if theme::tick_tween() {
            theme::apply_visuals(ctx);
        }
        if self.voice.as_ref().map(|v| v.poll()).unwrap_or(false)
            && matches!(self.phase, Phase::Ready)
        {
            self.save_replay(ctx);
        }

        if let Phase::Starting { frames } = &mut self.phase {
            // Draw a couple of "Starting…" frames before the blocking engine start.
            if *frames < 2 {
                *frames += 1;
                ctx.request_repaint();
            } else {
                self.start_engine();
            }
        }
        if let Some(t) = &self.toast {
            if t.at.elapsed().as_secs_f32() > TOAST_TOTAL {
                self.toast = None;
            }
        }

        // Repaint only when something is actually changing. Hotkeys, the tray and worker threads
        // all wake the loop themselves, so a hidden or idle window uses no CPU at all.
        let visible = !self.window_hidden && matches!(self.phase, Phase::Ready);
        let mut next: Option<Duration> = None;
        if visible {
            self.perf.sample_if_due();
            let live = self.buffer_active() && self.focused;
            // A full 60 fps burst only while something is actually moving (page slide, toast
            // fade, theme cross-fade). The recording pulse gets 10 fps, which is plenty for a
            // ~1.4 s breathing dot. Idle stays on the slow CPU/RAM sample cadence.
            let animating = matches!(self.update_state, UpdateState::Installing(..))
                || self.page_anim() < 1.0
                || theme::tweening()
                || self.toast_alpha() < 1.0
                || self.scroll_animating()
                || self.list_animating()
                || !self.fx.is_empty();
            let pulsing = live && self.cfg.animations && !self.cfg.low_power_ui;
            let mut ms: u64 = if animating {
                16
            } else if pulsing {
                33
            } else if live {
                1000
            } else {
                2000
            };
            // A toast at full opacity is idle, but we must wake up exactly when its fade-out
            // starts or the window would sleep straight through it.
            if self.toast.is_some() && !animating {
                let hold = self
                    .toast
                    .as_ref()
                    .map(|t| (TOAST_TOTAL - 0.35 - t.at.elapsed().as_secs_f32()).max(0.0))
                    .unwrap_or(0.0);
                ms = ms.min((hold * 1000.0) as u64).max(16);
            }
            next = Some(Duration::from_millis(ms));
        }
        if let Some(d) = next {
            ctx.request_repaint_after(d);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if ctx.input(|i| i.viewport().close_requested())
            && self.cfg.close_to_tray
            && !self.quitting
            && self.tray.is_some()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_to_tray(&ctx);
        }
        effects::set_style(if self.draft.animations && !self.draft.low_power_ui {
            self.draft.effects
        } else {
            EffectStyle::None
        });
        self.absorb_fx(&ctx);
        views::draw(self, ui);
        // Pick up anything queued by clicks during this frame so the celebration appears
        // immediately rather than one frame later.
        self.absorb_fx(&ctx);
        effects::draw(&ctx, &self.fx);
    }
}

pub fn now() -> std::time::SystemTime {
    std::time::SystemTime::now()
}
