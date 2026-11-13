//! Optional add-ons ("packs") that are downloaded on demand instead of shipping in the
//! installer: a voice-recognition runtime plus language models, and colour themes.
//!
//! Releases publish a `packs.toml` index next to the installer. Every file listed in it
//! carries a SHA-256, and downloads are only allowed from GitHub hosts.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackFile {
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoicePack {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub file: PackFile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemePack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    /// Accent colour shown as a swatch before downloading.
    #[serde(default)]
    pub swatch: String,
    #[serde(flatten)]
    pub file: PackFile,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PackIndex {
    pub version: u32,
    pub voice_runtime: Option<PackFile>,
    pub voice: Vec<VoicePack>,
    pub theme: Vec<ThemePack>,
}

impl PackIndex {
    pub fn from_toml(text: &str) -> Result<PackIndex, String> {
        let idx: PackIndex = toml::from_str(text).map_err(|e| e.to_string())?;
        idx.validate()?;
        Ok(idx)
    }

    /// Reject anything we wouldn't be willing to download.
    pub fn validate(&self) -> Result<(), String> {
        let mut files: Vec<(&str, &PackFile)> = Vec::new();
        if let Some(r) = &self.voice_runtime {
            files.push(("voice runtime", r));
        }
        files.extend(self.voice.iter().map(|v| (v.id.as_str(), &v.file)));
        files.extend(self.theme.iter().map(|t| (t.id.as_str(), &t.file)));
        for (name, f) in files {
            if !allowed_download_url(&f.url) {
                return Err(format!("pack '{name}' points at a disallowed URL: {}", f.url));
            }
            if !valid_sha256(&f.sha256) {
                return Err(format!("pack '{name}' has no valid SHA-256"));
            }
        }
        for id in self.voice.iter().map(|v| &v.id).chain(self.theme.iter().map(|t| &t.id)) {
            if !valid_id(id) {
                return Err(format!("pack id '{id}' contains unsafe characters"));
            }
        }
        Ok(())
    }
}

pub fn index_url(repo: &str) -> String {
    format!("https://github.com/{repo}/releases/latest/download/packs.toml")
}

pub fn valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Pack ids become folder / file names, so keep them boring.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Only HTTPS, and only GitHub's own download hosts.
pub fn allowed_download_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.contains('@') || host.contains(':') {
        return false;
    }
    const HOSTS: [&str; 4] = ["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com", "raw.githubusercontent.com"];
    HOSTS.contains(&host)
}

/// Turn a zip entry name into a path that stays inside the extraction root, or `None` if
/// it tries to escape ("zip-slip") or is absolute.
pub fn safe_entry_path(name: &str) -> Option<PathBuf> {
    let name = name.replace('\\', "/");
    if name.is_empty() || name.starts_with('/') {
        return None;
    }
    let mut out = PathBuf::new();
    for comp in Path::new(&name).components() {
        match comp {
            Component::Normal(c) => {
                let c = c.to_string_lossy();
                if c.contains(':') {
                    return None;
                }
                out.push(c.as_ref());
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn url_allow_list() {
        assert!(allowed_download_url("https://github.com/o/r/releases/download/v1/a.zip"));
        assert!(allowed_download_url("https://objects.githubusercontent.com/x"));
        assert!(!allowed_download_url("http://github.com/o/r/a.zip"));
        assert!(!allowed_download_url("https://evil.example/github.com/a.zip"));
        assert!(!allowed_download_url("https://github.com.evil.example/a.zip"));
        assert!(!allowed_download_url("https://user@github.com/a.zip"));
        assert!(!allowed_download_url("https://github.com:8443/a.zip"));
        assert!(!allowed_download_url("ftp://github.com/a"));
    }

    #[test]
    fn zip_slip_is_blocked() {
        assert_eq!(safe_entry_path("model/am/final.mdl"), Some(PathBuf::from("model/am/final.mdl")));
        assert_eq!(safe_entry_path("./a/b.txt"), Some(PathBuf::from("a/b.txt")));
        assert_eq!(safe_entry_path("a\\b.txt"), Some(PathBuf::from("a/b.txt")));
        assert_eq!(safe_entry_path("../evil.dll"), None);
        assert_eq!(safe_entry_path("a/../../evil.dll"), None);
        assert_eq!(safe_entry_path("/etc/passwd"), None);
        assert_eq!(safe_entry_path("C:/Windows/x.dll"), None);
        assert_eq!(safe_entry_path("C:evil.dll"), None);
        assert_eq!(safe_entry_path(""), None);
    }

    #[test]
    fn sha_and_id_validation() {
        assert!(valid_sha256(HASH));
        assert!(!valid_sha256("abc"));
        assert!(!valid_sha256(&"z".repeat(64)));
        assert!(valid_id("en-us"));
        assert!(!valid_id("../x"));
        assert!(!valid_id("Upper"));
        assert!(!valid_id(""));
    }

    #[test]
    fn parses_a_realistic_index() {
        let text = format!(
            r##"
            version = 1
            [voice_runtime]
            url = "https://github.com/o/r/releases/download/v1/voice-runtime.zip"
            sha256 = "{HASH}"
            size_bytes = 15000000

            [[voice]]
            id = "en-us"
            name = "English (US)"
            url = "https://github.com/o/r/releases/download/v1/voice-en-us.zip"
            sha256 = "{HASH}"
            size_bytes = 41000000

            [[theme]]
            id = "ember"
            name = "Ember"
            author = "Rewind"
            swatch = "#ff7a45"
            url = "https://github.com/o/r/releases/download/v1/theme-ember.toml"
            sha256 = "{HASH}"
        "##
        );
        let idx = PackIndex::from_toml(&text).unwrap();
        assert_eq!(idx.voice.len(), 1);
        assert_eq!(idx.theme[0].swatch, "#ff7a45");
        assert_eq!(idx.voice_runtime.unwrap().size_bytes, 15_000_000);
    }

    #[test]
    fn index_with_bad_url_or_hash_is_rejected() {
        let bad_url = format!("[[theme]]\nid=\"a\"\nname=\"A\"\nurl=\"https://evil.example/a.toml\"\nsha256=\"{HASH}\"\n");
        assert!(PackIndex::from_toml(&bad_url).is_err());
        let bad_hash = "[[theme]]\nid=\"a\"\nname=\"A\"\nurl=\"https://github.com/o/r/a.toml\"\nsha256=\"nope\"\n";
        assert!(PackIndex::from_toml(bad_hash).is_err());
        let bad_id = format!("[[theme]]\nid=\"../a\"\nname=\"A\"\nurl=\"https://github.com/o/r/a.toml\"\nsha256=\"{HASH}\"\n");
        assert!(PackIndex::from_toml(&bad_id).is_err());
    }

    #[test]
    fn empty_index_is_fine_and_url_is_built_from_repo() {
        assert_eq!(PackIndex::from_toml("").unwrap(), PackIndex::default());
        assert_eq!(index_url("me/rewind"), "https://github.com/me/rewind/releases/latest/download/packs.toml");
    }
}
