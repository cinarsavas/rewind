//! The speech model's dictionary, and building what the recogniser listens for.
//!
//! Vosk's small models can only output words they know (the Turkish one knows about 98 000,
//! the English one about 152 000). Rewind reads that word list straight out of the model's
//! grammar file (`Gr.fst`), then, for a trigger phrase:
//!
//! - spells it with known words that **sound** the same, so a phrase with words the model
//!   has never seen ("kliple", "klipşunu") still works: "klip le", "klip şunu";
//! - adds the ways people actually say it ("klip" → "kilip");
//! - picks **near-miss words** ("kalıp", "kulüp", "galip" for "klip"; "keep", "slip" for
//!   "clip") so ordinary speech that only sounds a bit like the phrase lands on one of those
//!   instead of on the trigger. Tested on synthetic speech, this took Turkish false
//!   triggers from 4 % to none and English ones from 23 % to 16 % at the same hit rate.

use std::collections::HashSet;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::phonetic::{distance, phones, score, sub_cost, turkish_epenthesis, Phone};
use crate::voice::{decoy_words, grammar_phrase, split_phrases};

/// A spelling counts as the phrase when it sounds at most this different (per sound).
pub const SPELLING_TOLERANCE: f32 = 0.12;
/// Near-miss words: further than a spelling, closer than this.
const NEAR_MAX: f32 = 0.45;
const NEAR_PER_WORD: usize = 24;
const MAX_SPELLINGS: usize = 10;
/// Keeps the grammar small enough to compile in a blink.
const MAX_ENTRIES: usize = 400;

/// Read the word list from an OpenFst file's header (the output symbol table).
pub fn read_fst_words(path: &Path) -> std::io::Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let mut r = BufReader::with_capacity(1 << 16, file);
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    fn i32_(r: &mut impl Read) -> std::io::Result<i32> {
        let mut b = [0u8; 4];
        r.read_exact(&mut b)?;
        Ok(i32::from_le_bytes(b))
    }
    fn i64_(r: &mut impl Read) -> std::io::Result<i64> {
        let mut b = [0u8; 8];
        r.read_exact(&mut b)?;
        Ok(i64::from_le_bytes(b))
    }
    fn string(r: &mut impl Read) -> std::io::Result<String> {
        let n = i32_(r)?;
        if !(0..=1 << 20).contains(&n) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "bad string length",
            ));
        }
        let mut b = vec![0u8; n as usize];
        r.read_exact(&mut b)?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }
    const FST_MAGIC: i32 = 2_125_659_606;
    const SYM_MAGIC: i32 = 2_125_658_996;
    if i32_(&mut r)? != FST_MAGIC {
        return Err(bad("not an OpenFst file"));
    }
    let _fst_type = string(&mut r)?;
    let _arc_type = string(&mut r)?;
    let _version = i32_(&mut r)?;
    let flags = i32_(&mut r)?;
    let _properties = i64_(&mut r)?;
    let _start = i64_(&mut r)?;
    let _states = i64_(&mut r)?;
    let _arcs = i64_(&mut r)?;
    let read_table = |r: &mut BufReader<std::fs::File>| -> std::io::Result<Vec<String>> {
        if i32_(r)? != SYM_MAGIC {
            return Err(bad("bad symbol table"));
        }
        let _name = string(r)?;
        let _available = i64_(r)?;
        let size = i64_(r)?;
        if !(0..=5_000_000).contains(&size) {
            return Err(bad("bad symbol table size"));
        }
        let mut words = Vec::with_capacity(size as usize);
        for _ in 0..size {
            let s = string(r)?;
            let _key = i64_(r)?;
            words.push(s);
        }
        Ok(words)
    };
    const HAS_ISYMBOLS: i32 = 1;
    const HAS_OSYMBOLS: i32 = 2;
    let mut words = Vec::new();
    if flags & HAS_ISYMBOLS != 0 {
        words = read_table(&mut r)?;
    }
    if flags & HAS_OSYMBOLS != 0 {
        words = read_table(&mut r)?;
    }
    if words.is_empty() {
        return Err(bad("no word list in the model"));
    }
    Ok(words)
}

