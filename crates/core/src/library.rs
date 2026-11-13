//! Clip library: list saved replays and format them for display.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const CLIP_EXTENSIONS: &[&str] = &["mp4", "mkv", "mov"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clip {
    pub path: PathBuf,
    pub name: String,
    /// Sub-folder name when clips are organised per game; empty for the root folder.
    pub group: String,
    pub size_bytes: u64,
    pub modified: SystemTime,
}

fn is_clip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| CLIP_EXTENSIONS.iter().any(|c| c.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

fn push_dir(dir: &Path, group: &str, out: &mut Vec<Clip>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() && is_clip(&path) {
            out.push(Clip {
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                group: group.to_string(),
                size_bytes: meta.len(),
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                path,
            });
        }
    }
}

/// Scan `dir` and its immediate sub-folders (per-game folders), newest first.
pub fn scan(dir: &Path) -> Vec<Clip> {
    let mut clips = Vec::new();
    push_dir(dir, "", &mut clips);
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                let group = entry.file_name().to_string_lossy().into_owned();
                push_dir(&entry.path(), &group, &mut clips);
            }
        }
    }
    clips.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    clips
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

pub fn human_age(now: SystemTime, then: SystemTime) -> String {
    let secs = now.duration_since(then).unwrap_or(Duration::ZERO).as_secs();
    match secs {
        0..=9 => "just now".into(),
        10..=59 => format!("{secs} s ago"),
        60..=3_599 => format!("{} min ago", secs / 60),
        3_600..=86_399 => format!("{} h ago", secs / 3_600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

/// Replace characters Windows forbids in file/folder names and trim trailing dots/spaces.
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim().to_string();
    if trimmed.is_empty() { "clip".into() } else { trimmed }
}

/// Return `dir/stem.ext`, or `dir/stem (2).ext` etc. if that already exists.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rewind-lib-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn scan_finds_clips_in_root_and_game_folders_only() {
        let d = tmp("scan");
        fs::write(d.join("a.mp4"), b"1234").unwrap();
        fs::write(d.join("notes.txt"), b"x").unwrap();
        fs::create_dir_all(d.join("Hades")).unwrap();
        fs::write(d.join("Hades").join("b.MKV"), b"12").unwrap();
        fs::create_dir_all(d.join("Hades").join("deep")).unwrap();
        fs::write(d.join("Hades").join("deep").join("c.mp4"), b"1").unwrap();
        let clips = scan(&d);
        let mut names: Vec<_> = clips.iter().map(|c| (c.group.as_str(), c.name.as_str())).collect();
        names.sort();
        assert_eq!(names, vec![("", "a.mp4"), ("Hades", "b.MKV")]);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn scan_of_missing_dir_is_empty() {
        assert!(scan(Path::new("/definitely/not/here")).is_empty());
    }

    #[test]
    fn formats_sizes_and_ages() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(human_age(now, now), "just now");
        assert_eq!(human_age(now, now - Duration::from_secs(300)), "5 min ago");
        assert_eq!(human_age(now, now - Duration::from_secs(7_300)), "2 h ago");
        assert_eq!(human_age(now, now - Duration::from_secs(200_000)), "2 d ago");
    }

    #[test]
    fn sanitizes_windows_hostile_names() {
        assert_eq!(sanitize_file_name("a/b:c*d?.mp4."), "a_b_c_d_.mp4");
        assert_eq!(sanitize_file_name("  ..  "), "clip");
    }

    #[test]
    fn unique_path_avoids_collisions() {
        let d = tmp("uniq");
        assert_eq!(unique_path(&d, "x", "mp4"), d.join("x.mp4"));
        fs::write(d.join("x.mp4"), b"").unwrap();
        assert_eq!(unique_path(&d, "x", "mp4"), d.join("x (2).mp4"));
        fs::write(d.join("x (2).mp4"), b"").unwrap();
        assert_eq!(unique_path(&d, "x", "mp4"), d.join("x (3).mp4"));
        let _ = fs::remove_dir_all(d);
    }
}
