//! Optional "clip that" voice trigger.
//!
//! Nothing here ships in the installer: the Vosk speech runtime (libvosk.dll) and a small
//! language model are downloaded on demand into the add-ons folder and loaded at run time. A
//! noise gate keeps the recogniser idle during silence, so listening costs almost no CPU.
//! Speech is recognised on this PC; no audio ever leaves it.

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use libloading::Library;
use rewind_core::config::VoiceConfig;
use rewind_core::voice::{grammar_json, parse_result, PhraseMatcher, VoiceGate};
use wasapi::{initialize_mta, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

const SAMPLE_RATE: usize = 16_000;
const CHUNK_FRAMES: usize = 1_600; // 100 ms

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceStatus {
    Starting,
    Listening,
    Error(String),
    Stopped,
}

pub struct VoiceShared {
    pub stop: AtomicBool,
    level_bits: AtomicU32,
    pub status: Mutex<VoiceStatus>,
}

impl VoiceShared {
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed))
    }
    pub fn status(&self) -> VoiceStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or(VoiceStatus::Stopped)
    }
    fn set_status(&self, s: VoiceStatus) {
        if let Ok(mut g) = self.status.lock() {
            *g = s;
        }
    }
}

pub struct VoiceHandle {
    pub shared: Arc<VoiceShared>,
    triggers: Receiver<()>,
}

impl VoiceHandle {
    /// True once per recognised trigger phrase.
    pub fn poll(&self) -> bool {
        self.triggers.try_iter().count() > 0
    }
}

impl Drop for VoiceHandle {
    fn drop(&mut self) {
        // The worker notices within ~0.5 s; we deliberately don't block the UI waiting for it.
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

struct Vosk {
    _lib: Library,
    set_log_level: unsafe extern "C" fn(c_int),
    model_new: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    model_free: unsafe extern "C" fn(*mut c_void),
    rec_new_grm: unsafe extern "C" fn(*mut c_void, f32, *const c_char) -> *mut c_void,
    rec_free: unsafe extern "C" fn(*mut c_void),
    accept: unsafe extern "C" fn(*mut c_void, *const i16, c_int) -> c_int,
    result: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    partial: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    reset: unsafe extern "C" fn(*mut c_void),
}

impl Vosk {
    fn load(runtime_dir: &Path) -> Result<Vosk, String> {
        let dll = runtime_dir.join("libvosk.dll");
        if !dll.is_file() {
            return Err("the voice add-on isn't installed".into());
        }
        unsafe {
            // 0x8 = LOAD_WITH_ALTERED_SEARCH_PATH, so libvosk finds its sibling DLLs.
            let lib: Library = libloading::os::windows::Library::load_with_flags(&dll, 0x8).map_err(|e| format!("couldn't load libvosk.dll: {e}"))?.into();
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("libvosk.dll is missing {}: {e}", $name))?
                };
            }
            Ok(Vosk {
                set_log_level: sym!("vosk_set_log_level"),
                model_new: sym!("vosk_model_new"),
                model_free: sym!("vosk_model_free"),
                rec_new_grm: sym!("vosk_recognizer_new_grm"),
                rec_free: sym!("vosk_recognizer_free"),
                accept: sym!("vosk_recognizer_accept_waveform_s"),
                result: sym!("vosk_recognizer_result"),
                partial: sym!("vosk_recognizer_partial_result"),
                reset: sym!("vosk_recognizer_reset"),
                _lib: lib,
            })
        }
    }
}

fn c_str(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() } }
}

/// Start listening on a background thread. `runtime_dir` holds libvosk.dll, `model_dir` the
/// unpacked language model.
pub fn start(cfg: &VoiceConfig, runtime_dir: PathBuf, model_dir: PathBuf, ctx: egui::Context) -> VoiceHandle {
    let shared = Arc::new(VoiceShared { stop: AtomicBool::new(false), level_bits: AtomicU32::new(0), status: Mutex::new(VoiceStatus::Starting) });
    let (tx, rx) = channel();
    let (cfg, sh) = (cfg.clone(), shared.clone());
    std::thread::Builder::new()
        .name("rewind-voice".into())
        .spawn(move || {
            let result = run(&cfg, &runtime_dir, &model_dir, &sh, &tx, &ctx);
            sh.set_status(match result {
                Ok(()) => VoiceStatus::Stopped,
                Err(e) => VoiceStatus::Error(e),
            });
            ctx.request_repaint();
        })
        .ok();
    VoiceHandle { shared, triggers: rx }
}

