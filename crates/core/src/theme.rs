//! Colour themes. The default theme is built in; extra themes are tiny optional TOML files.
//!
//! ```toml
//! name = "Midnight"
//! author = "you"
//! [colors]
//! nav = "#0a0c11"
//! panel = "#10131a"
//! card = "#171b24"
//! card_hi = "#1f2431"
//! stroke = "#252b3a"
//! text = "#e8ebf2"
//! muted = "#8b94a7"
//! faint = "#5c6578"
//! accent = "#4ad6a7"
//! danger = "#ff5d6c"
//! warn = "#f2b84b"
//! ```

use serde::{Deserialize, Serialize};

pub type Rgb = [u8; 3];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeColors {
    pub nav: String,
    pub panel: String,
    pub card: String,
    pub card_hi: String,
    pub stroke: String,
    pub text: String,
    pub muted: String,
    pub faint: String,
    pub accent: String,
    pub danger: String,
    pub warn: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    #[serde(default)]
    pub author: String,
    pub colors: ThemeColors,
}

/// Every colour the UI uses, base colours from the theme file plus derived ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub nav: Rgb,
    pub panel: Rgb,
    pub card: Rgb,
    pub card_hi: Rgb,
    pub stroke: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub faint: Rgb,
    pub accent: Rgb,
    pub danger: Rgb,
    pub warn: Rgb,
    // derived
    pub accent_dim: Rgb,
    pub on_accent: Rgb,
    pub deep: Rgb,
    pub accent_tint: Rgb,
    pub hero_on: Rgb,
    pub danger_bg: Rgb,
    pub accent_bg: Rgb,
    pub warn_bg: Rgb,
    pub hover: Rgb,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ThemeError {
    BadColor { field: &'static str, value: String },
    Unreadable(String),
    Parse(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeError::BadColor { field, value } => write!(f, "'{field}' isn't a valid colour: {value:?} (use #rrggbb)"),
            ThemeError::Unreadable(m) => write!(f, "{m}"),
            ThemeError::Parse(m) => write!(f, "theme file couldn't be read: {m}"),
        }
    }
}

impl std::error::Error for ThemeError {}

pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.trim().strip_prefix('#')?;
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        6 if h.is_ascii() => Some([v(0)?, v(2)?, v(4)?]),
        3 if h.is_ascii() => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|n| n * 17);
            Some([d(0)?, d(1)?, d(2)?])
        }
        _ => None,
    }
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

fn channel(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn luminance(c: Rgb) -> f32 {
    0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2])
}

/// WCAG contrast ratio, 1.0 (identical) to 21.0 (black on white).
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

impl Palette {
    pub fn default_dark() -> Palette {
        ThemeFile::default_theme().to_palette().expect("built-in theme is valid")
    }

    fn derive(base: [Rgb; 11]) -> Palette {
        let [nav, panel, card, card_hi, stroke, text, muted, faint, accent, danger, warn] = base;
        const BLACK: Rgb = [0, 0, 0];
        // Whichever of dark or white text reads better on the accent colour.
        const DARK: Rgb = [0x05, 0x14, 0x10];
        const WHITE: Rgb = [0xff, 0xff, 0xff];
        let on_accent = if contrast(DARK, accent) >= contrast(WHITE, accent) { DARK } else { WHITE };
        Palette {
            nav, panel, card, card_hi, stroke, text, muted, faint, accent, danger, warn,
            accent_dim: mix(card, accent, 0.32),
            on_accent,
            // Inset areas (chips, tracks): darker than the panel, but gently so on light themes.
            deep: mix(panel, BLACK, if luminance(panel) > 0.5 { 0.07 } else { 0.35 }),
            accent_tint: mix(nav, accent, 0.14),
            hero_on: mix(card, accent, 0.07),
            danger_bg: mix(card, danger, 0.18),
            accent_bg: mix(card, accent, 0.14),
            warn_bg: mix(card, warn, 0.14),
            hover: mix(card_hi, text, 0.07),
        }
    }
}

impl ThemeFile {
    pub fn default_theme() -> ThemeFile {
        let c = |s: &str| s.to_string();
        ThemeFile {
            name: "Mint (default)".into(),
            author: "Rewind".into(),
            colors: ThemeColors {
                nav: c("#0a0c11"),
                panel: c("#10131a"),
                card: c("#171b24"),
                card_hi: c("#1f2431"),
                stroke: c("#252b3a"),
                text: c("#e8ebf2"),
                muted: c("#8b94a7"),
                faint: c("#5c6578"),
                accent: c("#4ad6a7"),
                danger: c("#ff5d6c"),
                warn: c("#f2b84b"),
            },
        }
    }

    pub fn from_toml(text: &str) -> Result<ThemeFile, ThemeError> {
        toml::from_str(text).map_err(|e| ThemeError::Parse(e.to_string()))
    }

