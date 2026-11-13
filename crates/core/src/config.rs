//! User settings, stored as TOML in the platform config directory.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::hotkey::Hotkey;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    /// Record the chosen monitor. A fullscreen game is picked up by the game-capture
    /// layer stacked above it, so you get one continuous buffer.
    Desktop,
    /// Only hook fullscreen games; the buffer records nothing on the plain desktop.
    GameOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    H264,
    Hevc,
    Av1,
}

impl Codec {
    pub const ALL: [Codec; 3] = [Codec::H264, Codec::Hevc, Codec::Av1];
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC (H.265)",
            Codec::Av1 => "AV1",
        }
    }
    /// Compact form for segmented controls.
    pub fn short_label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
            Codec::Av1 => "AV1",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Speed,
    Balanced,
    Quality,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Speed, Quality::Balanced, Quality::Quality];
    pub fn label(self) -> &'static str {
        match self {
            Quality::Speed => "Fastest",
            Quality::Balanced => "Balanced",
            Quality::Quality => "Best quality",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    Mp4,
    Mkv,
}

impl Container {
    pub const ALL: [Container; 2] = [Container::Mp4, Container::Mkv];
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mkv => "mkv",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Container::Mp4 => "MP4 (most compatible)",
            Container::Mkv => "MKV (safest if the PC crashes)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight];
    pub fn label(self) -> &'static str {
        match self {
            Corner::TopLeft => "Top left",
            Corner::TopRight => "Top right",
            Corner::BottomLeft => "Bottom left",
            Corner::BottomRight => "Bottom right",
        }
    }
}

/// Output resolution as a target height; width follows the monitor's aspect ratio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Native,
    P2160,
    P1440,
    P1080,
    P720,
}

impl Resolution {
    pub const ALL: [Resolution; 5] = [
        Resolution::Native,
        Resolution::P2160,
        Resolution::P1440,
        Resolution::P1080,
        Resolution::P720,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Resolution::Native => "Same as monitor",
            Resolution::P2160 => "2160p",
            Resolution::P1440 => "1440p",
            Resolution::P1080 => "1080p",
            Resolution::P720 => "720p",
        }
    }
    pub fn target_height(self) -> Option<u32> {
        match self {
            Resolution::Native => None,
            Resolution::P2160 => Some(2160),
            Resolution::P1440 => Some(1440),
            Resolution::P1080 => Some(1080),
            Resolution::P720 => Some(720),
        }
    }
}

/// Compute the output size for a monitor, never upscaling, keeping the aspect ratio and
/// rounding to even numbers (hardware encoders reject odd dimensions).
pub fn output_size(base_w: u32, base_h: u32, res: Resolution) -> (u32, u32) {
    let (bw, bh) = (base_w.max(2), base_h.max(2));
    let h = match res.target_height() {
        Some(t) if t < bh => t,
        _ => bh,
    };
    let w = (bw as u64 * h as u64 / bh as u64) as u32;
    (w & !1, h & !1)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub desktop_enabled: bool,
    pub desktop_volume: f32,
    pub mic_enabled: bool,
    pub mic_volume: f32,
    pub bitrate_kbps: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            desktop_enabled: true,
            desktop_volume: 1.0,
            mic_enabled: false,
            mic_volume: 1.0,
            bitrate_kbps: 192,
        }
    }
}

/// How monitors are captured on Windows.
///
/// DXGI Desktop Duplication is the default because it is the only method that does not
/// draw the yellow "you are being recorded" border that Windows 10 adds to Windows
/// Graphics Capture (WGC). WGC is kept as an option for Windows 11, where it is the
/// better-behaved API and does not draw a border.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMethod {
    Dxgi,
    Wgc,
}

impl DisplayMethod {
    pub const ALL: [DisplayMethod; 2] = [DisplayMethod::Dxgi, DisplayMethod::Wgc];
    pub fn label(self) -> &'static str {
        match self {
            DisplayMethod::Dxgi => "DXGI (no border)",
            DisplayMethod::Wgc => "Windows Graphics Capture",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            DisplayMethod::Dxgi => "Best on Windows 10: fast and no yellow recording border.",
            DisplayMethod::Wgc => "Needed on some Windows 11 setups, but Windows 10 draws a yellow border.",
        }
    }
}

