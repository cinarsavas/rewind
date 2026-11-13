//! The optional theme files shipped in /themes must all be valid and readable.

use std::fs;
use std::path::PathBuf;

use rewind_core::packs::valid_id;
use rewind_core::theme::{contrast, ThemeFile};

fn themes_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../themes")
}

#[test]
fn every_bundled_theme_is_valid() {
    let mut count = 0;
    for entry in fs::read_dir(themes_dir()).expect("themes folder exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let id = path.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(valid_id(&id), "theme file name '{id}' must be lower-case letters, digits and dashes");
        let text = fs::read_to_string(&path).unwrap();
        let theme = ThemeFile::from_toml(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let p = theme.to_palette().unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(contrast(p.text, p.panel) >= 4.5, "{id}: text on the panel background is too faint");
        assert!(contrast(p.accent, p.card) >= 3.0, "{id}: accent is too faint on cards");
        assert!(contrast(p.on_accent, p.accent) >= 4.5, "{id}: text on accent buttons is too faint");
        assert!(contrast(p.danger, p.card) >= 3.0, "{id}: danger colour is too faint");
        count += 1;
    }
    assert!(count >= 3, "expected the bundled themes to be present");
}

/// The example theme in docs/ is embedded in the app, so it must always be valid too.
#[test]
fn theme_template_is_valid() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/theme-template.toml");
    let text = fs::read_to_string(&path).expect("docs/theme-template.toml exists");
    let theme = ThemeFile::from_toml(&text).expect("the template parses");
    let p = theme.to_palette().expect("the template passes the contrast rules");
    assert!(contrast(p.text, p.card) >= 4.5, "template text on cards is too faint");
    assert!(contrast(p.accent, p.card) >= 2.5, "template accent is too faint on cards");
}