fn run(cfg: &VoiceConfig, runtime_dir: &Path, model_dir: &Path, sh: &Arc<VoiceShared>, tx: &std::sync::mpsc::Sender<()>, ctx: &egui::Context) -> Result<(), String> {
    let vosk = Vosk::load(runtime_dir)?;
    unsafe { (vosk.set_log_level)(-1) };
    let model_path = CString::new(model_dir.to_string_lossy().as_bytes()).map_err(|_| "bad model path")?;
    let model = unsafe { (vosk.model_new)(model_path.as_ptr()) };
    if model.is_null() {
        return Err("couldn't load the language model (re-download it from Add-ons)".into());
    }
    let grammar = CString::new(grammar_json(&cfg.phrase)).map_err(|_| "bad phrase")?;
    let rec = unsafe { (vosk.rec_new_grm)(model, SAMPLE_RATE as f32, grammar.as_ptr()) };
    if rec.is_null() {
        unsafe { (vosk.model_free)(model) };
        return Err("couldn't start the recogniser".into());
    }
    let outcome = listen(&vosk, rec, cfg, sh, tx, ctx);
    unsafe {
        (vosk.rec_free)(rec);
        (vosk.model_free)(model);
    }
    outcome
}

fn listen(vosk: &Vosk, rec: *mut c_void, cfg: &VoiceConfig, sh: &Arc<VoiceShared>, tx: &std::sync::mpsc::Sender<()>, ctx: &egui::Context) -> Result<(), String> {
    initialize_mta().ok().map_err(|e| format!("audio init: {e}"))?;
    let enumerator = DeviceEnumerator::new().map_err(|e| format!("audio devices: {e}"))?;
    let device = enumerator.get_default_device(Direction::Capture).map_err(|_| "no microphone found".to_string())?;
    let mut client = device.get_iaudioclient().map_err(|e| format!("microphone: {e}"))?;
    // Ask Windows to hand us 16 kHz mono 16-bit directly (it converts for us).
    let format = WaveFormat::new(16, 16, SampleType::Int, SAMPLE_RATE, 1, None);
    let (_, min_period) = client.get_device_period().map_err(|e| format!("microphone: {e}"))?;
    client
        .initialize_client(&format, Direction::Capture, StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: min_period }, None)
        .map_err(|e| format!("couldn't open the microphone: {e}"))?;
    let event = client.set_get_eventhandle().map_err(|e| format!("microphone: {e}"))?;
    let capture = client.get_audiocaptureclient().map_err(|e| format!("microphone: {e}"))?;
    let mut queue: VecDeque<u8> = VecDeque::with_capacity(SAMPLE_RATE * 2);
    client.start_stream().map_err(|e| format!("microphone: {e}"))?;

    sh.set_status(VoiceStatus::Listening);
    ctx.request_repaint();

    let mut gate = VoiceGate::new(SAMPLE_RATE as u32, cfg.sensitivity);
    let mut matcher = PhraseMatcher::new(&cfg.phrase, 3_000);
    let started = Instant::now();
    let chunk_bytes = CHUNK_FRAMES * 2;
    let mut was_open = false;

    while !sh.stop.load(Ordering::Relaxed) {
        capture.read_from_device_to_deque(&mut queue).map_err(|e| format!("microphone: {e}"))?;
        while queue.len() >= chunk_bytes {
            let samples: Vec<i16> = (0..CHUNK_FRAMES)
                .map(|_| {
                    let lo = queue.pop_front().unwrap_or(0);
                    let hi = queue.pop_front().unwrap_or(0);
                    i16::from_le_bytes([lo, hi])
                })
                .collect();
            let audio = gate.push(&samples);
            sh.level_bits.store(gate.level().to_bits(), Ordering::Relaxed);
            if let Some(audio) = audio {
                let accepted = unsafe { (vosk.accept)(rec, audio.as_ptr(), audio.len() as c_int) };
                let json = unsafe { c_str(if accepted == 1 { (vosk.result)(rec) } else { (vosk.partial)(rec) }) };
                if let Some(r) = parse_result(&json) {
                    if matcher.matches(&r.text, started.elapsed().as_millis() as u64) {
                        let _ = tx.send(());
                        ctx.request_repaint();
                        unsafe { (vosk.reset)(rec) };
                    }
                }
            }
            if was_open && !gate.is_open() {
                unsafe { (vosk.reset)(rec) }; // speech ended; start the next utterance clean
            }
            was_open = gate.is_open();
        }
        if event.wait_for_event(500).is_err() {
            // No audio for half a second (device idle): just check the stop flag again.
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let _ = client.stop_stream();
    Ok(())
}
