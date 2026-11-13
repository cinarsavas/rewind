//! A tiny, serde-friendly hotkey type: parses and prints strings like `Ctrl+Shift+F10`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    /// Canonical key name: `F1`..`F24`, `A`..`Z`, `0`..`9`, or a named key from [`NAMED_KEYS`].
    pub key: String,
}

pub const NAMED_KEYS: &[&str] = &[
    "Space", "Enter", "Tab", "Backspace", "Insert", "Delete", "Home", "End", "PageUp",
    "PageDown", "Up", "Down", "Left", "Right", "PrintScreen", "ScrollLock", "Pause",
];

#[derive(Debug, PartialEq, Eq)]
pub enum HotkeyError {
    Empty,
    NoKey,
    UnknownKey(String),
    MultipleKeys,
    NoModifierOnPlainKey(String),
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotkeyError::Empty => write!(f, "hotkey is empty"),
            HotkeyError::NoKey => write!(f, "hotkey has modifiers but no key"),
            HotkeyError::UnknownKey(k) => write!(f, "unknown key '{k}'"),
            HotkeyError::MultipleKeys => write!(f, "hotkey has more than one non-modifier key"),
            HotkeyError::NoModifierOnPlainKey(k) => {
                write!(f, "'{k}' needs a modifier (Ctrl/Shift/Alt/Win) so it doesn't hijack typing")
            }
        }
    }
}

impl std::error::Error for HotkeyError {}

fn canonical_key(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let upper = t.to_ascii_uppercase();
    // Function keys
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        if (1..=24).contains(&n) {
            return Some(format!("F{n}"));
        }
    }
    // Single letter / digit
    if upper.len() == 1 {
        let c = upper.chars().next()?;
        if c.is_ascii_alphanumeric() {
            return Some(upper);
        }
    }
    NAMED_KEYS
        .iter()
        .find(|k| k.eq_ignore_ascii_case(t))
        .map(|k| k.to_string())
}

impl Hotkey {
    pub fn has_modifier(&self) -> bool {
        self.ctrl || self.shift || self.alt || self.win
    }

    fn is_function_key(&self) -> bool {
        self.key.starts_with('F') && self.key[1..].parse::<u8>().is_ok()
    }
}

impl FromStr for Hotkey {
    type Err = HotkeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(HotkeyError::Empty);
        }
        let mut hk = Hotkey { ctrl: false, shift: false, alt: false, win: false, key: String::new() };
        for part in s.split('+') {
            let p = part.trim();
            match p.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => hk.ctrl = true,
                "shift" => hk.shift = true,
                "alt" => hk.alt = true,
                "win" | "super" | "meta" | "cmd" => hk.win = true,
                _ => {
                    if !hk.key.is_empty() {
                        return Err(HotkeyError::MultipleKeys);
                    }
                    hk.key = canonical_key(p).ok_or_else(|| HotkeyError::UnknownKey(p.to_string()))?;
                }
            }
        }
        if hk.key.is_empty() {
            return Err(HotkeyError::NoKey);
        }
        if !hk.has_modifier() && !hk.is_function_key() {
            return Err(HotkeyError::NoModifierOnPlainKey(hk.key));
        }
        Ok(hk)
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.win {
            parts.push("Win");
        }
        parts.push(&self.key);
        write!(f, "{}", parts.join("+"))
    }
}

impl Serialize for Hotkey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Hotkey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_canonically() {
        let hk: Hotkey = "ctrl + shift + f10".parse().unwrap();
        assert!(hk.ctrl && hk.shift && !hk.alt && !hk.win);
        assert_eq!(hk.key, "F10");
        assert_eq!(hk.to_string(), "Ctrl+Shift+F10");
    }

    #[test]
    fn letters_digits_and_named_keys() {
        assert_eq!("Alt+r".parse::<Hotkey>().unwrap().to_string(), "Alt+R");
        assert_eq!("Ctrl+5".parse::<Hotkey>().unwrap().to_string(), "Ctrl+5");
        assert_eq!("win+printscreen".parse::<Hotkey>().unwrap().to_string(), "Win+PrintScreen");
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!("".parse::<Hotkey>(), Err(HotkeyError::Empty));
        assert_eq!("Ctrl+Shift".parse::<Hotkey>(), Err(HotkeyError::NoKey));
        assert_eq!("Ctrl+A+B".parse::<Hotkey>(), Err(HotkeyError::MultipleKeys));
        assert!(matches!("Ctrl+Banana".parse::<Hotkey>(), Err(HotkeyError::UnknownKey(_))));
        assert!(matches!("A".parse::<Hotkey>(), Err(HotkeyError::NoModifierOnPlainKey(_))));
        assert!(matches!("F25".parse::<Hotkey>(), Err(HotkeyError::UnknownKey(_))));
    }

    #[test]
    fn bare_function_key_is_allowed() {
        assert_eq!("F9".parse::<Hotkey>().unwrap().to_string(), "F9");
    }

    #[test]
    fn serde_round_trip_through_toml() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct W {
            k: Hotkey,
        }
        let w = W { k: "Ctrl+Alt+Delete".parse().unwrap() };
        let s = toml::to_string(&w).unwrap();
        assert_eq!(toml::from_str::<W>(&s).unwrap(), w);
    }
}
