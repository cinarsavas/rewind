//! Pronunciation-based matching for voice clipping.
//!
//! Turkish is written the way it sounds, so a phrase's letters give its sounds almost
//! directly. That lets Rewind compare *sounds* instead of exact words: "klip şunu" and
//! "klipşunu" are the same, and a phrase with a word the small speech model has never seen
//! can still be spelled with words it does know that sound the same (see [`crate::vocab`]).
//!
//! English spelling is far less regular; for it a rough sound key (th, ph, ck, soft c, …)
//! is enough, because the English model knows nearly every word anyway.
//!
//! Sounds are single ASCII bytes. Turkish: `a e I(ı) i o O(ö) u U(ü)`, `C(ç) S(ş)`, `c` is
//! the Turkish c (as in "can"), `j`, and the plain consonants. English adds `T` (th).

/// One sound.
pub type Phone = u8;

const VOWELS: &[u8] = b"aeIioOuU";

pub fn is_vowel(p: Phone) -> bool {
    VOWELS.contains(&p)
}

/// The sounds of `text` in the given language, word boundaries removed and repeated sounds
/// merged (word boundaries and long sounds blur together in speech).
pub fn phones(text: &str, turkish: bool) -> Vec<Phone> {
    let raw = if turkish {
        tr_phones(text)
    } else {
        en_phones(text)
    };
    dedup(raw)
}

/// Sounds of each word separately (no merging across words).
pub fn word_phones(word: &str, turkish: bool) -> Vec<Phone> {
    phones(word, turkish)
}

fn dedup(raw: Vec<Phone>) -> Vec<Phone> {
    let mut out: Vec<Phone> = Vec::with_capacity(raw.len());
    for p in raw {
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    out
}

fn tr_phones(text: &str) -> Vec<Phone> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let p: &[u8] = match ch {
            'a' | 'A' | 'â' | 'Â' => b"a",
            'e' | 'E' => b"e",
            'ı' | 'I' => b"I",
            'i' | 'İ' | 'î' | 'Î' => b"i",
            'o' | 'O' => b"o",
            'ö' | 'Ö' => b"O",
            'u' | 'U' | 'û' | 'Û' => b"u",
            'ü' | 'Ü' => b"U",
            'ç' | 'Ç' => b"C",
            'ş' | 'Ş' => b"S",
            // Soft g only lengthens the vowel before it; it has no sound of its own.
            'ğ' | 'Ğ' => b"",
            'q' | 'Q' => b"k",
            'w' | 'W' => b"v",
            'x' | 'X' => b"ks",
            c if c.is_ascii_alphabetic() => {
                out.push(c.to_ascii_lowercase() as u8);
                continue;
            }
            _ => b"",
        };
        out.extend_from_slice(p);
    }
    out
}

fn en_phones(text: &str) -> Vec<Phone> {
    let mut out = Vec::with_capacity(text.len());
    let lowered: String = text
        .chars()
        .map(|c| {
            if c.is_alphabetic() {
                c.to_ascii_lowercase()
            } else if c == '\'' {
                '\''
            } else {
                ' '
            }
        })
        .collect();
    for word in lowered.split_whitespace() {
        let mut w: Vec<u8> = word.bytes().filter(|b| b.is_ascii_lowercase()).collect();
        // A silent final e: "make", "time".
        if w.len() > 3 && w.last() == Some(&b'e') && !b"aeiouy".contains(&w[w.len() - 2]) {
            w.pop();
        }
        let mut i = 0;
        while i < w.len() {
            let c = w[i];
            let n = w.get(i + 1).copied().unwrap_or(0);
            let two = [c, n];
            let (push, step): (&[u8], usize) = match &two {
                b"ph" => (b"f", 2),
                b"ck" => (b"k", 2),
                b"th" => (b"T", 2),
                b"sh" => (b"S", 2),
                b"ch" => (b"C", 2),
                b"wh" => (b"w", 2),
                b"gh" => (b"", 2),
                b"kn" | b"wr" if i == 0 => (std::slice::from_ref(&w[i + 1]), 2),
                b"ee" | b"ea" => (b"i", 2),
                b"oo" => (b"u", 2),
                _ => match c {
                    b'c' => (if b"eiy".contains(&n) { b"s" } else { b"k" }, 1),
                    b'q' => (b"k", 1),
                    b'x' => (b"ks", 1),
                    b'y' => (if i > 0 { b"i" } else { b"y" }, 1),
                    _ => (std::slice::from_ref(&w[i]), 1),
                },
            };
            out.extend_from_slice(push);
            i += step;
        }
    }
    out
}