    /// Validate colours and readability, then derive the full palette.
    pub fn to_palette(&self) -> Result<Palette, ThemeError> {
        let c = &self.colors;
        let get = |field: &'static str, value: &String| parse_hex(value).ok_or_else(|| ThemeError::BadColor { field, value: value.clone() });
        let base = [
            get("nav", &c.nav)?,
            get("panel", &c.panel)?,
            get("card", &c.card)?,
            get("card_hi", &c.card_hi)?,
            get("stroke", &c.stroke)?,
            get("text", &c.text)?,
            get("muted", &c.muted)?,
            get("faint", &c.faint)?,
            get("accent", &c.accent)?,
            get("danger", &c.danger)?,
            get("warn", &c.warn)?,
        ];
        let p = Palette::derive(base);
        if contrast(p.text, p.card) < 4.5 {
            return Err(ThemeError::Unreadable("text on cards is too low-contrast to read (needs at least 4.5:1)".into()));
        }
        if contrast(p.muted, p.card) < 3.0 {
            return Err(ThemeError::Unreadable("secondary text is too faint against cards (needs at least 3:1)".into()));
        }
        if contrast(p.muted, p.deep) < 3.0 {
            return Err(ThemeError::Unreadable("secondary text is too faint on chips and tracks (needs at least 3:1)".into()));
        }
        if contrast(p.accent, p.card) < 2.5 {
            return Err(ThemeError::Unreadable("the accent colour disappears against cards (needs at least 2.5:1)".into()));
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#4ad6a7"), Some([0x4a, 0xd6, 0xa7]));
        assert_eq!(parse_hex("#FFF"), Some([255, 255, 255]));
        assert_eq!(parse_hex(" #000000 "), Some([0, 0, 0]));
        assert_eq!(parse_hex("4ad6a7"), None);
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(parse_hex("#ééé"), None);
    }

    #[test]
    fn contrast_matches_wcag_extremes() {
        assert!((contrast([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 0.01);
        assert!((contrast([10, 10, 10], [10, 10, 10]) - 1.0).abs() < 0.001);
    }

    #[test]
    fn built_in_theme_is_valid_and_readable() {
        let p = Palette::default_dark();
        assert!(contrast(p.text, p.card) > 10.0);
        assert!(contrast(p.on_accent, p.accent) > 7.0);
        assert!(luminance(p.deep) < luminance(p.panel));
    }

    #[test]
    fn light_themes_get_a_gentle_inset_colour() {
        let mut t = ThemeFile::default_theme();
        t.colors.panel = "#f4f5f9".into();
        t.colors.card = "#ffffff".into();
        t.colors.nav = "#e8eaf0".into();
        t.colors.card_hi = "#edeff5".into();
        t.colors.stroke = "#d3d8e3".into();
        t.colors.text = "#1b1f2a".into();
        t.colors.muted = "#566074".into();
        t.colors.faint = "#8a92a5".into();
        t.colors.accent = "#0e9a73".into();
        let p = t.to_palette().unwrap();
        assert!(luminance(p.deep) > 0.6, "inset colour must stay light on a light theme");
        assert!(contrast(p.muted, p.deep) >= 3.0);
    }

    #[test]
    fn light_accent_gets_dark_text_and_dark_accent_gets_light_text() {
        let mut t = ThemeFile::default_theme();
        t.colors.accent = "#ffd166".into();
        assert_eq!(t.to_palette().unwrap().on_accent, [0x05, 0x14, 0x10]);
        t.colors.accent = "#3a49d6".into();
        t.colors.text = "#ffffff".into();
        assert_eq!(t.to_palette().unwrap().on_accent, [255, 255, 255]);
    }

    #[test]
    fn rejects_bad_colours_and_unreadable_themes() {
        let mut t = ThemeFile::default_theme();
        t.colors.card = "blue".into();
        assert!(matches!(t.to_palette(), Err(ThemeError::BadColor { field: "card", .. })));
        let mut t = ThemeFile::default_theme();
        t.colors.text = "#1a1e28".into();
        assert!(matches!(t.to_palette(), Err(ThemeError::Unreadable(_))));
    }

    #[test]
    fn theme_files_round_trip_through_toml() {
        let t = ThemeFile::default_theme();
        let text = toml::to_string_pretty(&t).unwrap();
        assert_eq!(ThemeFile::from_toml(&text).unwrap(), t);
        assert!(matches!(ThemeFile::from_toml("name = 3"), Err(ThemeError::Parse(_))));
    }

    #[test]
    fn a_light_theme_can_be_valid() {
        let toml = r##"
            name = "Paper"
            [colors]
            nav = "#e9ebf0"
            panel = "#f5f6f9"
            card = "#ffffff"
            card_hi = "#eef0f5"
            stroke = "#d5d9e3"
            text = "#1b1f2a"
            muted = "#5a6275"
            faint = "#8b93a5"
            accent = "#0f9d76"
            danger = "#d93a4b"
            warn = "#b87400"
        "##;
        let p = ThemeFile::from_toml(toml).unwrap().to_palette().unwrap();
        assert!(luminance(p.card) > luminance(p.text));
    }
}
