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
use rewind_core::i18n::{default_phrase, Lang};
use rewind_core::voice::{grammar_json, grammar_phrase, parse_result, PhraseMatcher, VoiceGate};
use wasapi::{initialize_mta, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

const SAMPLE_RATE: usize = 16_000;
const CHUNK_FRAMES: usize = 1_600; // 100 ms

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceStatus {
    Starting,
    Listening,
    /// The model loaded, but it has no such words in its vocabulary, so the configured
    /// phrase can never be heard. The message suggests the language's default phrase.
    Phrase(PhraseWarning),
    Error(String),
    Stopped,
}

/// A trigger phrase the installed model cannot hear, with the words it is missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhraseWarning {
    pub lang: Lang,
    pub missing: Vec<String>,
    pub suggestion: &'static str,
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
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(VoiceStatus::Stopped)
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
    /// Optional: lets us check a phrase against the model's vocabulary before listening.
    find_word: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int>,
    rec_new: unsafe extern "C" fn(*mut c_void, f32) -> *mut c_void,
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
            let lib: Library = libloading::os::windows::Library::load_with_flags(&dll, 0x8)
                .map_err(|e| format!("couldn't load libvosk.dll: {e}"))?
                .into();
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes())
                        .map_err(|e| format!("libvosk.dll is missing {}: {e}", $name))?
                };
            }
            Ok(Vosk {
                set_log_level: sym!("vosk_set_log_level"),
                model_new: sym!("vosk_model_new"),
                model_free: sym!("vosk_model_free"),
                find_word: lib
                    .get(b"vosk_model_find_word\0")
                    .ok()
                    .map(|symbol| *symbol),
                rec_new: sym!("vosk_recognizer_new"),
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
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
    }
}

/// The 8.3 short form of a path. Vosk opens its model files through the C library, which
/// on some Windows 10 machines cannot handle non-ASCII paths (a Turkish user name, for
/// example). The short form is always plain ASCII, so the model loads everywhere.
fn short_path(path: &Path) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut buf = vec![0u16; 32_768];
    let n = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    if n == 0 || n as usize >= buf.len() {
        return path.to_path_buf();
    }
    PathBuf::from(std::ffi::OsString::from_wide(&buf[..n as usize]))
}

/// Open the configured microphone, or the Windows default when `device_id` is empty.
fn open_capture_device(
    enumerator: &DeviceEnumerator,
    device_id: &str,
) -> Result<wasapi::Device, String> {
    if device_id.trim().is_empty() {
        return enumerator
            .get_default_device(Direction::Capture)
            .map_err(|_| "no microphone found".to_string());
    }
    let collection = enumerator
        .get_device_collection(Direction::Capture)
        .map_err(|e| format!("audio devices: {e}"))?;
    for device in &collection {
        let Ok(device) = device else { continue };
        if device.get_id().ok().as_deref() == Some(device_id) {
            return Ok(device);
        }
    }
    // The chosen mic was unplugged: fall back to the default instead of going silent.
    enumerator
        .get_default_device(Direction::Capture)
        .map_err(|_| "no microphone found".to_string())
}

/// Start listening on a background thread. `runtime_dir` holds libvosk.dll, `model_dir` the
/// unpacked language model, `device_id` the chosen microphone (empty = Windows default).
pub fn start(
    cfg: &VoiceConfig,
    runtime_dir: PathBuf,
    model_dir: PathBuf,
    device_id: String,
    ctx: egui::Context,
) -> VoiceHandle {
    let shared = Arc::new(VoiceShared {
        stop: AtomicBool::new(false),
        level_bits: AtomicU32::new(0),
        status: Mutex::new(VoiceStatus::Starting),
    });
    let (tx, rx) = channel();
    let (cfg, sh) = (cfg.clone(), shared.clone());
    std::thread::Builder::new()
        .name("rewind-voice".into())
        .spawn(move || {
            let result = run(&cfg, &runtime_dir, &model_dir, &device_id, &sh, &tx, &ctx);
            // `run` may already have replaced the status with a phrase warning, which is
            // more useful than "Stopped" and must not trigger the automatic retry loop.
            if !matches!(sh.status(), VoiceStatus::Phrase(_)) {
                sh.set_status(match result {
                    Ok(()) => VoiceStatus::Stopped,
                    Err(e) => VoiceStatus::Error(e),
                });
            }
            ctx.request_repaint();
        })
        .ok();
    VoiceHandle {
        shared,
        triggers: rx,
    }
}