/// Little celebration played when a switch is turned on. Purely cosmetic, and skipped
/// entirely when animations or low-power mode are off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectStyle {
    None,
    Shine,
    Sparkle,
    Creeper,
    Ripple,
}

impl EffectStyle {
    pub const ALL: [EffectStyle; 5] = [
        EffectStyle::None,
        EffectStyle::Shine,
        EffectStyle::Sparkle,
        EffectStyle::Creeper,
        EffectStyle::Ripple,
    ];
    /// Short label for the segmented control.
    pub fn label(self) -> &'static str {
        match self {
            EffectStyle::None => "Off",
            EffectStyle::Shine => "Shine",
            EffectStyle::Sparkle => "Sparkle",
            EffectStyle::Creeper => "Creeper",
            EffectStyle::Ripple => "Ripple",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            EffectStyle::None => "No celebration.",
            EffectStyle::Shine => "A soft light sweep across the switch.",
            EffectStyle::Sparkle => "A burst of tiny stars.",
            EffectStyle::Creeper => "A pixel creeper goes boom.",
            EffectStyle::Ripple => "Calm expanding rings.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    LowEnd,
    Balanced,
    HighQuality,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::LowEnd, Preset::Balanced, Preset::HighQuality];
    pub fn label(self) -> &'static str {
        match self {
            Preset::LowEnd => "Low-end PC",
            Preset::Balanced => "Balanced",
            Preset::HighQuality => "High quality",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Preset::LowEnd => "720p30, light on CPU and GPU, no UI animations",
            Preset::Balanced => "1080p60, a good default for most PCs",
            Preset::HighQuality => "Native resolution, high bitrate, for strong GPUs",
        }
    }
}

/// Optional voice trigger ("clip that"). Needs the downloadable voice add-on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub enabled: bool,
    /// Language pack id, e.g. "en-us" or "tr".
    pub language: String,
    /// The phrase that saves a replay.
    pub phrase: String,
    /// 0.0 (needs a loud voice) ..= 1.0 (picks up quiet speech).
    pub sensitivity: f32,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self { enabled: false, language: "en-us".into(), phrase: "clip that".into(), sensitivity: 0.5 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub capture_mode: CaptureMode,
    /// Index into the monitor list shown in the UI.
    pub monitor_index: usize,
    /// Windows screen-capture API. DXGI avoids the Windows 10 yellow capture border.
    pub display_method: DisplayMethod,
    pub fps: u32,
    pub resolution: Resolution,
    pub codec: Codec,
    pub quality: Quality,
    pub bitrate_kbps: u32,
    pub buffer_secs: u32,
    pub buffer_max_mb: u32,
    pub container: Container,
    pub capture_cursor: bool,
    pub output_dir: PathBuf,
    pub save_hotkey: Hotkey,
    pub toggle_hotkey: Hotkey,
    pub start_buffer_on_launch: bool,
    /// Show a notification over games and apps when a clip is saved, and so on.
    pub overlay_enabled: bool,
    pub overlay_corner: Corner,
    pub overlay_secs: u32,
    /// Play a short system sound when a clip is saved (works even where the overlay can't draw).
    pub sound_on_save: bool,
    /// Closing the window hides it to the tray instead of quitting.
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub launch_at_login: bool,
    /// Smooth UI transitions. Turn off on slow PCs.
    pub animations: bool,
    /// Eased mouse-wheel scrolling. Separate from animations so it can be tuned on its own.
    pub smooth_scroll: bool,
    /// Celebration played when a switch is turned on.
    pub effects: EffectStyle,
    /// Minimal-power UI: no pulsing, slow refresh. Implied by the Low-end preset.
    pub low_power_ui: bool,
    /// Show video thumbnails in the clip list (uses Windows' own thumbnail cache).
    pub thumbnails: bool,
    /// Id of the active theme; "default" is built in, others are optional downloads.
    pub theme: String,
    pub voice: VoiceConfig,
    pub audio: AudioConfig,
}

