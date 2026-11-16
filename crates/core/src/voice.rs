//! Pure logic behind voice clipping: a noise gate that keeps the speech recogniser idle
//! during silence (so it costs almost no CPU), phrase matching, and Vosk JSON helpers.

use std::collections::VecDeque;

/// Energy gate with pre-roll and hang-over. Feed it microphone chunks; it hands back audio
/// only while someone is (or just was) speaking.
pub struct VoiceGate {
    sample_rate: usize,
    threshold: f32,
    hangover: usize,
    remaining: usize,
    open: bool,
    preroll: VecDeque<i16>,
    preroll_cap: usize,
    level: f32,
}

fn threshold_for(sensitivity: f32) -> f32 {
    let s = sensitivity.clamp(0.0, 1.0);
    0.05 + (0.004 - 0.05) * s
}

impl VoiceGate {
    pub fn new(sample_rate: u32, sensitivity: f32) -> Self {
        let sr = sample_rate as usize;
        Self {
            sample_rate: sr,
            threshold: threshold_for(sensitivity),
            hangover: sr * 7 / 10,
            remaining: 0,
            open: false,
            preroll: VecDeque::new(),
            preroll_cap: sr * 3 / 10,
            level: 0.0,
        }
    }

    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.threshold = threshold_for(sensitivity);
    }

    /// Loudness of the last chunk, 0..=1, for a level meter.
    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn sample_rate(&self) -> usize {
        self.sample_rate
    }

    pub fn push(&mut self, chunk: &[i16]) -> Option<Vec<i16>> {
        if chunk.is_empty() {
            return None;
        }
        let energy: f64 =
            chunk.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / chunk.len() as f64;
        let rms = (energy.sqrt() / 32768.0) as f32;
        self.level = rms.clamp(0.0, 1.0);

        if rms >= self.threshold {
            self.remaining = self.hangover;
            if !self.open {
                self.open = true;
                let mut out: Vec<i16> = self.preroll.drain(..).collect();
                out.extend_from_slice(chunk);
                return Some(out);
            }
            return Some(chunk.to_vec());
        }
        if self.open {
            self.remaining = self.remaining.saturating_sub(chunk.len());
            if self.remaining == 0 {
                self.open = false;
            }
            return Some(chunk.to_vec());
        }
        for s in chunk {
            if self.preroll.len() == self.preroll_cap {
                self.preroll.pop_front();
            }
            self.preroll.push_back(*s);
        }
        None
    }
}

/// Cleans speech before it reaches the recogniser: a high-pass filter removes rumble, fan
/// noise and DC offset, and an automatic gain brings quiet or far-away microphones up to a
/// steady level. Small models (the Turkish one especially) recognise much better with it.
pub struct SpeechCleaner {
    prev_in: f32,
    prev_out: f32,
    gain: f32,
}

impl Default for SpeechCleaner {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeechCleaner {
    pub fn new() -> Self {
        Self {
            prev_in: 0.0,
            prev_out: 0.0,
            gain: 1.0,
        }
    }

    /// Start of a new utterance: keep the learned gain, drop filter state.
    pub fn reset_filter(&mut self) {
        self.prev_in = 0.0;
        self.prev_out = 0.0;
    }