fn run(
    cfg: &VoiceConfig,
    runtime_dir: &Path,
    model_dir: &Path,
    device_id: &str,
    sh: &Arc<VoiceShared>,
    tx: &std::sync::mpsc::Sender<()>,
    ctx: &egui::Context,
) -> Result<(), String> {
    let vosk = Vosk::load(runtime_dir)?;
    unsafe { (vosk.set_log_level)(-1) };
    let model_short = short_path(model_dir);
    let model_path =
        CString::new(model_short.to_string_lossy().as_bytes()).map_err(|_| "bad model path")?;
    let model = unsafe { (vosk.model_new)(model_path.as_ptr()) };
    if model.is_null() {
        return Err("couldn't load the language model (re-download it from Add-ons)".into());
    }

    // A grammar only helps when the model actually knows every word. The Turkish small
    // model, for example, has "klip" and "al" but not "clip" or "that", so a phrase left
    // over from the English model would otherwise silently never match.
    let lang = Lang::from_id(&cfg.language);
    let turkish = lang == Lang::Turkish;
    let words = grammar_phrase(&cfg.phrase, turkish);
    if words.is_empty() {
        unsafe { (vosk.model_free)(model) };
        return Err("the trigger phrase is empty".into());
    }
    if let Some(find_word) = vosk.find_word {
        let mut missing: Vec<String> = Vec::new();
        for word in words.split(' ') {
            let Ok(word_c) = CString::new(word) else {
                continue;
            };
            if unsafe { find_word(model, word_c.as_ptr()) } < 0 {
                missing.push(word.to_string());
            }
        }
        if !missing.is_empty() {
            sh.set_status(VoiceStatus::Phrase(PhraseWarning {
                lang,
                missing,
                suggestion: default_phrase(lang),
            }));
            unsafe { (vosk.model_free)(model) };
            return Ok(());
        }
    }

    let grammar = CString::new(grammar_json(&cfg.phrase, turkish)).map_err(|_| "bad phrase")?;
    let mut rec = unsafe { (vosk.rec_new_grm)(model, SAMPLE_RATE as f32, grammar.as_ptr()) };
    if rec.is_null() {
        // A model without grammar support still recognises free speech; the phrase matcher
        // then has to pick the trigger out of everything that was said.
        rec = unsafe { (vosk.rec_new)(model, SAMPLE_RATE as f32) };
    }
    if rec.is_null() {
        unsafe { (vosk.model_free)(model) };
        return Err("couldn't start the recogniser".into());
    }
    let outcome = listen(&vosk, rec, cfg, device_id, sh, tx, ctx);
    unsafe {
        (vosk.rec_free)(rec);
        (vosk.model_free)(model);
    }
    outcome
}

fn listen(
    vosk: &Vosk,
    rec: *mut c_void,
    cfg: &VoiceConfig,
    device_id: &str,
    sh: &Arc<VoiceShared>,
    tx: &std::sync::mpsc::Sender<()>,
    ctx: &egui::Context,
) -> Result<(), String> {
    initialize_mta()
        .ok()
        .map_err(|e| format!("audio init: {e}"))?;
    let enumerator = DeviceEnumerator::new().map_err(|e| format!("audio devices: {e}"))?;
    let device = open_capture_device(&enumerator, device_id)?;
    let mut client = device
        .get_iaudioclient()
        .map_err(|e| format!("microphone: {e}"))?;
    // Ask Windows to hand us 16 kHz mono 16-bit directly (it converts for us).
    let format = WaveFormat::new(16, 16, SampleType::Int, SAMPLE_RATE, 1, None);
    let (_, min_period) = client
        .get_device_period()
        .map_err(|e| format!("microphone: {e}"))?;
    client
        .initialize_client(
            &format,
            Direction::Capture,
            StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: min_period,
            },
            None,
        )
        .map_err(|e| format!("couldn't open the microphone: {e}"))?;
    let event = client
        .set_get_eventhandle()
        .map_err(|e| format!("microphone: {e}"))?;
    let capture = client
        .get_audiocaptureclient()
        .map_err(|e| format!("microphone: {e}"))?;
    let mut queue: VecDeque<u8> = VecDeque::with_capacity(SAMPLE_RATE * 2);
    client
        .start_stream()
        .map_err(|e| format!("microphone: {e}"))?;

    sh.set_status(VoiceStatus::Listening);
    ctx.request_repaint();

    let mut gate = VoiceGate::new(SAMPLE_RATE as u32, cfg.sensitivity);
    let mut matcher = PhraseMatcher::new(&cfg.phrase, 3_000);
    let started = Instant::now();
    let chunk_bytes = CHUNK_FRAMES * 2;
    let mut was_open = false;
    // Reused for every chunk instead of allocating a fresh Vec several times a second.
    let mut samples: Vec<i16> = Vec::with_capacity(CHUNK_FRAMES);

    while !sh.stop.load(Ordering::Relaxed) {
        capture
            .read_from_device_to_deque(&mut queue)
            .map_err(|e| format!("microphone: {e}"))?;
        while queue.len() >= chunk_bytes {
            samples.clear();
            let mut bytes = queue.drain(..chunk_bytes);
            while let (Some(lo), Some(hi)) = (bytes.next(), bytes.next()) {
                samples.push(i16::from_le_bytes([lo, hi]));
            }
            drop(bytes);
            let audio = gate.push(&samples);
            sh.level_bits
                .store(gate.level().to_bits(), Ordering::Relaxed);
            if let Some(audio) = audio {
                let accepted = unsafe { (vosk.accept)(rec, audio.as_ptr(), audio.len() as c_int) };
                let json = unsafe {
                    c_str(if accepted == 1 {
                        (vosk.result)(rec)
                    } else {
                        (vosk.partial)(rec)
                    })
                };
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