/// How different two sounds are, 0 (same) to 1 (unrelated).
pub fn sub_cost(a: Phone, b: Phone) -> f32 {
    if a == b {
        return 0.0;
    }
    let (x, y) = if a < b { (a, b) } else { (b, a) };
    let pair = |p: &[u8]| (p[0].min(p[1]), p[0].max(p[1])) == (x, y);
    // Voiced / voiceless partners: "klip" / "klibi", "kitap" / "kitabı".
    const VOICING: [&[u8]; 11] = [
        b"bp", b"cC", b"dt", b"gk", b"vf", b"zs", b"jS", b"dT", b"tT", b"Ts", b"Tf",
    ];
    if VOICING.iter().any(|p| pair(p)) {
        return 0.25;
    }
    if [b"mn"].iter().any(|p| pair(*p)) {
        return 0.4;
    }
    const NEAR: [&[u8]; 7] = [b"cj", b"CS", b"sS", b"kh", b"lr", b"nl", b"zj"];
    if NEAR.iter().any(|p| pair(p)) {
        return 0.55;
    }
    // Vowels: front/back partners are closest (ı/i, o/ö, u/ü).
    const V_CLOSE: [&[u8]; 3] = [b"Ii", b"oO", b"uU"];
    if V_CLOSE.iter().any(|p| pair(p)) {
        return 0.3;
    }
    const V_NEAR: [&[u8]; 6] = [b"Iu", b"iU", b"ei", b"ou", b"OU", b"iy"];
    if V_NEAR.iter().any(|p| pair(p)) {
        return 0.4;
    }
    const V_MID: [&[u8]; 5] = [b"ae", b"aI", b"ao", b"eO", b"Ie"];
    if V_MID.iter().any(|p| pair(p)) {
        return 0.5;
    }
    match (is_vowel(a), is_vowel(b)) {
        (true, true) => 0.65,
        (false, false) => 0.8,
        _ => 1.0,
    }
}

/// Cost of a sound that is there in one version and missing in the other. Vowels come and
/// go easily (Turkish adds one inside "klip": "kılip"); h, y, r and soft consonants too.
pub fn indel_cost(p: Phone) -> f32 {
    if is_vowel(p) {
        return 0.45;
    }
    match p {
        b'h' => 0.35,
        b'y' => 0.4,
        b'r' => 0.5,
        b'n' | b'l' => 0.6,
        _ => 0.7,
    }
}

