//! Capture engine: a thin, opinionated layer over libobs (via libobs-rs).
//!
//! Layout of the OBS scene we build:
//!   [bottom] monitor capture  (Desktop mode only)
//!   [top]    game capture     (hooks any fullscreen game and covers the monitor layer)
//!   audio:   desktop output + optional microphone, mixed to one track
//! The replay buffer output encodes that scene into a rolling in-RAM buffer.

use std::any::Any;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use libobs_simple::output::replay::ObsContextReplayExt;
use libobs_simple::output::simple::{HardwareCodec, HardwarePreset, X264Preset};
use libobs_simple::sources::windows::{
    GameCaptureSourceBuilder, MonitorCaptureSourceBuilder, ObsDisplayCaptureMethod,
    ObsGameCaptureMode,
};
use libobs_wrapper::context::ObsContext;
use libobs_wrapper::data::audio::LinearVolume;
use libobs_wrapper::data::output::{ObsOutputTrait, ObsReplayBufferOutputRef};
use libobs_wrapper::data::video::ObsVideoInfoBuilder;
use libobs_wrapper::data::ObsDataSetters;
use libobs_wrapper::enums::ObsScaleType;
use libobs_wrapper::scenes::ObsSceneRef;
use libobs_wrapper::sources::{ObsSourceAudioExt, ObsSourceBuilder, ObsSourceRef};
use libobs_wrapper::utils::{ObsError, ObsPath, StartupInfo};

use rewind_core::config::{
    output_size, AudioConfig, CaptureMode, Codec, Config, DisplayMethod, Quality,
};

pub type EngineResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(what: &str) -> impl FnOnce(E) -> String + '_ {
    move |e| format!("{what}: {e}")
}

pub struct MonitorInfo {
    pub label: String,
    pub width: u32,
    pub height: u32,
}

