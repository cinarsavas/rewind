//! The interface language. English is always the default; Turkish is the first translation.
//!
//! Translations are keyed by the English source string, so a missing translation simply falls
//! back to English instead of showing a placeholder or an empty label.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    #[serde(alias = "en", alias = "en-us", alias = "en_us")]
    English,
    #[serde(alias = "tr", alias = "tr-tr", alias = "tr_tr")]
    Turkish,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::English, Lang::Turkish];

    pub fn id(self) -> &'static str {
        match self {
            Lang::English => "en",
            Lang::Turkish => "tr",
        }
    }

    pub fn from_id(id: &str) -> Lang {
        match id.trim().to_lowercase().as_str() {
            "tr" | "tr-tr" | "tr_tr" | "turkish" | "türkçe" => Lang::Turkish,
            _ => Lang::English,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::English => "English",
            Lang::Turkish => "Türkçe",
        }
    }
}

/// Default voice trigger for a language. Used when the phrase is empty and when the user
/// switches language without having typed a custom phrase.
pub fn default_phrase(lang: Lang) -> &'static str {
    match lang {
        Lang::English => "clip that",
        Lang::Turkish => "klip al",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_aliases_parse() {
        assert_eq!(Lang::from_id(Lang::English.id()), Lang::English);
        assert_eq!(Lang::from_id(Lang::Turkish.id()), Lang::Turkish);
        assert_eq!(Lang::from_id("tr-TR"), Lang::Turkish);
        assert_eq!(Lang::from_id("türkçe"), Lang::Turkish);
        assert_eq!(Lang::from_id("de"), Lang::English);
    }

    #[test]
    fn config_accepts_short_language_codes() {
        #[derive(serde::Deserialize)]
        struct W {
            x: Lang,
        }
        let w: W = toml::from_str("x = \"tr\"").unwrap();
        assert_eq!(w.x, Lang::Turkish);
        let w: W = toml::from_str("x = \"en\"").unwrap();
        assert_eq!(w.x, Lang::English);
    }

    #[test]
    fn turkish_has_its_own_trigger() {
        assert_eq!(default_phrase(Lang::English), "clip that");
        assert_eq!(default_phrase(Lang::Turkish), "klip al");
    }
}