impl Default for Config {
    fn default() -> Self {
        let output_dir = directories::UserDirs::new()
            .and_then(|d| d.video_dir().map(|v| v.join("Rewind")))
            .unwrap_or_else(|| PathBuf::from("Rewind"));
        Self {
            capture_mode: CaptureMode::Desktop,
            monitor_index: 0,
            display_method: DisplayMethod::Dxgi,
            fps: 60,
            resolution: Resolution::P1080,
            codec: Codec::H264,
            quality: Quality::Balanced,
            bitrate_kbps: 20_000,
            buffer_secs: 60,
            buffer_max_mb: 2048,
            container: Container::Mp4,
            capture_cursor: true,
            output_dir,
            save_hotkey: "Ctrl+Shift+F10".parse().expect("valid default hotkey"),
            toggle_hotkey: "Ctrl+Shift+F9".parse().expect("valid default hotkey"),
            start_buffer_on_launch: true,
            overlay_enabled: true,
            overlay_corner: Corner::TopRight,
            overlay_secs: 3,
            sound_on_save: true,
            close_to_tray: true,
            start_minimized: false,
            launch_at_login: false,
            animations: true,
            smooth_scroll: true,
            effects: EffectStyle::Shine,
            low_power_ui: false,
            thumbnails: true,
            theme: "default".into(),
            voice: VoiceConfig::default(),
            audio: AudioConfig::default(),
        }
    }
}

pub const FPS_CHOICES: [u32; 5] = [30, 60, 120, 144, 240];

/// Display label for each entry in [`FPS_CHOICES`], in the same order. Static so the
/// settings page can draw a segmented control without allocating.
pub const FPS_LABELS: [&str; FPS_CHOICES.len()] = ["30", "60", "120", "144", "240"];

impl Config {
    /// Clamp every value into a range the engine can actually use. Called after loading
    /// from disk and after every UI edit so hand-edited files can't wedge the app.
    pub fn sanitize(&mut self) {
        if !FPS_CHOICES.contains(&self.fps) {
            self.fps = *FPS_CHOICES
                .iter()
                .min_by_key(|f| (**f as i64 - self.fps as i64).abs())
                .unwrap_or(&60);
        }
        self.bitrate_kbps = self.bitrate_kbps.clamp(1_000, 150_000);
        self.buffer_secs = self.buffer_secs.clamp(5, 1_800);
        self.buffer_max_mb = self.buffer_max_mb.clamp(128, 32_768);
        self.overlay_secs = self.overlay_secs.clamp(1, 10);
        self.voice.sensitivity = self.voice.sensitivity.clamp(0.0, 1.0);
        if self.voice.phrase.trim().is_empty() {
            self.voice.phrase = VoiceConfig::default().phrase;
        }
        if self.theme.trim().is_empty() {
            self.theme = "default".into();
        }
        self.audio.desktop_volume = self.audio.desktop_volume.clamp(0.0, 2.0);
        self.audio.mic_volume = self.audio.mic_volume.clamp(0.0, 4.0);
        self.audio.bitrate_kbps = self.audio.bitrate_kbps.clamp(64, 320);
        if self.save_hotkey == self.toggle_hotkey {
            self.toggle_hotkey = Config::default().toggle_hotkey;
        }
    }

    /// Overwrite the performance-related fields with a preset. Leaves hotkeys, folders,
    /// theme and notification choices alone.
    pub fn apply_preset(&mut self, preset: Preset) {
        match preset {
            Preset::LowEnd => {
                self.resolution = Resolution::P720;
                self.fps = 30;
                self.codec = Codec::H264;
                self.quality = Quality::Speed;
                self.bitrate_kbps = 5_000;
                self.buffer_secs = 30;
                self.audio.mic_enabled = false;
                self.audio.bitrate_kbps = 96;
                self.animations = false;
                self.smooth_scroll = false;
                self.effects = EffectStyle::None;
                self.low_power_ui = true;
                self.thumbnails = false;
            }
            Preset::Balanced => {
                self.resolution = Resolution::P1080;
                self.fps = 60;
                self.codec = Codec::H264;
                self.quality = Quality::Balanced;
                self.bitrate_kbps = 16_000;
                self.buffer_secs = 60;
                self.audio.bitrate_kbps = 160;
                self.animations = true;
                self.smooth_scroll = true;
                self.effects = EffectStyle::Shine;
                self.low_power_ui = false;
                self.thumbnails = true;
            }
            Preset::HighQuality => {
                self.resolution = Resolution::Native;
                self.fps = 60;
                self.codec = Codec::H264;
                self.quality = Quality::Quality;
                self.bitrate_kbps = 40_000;
                self.buffer_secs = 90;
                self.audio.bitrate_kbps = 192;
                self.animations = true;
                self.smooth_scroll = true;
                self.effects = EffectStyle::Shine;
                self.low_power_ui = false;
                self.thumbnails = true;
            }
        }
        self.sanitize();
    }