/// List monitors for the settings dropdown. Safe to call before the engine exists.
pub fn list_monitors() -> Vec<MonitorInfo> {
    MonitorCaptureSourceBuilder::get_monitors()
        .map(|all| {
            all.iter()
                .enumerate()
                .map(|(i, m)| MonitorInfo {
                    label: format!(
                        "{}{} ({}×{})",
                        if m.friendly_name.is_empty() {
                            format!("Display {}", i + 1)
                        } else {
                            m.friendly_name.clone()
                        },
                        if m.is_primary { " — primary" } else { "" },
                        m.width,
                        m.height
                    ),
                    width: m.width,
                    height: m.height,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub struct Engine {
    context: ObsContext,
    replay: Option<ObsReplayBufferOutputRef>,
    // Keep scene, sources and scene items alive for as long as the buffer runs.
    scene: Option<ObsSceneRef>,
    audio_sources: Vec<(AudioKind, ObsSourceRef)>,
    _scene_items: Vec<Box<dyn Any>>,
    generation: u32,
    /// Where OBS writes clips, and the file extension it uses. Kept here so a save can
    /// recognise the finished file even when OBS's completion signal is missed.
    output_dir: PathBuf,
    extension: String,
    /// Human-readable description of what was actually set up (encoder fallbacks etc.).
    pub notes: Vec<String>,
    /// True when video is being encoded on the CPU (x264), far heavier than GPU encoding.
    pub cpu_encoding: bool,
    /// What is really encoding the video, e.g. "H.264 · CPU" or "HEVC · GPU".
    pub encoder_label: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AudioKind {
    Desktop,
    Mic,
}

fn hardware_codec(c: Codec) -> HardwareCodec {
    match c {
        Codec::H264 => HardwareCodec::H264,
        Codec::Hevc => HardwareCodec::HEVC,
        Codec::Av1 => HardwareCodec::AV1,
    }
}

fn hardware_preset(q: Quality) -> HardwarePreset {
    match q {
        Quality::Speed => HardwarePreset::Speed,
        Quality::Balanced => HardwarePreset::Balanced,
        Quality::Quality => HardwarePreset::Quality,
    }
}

fn display_method(m: DisplayMethod) -> ObsDisplayCaptureMethod {
    match m {
        DisplayMethod::Dxgi => ObsDisplayCaptureMethod::MethodDXGI,
        DisplayMethod::Wgc => ObsDisplayCaptureMethod::MethodWgc,
    }
}

/// Video settings for libobs. Note the scaler: libobs-rs defaults to Lanczos, the most
/// expensive filter, which is wasteful for a background recorder. Bicubic is plenty for
/// normal PCs and bilinear is the cheapest option for the low-power mode.
fn video_info(cfg: &Config) -> libobs_wrapper::data::video::ObsVideoInfo {
    let (bw, bh) = base_resolution(cfg);
    let (ow, oh) = output_size(bw, bh, cfg.resolution);
    ObsVideoInfoBuilder::new()
        .fps_num(cfg.fps)
        .fps_den(1)
        .base_width(bw)
        .base_height(bh)
        .output_width(ow)
        .output_height(oh)
        .scale_type(if cfg.low_power_ui {
            ObsScaleType::Bilinear
        } else {
            ObsScaleType::Bicubic
        })
        .build()
}

fn base_resolution(cfg: &Config) -> (u32, u32) {
    let monitors = list_monitors();
    monitors
        .get(cfg.monitor_index)
        .or_else(|| monitors.first())
        .map(|m| (m.width, m.height))
        .unwrap_or((1920, 1080))
}

impl Engine {
    /// Start libobs and build the first scene + replay buffer from `cfg`.
    /// The OBS runtime must already be on disk (see `runtime::ensure_obs`).
    pub fn start(cfg: &Config, module_config_dir: &Path) -> EngineResult<Engine> {
        std::fs::create_dir_all(module_config_dir).map_err(err("creating OBS config dir"))?;
        let video = video_info(cfg);
        let context = StartupInfo::new()
            .set_video_info(video)
            .set_module_config_path(ObsPath::new(module_config_dir.to_string_lossy().as_ref()))
            .start()
            .map_err(err("starting OBS"))?;
        let mut engine = Engine {
            context,
            replay: None,
            scene: None,
            audio_sources: Vec::new(),
            _scene_items: Vec::new(),
            generation: 0,
            output_dir: cfg.output_dir.clone(),
            extension: cfg.container.extension().to_string(),
            notes: Vec::new(),
            cpu_encoding: false,
            encoder_label: String::new(),
        };
        engine.build(cfg)?;
        Ok(engine)
    }

    /// Tear down the output and scene (keeping the OBS context alive) and rebuild them from
    /// `cfg`. Used when the user changes resolution, FPS, codec, sources and so on.
    pub fn apply(&mut self, cfg: &Config, was_running: bool) -> EngineResult<()> {
        if let Some(r) = &self.replay {
            if r.is_active().unwrap_or(false) {
                let _ = r.stop();
            }
        }
        self.replay = None;
        self.audio_sources.clear();
        self._scene_items.clear();
        self.scene = None;

        let video = video_info(cfg);
        self.context
            .reset_video(video)
            .map_err(err("applying video settings"))?;
        self.build(cfg)?;
        if was_running {
            self.set_buffer_active(true)?;
        }
        Ok(())
    }

    fn build(&mut self, cfg: &Config) -> EngineResult<()> {
        self.notes.clear();
        self.generation += 1;
        self.output_dir = cfg.output_dir.clone();
        self.extension = cfg.container.extension().to_string();
        let scene_name = format!("rewind-scene-{}", self.generation);
        let mut scene = self
            .context
            .scene(scene_name.as_str(), Some(0))
            .map_err(err("creating scene"))?;
        let mut items: Vec<Box<dyn Any>> = Vec::new();

        // --- Video ---
        if cfg.capture_mode == CaptureMode::Desktop {
            let monitors =
                MonitorCaptureSourceBuilder::get_monitors().map_err(err("listing monitors"))?;
            let monitor = monitors
                .get(cfg.monitor_index)
                .or_else(|| monitors.first())
                .ok_or_else(|| "no monitors found".to_string())?
                .clone();
            let methods: &[DisplayMethod] = match cfg.display_method {
                DisplayMethod::Dxgi => &[DisplayMethod::Dxgi, DisplayMethod::Wgc],
                DisplayMethod::Wgc => &[DisplayMethod::Wgc],
            };
            let mut added = false;
            let mut last_err = None;
            for (attempt, &method) in methods.iter().enumerate() {
                // Unique names keep a failed first attempt from colliding with the retry.
                let name = format!("rewind-monitor-{}-{attempt}", self.generation);
                let result = self
                    .context
                    .source_builder::<MonitorCaptureSourceBuilder, _>(name.as_str())
                    .map_err(err("creating monitor source"))
                    .and_then(|b| {
                        b.set_monitor(&monitor)
                            .set_capture_method(display_method(method))
                            .set_capture_cursor(cfg.capture_cursor)
                            .add_to_scene(&mut scene)
                            .map_err(err("adding monitor to scene"))
                    });
                match result {
                    Ok(item) => {
                        items.push(Box::new(item));
                        if method != cfg.display_method {
                            self.notes.push(
                                "DXGI screen capture wasn't available on this PC, so Rewind switched to Windows Graphics Capture. On Windows 10 that may draw a yellow border around the screen.".into(),
                            );
                        }
                        added = true;
                        break;
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            if !added {
                return Err(last_err.unwrap_or_else(|| "couldn't start screen capture".into()));
            }
        }

        // Game capture sits above the monitor. It draws nothing until a fullscreen game is
        // hooked, then it covers the monitor layer, so the buffer follows you into games.
        match self
            .context
            .source_builder::<GameCaptureSourceBuilder, _>("rewind-game")
            .map_err(err("creating game capture source"))
            .and_then(|b| {
                b.set_capture_mode(ObsGameCaptureMode::Any)
                    .set_capture_cursor(cfg.capture_cursor)
                    .add_to_scene(&mut scene)
                    .map_err(err("adding game capture to scene"))
            }) {
            Ok(item) => items.push(Box::new(item)),
            Err(e) if cfg.capture_mode == CaptureMode::Desktop => {
                // Desktop capture still works without the game hook.
                self.notes.push(format!(
                    "Game capture unavailable ({e}); recording the monitor only."
                ));
            }
            Err(e) => return Err(e),
        }

        // --- Audio ---
        let mut audio_sources = Vec::new();
        if cfg.audio.desktop_enabled {
            match self.create_audio(
                "wasapi_output_capture",
                "rewind-desktop-audio",
                &cfg.audio.desktop_device,
                &mut scene,
            ) {
                Ok(s) => audio_sources.push((AudioKind::Desktop, s)),
                Err(e) => self.notes.push(format!("Desktop audio unavailable: {e}")),
            }
        }
        if cfg.audio.mic_enabled {
            match self.create_audio(
                "wasapi_input_capture",
                "rewind-mic",
                &cfg.audio.mic_device,
                &mut scene,
            ) {
                Ok(s) => audio_sources.push((AudioKind::Mic, s)),
                Err(e) => self.notes.push(format!("Microphone unavailable: {e}")),
            }
        }
        for (kind, src) in &audio_sources {
            apply_volume(*kind, src, &cfg.audio);
        }

        // --- Replay buffer ---
        std::fs::create_dir_all(&cfg.output_dir).map_err(err("creating clip folder"))?;
        let replay = self.build_replay(cfg)?;

        self.scene = Some(scene);
        self.audio_sources = audio_sources;
        self._scene_items = items;
        self.replay = Some(replay);
        Ok(())
    }

    fn create_audio(
        &self,
        type_id: &str,
        name: &str,
        device: &str,
        scene: &mut ObsSceneRef,
    ) -> EngineResult<ObsSourceRef> {
        let ty = self
            .context
            .source_type(type_id)
            .map_err(err("querying sources"))?
            .ok_or_else(|| format!("OBS runtime has no '{type_id}' source"))?;
        let mut settings = ty.default_settings_mut().map_err(err("source defaults"))?;
        // OBS expects "default" for the Windows default endpoint, or a full WASAPI id.
        let device = if device.trim().is_empty() {
            "default"
        } else {
            device.trim()
        };
        settings
            .set_string("device_id", device)
            .map_err(err("source settings"))?;
        let source = self
            .context
            .create_source(&ty, name, Some(settings))
            .map_err(err("creating audio source"))?;
        scene
            .add(source.clone())
            .map_err(err("adding audio source"))?;
        Ok(source)
    }

    fn build_replay(&mut self, cfg: &Config) -> EngineResult<ObsReplayBufferOutputRef> {
        let make = |ctx: &ObsContext| {
            ctx.replay_buffer_builder(
                "rewind-replay",
                ObsPath::new(cfg.output_dir.to_string_lossy().as_ref()),
            )
            .max_time_sec(cfg.buffer_secs as i64)
            .max_size_mb(cfg.buffer_max_mb as i64)
            .format("Replay %CCYY-%MM-%DD %hh-%mm-%ss")
            .extension(cfg.container.extension())
            .video_bitrate(cfg.bitrate_kbps)
            .audio_bitrate(cfg.audio.bitrate_kbps)
        };
        self.cpu_encoding = false;
        self.encoder_label.clear();
        match make(&self.context)
            .hardware_encoder(hardware_codec(cfg.codec), hardware_preset(cfg.quality))
            .build()
        {
            Ok(r) => {
                self.encoder_label = format!("{} · GPU", cfg.codec.short_label());
                Ok(r)
            }
            Err(hw_err) => {
                // CPU encoding is normal on older machines: Arrandale-era Intel chips have no
                // Quick Sync encoder and many older AMD GPUs have no VCE. Keep the raw OBS
                // error in the log (useful for bug reports) and tell the user calmly.
                log::warn!(
                    "{} hardware encoder unavailable, falling back to x264: {hw_err}",
                    cfg.codec.label()
                );
                let preset = match cfg.quality {
                    Quality::Speed => X264Preset::UltraFast,
                    Quality::Balanced => X264Preset::SuperFast,
                    Quality::Quality => X264Preset::VeryFast,
                };
                self.cpu_encoding = true;
                self.encoder_label = "H.264 · CPU".into();
                let replay = make(&self.context)
                    .x264_encoder(preset)
                    .build()
                    .map_err(err("creating replay buffer (x264 fallback)"))?;
                if cfg.codec == Codec::H264 {
                    self.notes.push(if cfg.low_power_ui {
                        "This PC has no hardware video encoder, so Rewind records H.264 on the CPU. That is expected on older laptops, and the Low-end preset is already keeping it light.".into()
                    } else {
                        "This PC has no hardware video encoder, so Rewind records H.264 on the CPU. That is expected on older laptops and still looks good. If it feels heavy, the Low-end preset keeps it light.".into()
                    });
                } else {
                    self.notes.push(
                        "Hardware encoding for this codec isn't available on this PC, so Rewind is recording H.264 on the CPU instead. The clips play everywhere and still look good."
                            .into(),
                    );
                }
                Ok(replay)
            }
        }
    }

    pub fn set_buffer_active(&self, on: bool) -> EngineResult<()> {
        let Some(r) = &self.replay else {
            return Err("replay buffer not created".into());
        };
        let active = r.is_active().unwrap_or(false);
        match (on, active) {
            (true, false) => r.start().map_err(err("starting replay buffer")),
            (false, true) => r.stop().map_err(err("stopping replay buffer")),
            _ => Ok(()),
        }
    }

    pub fn is_buffer_active(&self) -> bool {
        self.replay
            .as_ref()
            .map(|r| r.is_active().unwrap_or(false))
            .unwrap_or(false)
    }

    /// A cheap handle that can save the buffer from another thread (saving blocks).
    pub fn saver(&self) -> Option<Saver> {
        self.replay.clone().map(|replay| Saver {
            replay,
            output_dir: self.output_dir.clone(),
            extension: self.extension.clone(),
        })
    }

    /// Live volume changes, no rebuild needed.
    pub fn set_volumes(&self, audio: &AudioConfig) {
        for (kind, src) in &self.audio_sources {
            apply_volume(*kind, src, audio);
        }
    }
}

fn apply_volume(kind: AudioKind, src: &ObsSourceRef, audio: &AudioConfig) {
    let v = match kind {
        AudioKind::Desktop => audio.desktop_volume,
        AudioKind::Mic => audio.mic_volume,
    };
    if let Ok(lv) = LinearVolume::new(v) {
        let _ = src.set_volume(lv);
    }
}

#[derive(Clone)]
pub struct Saver {
    replay: ObsReplayBufferOutputRef,
    output_dir: PathBuf,
    extension: String,
}

/// Only one save may be in flight at a time: OBS exposes a single `saved` signal and a
/// single `get_last_replay` path, with no request identity.
static SAVE_LOCK: Mutex<()> = Mutex::new(());

impl Saver {
    /// Asks OBS to write the current buffer and waits for the finished file. Call from a
    /// worker thread.
    ///
    /// OBS does not write the clip immediately: `save` only marks the current timestamp,
    /// and the mux happens on the next encoded frame. The wrapper's own completion wait is
    /// therefore easy to miss, and when it times out the file is often already on disk.
    /// We trigger the save without that wait, watch OBS's `saved` signal, and fall back to
    /// recognising the finished file by its extension and modification time.
    pub fn save(&self) -> EngineResult<PathBuf> {
        let _guard = SAVE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        if !self.replay.is_active().unwrap_or(false) {
            return Err("replay buffer is off — turn it on first".into());
        }

        let started = SystemTime::now();
        let before = self.snapshot();
        // Subscribe before asking OBS to save, so a very fast save cannot be lost.
        let saved = self
            .replay
            .replay_signals()
            .on_saved()
            .map_err(err("waiting for replay save"))?;

        // A 1 ms wait is enough to get the request into OBS; the native save itself is
        // deferred to the next encoded packet. Treat that timeout as "started", not as a
        // failure: the file is usually written a second or two later.
        match self
            .replay
            .save_buffer_with_timeout(Duration::from_millis(1))
        {
            Ok(path) => return Ok(path.to_path_buf()),
            Err(ObsError::OutputOperationTimeout { operation, .. })
                if operation == "replay-save" => {}
            Err(e) => return Err(format!("saving replay: {e}")),
        }

        let deadline = Instant::now() + Duration::from_secs(60);
        let mut candidate: Option<(PathBuf, u64)> = None;
        let mut stable = 0u8;
        loop {
            let completed = saved.try_recv().is_ok();
            if let Some(path) = self.newest_clip(started, &before) {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                if completed {
                    return Ok(path);
                }
                // The signal can be missed, so also accept a file whose size has stopped
                // growing. Three quiet polls (~300 ms) avoid returning a half-written clip.
                match &candidate {
                    Some((prev, prev_size)) if *prev == path && *prev_size == size && size > 0 => {
                        stable += 1;
                        if stable >= 3 {
                            return Ok(path);
                        }
                    }
                    _ => {
                        candidate = Some((path, size));
                        stable = 0;
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(
                    "OBS didn't finish writing the clip. The replay buffer is still on; try again in a moment. If it keeps happening, check that the clip folder is writable."
                        .into(),
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn snapshot(&self) -> HashSet<PathBuf> {
        std::fs::read_dir(&self.output_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| self.is_clip(entry))
            .map(|entry| entry.path())
            .collect()
    }

    /// Extension first (free), then the file type from the directory listing itself, so the
    /// 100 ms save poll doesn't stat every file in a big clips folder.
    fn is_clip(&self, entry: &std::fs::DirEntry) -> bool {
        Path::new(&entry.file_name())
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case(&self.extension))
            .unwrap_or(false)
            && entry.file_type().map(|t| t.is_file()).unwrap_or(false)
    }

    /// Newest clip written since the save request. Existing files are only accepted when
    /// their timestamp moved forward, so a just-finished previous clip is never returned.
    fn newest_clip(&self, started: SystemTime, before: &HashSet<PathBuf>) -> Option<PathBuf> {
        let cutoff = started
            .checked_sub(Duration::from_secs(2))
            .unwrap_or(started);
        let mut best: Option<(PathBuf, SystemTime)> = None;
        for entry in std::fs::read_dir(&self.output_dir)
            .into_iter()
            .flatten()
            .flatten()
        {
            if !self.is_clip(&entry) {
                continue;
            }
            let path = entry.path();
            let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if modified < cutoff {
                continue;
            }
            if before.contains(&path) && modified < started {
                continue;
            }
            if best
                .as_ref()
                .map(|(_, newest)| modified > *newest)
                .unwrap_or(true)
            {
                best = Some((path, modified));
            }
        }
        best.map(|(path, _)| path)
    }
}
