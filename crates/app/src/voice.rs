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
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use libloading::Library;
use rewind_core::config::VoiceConfig;
use rewind_core::i18n::{default_phrase, Lang};
use rewind_core::vocab::{plan, Vocab};
use rewind_core::voice::{
    parse_result, MicConverter, PhraseMatcher, SpeechCleaner, Verdict, VoiceGate,
};
use wasapi::{
    initialize_mta, DeviceEnumerator, Direction, Role, SampleType, StreamMode, WaveFormat,
};

/// Called the moment the phrase is recognised (on the voice thread), so the clip sound and
/// the save start right away instead of waiting for the app's window to wake up.
pub type OnTrigger = Arc<dyn Fn() + Send + Sync>;

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

/// The last finished thing the recogniser heard, for the voice test readout.
#[derive(Clone, Debug)]
pub struct Heard {
    pub text: String,
    pub conf: Option<f32>,
    pub verdict: Verdict,
    /// The Whisper add-on made the final decision.
    pub by_whisper: bool,
}

pub struct VoiceShared {
    pub stop: AtomicBool,
    level_bits: AtomicU32,
    pub status: Mutex<VoiceStatus>,
    heard: Mutex<Option<Heard>>,
    /// The microphone actually being listened to.
    mic_name: Mutex<String>,
    /// The microphone has sent nothing but digital silence for a while (muted, wrong
    /// device, or blocked by its driver).
    silent: AtomicBool,
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
    pub fn clear_heard(&self) {
        if let Ok(mut h) = self.heard.lock() {
            *h = None;
        }
    }
    pub fn last_heard(&self) -> Option<Heard> {
        self.heard.lock().ok().and_then(|h| h.clone())
    }
    pub fn mic_name(&self) -> String {
        self.mic_name.lock().map(|n| n.clone()).unwrap_or_default()
    }
    pub fn mic_silent(&self) -> bool {
        self.silent.load(Ordering::Relaxed)
    }
    fn set_status(&self, s: VoiceStatus) {
        if let Ok(mut g) = self.status.lock() {
            *g = s;
        }
    }
}