    /// True if switching from `self` to `next` needs the capture graph rebuilt. Volumes,
    /// hotkeys and the start-on-launch flag can change without touching the recording.
    pub fn requires_engine_rebuild(&self, next: &Config) -> bool {
        let mut same = next.clone();
        same.audio.desktop_volume = self.audio.desktop_volume;
        same.audio.mic_volume = self.audio.mic_volume;
        same.save_hotkey = self.save_hotkey.clone();
        same.toggle_hotkey = self.toggle_hotkey.clone();
        same.start_buffer_on_launch = self.start_buffer_on_launch;
        same.overlay_enabled = self.overlay_enabled;
        same.overlay_corner = self.overlay_corner;
        same.overlay_secs = self.overlay_secs;
        same.sound_on_save = self.sound_on_save;
        same.close_to_tray = self.close_to_tray;
        same.start_minimized = self.start_minimized;
        same.launch_at_login = self.launch_at_login;
        same.animations = self.animations;
        same.smooth_scroll = self.smooth_scroll;
        same.effects = self.effects;
        same.low_power_ui = self.low_power_ui;
        same.thumbnails = self.thumbnails;
        same.theme = self.theme.clone();
        same.voice = self.voice.clone();
        *self != same
    }

    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "rewind", "Rewind")
            .map(|d| d.config_dir().join("config.toml"))
    }

    pub fn load_from(path: &Path) -> Config {
        let mut cfg = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str::<Config>(&s).ok())
            .unwrap_or_default();
        cfg.sanitize();
        cfg
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        // Write-then-rename so a crash mid-save can't leave a half-written config.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_survive_round_trip() {
        let dir = std::env::temp_dir().join(format!("rewind-cfg-{}", std::process::id()));
        let path = dir.join("config.toml");
        let cfg = Config::default();
        cfg.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path), cfg);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_or_garbage_file_gives_defaults() {
        let dir = std::env::temp_dir().join(format!("rewind-cfg-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        assert_eq!(Config::load_from(&path), Config::default());
        std::fs::write(&path, "this is [[ not toml").unwrap();
        assert_eq!(Config::load_from(&path), Config::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn partial_file_fills_in_defaults() {
        let dir = std::env::temp_dir().join(format!("rewind-cfg-part-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "fps = 144\nbuffer_secs = 90\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.fps, 144);
        assert_eq!(cfg.buffer_secs, 90);
        assert_eq!(cfg.codec, Codec::H264);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn old_config_files_without_new_fields_still_load() {
        let dir = std::env::temp_dir().join(format!("rewind-cfg-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "fps = 60\ncodec = \"hevc\"\n").unwrap();
        let cfg = Config::load_from(&path);
        assert!(cfg.overlay_enabled && cfg.close_to_tray);
        assert_eq!(cfg.overlay_corner, Corner::TopRight);
        assert_eq!(cfg.codec, Codec::Hevc);
        assert_eq!(cfg.display_method, DisplayMethod::Dxgi);
        assert!(cfg.smooth_scroll);
        assert_eq!(cfg.effects, EffectStyle::Shine);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn presets_scale_sensibly_and_stay_valid() {
        let mut low = Config::default();
        low.apply_preset(Preset::LowEnd);
        let mut bal = Config::default();
        bal.apply_preset(Preset::Balanced);
        let mut high = Config::default();
        high.apply_preset(Preset::HighQuality);
        assert!(low.bitrate_kbps < bal.bitrate_kbps && bal.bitrate_kbps < high.bitrate_kbps);
        assert!(low.fps < bal.fps);
        assert!(!low.animations && !low.smooth_scroll && low.effects == EffectStyle::None && low.low_power_ui && !low.thumbnails);
        assert!(bal.animations && bal.smooth_scroll && bal.effects == EffectStyle::Shine && !bal.low_power_ui);
        for c in [&low, &bal, &high] {
            let mut copy = (*c).clone();
            copy.sanitize();
            assert_eq!(&copy, c, "presets must already be sanitized");
        }
    }

    #[test]
    fn presets_keep_personal_settings() {
        let mut c = Config::default();
        c.theme = "midnight".into();
        c.overlay_corner = Corner::BottomLeft;
        c.apply_preset(Preset::LowEnd);
        assert_eq!(c.theme, "midnight");
        assert_eq!(c.overlay_corner, Corner::BottomLeft);
    }

    #[test]
    fn theme_and_voice_changes_do_not_rebuild_the_capture_graph() {
        let base = Config::default();
        let mut next = base.clone();
        next.theme = "ember".into();
        next.animations = false;
        next.smooth_scroll = false;
        next.effects = EffectStyle::Creeper;
        next.voice.enabled = true;
        next.voice.phrase = "save it".into();
        assert!(!base.requires_engine_rebuild(&next));
    }

    #[test]
    fn capture_method_change_rebuilds_the_engine() {
        let base = Config::default();
        let mut next = base.clone();
        next.display_method = DisplayMethod::Wgc;
        assert!(base.requires_engine_rebuild(&next));
    }

    #[test]
    fn empty_phrase_and_theme_fall_back() {
        let mut c = Config::default();
        c.voice.phrase = "  ".into();
        c.theme = String::new();
        c.sanitize();
        assert_eq!(c.voice.phrase, "clip that");
        assert_eq!(c.theme, "default");
    }

    #[test]
    fn overlay_duration_is_clamped() {
        let mut cfg = Config::default();
        cfg.overlay_secs = 0;
        cfg.sanitize();
        assert_eq!(cfg.overlay_secs, 1);
        cfg.overlay_secs = 99;
        cfg.sanitize();
        assert_eq!(cfg.overlay_secs, 10);
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut cfg = Config::default();
        cfg.fps = 100;
        cfg.bitrate_kbps = 5;
        cfg.buffer_secs = 99_999;
        cfg.audio.mic_volume = 99.0;
        cfg.toggle_hotkey = cfg.save_hotkey.clone();
        cfg.sanitize();
        assert_eq!(cfg.fps, 120);
        assert_eq!(cfg.bitrate_kbps, 1_000);
        assert_eq!(cfg.buffer_secs, 1_800);
        assert_eq!(cfg.audio.mic_volume, 4.0);
        assert_ne!(cfg.toggle_hotkey, cfg.save_hotkey);
    }

    #[test]
    fn volume_hotkey_and_launch_changes_do_not_rebuild() {
        let base = Config::default();
        let mut next = base.clone();
        next.audio.desktop_volume = 0.5;
        next.audio.mic_volume = 2.0;
        next.save_hotkey = "Ctrl+Alt+F8".parse().unwrap();
        next.start_buffer_on_launch = !base.start_buffer_on_launch;
        next.overlay_corner = Corner::BottomLeft;
        next.overlay_secs = 6;
        next.sound_on_save = !base.sound_on_save;
        next.close_to_tray = !base.close_to_tray;
        next.launch_at_login = true;
        assert!(!base.requires_engine_rebuild(&next));
    }

    #[test]
    fn video_audio_source_and_folder_changes_rebuild() {
        let base = Config::default();
        for edit in [
            (|c: &mut Config| c.fps = 144) as fn(&mut Config),
            |c| c.codec = Codec::Hevc,
            |c| c.audio.mic_enabled = !c.audio.mic_enabled,
            |c| c.buffer_secs += 30,
            |c| c.output_dir = PathBuf::from("elsewhere"),
            |c| c.capture_mode = CaptureMode::GameOnly,
        ] {
            let mut next = base.clone();
            edit(&mut next);
            assert!(base.requires_engine_rebuild(&next));
        }
    }

    #[test]
    fn output_size_never_upscales_and_stays_even() {
        assert_eq!(output_size(2560, 1440, Resolution::P1080), (1920, 1080));
        assert_eq!(output_size(1920, 1080, Resolution::P2160), (1920, 1080));
        assert_eq!(output_size(3440, 1440, Resolution::P1080), (2580, 1080));
        assert_eq!(output_size(1366, 768, Resolution::Native), (1366, 768));
        let (w, h) = output_size(1919, 1081, Resolution::P720);
        assert!(w % 2 == 0 && h % 2 == 0);
    }
}