    pub fn process(&mut self, audio: &mut [i16]) {
        if audio.is_empty() {
            return;
        }
        // One-pole high-pass at about 100 Hz for 16 kHz audio.
        const A: f32 = 0.962;
        let mut sum_sq = 0.0f64;
        for s in audio.iter_mut() {
            let x = *s as f32;
            let y = A * (self.prev_out + x - self.prev_in);
            self.prev_in = x;
            self.prev_out = y;
            *s = y.clamp(-32768.0, 32767.0) as i16;
            sum_sq += (y as f64) * (y as f64);
        }
        let rms = (sum_sq / audio.len() as f64).sqrt() as f32;
        if rms > 30.0 {
            // Aim speech at about -18 dBFS; never boost more than 8x or cut below 0.5x.
            let target = (4100.0 / rms).clamp(0.5, 8.0);
            // Fast to turn down (no clipping), slow to turn up (no pumping).
            let k = if target < self.gain { 0.5 } else { 0.15 };
            self.gain += (target - self.gain) * k;
        }
        if (self.gain - 1.0).abs() > 0.01 {
            for s in audio.iter_mut() {
                let v = *s as f32 * self.gain;
                // Soft limit instead of hard clipping.
                let limited = if v.abs() > 24000.0 {
                    v.signum() * (24000.0 + (v.abs() - 24000.0) * 0.25)
                } else {
                    v
                };
                *s = limited.clamp(-32768.0, 32767.0) as i16;
            }
        }
    }
}

/// Lower-case, strip accents' combining marks and punctuation, collapse whitespace.
///
/// Turkish needs care: `to_lowercase()` turns `I` into `i` (English) and `İ` into `i` plus
/// a combining dot, which would make `KAYIT` and `kayıt` different. Folding the Turkish
/// dotless `ı`, `I` and `İ` all onto `i` makes a trigger phrase match whichever spelling
/// the recogniser produces, in English and Turkish alike.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for ch in s.chars() {
        if matches!(ch, 'I' | 'İ' | 'ı') {
            out.push('i');
            last_space = false;
            continue;
        }
        for lc in ch.to_lowercase() {
            if ('\u{0300}'..='\u{036f}').contains(&lc) {
                continue; // combining marks, e.g. the dot Rust leaves when lower-casing 'İ'
            }
            if lc.is_alphanumeric() {
                out.push(lc);
                last_space = false;
            } else if !last_space {
                out.push(' ');
                last_space = true;
            }
        }
    }
    out.trim().to_string()
}

/// Decides whether recognised text is the trigger phrase, with a cool-down so one
/// utterance can't save two clips.
pub struct PhraseMatcher {
    /// Normalised words of the first trigger phrase (kept for [`Self::matches`]).
    words: Vec<String>,
    /// Every trigger phrase; the user can give several, separated by commas.
    alternatives: Vec<Vec<String>>,
    cooldown_ms: u64,
    last_fire_ms: Option<u64>,
    /// Extra words allowed around the phrase (0 = the phrase must be said on its own).
    slack: usize,
}

