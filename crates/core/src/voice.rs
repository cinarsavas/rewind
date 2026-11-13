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
        let energy: f64 = chunk.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / chunk.len() as f64;
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

/// Lower-case, strip accents' combining marks and punctuation, collapse whitespace.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for ch in s.chars().flat_map(|c| c.to_lowercase()) {
        if ('\u{0300}'..='\u{036f}').contains(&ch) {
            continue; // combining marks, e.g. the dot Rust leaves when lower-casing Turkish 'İ'
        }
        if ch.is_alphanumeric() {
            out.push(ch);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

/// Decides whether recognised text is the trigger phrase, with a cool-down so one
/// utterance can't save two clips.
pub struct PhraseMatcher {
    words: Vec<String>,
    cooldown_ms: u64,
    last_fire_ms: Option<u64>,
}

impl PhraseMatcher {
    pub fn new(phrase: &str, cooldown_ms: u64) -> Self {
        Self { words: normalize(phrase).split(' ').filter(|w| !w.is_empty()).map(String::from).collect(), cooldown_ms, last_fire_ms: None }
    }

    pub fn phrase(&self) -> String {
        self.words.join(" ")
    }

    pub fn matches(&mut self, recognised: &str, now_ms: u64) -> bool {
        if self.words.is_empty() {
            return false;
        }
        let norm = normalize(recognised);
        let heard: Vec<&str> = norm.split(' ').filter(|w| !w.is_empty()).collect();
        let hit = heard.windows(self.words.len()).any(|w| w.iter().zip(&self.words).all(|(a, b)| a == b));
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

/// Vosk grammar: only the trigger phrase (plus `[unk]`) can be recognised, which is both
/// faster and far less prone to false triggers than free dictation.
pub fn grammar_json(phrase: &str) -> String {
    serde_json::to_string(&[normalize(phrase).as_str(), "[unk]"]).unwrap_or_else(|_| "[\"[unk]\"]".into())
}

#[derive(Debug, PartialEq, Eq)]
pub struct Recognised {
    pub text: String,
    pub partial: bool,
}

/// Parse `{"text": "..."}` or `{"partial": "..."}` as produced by Vosk.
pub fn parse_result(json: &str) -> Option<Recognised> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
        return Some(Recognised { text: t.to_string(), partial: false });
    }
    v.get("partial").and_then(|t| t.as_str()).map(|t| Recognised { text: t.to_string(), partial: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amplitude: i16, n: usize) -> Vec<i16> {
        (0..n).map(|i| if i % 2 == 0 { amplitude } else { -amplitude }).collect()
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
        let out = g.push(&tone(6000, 1600)).expect("loud chunk opens the gate");
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
        assert_eq!(normalize("İyi kayıt"), "iyi kayıt");
        assert_eq!(normalize("!!!"), "");
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
    fn empty_phrase_never_matches() {
        let mut m = PhraseMatcher::new("  ", 0);
        assert!(!m.matches("anything", 0));
    }

    #[test]
    fn grammar_is_valid_json_with_unknown_token() {
        assert_eq!(grammar_json("Clip That!"), "[\"clip that\",\"[unk]\"]");
        assert_eq!(grammar_json("say \"hi\""), "[\"say hi\",\"[unk]\"]");
    }

    #[test]
    fn parses_vosk_results() {
        assert_eq!(parse_result("{\"text\" : \"clip that\"}"), Some(Recognised { text: "clip that".into(), partial: false }));
        assert_eq!(parse_result("{\"partial\" : \"clip\"}"), Some(Recognised { text: "clip".into(), partial: true }));
        assert_eq!(parse_result("not json"), None);
        assert_eq!(parse_result("{\"other\": 1}"), None);
    }
}
