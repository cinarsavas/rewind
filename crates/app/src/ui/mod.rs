mod effects;
mod theme;
mod views;
mod widgets;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use rewind_core::config::{Config, EffectStyle, Preset, VoiceConfig};
use rewind_core::library::{self, human_age, human_size, Clip};
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
use crate::voice::{self, VoiceHandle};

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
    let group = if clip.group.is_empty() { String::new() } else { format!("{} · ", clip.group) };
    format!("{group}{} · {}", human_size(clip.size_bytes), human_age(now, clip.modified))
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
    pub trim: Option<TrimState>,
    pub confirm_delete: Option<Clip>,
    pub buffer_started_at: Option<Instant>,
    quitting: bool,
    window_hidden: bool,
    focused: bool,
    tray_hint_shown: bool,
    ctx: egui::Context,
    pub perf: Sampler,
    pub thumbs: Thumbs,
    pub packs: PackManager,
    voice: Option<VoiceHandle>,
    voice_running_cfg: Option<VoiceConfig>,
    pub voice_error: Option<String>,
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
    /// Live celebrations (toggle effects), painted above the UI.
    fx: Vec<effects::FxEvent>,
    fx_seed: u32,
}

impl RewindApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start_hidden: bool) -> Self {
        let cfg_path = Config::default_path().unwrap_or_else(|| PathBuf::from("rewind-config.toml"));
        let first_run = !cfg_path.exists();
        let mut cfg = Config::load_from(&cfg_path);
        if first_run {
            // No config yet: pick a light preset for clearly older machines instead of
            // starting at 1080p60 and letting the first recording stutter. The friend's
            // Arrandale i5 reports 4 logical cores, so it lands on Low-end.
            let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            if cores <= 4 {
                cfg.apply_preset(Preset::LowEnd);
            }
            // Persist the detected preset so later launches don't re-detect.
            let _ = cfg.save_to(&cfg_path);
        }
        let packs = PackManager::new(directories_dir().join("packs"), cc.egui_ctx.clone());
        theme::install_fonts(&cc.egui_ctx);
        theme::set_animations(&cc.egui_ctx, cfg.animations);
        theme::set_palette(&packs.palette_for(&cfg.theme));
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
        let app_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let hotkeys = Hotkeys::new(cc.egui_ctx.clone()).ok();
        let tray = Tray::new(cc.egui_ctx.clone(), cfg.start_buffer_on_launch).map_err(|e| log::warn!("{e}")).ok();
        let hide_now = (start_hidden || cfg.start_minimized) && tray.is_some();
        let cfg_theme = cfg.theme.clone();
        if hide_now {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
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
            toast: None,
            tx,
            rx,
            clips: Vec::new(),
            clips_total_bytes: 0,
            clip_rows: Vec::new(),
            filter_idx: Vec::new(),
            clip_filter: String::new(),
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
            ffmpeg: trim::find_ffmpeg(app_dir.as_deref()),
            trim: None,
            confirm_delete: None,
            buffer_started_at: None,
            quitting: false,
            window_hidden: hide_now,
            focused: !hide_now,
            tray_hint_shown: false,
            ctx: cc.egui_ctx.clone(),
            perf: Sampler::new(),
            thumbs: Thumbs::new(cc.egui_ctx.clone()),
            packs,
            voice: None,
            voice_running_cfg: None,
            voice_error: None,
            page_changed: Instant::now(),
            page_dir: 1.0,
            applied_theme: cfg_theme,
            scroll_pos: 0.0,
            scroll_target: 0.0,
            scroll_max: 0.0,
            list_shown: Instant::now(),
            fx: Vec::new(),
            fx_seed: 0x9E37_79B9,
        }
    }

    /// Switch page, restarting the page-enter transition.
    pub fn set_page(&mut self, page: Page) {
        if self.page != page {
            self.page_dir = if page.rank() >= self.page.rank() { 1.0 } else { -1.0 };
            self.page = page;
            self.page_changed = Instant::now();
            // Start each page at the top, without the old scroll position leaking across.
            self.scroll_pos = 0.0;
            self.scroll_target = 0.0;
            self.scroll_max = 0.0;
            self.list_shown = Instant::now();
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
        row_reveal_at(self.list_shown, index, theme::animations_on() && !self.cfg.low_power_ui)
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
    fn absorb_fx(&mut self) {
        let enabled = theme::animations_on() && !self.draft.low_power_ui;
        for (style, at) in effects::take_queued() {
            if !enabled || style == EffectStyle::None || self.fx.len() >= 6 {
                continue;
            }
            self.fx_seed = self.fx_seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            self.fx.push(effects::FxEvent::new(style, at, self.fx_seed));
        }
        self.fx.retain(|e| !e.expired());
    }

    /// Refresh the cached clip rows (and their display strings) when the library or the
    /// relative-age bucket changed. Cheap to call every frame; does nothing most of the time.
    pub fn sync_clip_rows(&mut self) {
        let bucket = now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 5).unwrap_or(0);
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
            self.clip_rows = self.clips.iter().map(|c| ClipRow { meta: clip_meta(now, c), clip: c.clone() }).collect();
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
        self.filter_idx = if needle.is_empty() {
            (0..self.clip_rows.len()).collect()
        } else {
            self.clip_rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.clip.name.to_lowercase().contains(&needle))
                .map(|(i, _)| i)
                .collect()
        };
        self.filter_dirty = false;
        self.filter_rev = self.clips_rev;
    }

    /// Called by the search box so filtering re-runs only on real edits.
    pub fn mark_filter_dirty(&mut self) {
        self.filter_dirty = true;
        self.list_shown = Instant::now();
    }

    /// Start, restart or stop the voice listener so it matches the settings and what's installed.
    pub fn sync_voice(&mut self) {
        let want = self.cfg.voice.enabled
            && self.engine.is_some()
            && self.packs.runtime_installed()
            && self.packs.model_installed(&self.cfg.voice.language);
        if !want {
            self.voice = None;
            self.voice_running_cfg = None;
            return;
        }
        if self.voice.is_some() && self.voice_running_cfg.as_ref() == Some(&self.cfg.voice) {
            return;
        }
        self.voice = None;
        self.voice_error = None;
        let handle = voice::start(&self.cfg.voice, self.packs.voice_runtime_dir(), self.packs.voice_model_dir(&self.cfg.voice.language), self.ctx.clone());
        self.voice = Some(handle);
        self.voice_running_cfg = Some(self.cfg.voice.clone());
    }

    pub fn voice_handle(&self) -> Option<&VoiceHandle> {
        self.voice.as_ref()
    }

    /// In-app toast only.
    pub fn notify(&mut self, text: impl Into<String>, is_error: bool) {
        self.toast = Some(Toast { text: text.into(), is_error, at: Instant::now() });
    }

    /// In-app toast plus the over-the-game overlay (skipped while Rewind itself is focused).
    pub fn announce(&mut self, kind: ToastKind, title: &str, subtitle: &str) {
        self.notify(if subtitle.is_empty() { title.to_string() } else { format!("{title} — {subtitle}") }, kind == ToastKind::Error);
        self.show_overlay(kind, title, subtitle, false);
    }

    pub fn show_overlay(&self, kind: ToastKind, title: &str, subtitle: &str, force: bool) {
        if self.cfg.overlay_enabled && (force || !self.focused) {
            self.overlay.show(
                ToastSpec { kind, title: title.to_string(), subtitle: subtitle.to_string() },
                self.cfg.overlay_corner,
                self.cfg.overlay_secs,
            );
        }
    }

    pub fn preview_overlay(&self) {
        // Use the draft values so the user sees what they're about to apply.
        self.overlay.show(
            ToastSpec { kind: ToastKind::Success, title: "Replay saved".into(), subtitle: "This is how notifications look".into() },
            self.draft.overlay_corner,
            self.draft.overlay_secs,
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
        self.engine.as_ref().map(|e| e.is_buffer_active()).unwrap_or(false)
    }

    pub fn set_buffer(&mut self, on: bool) {
        let Some(engine) = &self.engine else { return };
        match engine.set_buffer_active(on) {
            Ok(()) => {
                self.buffer_started_at = if on { Some(Instant::now()) } else { None };
                let (title, sub) = if on {
                    ("Instant Replay on", format!("Keeping the last {} s", self.cfg.buffer_secs))
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
        let Some(saver) = self.engine.as_ref().and_then(|e| e.saver()) else { return };
        self.saving = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Saved(saver.save()));
            ctx.request_repaint();
        });
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
                    self.notify(format!("Couldn't apply settings: {e}"), true);
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
                note = Some(format!("Start with Windows: {e}"));
            }
        }
        self.cfg = next;
        self.draft = self.cfg.clone();
        if let Err(e) = self.cfg.save_to(&self.cfg_path) {
            self.notify(format!("Settings applied but couldn't be saved: {e}"), true);
        } else {
            self.notify(note.unwrap_or_else(|| "Settings applied".into()), false);
        }
        self.register_hotkeys();
        self.sync_voice();
        self.refresh_clips(true);
    }

    /// Ask the scan worker for a fresh clip list. Never blocks the UI thread; the results
    /// arrive through [`Msg::Scanned`] a frame or two later.
    pub fn refresh_clips(&mut self, force: bool) {
        let stale = self.clips_scanned.map(|t| t.elapsed() > Duration::from_secs(5)).unwrap_or(true);
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
        if self.scan_tx.as_ref().map(|tx| tx.send(dir.clone()).is_ok()).unwrap_or(false) {
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
        }
    }

    pub fn run_trim(&mut self, ctx: &egui::Context) {
        let (Some(ts), Some(ffmpeg)) = (&mut self.trim, self.ffmpeg.clone()) else { return };
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
                Ok(o) => Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg failed").to_string()),
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
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.last_saved = Some(path);
                    if self.cfg.sound_on_save {
                        platform::beep();
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
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.notify(format!("Trimmed copy saved: {name}"), false);
                    self.refresh_clips(true);
                }
                Msg::Trimmed(Err(e)) => {
                    if let Some(t) = &mut self.trim {
                        t.running = false;
                    }
                    self.notify(format!("Trim failed: {e}"), true);
                }
                Msg::Scanned(clips) => self.accept_scan(clips),
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
                TrayAction::SaveReplay if matches!(self.phase, Phase::Ready) => self.save_replay(ctx),
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
            self.show_overlay(ToastKind::Info, "Rewind is still running", "Find it in the system tray", true);
        }
    }
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
        self.drain_messages();
        self.handle_hotkeys(ctx);
        self.handle_tray(ctx);

        for ev in self.packs.poll() {
            match ev {
                PackEvent::Installed(key) => {
                    self.notify(format!("Installed {key}"), false);
                    self.sync_voice();
                }
                PackEvent::Failed(key, e) => self.notify(format!("Couldn't install {key}: {e}"), true),
            }
        }
        // Live theme / animation preview while editing settings.
        if self.applied_theme != self.draft.theme {
            let palette = self.packs.palette_for(&self.draft.theme);
            theme::fade_to(ctx, &palette);
            theme::apply_visuals(ctx);
            self.applied_theme = self.draft.theme.clone();
        }
        if theme::animations_on() != self.draft.animations {
            theme::set_animations(ctx, self.draft.animations);
        }
        if theme::tick_tween() {
            theme::apply_visuals(ctx);
        }
        if self.voice.as_ref().map(|v| v.poll()).unwrap_or(false) && matches!(self.phase, Phase::Ready) {
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
            let animating = self.page_anim() < 1.0
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
                let hold = self.toast.as_ref().map(|t| (TOAST_TOTAL - 0.35 - t.at.elapsed().as_secs_f32()).max(0.0)).unwrap_or(0.0);
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
        if ctx.input(|i| i.viewport().close_requested()) && self.cfg.close_to_tray && !self.quitting && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_to_tray(&ctx);
        }
        effects::set_style(if theme::animations_on() && !self.draft.low_power_ui {
            self.draft.effects
        } else {
            EffectStyle::None
        });
        self.absorb_fx();
        views::draw(self, ui);
        // Pick up anything queued by clicks during this frame so the celebration appears
        // immediately rather than one frame later.
        self.absorb_fx();
        effects::draw(&ctx, &self.fx);
    }
}

pub fn now() -> std::time::SystemTime {
    std::time::SystemTime::now()
}