/// Where a Vosk model keeps its grammar file.
pub fn find_grammar_fst(model_dir: &Path) -> Option<std::path::PathBuf> {
    [
        model_dir.join("Gr.fst"),
        model_dir.join("graph").join("Gr.fst"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

struct Entry {
    word: String,
    phones: Vec<Phone>,
}

/// The model's dictionary with each word's sounds, grouped by length for quick lookups.
pub struct Vocab {
    turkish: bool,
    all: HashSet<String>,
    by_len: Vec<Vec<Entry>>,
}

impl Vocab {
    pub fn load(model_dir: &Path, turkish: bool) -> Option<Vocab> {
        let fst = find_grammar_fst(model_dir)?;
        let words = read_fst_words(&fst).ok()?;
        Some(Self::from_words(words, turkish))
    }

    pub fn from_words(words: impl IntoIterator<Item = String>, turkish: bool) -> Vocab {
        let mut all = HashSet::new();
        let mut by_len: Vec<Vec<Entry>> = Vec::new();
        for w in words {
            // Letters only; no single letters (except the real words "o" and "a") and no
            // "aaa"-style noise entries.
            let n = w.chars().count();
            let single_word = w == "o" || w == "a";
            let usable = single_word
                || (n >= 2
                    && n <= 14
                    && w.chars().all(char::is_alphabetic)
                    && w.chars().collect::<HashSet<_>>().len() > 1);
            if usable {
                let p = phones(&w, turkish);
                if !p.is_empty() {
                    if by_len.len() <= p.len() {
                        by_len.resize_with(p.len() + 1, Vec::new);
                    }
                    by_len[p.len()].push(Entry {
                        word: w.clone(),
                        phones: p,
                    });
                }
            }
            all.insert(w);
        }
        Vocab {
            turkish,
            all,
            by_len,
        }
    }

    pub fn len(&self) -> usize {
        self.all.len()
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    pub fn contains(&self, word: &str) -> bool {
        self.all.contains(word)
    }

    fn lengths(&self, from: usize, to: usize) -> impl Iterator<Item = &Entry> {
        let to = to.min(self.by_len.len().saturating_sub(1));
        (from.max(1)..=to).flat_map(move |l| self.by_len.get(l).into_iter().flatten())
    }

    /// Known words that sound most like `span`, cheapest first, with their total cost.
    fn best_words(&self, span: &[Phone], k: usize) -> Vec<(f32, &str)> {
        let l = span.len();
        let mut out: Vec<(f32, &str)> = Vec::new();
        for e in self.lengths(l.saturating_sub(1), l + 1) {
            if sub_cost(e.phones[0], span[0]) > 0.3 {
                continue;
            }
            let c = distance(span, &e.phones);
            if c / l as f32 <= 0.25 {
                out.push((c, &e.word));
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.truncate(k);
        out
    }

    /// Spell a run of sounds with known words: the `k` cheapest ways, each with its cost per
    /// sound. Prefers fewer, longer words.
    pub fn spell(&self, target: &[Phone], k: usize) -> Vec<(f32, String)> {
        const WORD_PENALTY: f32 = 0.35;
        let n = target.len();
        if n == 0 {
            return Vec::new();
        }
        // beams[j]: cheapest spellings of target[..j].
        let mut beams: Vec<Vec<(f32, Vec<&str>)>> = vec![Vec::new(); n + 1];
        beams[0].push((0.0, Vec::new()));
        let mut cache: std::collections::HashMap<(usize, usize), Vec<(f32, &str)>> =
            std::collections::HashMap::new();
        for j in 1..=n {
            let mut cands: Vec<(f32, Vec<&str>)> = Vec::new();
            for i in j.saturating_sub(10)..j {
                if beams[i].is_empty() {
                    continue;
                }
                let words = cache
                    .entry((i, j))
                    .or_insert_with(|| self.best_words(&target[i..j], 3))
                    .clone();
                for (c, w) in words {
                    for (bc, ws) in &beams[i] {
                        let mut v = ws.clone();
                        v.push(w);
                        cands.push((bc + c + WORD_PENALTY, v));
                    }
                }
            }
            cands.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut seen: HashSet<Vec<&str>> = HashSet::new();
            let mut keep = Vec::new();
            for (c, ws) in cands {
                if seen.insert(ws.clone()) {
                    keep.push((c, ws));
                    if keep.len() == k * 2 {
                        break;
                    }
                }
            }
            beams[j] = keep;
        }
        beams[n]
            .iter()
            .take(k)
            .map(|(c, ws)| {
                (
                    (c - WORD_PENALTY * ws.len() as f32) / n as f32,
                    ws.join(" "),
                )
            })
            .collect()
    }

    /// Known words that sound a bit like `word` but not like it: the near misses.
    fn near_words(&self, word: &str, exclude: &HashSet<&str>) -> Vec<String> {
        let wp = phones(word, self.turkish);
        let l = wp.len();
        if l == 0 {
            return Vec::new();
        }
        let mut near: Vec<(f32, &str)> = Vec::new();
        for e in self.lengths(l.saturating_sub(2), l + 2) {
            if exclude.contains(e.word.as_str()) {
                continue;
            }
            let c = distance(&wp, &e.phones) / l.max(e.phones.len()) as f32;
            if c > SPELLING_TOLERANCE && c <= NEAR_MAX {
                near.push((c, &e.word));
            }
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.into_iter()
            .take(NEAR_PER_WORD)
            .map(|(_, w)| w.to_string())
            .collect()
    }
}

/// What the recogniser should listen for, built from the user's trigger setting.
#[derive(Debug, Clone, PartialEq)]
pub struct ListenPlan {
    /// Word sequences that count as the trigger when recognised.
    pub spellings: Vec<String>,
    /// The Vosk grammar: spellings, `[unk]`, then near misses and everyday words.
    pub grammar: Vec<String>,
    /// Words of the phrase that couldn't be spelled at all (only when nothing usable is left).
    pub missing: Vec<String>,
}

fn add_missing(missing: &mut Vec<String>, phrase: &str, knows: &dyn Fn(&str) -> bool) {
    for w in phrase.split(' ') {
        if !knows(w) && !missing.iter().any(|m| m == w) {
            missing.push(w.to_string());
        }
    }
}

/// Plan the grammar for `setting` ("klip al, klibi al"). Without a dictionary (`vocab` is
/// `None`), `known` says whether the model knows a word, and phrases with unknown words are
/// dropped as before.
pub fn plan(
    setting: &str,
    turkish: bool,
    vocab: Option<&Vocab>,
    known: &dyn Fn(&str) -> bool,
) -> ListenPlan {
    let knows = |w: &str| vocab.map(|v| v.contains(w)).unwrap_or_else(|| known(w));
    let phrases: Vec<String> = split_phrases(setting)
        .into_iter()
        .map(|p| grammar_phrase(p, turkish))
        .filter(|p| !p.is_empty())
        .collect();
    let mut spellings: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let push = |s: String, spellings: &mut Vec<String>| {
        if !s.is_empty() && !spellings.contains(&s) && spellings.len() < MAX_SPELLINGS {
            spellings.push(s);
        }
    };
    for p in &phrases {
        let all_known = p.split(' ').all(knows);
        if all_known {
            push(p.clone(), &mut spellings);
        }
        let Some(v) = vocab else {
            if !all_known {
                add_missing(&mut missing, p, &knows);
            }
            continue;
        };
        // English: only when a word is unknown. Turkish: always, plus the spoken variant,
        // because people don't say every Turkish phrase the way it's written.
        if !turkish && all_known {
            continue;
        }
        let mut variants = vec![p.clone()];
        if turkish {
            variants.extend(turkish_epenthesis(p));
        }
        let before = spellings.len();
        for var in variants {
            for (cost, s) in v.spell(&phones(&var, turkish), 3) {
                if cost <= SPELLING_TOLERANCE {
                    push(s, &mut spellings);
                }
            }
        }
        if !all_known && spellings.len() == before {
            add_missing(&mut missing, p, &knows);
        }
    }
    if spellings.is_empty() {
        return ListenPlan {
            spellings,
            grammar: Vec::new(),
            missing,
        };
    }
    missing.clear();

    let targets: Vec<Vec<Phone>> = spellings.iter().map(|s| phones(s, turkish)).collect();
    let words: Vec<String> = {
        let mut w: Vec<String> = Vec::new();
        for s in &spellings {
            for x in s.split(' ') {
                if !w.iter().any(|y| y == x) {
                    w.push(x.to_string());
                }
            }
        }
        w
    };
    let mut sinks: Vec<String> = Vec::new();
    if let Some(v) = vocab {
        let exclude: HashSet<&str> = words.iter().map(String::as_str).collect();
        for w in &words {
            sinks.extend(v.near_words(w, &exclude));
        }
    }
    // The phrase's own words alone, so half a phrase doesn't get forced onto all of it.
    sinks.extend(words.iter().cloned());
    sinks.extend(decoy_words(turkish).iter().map(|w| w.to_string()));
    let mut grammar: Vec<String> = spellings.clone();
    grammar.push("[unk]".into());
    let mut seen: HashSet<String> = grammar.iter().cloned().collect();
    for s in sinks {
        if grammar.len() >= MAX_ENTRIES {
            break;
        }
        if seen.contains(&s) || !knows(&s) {
            continue;
        }
        // A near miss must never itself sound like the whole phrase.
        let sp = phones(&s, turkish);
        if targets.iter().any(|t| score(t, &sp) <= SPELLING_TOLERANCE) {
            continue;
        }
        seen.insert(s.clone());
        grammar.push(s);
    }
    ListenPlan {
        spellings,
        grammar,
        missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr_vocab() -> Vocab {
        let words =
            "klip klibi al ala alan kalp kalıp kılıp kulüp galip gelip kip le ile şunu şu nu \
                     bunu kaydet kaybet kayda geç genç gece yorum li lı kitap kilit aç tamam abi \
                     olur bu klima iple ipe";
        Vocab::from_words(words.split(' ').map(String::from), true)
    }

    #[test]
    fn spells_unknown_words_with_known_ones() {
        let v = tr_vocab();
        let best = v.spell(&phones("klipşunu", true), 3);
        assert_eq!(best[0].1, "klip şunu");
        assert_eq!(best[0].0, 0.0);
        let best = v.spell(&phones("kliple", true), 3);
        assert_eq!(best[0].1, "klip le");
    }

    #[test]
    fn plan_keeps_known_phrase_and_adds_near_misses() {
        let v = tr_vocab();
        let p = plan("klip al", true, Some(&v), &|_| false);
        assert_eq!(p.spellings[0], "klip al");
        assert!(p.grammar.contains(&"[unk]".to_string()));
        // Near misses for "klip" are in; nothing that sounds like the phrase itself is.
        assert!(p.grammar.contains(&"kalıp".to_string()));
        assert!(p.grammar.contains(&"galip".to_string()));
        assert!(p.missing.is_empty());
    }

    #[test]
    fn plan_makes_unknown_words_usable() {
        let v = tr_vocab();
        let p = plan("klipşunu", true, Some(&v), &|_| false);
        assert!(
            p.spellings.contains(&"klip şunu".to_string()),
            "{:?}",
            p.spellings
        );
        assert!(p.missing.is_empty());
    }

    #[test]
    fn plan_without_dictionary_falls_back_to_known_words() {
        let p = plan("klip al, uydurma kelime", true, None, &|w| {
            w == "klip" || w == "al"
        });
        assert_eq!(p.spellings, vec!["klip al".to_string()]);
        let p = plan("uydurma", true, None, &|_| false);
        assert!(p.spellings.is_empty());
        assert_eq!(p.missing, vec!["uydurma".to_string()]);
    }

    #[test]
    fn english_keeps_exact_phrase() {
        let words = "clip that keep slip flip click it a the";
        let v = Vocab::from_words(words.split(' ').map(String::from), false);
        let p = plan("clip that", false, Some(&v), &|_| false);
        assert_eq!(p.spellings, vec!["clip that".to_string()]);
        assert!(p.grammar.contains(&"keep".to_string()));
    }
}

#[cfg(test)]
mod real_models {
    use super::*;

    /// Runs against real Vosk models when REWIND_VOSK_MODELS points at a folder with
    /// `tr/` and `en/` unpacked (skipped otherwise).
    #[test]
    fn plans_with_real_dictionaries() {
        let Ok(root) = std::env::var("REWIND_VOSK_MODELS") else {
            return;
        };
        for (lang, turkish, phrases) in [
            (
                "tr",
                true,
                vec![
                    "klip al",
                    "kliple",
                    "klipşunu",
                    "klipliyorum",
                    "bunu kaydet",
                    "rivayınd",
                ],
            ),
            (
                "en",
                false,
                vec!["clip that", "save that clip", "rewind that"],
            ),
        ] {
            let t = std::time::Instant::now();
            let v = Vocab::load(&Path::new(&root).join(lang), turkish).expect("vocab");
            eprintln!("{lang}: {} words loaded in {:?}", v.len(), t.elapsed());
            for ph in phrases {
                let t = std::time::Instant::now();
                let p = plan(ph, turkish, Some(&v), &|_| false);
                eprintln!(
                    "  {ph:14} -> {:?} ({} grammar entries, {:?}) near: {:?}",
                    p.spellings,
                    p.grammar.len(),
                    t.elapsed(),
                    &p.grammar
                        [p.spellings.len() + 1..(p.spellings.len() + 13).min(p.grammar.len())]
                );
                assert!(!p.spellings.is_empty());
            }
        }
    }
}