/// Split a trigger setting like "klip al, klibi al" into its phrases.
pub fn split_phrases(phrase: &str) -> Vec<&str> {
    phrase
        .split([',', '|', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

impl PhraseMatcher {
    pub fn new(phrase: &str, cooldown_ms: u64) -> Self {
        let alternatives: Vec<Vec<String>> = split_phrases(phrase)
            .into_iter()
            .map(|p| {
                normalize(p)
                    .split(' ')
                    .filter(|w| !w.is_empty())
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .filter(|w| !w.is_empty())
            .collect();
        Self {
            words: alternatives.first().cloned().unwrap_or_default(),
            alternatives,
            cooldown_ms,
            last_fire_ms: None,
            slack: 0,
        }
    }

    /// Allow `extra` other words around the phrase (Relaxed strictness).
    pub fn with_slack(mut self, extra: usize) -> Self {
        self.slack = extra;
        self
    }

    pub fn phrase(&self) -> String {
        self.words.join(" ")
    }

    /// Strict check for a *finished* utterance: the trigger must be the whole thing that was
    /// said (ignoring `[unk]`), not a phrase buried in a sentence, and every word must be
    /// recognised with at least `min_conf` confidence when the recogniser reports it.
    ///
    /// This is what stops a small model (Turkish especially) from saving a clip for half of
    /// what you say: with a tiny grammar, anything that sounds a bit like "klip al" gets
    /// forced onto it, but those guesses come back as longer utterances or low confidence.
    pub fn matches_utterance(
        &mut self,
        recognised: &str,
        word_conf: Option<f32>,
        min_conf: f32,
        now_ms: u64,
    ) -> bool {
        self.judge(recognised, word_conf, min_conf, now_ms) == Verdict::Match
    }

    /// Like [`Self::matches_utterance`], but says why an utterance didn't count, for the
    /// voice test readout in the app.
    pub fn judge(
        &mut self,
        recognised: &str,
        word_conf: Option<f32>,
        min_conf: f32,
        now_ms: u64,
    ) -> Verdict {
        let norm = normalize(recognised);
        let heard: Vec<&str> = norm
            .split(' ')
            .filter(|w| !w.is_empty() && *w != "unk")
            .collect();
        if self.alternatives.is_empty() || heard.is_empty() {
            return Verdict::Nothing;
        }
        let mut alone = false;
        let mut inside = false;
        for words in &self.alternatives {
            if heard.len() < words.len() {
                continue;
            }
            let found = heard
                .windows(words.len())
                .any(|w| w.iter().zip(words).all(|(a, b)| a == b));
            if found {
                inside = true;
                if heard.len() <= words.len() + self.slack {
                    alone = true;
                }
            }
        }
        if !alone {
            return if inside {
                Verdict::NotAlone
            } else {
                Verdict::Different
            };
        }
        if word_conf.map(|c| c < min_conf).unwrap_or(false) {
            return Verdict::Unsure;
        }
        if let Some(last) = self.last_fire_ms {
            if now_ms.saturating_sub(last) < self.cooldown_ms {
                return Verdict::Cooldown;
            }
        }
        self.last_fire_ms = Some(now_ms);
        Verdict::Match
    }

    /// Worth a second opinion: a short utterance that shares at least half the words of a
    /// trigger phrase (for "klip al": "klip", "al", "kilit al" …). Used to decide when to ask
    /// the heavier Whisper add-on.
    pub fn might_be(&self, recognised: &str) -> bool {
        let norm = normalize(recognised);
        let heard: Vec<&str> = norm
            .split(' ')
            .filter(|w| !w.is_empty() && *w != "unk")
            .collect();
        if heard.is_empty() {
            return false;
        }
        self.alternatives.iter().any(|words| {
            let shared = words.iter().filter(|w| heard.contains(&w.as_str())).count();
            heard.len() <= words.len() + 3 && shared * 2 >= words.len()
        })
    }

    /// Looser check: the phrase anywhere in `recognised`, with the cool-down.
    pub fn matches(&mut self, recognised: &str, now_ms: u64) -> bool {
        if self.words.is_empty() {
            return false;
        }
        let norm = normalize(recognised);
        let heard: Vec<&str> = norm.split(' ').filter(|w| !w.is_empty()).collect();
        let hit = heard
            .windows(self.words.len())
            .any(|w| w.iter().zip(&self.words).all(|(a, b)| a == b));
        if !hit {
            return false;
        }
        if let Some(last) = self.last_fire_ms {
            if now_ms.saturating_sub(last) < self.cooldown_ms {
                return false;
            }
        }
        self.last_fire_ms = Some(now_ms);
        true
    }
}

/// Why a finished utterance did or didn't save a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Match,
    /// Only `[unk]` / nothing usable was heard.
    Nothing,
    /// Something else was said.
    Different,
    /// The phrase was said inside a longer sentence.
    NotAlone,
    /// The phrase was heard, but the recogniser wasn't confident enough.
    Unsure,
    /// Heard again right after a clip was saved.
    Cooldown,
}

/// Lower-case and clean a phrase for Vosk's grammar, without folding Turkish letters onto
/// ASCII. Vosk's Turkish vocabulary keeps `ı`, `ş`, `ç`, `ğ`, `ö` and `ü`, so `kayıt` must
/// stay `kayıt`; the matcher's [`normalize`] is deliberately looser and is only used for
/// comparing what was heard.
pub fn grammar_phrase(phrase: &str, turkish: bool) -> String {
    let mut out = String::with_capacity(phrase.len());
    let mut last_space = true;
    for ch in phrase.chars() {
        let lowered: String = if turkish && ch == 'I' {
            "ı".into()
        } else if turkish && ch == 'İ' {
            "i".into()
        } else {
            ch.to_lowercase().collect()
        };
        for lc in lowered.chars() {
            if lc.is_alphanumeric() {
                out.push(lc);
                last_space = false;
            } else if !last_space {
                out.push(' ');
                last_space = true;
            }
        }
    }
    out.trim().to_string()
}

/// Vosk grammar: only the trigger phrase (plus `[unk]`) can be recognised, which is both
/// faster and far less prone to false triggers than free dictation.
pub fn grammar_json(phrase: &str, turkish: bool) -> String {
    grammar_json_with(phrase, turkish, &[])
}

/// Like [`grammar_json`], plus "decoy" words. With only the trigger to choose from, a small
/// model maps almost any speech onto it; common everyday words (and the trigger's own words
/// on their own) give that speech somewhere else to go. Callers should pass only words the
/// model actually knows.
pub fn grammar_json_with(phrase: &str, turkish: bool, decoys: &[String]) -> String {
    let mut entries: Vec<String> = Vec::new();
    for p in split_phrases(phrase) {
        let p = grammar_phrase(p, turkish);
        if !p.is_empty() && !entries.contains(&p) {
            entries.push(p);
        }
    }
    entries.push("[unk]".into());
    for d in decoys {
        let d = grammar_phrase(d, turkish);
        if !d.is_empty() && !entries.contains(&d) {
            entries.push(d);
        }
    }
    serde_json::to_string(&entries).unwrap_or_else(|_| "[\"[unk]\"]".into())
}

/// Everyday words used as grammar decoys, per language. Short, frequent and phonetically
/// varied, so ordinary talk and game chatter land on them instead of on the trigger.
pub fn decoy_words(turkish: bool) -> &'static [&'static str] {
    if turkish {
        &[
            "bir", "bu", "şu", "o", "ve", "ama", "ne", "evet", "hayır", "tamam", "yok", "var",
            "gel", "git", "at", "ver", "bak", "dur", "koş", "vur", "öl", "kal", "olur", "oldu",
            "abi", "kanka", "lan", "hadi", "şimdi", "burada", "orada", "sol", "sağ", "ileri",
            "geri", "aşağı", "yukarı", "kilit", "klima", "kitap", "alın", "almak", "aldım", "kim",
            "kip", "ilk", "ip", "al", "klip", "kayıt", "dikkat", "oyun", "takım",
            // Words that sound close to "klip al" / "klibi al", so near-misses land here.
            "kulp", "kalp", "kil", "kilim", "kalıp", "kale", "kalk", "alt", "alan", "ali", "alp",
            "kapı", "kibir", "kilo", "alay", "aldı", "alır", "alsın", "bilet", "bil", "ilaç",
            "kır", "kaç", "kapat", "aç", "yap", "yaz", "bekle", "hızlı", "yavaş", "ateş", "silah",
            "mermi", "öldüm",
        ]
    } else {
        &[
            "a", "the", "that", "this", "it", "and", "but", "yes", "no", "okay", "go", "come",
            "left", "right", "back", "up", "down", "stop", "wait", "nice", "shoot", "reload",
            "flip", "click", "clap", "lip", "chip", "clips", "cap", "hat", "cat", "what", "clip",
            "save", "game", "team", "push", "run",
        ]
    }
}

#[derive(Debug, PartialEq)]
pub struct Recognised {
    pub text: String,
    pub partial: bool,
    /// Lowest per-word confidence, when the recogniser was asked for word details.
    pub min_conf: Option<f32>,
}

/// Parse `{"text": "..."}` or `{"partial": "..."}` as produced by Vosk.
pub fn parse_result(json: &str) -> Option<Recognised> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
        let min_conf = v
            .get("result")
            .and_then(|r| r.as_array())
            .and_then(|words| {
                words
                    .iter()
                    .filter(|w| w.get("word").and_then(|x| x.as_str()) != Some("[unk]"))
                    .filter_map(|w| w.get("conf").and_then(|c| c.as_f64()))
                    .map(|c| c as f32)
                    .reduce(f32::min)
            });
        return Some(Recognised {
            text: t.to_string(),
            partial: false,
            min_conf,
        });
    }
    v.get("partial")
        .and_then(|t| t.as_str())
        .map(|t| Recognised {
            text: t.to_string(),
            partial: true,
            min_conf: None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amplitude: i16, n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| if i % 2 == 0 { amplitude } else { -amplitude })
            .collect()
    }

    #[test]
    fn silence_stays_closed() {
        let mut g = VoiceGate::new(16_000, 0.5);
        for _ in 0..20 {
            assert!(g.push(&tone(10, 1600)).is_none());
        }
        assert!(!g.is_open());
    }

    #[test]
    fn speech_opens_with_preroll_then_closes_after_hangover() {
        let mut g = VoiceGate::new(16_000, 0.5);
        for _ in 0..5 {
            g.push(&tone(10, 1600));
        }
        let out = g
            .push(&tone(6000, 1600))
            .expect("loud chunk opens the gate");
        // 0.3 s of pre-roll (4800 samples) + the loud chunk.
        assert_eq!(out.len(), 4800 + 1600);
        assert!(g.is_open());
        // Quiet audio is still forwarded during the 0.7 s hang-over (11 200 samples)…
        for _ in 0..6 {
            assert!(g.push(&tone(10, 1600)).is_some());
        }
        assert!(g.is_open());
        // …and then the gate closes.
        for _ in 0..2 {
            g.push(&tone(10, 1600));
        }
        assert!(!g.is_open());
        assert!(g.push(&tone(10, 1600)).is_none());
    }

    #[test]
    fn sensitivity_changes_what_counts_as_speech() {
        let quiet = tone(300, 1600); // ~0.009 rms
        let mut strict = VoiceGate::new(16_000, 0.0);
        let mut keen = VoiceGate::new(16_000, 1.0);
        assert!(strict.push(&quiet).is_none());
        assert!(keen.push(&quiet).is_some());
    }

    #[test]
    fn level_tracks_loudness() {
        let mut g = VoiceGate::new(16_000, 0.5);
        g.push(&tone(16384, 800));
        assert!((g.level() - 0.5).abs() < 0.01);
        g.push(&[]);
        assert!((g.level() - 0.5).abs() < 0.01);
    }

    #[test]
    fn normalisation_handles_case_punctuation_and_turkish() {
        assert_eq!(normalize("  Clip   THAT!! "), "clip that");
        assert_eq!(normalize("Kaydet."), "kaydet");
        assert_eq!(normalize("İyi kayıt"), "iyi kayit");
        assert_eq!(normalize("KAYIT"), "kayit");
        assert_eq!(normalize("KAYIT"), normalize("kayıt"));
        assert_eq!(normalize("KLİP AL"), "klip al");
        assert_eq!(normalize("!!!"), "");
    }

    #[test]
    fn matches_turkish_phrase_regardless_of_dotted_i() {
        let mut m = PhraseMatcher::new("KLİP AL", 0);
        assert!(m.matches("klip al", 1));
        let mut m = PhraseMatcher::new("kayıt al", 0);
        assert!(m.matches("KAYIT AL", 1));
    }

    #[test]
    fn matches_whole_word_sequences_only() {
        let mut m = PhraseMatcher::new("clip that", 3000);
        assert!(!m.matches("clipping that", 0));
        assert!(!m.matches("clip", 0));
        assert!(m.matches("ok Clip that please", 10));
    }

    #[test]
    fn cooldown_blocks_double_triggers() {
        let mut m = PhraseMatcher::new("clip that", 3000);
        assert!(m.matches("clip that", 1_000));
        assert!(!m.matches("clip that", 2_000));
        assert!(m.matches("clip that", 4_500));
    }

    #[test]
    fn utterance_must_be_just_the_phrase() {
        let mut m = PhraseMatcher::new("klip al", 0);
        assert!(m.matches_utterance("klip al", Some(0.95), 0.6, 1));
        assert!(m.matches_utterance("[unk] klip al", None, 0.6, 2));
        assert!(!m.matches_utterance("bu klip al dedim", Some(0.95), 0.6, 3));
        assert!(!m.matches_utterance("klip", Some(0.95), 0.6, 4));
        assert!(!m.matches_utterance("klip al", Some(0.4), 0.6, 5));
    }

    #[test]
    fn several_phrases_and_slack() {
        let mut m = PhraseMatcher::new("klip al, klibi al", 0);
        assert!(m.matches_utterance("klibi al", None, 0.6, 1));
        assert!(m.matches_utterance("klip al", None, 0.6, 2));
        assert!(!m.matches_utterance("hadi klip al", None, 0.6, 3));
        let mut relaxed = PhraseMatcher::new("klip al", 0).with_slack(2);
        assert!(relaxed.matches_utterance("hadi klip al", None, 0.6, 4));
        assert!(!relaxed.matches_utterance("hadi abi şunu klip al", None, 0.6, 5));
        assert_eq!(
            grammar_json("klip al, klibi al", true),
            "[\"klip al\",\"klibi al\",\"[unk]\"]"
        );
    }

    #[test]
    fn might_be_picks_short_phrase_like_utterances() {
        let m = PhraseMatcher::new("klip al", 0);
        assert!(m.might_be("kilit al"));
        assert!(m.might_be("klip"));
        assert!(!m.might_be("bir iki üç dört beş altı al"));
        assert!(!m.might_be("tamam abi"));
    }

    #[test]
    fn cleaner_lifts_quiet_speech_and_removes_offset() {
        let mut c = SpeechCleaner::new();
        let mut last = Vec::new();
        for _ in 0..20 {
            let mut chunk: Vec<i16> = (0..1600)
                .map(|i| 300 + if i % 20 < 10 { 400 } else { -400 })
                .collect();
            c.process(&mut chunk);
            last = chunk;
        }
        let mean: f64 = last.iter().map(|&s| s as f64).sum::<f64>() / last.len() as f64;
        let peak = last.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(mean.abs() < 200.0, "offset not removed: {mean}");
        assert!(peak > 1500, "quiet speech not lifted: {peak}");
    }

    #[test]
    fn decoys_are_added_without_duplicates() {
        let decoys: Vec<String> = ["klip", "al", "klip al", "bir"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            grammar_json_with("Klip Al", true, &decoys),
            "[\"klip al\",\"[unk]\",\"klip\",\"al\",\"bir\"]"
        );
    }

    #[test]
    fn empty_phrase_never_matches() {
        let mut m = PhraseMatcher::new("  ", 0);
        assert!(!m.matches("anything", 0));
    }

    #[test]
    fn grammar_is_valid_json_with_unknown_token() {
        assert_eq!(
            grammar_json("Clip That!", false),
            "[\"clip that\",\"[unk]\"]"
        );
        assert_eq!(grammar_json("say \"hi\"", false), "[\"say hi\",\"[unk]\"]");
    }

    #[test]
    fn turkish_grammar_keeps_its_own_letters() {
        assert_eq!(grammar_json("KLİP AL", true), "[\"klip al\",\"[unk]\"]");
        assert_eq!(grammar_json("KAYIT AL", true), "[\"kayıt al\",\"[unk]\"]");
        assert_eq!(grammar_json("Kayıt Al", true), "[\"kayıt al\",\"[unk]\"]");
        // English keeps the plain dotted i.
        assert_eq!(grammar_json("KAYIT AL", false), "[\"kayit al\",\"[unk]\"]");
    }

    #[test]
    fn parses_vosk_results() {
        assert_eq!(
            parse_result("{\"text\" : \"clip that\"}"),
            Some(Recognised {
                text: "clip that".into(),
                partial: false,
                min_conf: None
            })
        );
        assert_eq!(
            parse_result("{\"partial\" : \"clip\"}"),
            Some(Recognised {
                text: "clip".into(),
                partial: true,
                min_conf: None
            })
        );
        let detailed = parse_result(
            "{\"result\":[{\"conf\":0.93,\"word\":\"klip\"},{\"conf\":0.41,\"word\":\"al\"}],\"text\":\"klip al\"}",
        )
        .unwrap();
        assert_eq!(detailed.min_conf, Some(0.41));
        assert_eq!(parse_result("not json"), None);
        assert_eq!(parse_result("{\"other\": 1}"), None);
    }
}