pub struct VoiceHandle {
    pub shared: Arc<VoiceShared>,
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
    reset: unsafe extern "C" fn(*mut c_void),
    /// Optional: per-word confidences in results, used to reject shaky matches.
    set_words: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    /// Optional: flush the utterance when the noise gate closes.
    final_result: Option<unsafe extern "C" fn(*mut c_void) -> *const c_char>,
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
                reset: sym!("vosk_recognizer_reset"),
                set_words: lib
                    .get(b"vosk_recognizer_set_words\0")
                    .ok()
                    .map(|symbol| *symbol),
                final_result: lib
                    .get(b"vosk_recognizer_final_result\0")
                    .ok()
                    .map(|symbol| *symbol),
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

/// A sentence Vosk thought might be the trigger, for Whisper to judge.
struct WhisperJob {
    audio: Vec<i16>,
    vosk_verdict: Verdict,
    vosk_text: String,
    vosk_conf: Option<f32>,
}

/// Start the Whisper worker for this listener. It owns the Whisper server, which stops when
/// the listener does (the job channel closes).
fn spawn_whisper_checker(
    dir: &Path,
    cfg: &VoiceConfig,
    spellings: &[String],
    sh: &Arc<VoiceShared>,
    on_trigger: &OnTrigger,
    ctx: &egui::Context,
) -> std::sync::mpsc::Sender<WhisperJob> {
    let (job_tx, jobs) = channel::<WhisperJob>();
    let turkish = Lang::from_id(&cfg.language) == Lang::Turkish;
    let lang = if turkish { "tr" } else { "en" };
    let dir = dir.to_path_buf();
    let mut matcher = PhraseMatcher::for_language(&cfg.phrase, 3_000, turkish)
        .with_spellings(spellings)
        // Whisper writes whole sentences with punctuation and spells unfamiliar words its
        // own way: allow one more word and a little sound difference.
        .with_slack(cfg.strictness.slack() + 1)
        .with_tolerance(0.12);
    let (sh, on_trigger, ctx) = (sh.clone(), on_trigger.clone(), ctx.clone());
    std::thread::Builder::new()
        .name("rewind-whisper".into())
        .spawn(move || {
            let mut whisper = crate::whisper::Whisper::start(&dir, lang).ok();
            let started = Instant::now();
            while let Ok(mut job) = jobs.recv() {
                while let Ok(newer) = jobs.try_recv() {
                    job = newer;
                }
                let heard = match whisper.as_mut().map(|w| w.transcribe(&job.audio)) {
                    Some(Ok(text)) => {
                        let verdict =
                            matcher.judge(&text, None, 0.0, started.elapsed().as_millis() as u64);
                        if verdict == Verdict::Match {
                            on_trigger();
                        }
                        Heard {
                            text,
                            conf: None,
                            verdict,
                            by_whisper: true,
                        }
                    }
                    // Whisper missing or failing: fall back to what Vosk decided.
                    _ => {
                        if job.vosk_verdict == Verdict::Match {
                            on_trigger();
                        }
                        Heard {
                            text: job.vosk_text,
                            conf: job.vosk_conf,
                            verdict: job.vosk_verdict,
                            by_whisper: false,
                        }
                    }
                };
                if heard.verdict != Verdict::Nothing {
                    if let Ok(mut h) = sh.heard.lock() {
                        *h = Some(heard);
                    }
                }
                ctx.request_repaint();
            }
        })
        .ok();
    job_tx
}

/// The microphone to listen to. An empty id means "Windows default", and that is the
/// default **communications** microphone, exactly what OBS records for "Default". (Windows
/// keeps two defaults; when they differ, e.g. a headset for calls and a webcam mic as the
/// plain default, listening to the other one heard nothing while the recording was fine.)
fn open_capture_device(
    enumerator: &DeviceEnumerator,
    device_id: &str,
) -> Result<wasapi::Device, String> {
    let default = || {
        enumerator
            .get_default_device_for_role(Direction::Capture, Role::Communications)
            .or_else(|_| enumerator.get_default_device(Direction::Capture))
            .map_err(|_| "no microphone found".to_string())
    };
    if device_id.trim().is_empty() {
        return default();
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
    default()
}

/// The id of the current default communications microphone, to notice when it changes.
fn default_capture_id(enumerator: &DeviceEnumerator) -> Option<String> {
    enumerator
        .get_default_device_for_role(Direction::Capture, Role::Communications)
        .ok()
        .and_then(|d| d.get_id().ok())
}

/// Start listening on a background thread. `runtime_dir` holds libvosk.dll, `model_dir` the
/// unpacked language model, `device_id` the chosen microphone (empty = Windows default).
/// `on_trigger` runs on the voice thread the moment the phrase is recognised.
pub fn start(
    cfg: &VoiceConfig,
    runtime_dir: PathBuf,
    model_dir: PathBuf,
    whisper_dir: Option<PathBuf>,
    device_id: String,
    on_trigger: OnTrigger,
    ctx: egui::Context,
) -> VoiceHandle {
    let shared = Arc::new(VoiceShared {
        stop: AtomicBool::new(false),
        level_bits: AtomicU32::new(0),
        status: Mutex::new(VoiceStatus::Starting),
        heard: Mutex::new(None),
        mic_name: Mutex::new(String::new()),
        silent: AtomicBool::new(false),
    });
    let (cfg, sh) = (cfg.clone(), shared.clone());
    std::thread::Builder::new()
        .name("rewind-voice".into())
        .spawn(move || {
            let result = run(
                &cfg,
                &runtime_dir,
                &model_dir,
                whisper_dir.as_deref(),
                &device_id,
                &sh,
                &on_trigger,
                &ctx,
            );
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
    VoiceHandle { shared }
}

#[allow(clippy::too_many_arguments)]
fn run(
    cfg: &VoiceConfig,
    runtime_dir: &Path,
    model_dir: &Path,
    whisper_dir: Option<&Path>,
    device_id: &str,
    sh: &Arc<VoiceShared>,
    on_trigger: &OnTrigger,
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

    let lang = Lang::from_id(&cfg.language);
    let turkish = lang == Lang::Turkish;
    // The model's own dictionary (read from its grammar file). With it, the phrase is matched
    // by sound: words the model doesn't know are spelled with words it does, and near-miss
    // words give ordinary speech somewhere else to land.
    let vocab = Vocab::load(model_dir, turkish);
    let find_word = vosk.find_word;
    let known = |word: &str| match find_word {
        Some(f) => CString::new(word)
            .map(|c| unsafe { f(model, c.as_ptr()) } >= 0)
            .unwrap_or(false),
        // Can't check: assume the model knows it (the grammar is kept minimal below).
        None => true,
    };
    let mut listen_plan = plan(&cfg.phrase, turkish, vocab.as_ref(), &known);
    if listen_plan.spellings.is_empty() {
        if listen_plan.missing.is_empty() {
            unsafe { (vosk.model_free)(model) };
            return Err("the trigger phrase is empty".into());
        }
        sh.set_status(VoiceStatus::Phrase(PhraseWarning {
            lang,
            missing: listen_plan.missing,
            suggestion: default_phrase(lang),
        }));
        unsafe { (vosk.model_free)(model) };
        return Ok(());
    }
    if vocab.is_none() && find_word.is_none() {
        // No way to check words: an unknown one would make the whole grammar fail to load.
        listen_plan.grammar = listen_plan.spellings.clone();
        listen_plan.grammar.push("[unk]".into());
    }
    let grammar = CString::new(serde_json::to_string(&listen_plan.grammar).unwrap_or_default())
        .map_err(|_| "bad phrase")?;
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
    if let Some(set_words) = vosk.set_words {
        unsafe { set_words(rec, 1) };
    }
    let mut outcome = Ok(());
    // A changed default microphone restarts only the capture, not the model.
    while !sh.stop.load(Ordering::Relaxed) {
        match listen(
            &vosk,
            rec,
            cfg,
            &listen_plan.spellings,
            whisper_dir,
            device_id,
            sh,
            on_trigger,
            ctx,
        ) {
            Ok(Listen::Restart) => {
                unsafe { (vosk.reset)(rec) };
                continue;
            }
            Ok(Listen::Stopped) => break,
            Err(e) => {
                outcome = Err(e);
                break;
            }
        }
    }
    unsafe {
        (vosk.rec_free)(rec);
        (vosk.model_free)(model);
    }
    outcome
}

/// How near a verdict is to a saved clip, for picking the more telling of two.
fn closeness(v: Verdict) -> u8 {
    match v {
        Verdict::Match => 5,
        Verdict::Cooldown => 4,
        Verdict::Unsure => 3,
        Verdict::NotAlone => 2,
        Verdict::Different => 1,
        Verdict::Nothing => 0,
    }
}

enum Listen {
    Stopped,
    /// The Windows default microphone changed: open the new one.
    Restart,
}

/// An open microphone: either in its own format (converted here) or, as a fallback,
/// converted to 16 kHz mono by Windows.
struct Mic {
    client: wasapi::AudioClient,
    capture: wasapi::AudioCaptureClient,
    event: wasapi::Handle,
    /// `None` when Windows already delivers 16 kHz mono 16-bit.
    native: Option<(MicConverter, SampleType, usize, usize)>,
}

fn open_mic(device: &wasapi::Device) -> Result<Mic, String> {
    // 1. The device's own format, like OBS. Every driver supports this one.
    let native = (|| -> Result<Mic, String> {
        let mut client = device.get_iaudioclient().map_err(|e| e.to_string())?;
        let format = client.get_mixformat().map_err(|e| e.to_string())?;
        let kind = format.get_subformat().map_err(|e| e.to_string())?;
        let bits = format.get_bitspersample() as usize;
        let channels = format.get_nchannels() as usize;
        let rate = format.get_samplespersec();
        if !matches!(
            (kind, bits),
            (SampleType::Float, 32) | (SampleType::Int, 16 | 24 | 32)
        ) || channels == 0
        {
            return Err(format!("unsupported microphone format ({bits}-bit)"));
        }
        client
            .initialize_client(
                &format,
                Direction::Capture,
                // 200 ms of buffer: plenty of slack, events still arrive every ~10 ms.
                StreamMode::EventsShared {
                    autoconvert: false,
                    buffer_duration_hns: 2_000_000,
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        let event = client.set_get_eventhandle().map_err(|e| e.to_string())?;
        let capture = client.get_audiocaptureclient().map_err(|e| e.to_string())?;
        Ok(Mic {
            client,
            capture,
            event,
            native: Some((MicConverter::new(rate, channels), kind, bits / 8, channels)),
        })
    })();
    if native.is_ok() {
        return native;
    }
    // 2. Let Windows convert to 16 kHz mono (what earlier versions always did).
    let mut client = device
        .get_iaudioclient()
        .map_err(|e| format!("microphone: {e}"))?;
    let format = WaveFormat::new(16, 16, SampleType::Int, SAMPLE_RATE, 1, None);
    client
        .initialize_client(
            &format,
            Direction::Capture,
            StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 2_000_000,
            },
            None,
        )
        .map_err(|e| {
            format!(
                "couldn't open the microphone: {e} ({})",
                native.err().unwrap_or_default()
            )
        })?;
    let event = client
        .set_get_eventhandle()
        .map_err(|e| format!("microphone: {e}"))?;
    let capture = client
        .get_audiocaptureclient()
        .map_err(|e| format!("microphone: {e}"))?;
    Ok(Mic {
        client,
        capture,
        event,
        native: None,
    })
}

/// Turn raw capture bytes into interleaved floats in -1..=1.
fn to_f32(bytes: &[u8], kind: SampleType, width: usize, out: &mut Vec<f32>) {
    out.clear();
    match (kind, width) {
        (SampleType::Float, 4) => out.extend(
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        ),
        (SampleType::Int, 2) => out.extend(
            bytes
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0),
        ),
        (SampleType::Int, 3) => out.extend(
            bytes
                .chunks_exact(3)
                .map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0),
        ),
        (SampleType::Int, 4) => out.extend(
            bytes
                .chunks_exact(4)
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0),
        ),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn listen(
    vosk: &Vosk,
    rec: *mut c_void,
    cfg: &VoiceConfig,
    spellings: &[String],
    whisper_dir: Option<&Path>,
    device_id: &str,
    sh: &Arc<VoiceShared>,
    on_trigger: &OnTrigger,
    ctx: &egui::Context,
) -> Result<Listen, String> {
    initialize_mta()
        .ok()
        .map_err(|e| format!("audio init: {e}"))?;
    let enumerator = DeviceEnumerator::new().map_err(|e| format!("audio devices: {e}"))?;
    let device = open_capture_device(&enumerator, device_id)?;
    let opened_id = device.get_id().ok();
    if let Ok(mut n) = sh.mic_name.lock() {
        *n = device.get_friendlyname().unwrap_or_default();
    }
    let mut mic = open_mic(&device)?;
    let mut raw: VecDeque<u8> = VecDeque::with_capacity(SAMPLE_RATE * 8);
    let mut floats: Vec<f32> = Vec::new();
    // Reused every event (about 100 a second) instead of allocating each time.
    let mut bytes: Vec<u8> = Vec::new();
    let mut converted: Vec<i16> = Vec::new();
    let mut pcm: VecDeque<i16> = VecDeque::with_capacity(SAMPLE_RATE);
    mic.client
        .start_stream()
        .map_err(|e| format!("microphone: {e}"))?;

    sh.set_status(VoiceStatus::Listening);
    sh.silent.store(false, Ordering::Relaxed);
    ctx.request_repaint();

    let turkish = Lang::from_id(&cfg.language) == Lang::Turkish;
    // With the Whisper add-on, Vosk only nominates; Whisper makes the call.
    let checker =
        whisper_dir.map(|dir| spawn_whisper_checker(dir, cfg, spellings, sh, on_trigger, ctx));
    // The current sentence, for Whisper, and the most promising thing Vosk made of it.
    let mut utterance: Vec<i16> = Vec::new();
    let mut best: Option<(Verdict, String, Option<f32>)> = None;
    // Everything recognised since the speaker started: the recogniser sometimes splits a
    // phrase into two results ("klip" | "al"), so the whole utterance is judged too.
    let mut said: Vec<String> = Vec::new();
    let mut said_conf: Option<f32> = None;

    let mut gate = VoiceGate::new(SAMPLE_RATE as u32, cfg.sensitivity);
    let mut matcher = PhraseMatcher::for_language(&cfg.phrase, 3_000, turkish)
        .with_spellings(spellings)
        .with_slack(cfg.strictness.slack());
    let min_conf = cfg.strictness.min_confidence_for(turkish);
    // High-pass + automatic gain on what the recogniser hears (the gate still judges the raw
    // level, so the sensitivity slider keeps working the same way).
    let mut cleaner = SpeechCleaner::new();
    let started = Instant::now();
    let mut was_open = false;
    // Reused for every chunk instead of allocating a fresh Vec several times a second.
    let mut samples: Vec<i16> = Vec::with_capacity(CHUNK_FRAMES);
    let mut silent_since: Option<Instant> = None;
    let mut last_default_check = Instant::now();
    let follow_default = device_id.trim().is_empty();
    let whisper_on = checker.is_some();

    let outcome = loop {
        if sh.stop.load(Ordering::Relaxed) {
            break Ok(Listen::Stopped);
        }
        let info = mic
            .capture
            .read_from_device_to_deque(&mut raw)
            .map_err(|e| format!("microphone: {e}"));
        let info = match info {
            Ok(i) => i,
            Err(e) => break Err(e),
        };
        // Bytes -> 16 kHz mono.
        let got_sound;
        match &mut mic.native {
            Some((conv, kind, width, channels)) => {
                let frame = *width * *channels;
                let usable = raw.len() / frame * frame;
                bytes.clear();
                bytes.extend(raw.drain(..usable));
                to_f32(&bytes, *kind, *width, &mut floats);
                got_sound = !info.flags.silent && floats.iter().any(|v| v.abs() > 1e-6);
                converted.clear();
                conv.push(&floats, &mut converted);
                pcm.extend(converted.iter().copied());
            }
            None => {
                let usable = raw.len() / 2 * 2;
                bytes.clear();
                bytes.extend(raw.drain(..usable));
                let before = pcm.len();
                pcm.extend(
                    bytes
                        .chunks_exact(2)
                        .map(|b| i16::from_le_bytes([b[0], b[1]])),
                );
                got_sound = !info.flags.silent && pcm.iter().skip(before).any(|v| *v != 0);
            }
        }
        // A microphone that only ever sends exact zeros is muted or blocked: say so in the app
        // instead of "Listening" forever.
        if got_sound {
            silent_since = None;
            if sh.silent.swap(false, Ordering::Relaxed) {
                ctx.request_repaint();
            }
        } else if silent_since.get_or_insert_with(Instant::now).elapsed() > Duration::from_secs(6)
            && !sh.silent.swap(true, Ordering::Relaxed)
        {
            ctx.request_repaint();
        }

        while pcm.len() >= CHUNK_FRAMES {
            samples.clear();
            samples.extend(pcm.drain(..CHUNK_FRAMES));
            let audio = gate.push(&samples);
            sh.level_bits
                .store(gate.level().to_bits(), Ordering::Relaxed);
            // Only finished results count. Partial results are the recogniser's running
            // guess and were the main source of clips nobody asked for.
            let mut check = |json: String, said: &mut Vec<String>, said_conf: &mut Option<f32>| {
                let Some(r) = parse_result(&json) else {
                    return;
                };
                if r.text.trim().is_empty() {
                    return;
                }
                said.push(r.text.clone());
                *said_conf = match (*said_conf, r.min_conf) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                let now = started.elapsed().as_millis() as u64;
                // This result on its own, or the whole utterance so far.
                let mut verdict = matcher.judge(&r.text, r.min_conf, min_conf, now);
                let mut text = r.text.clone();
                let mut conf = r.min_conf;
                if verdict != Verdict::Match && said.len() > 1 {
                    let joined = said.join(" ");
                    let v = matcher.judge(&joined, *said_conf, min_conf, now);
                    if closeness(v) > closeness(verdict) {
                        verdict = v;
                        text = joined;
                        conf = *said_conf;
                    }
                }
                if whisper_on {
                    // Remember the most trigger-like result of this sentence; Whisper
                    // decides once the sentence ends.
                    let worth = matches!(
                        verdict,
                        Verdict::Match | Verdict::Unsure | Verdict::NotAlone
                    ) || matcher.might_be(&text);
                    let rank = |v: Verdict| match v {
                        Verdict::Match => 3,
                        Verdict::Unsure => 2,
                        Verdict::NotAlone => 1,
                        _ => 0,
                    };
                    if worth
                        && best
                            .as_ref()
                            .map(|(v, _, _)| rank(verdict) >= rank(*v))
                            .unwrap_or(true)
                    {
                        best = Some((verdict, text.clone(), conf));
                    }
                    if worth {
                        return;
                    }
                } else if verdict == Verdict::Match {
                    // Straight away, on this thread: the sound plays and the save starts now.
                    on_trigger();
                    said.clear();
                    *said_conf = None;
                }
                if verdict != Verdict::Nothing {
                    // Shown on the Add-ons page so people can see why it did or didn't
                    // fire. Only written after a finished result: no extra wake-ups.
                    if let Ok(mut h) = sh.heard.lock() {
                        *h = Some(Heard {
                            text,
                            conf,
                            verdict,
                            by_whisper: false,
                        });
                    }
                    ctx.request_repaint();
                }
            };
            if let Some(mut audio) = audio {
                cleaner.process(&mut audio);
                if whisper_on {
                    utterance.extend_from_slice(&audio);
                    // Keep at most the last 8 seconds; triggers are short.
                    let max = SAMPLE_RATE * 8;
                    if utterance.len() > max {
                        utterance.drain(..utterance.len() - max);
                    }
                }
                let accepted = unsafe { (vosk.accept)(rec, audio.as_ptr(), audio.len() as c_int) };
                if accepted == 1 {
                    check(
                        unsafe { c_str((vosk.result)(rec)) },
                        &mut said,
                        &mut said_conf,
                    );
                }
            }
            if was_open && !gate.is_open() {
                cleaner.reset_filter();
                // Speech ended: take what's left of the utterance, then start clean.
                if let Some(final_result) = vosk.final_result {
                    check(
                        unsafe { c_str(final_result(rec)) },
                        &mut said,
                        &mut said_conf,
                    );
                }
                unsafe { (vosk.reset)(rec) };
                said.clear();
                said_conf = None;
                if let (Some(tx_job), Some((verdict, text, conf))) = (&checker, best.take()) {
                    let _ = tx_job.send(WhisperJob {
                        audio: std::mem::take(&mut utterance),
                        vosk_verdict: verdict,
                        vosk_text: text,
                        vosk_conf: conf,
                    });
                }
                utterance.clear();
            }
            was_open = gate.is_open();
        }
        // "Windows default" follows the default: a headset plugged in mid-game is picked up
        // within a couple of seconds, like the recording does.
        if follow_default && last_default_check.elapsed() > Duration::from_secs(2) {
            last_default_check = Instant::now();
            if !gate.is_open() && default_capture_id(&enumerator) != opened_id {
                break Ok(Listen::Restart);
            }
        }
        if mic.event.wait_for_event(500).is_err() {
            // No audio for half a second (device idle): just check the stop flag again.
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let _ = mic.client.stop_stream();
    outcome
}