/// Total cost of turning `a` into `b` (weighted edit distance over sounds).
pub fn distance(a: &[Phone], b: &[Phone]) -> f32 {
    let m = b.len();
    let mut prev: Vec<f32> = Vec::with_capacity(m + 1);
    prev.push(0.0);
    for j in 0..m {
        prev.push(prev[j] + indel_cost(b[j]));
    }
    let mut cur = vec![0.0f32; m + 1];
    for &pa in a {
        cur[0] = prev[0] + indel_cost(pa);
        for j in 1..=m {
            let pb = b[j - 1];
            let d = prev[j - 1] + sub_cost(pa, pb);
            let del = prev[j] + indel_cost(pa);
            let ins = cur[j - 1] + indel_cost(pb);
            cur[j] = d.min(del).min(ins);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// [`distance`] divided by the length of `reference`: about 0.1 per sound changed in a
/// six-sound phrase. 0 means it sounds the same.
pub fn score(reference: &[Phone], heard: &[Phone]) -> f32 {
    distance(reference, heard) / reference.len().max(1) as f32
}

/// Like [`score`], but `heard` may have anything before and after the phrase: the cost of
/// the best-matching stretch of `heard`.
pub fn score_inside(reference: &[Phone], heard: &[Phone]) -> f32 {
    let m = heard.len();
    // Leading part of `heard` is free.
    let mut prev = vec![0.0f32; m + 1];
    let mut cur = vec![0.0f32; m + 1];
    for &pa in reference {
        cur[0] = prev[0] + indel_cost(pa);
        for j in 1..=m {
            let pb = heard[j - 1];
            let d = prev[j - 1] + sub_cost(pa, pb);
            let del = prev[j] + indel_cost(pa);
            let ins = cur[j - 1] + indel_cost(pb);
            cur[j] = d.min(del).min(ins);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    // Trailing part is free.
    let best = prev.iter().copied().fold(f32::MAX, f32::min);
    best / reference.len().max(1) as f32
}

/// Ways people say a Turkish phrase that its spelling hides: a vowel slips into a cluster
/// at the start of a borrowed word ("klip" → "kılip", "tren" → "tiren", "spor" → "sipor").
/// Returns the phrase's words with that vowel added, or nothing when no word has such a
/// cluster.
pub fn turkish_epenthesis(phrase: &str) -> Option<String> {
    let mut changed = false;
    let words: Vec<String> = phrase
        .split_whitespace()
        .map(|w| {
            let chars: Vec<char> = w.chars().collect();
            if chars.len() < 3 {
                return w.to_string();
            }
            let (c1, c2) = (chars[0], chars[1]);
            let vowel = |c: char| "aeıioöuüâîû".contains(c);
            let cluster = !vowel(c1)
                && !vowel(c2)
                && ("lr".contains(c2) || (c1 == 's' && "ptk".contains(c2)));
            if !cluster {
                return w.to_string();
            }
            // The added vowel follows vowel harmony with the next vowel of the word.
            let next = chars
                .iter()
                .skip(2)
                .copied()
                .find(|c| vowel(*c))
                .unwrap_or('ı');
            let v = match next {
                'a' | 'ı' | 'â' => 'ı',
                'o' | 'u' | 'û' => 'u',
                'ö' | 'ü' => 'ü',
                _ => 'i',
            };
            changed = true;
            let mut s = String::new();
            s.push(c1);
            s.push(v);
            s.extend(chars[1..].iter());
            s
        })
        .collect();
    changed.then(|| words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(p: &[Phone]) -> String {
        String::from_utf8_lossy(p).into_owned()
    }

    #[test]
    fn turkish_letters_to_sounds() {
        assert_eq!(s(&phones("klip al", true)), "klipal");
        assert_eq!(s(&phones("KLİP AL", true)), "klipal");
        assert_eq!(s(&phones("kayıt", true)), "kayIt");
        assert_eq!(s(&phones("KAYIT", true)), "kayIt");
        assert_eq!(s(&phones("çöğüş", true)), "COUS");
        // Word boundaries don't matter, repeated sounds merge.
        assert_eq!(phones("klip şunu", true), phones("klipşunu", true));
        assert_eq!(phones("klip all", true), phones("klip al", true));
    }

    #[test]
    fn english_sound_key() {
        assert_eq!(s(&phones("clip that", false)), "klipTat");
        assert_eq!(s(&phones("Click", false)), "klik");
        assert_eq!(s(&phones("phone", false)), "fon");
        assert_eq!(s(&phones("keep", false)), "kip");
        assert_eq!(s(&phones("city", false)), "siti");
    }

    #[test]
    fn similar_sounds_are_close() {
        let p = phones("klip al", true);
        assert_eq!(score(&p, &phones("klip al", true)), 0.0);
        assert!(score(&p, &phones("kılip al", true)) < 0.1);
        assert!(score(&p, &phones("klibi al", true)) < 0.15);
        assert!(score(&p, &phones("kilit aç", true)) > 0.25);
        assert!(score(&p, &phones("tamam abi", true)) > 0.5);
        // The phrase inside a sentence.
        assert_eq!(score_inside(&p, &phones("abi klip al", true)), 0.0);
        assert!(score(&p, &phones("abi klip al", true)) > 0.2);
    }

    #[test]
    fn distance_is_symmetric_for_symmetric_costs() {
        let a = phones("kayda geç", true);
        let b = phones("kayıt geç", true);
        assert!((distance(&a, &b) - distance(&b, &a)).abs() < 1e-5);
    }

    #[test]
    fn epenthesis_follows_vowel_harmony() {
        assert_eq!(turkish_epenthesis("klip al").as_deref(), Some("kilip al"));
        assert_eq!(turkish_epenthesis("kral").as_deref(), Some("kıral"));
        assert_eq!(turkish_epenthesis("tren").as_deref(), Some("tiren"));
        assert_eq!(turkish_epenthesis("spor").as_deref(), Some("supor"));
        assert_eq!(turkish_epenthesis("kayıt al"), None);
    }
}
