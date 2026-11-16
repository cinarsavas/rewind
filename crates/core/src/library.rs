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

/// Add the clips in `dir` to `out`. When `subdirs` is given, folders found along the way are
/// collected there, so the top level is listed once instead of twice.
fn push_dir(dir: &Path, group: &str, out: &mut Vec<Clip>, mut subdirs: Option<&mut Vec<PathBuf>>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if let Some(dirs) = subdirs.as_deref_mut() {
                dirs.push(path);
            }
            continue;
        }
        if meta.is_file() && is_clip(&path) {
            out.push(Clip {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
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
    let mut subdirs = Vec::new();
    push_dir(dir, "", &mut clips, Some(&mut subdirs));
    for sub in subdirs {
        let group = sub
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        push_dir(&sub, &group, &mut clips, None);
    }
    sort_clips(&mut clips);
    clips
}

/// Name of the clip a trim was made from: `X-trim.mp4`, `X-trim (2).mp4` and
/// `X-trim-trim.mp4` all belong to `X`. Lower-cased, without the extension.
fn trim_family(name: &str) -> (String, bool) {
    let stem = match name.rfind('.') {
        Some(i) => &name[..i],
        None => name,
    };
    let mut base = stem.to_lowercase();
    let mut trimmed = false;
    loop {
        // A " (n)" suffix added to avoid overwriting a file.
        if let Some(open) = base.rfind(" (") {
            let inner = &base[open + 2..];
            if inner.ends_with(')')
                && inner[..inner.len() - 1].chars().all(|c| c.is_ascii_digit())
                && inner.len() > 1
                && base[..open].ends_with("-trim")
            {
                base.truncate(open);
            }
        }
        match base.strip_suffix("-trim") {
            Some(b) if !b.is_empty() => {
                base = b.to_string();
                trimmed = true;
            }
            _ => break,
        }
    }
    (base, trimmed)
}

/// Newest first, except that trimmed copies sit right after the clip they were cut from
/// (newest trim first), even when the trim was made much later.
pub fn sort_clips(clips: &mut [Clip]) {
    use std::collections::HashMap;
    let family = |c: &Clip| {
        let (base, trimmed) = trim_family(&c.name);
        ((c.group.to_lowercase(), base), trimmed)
    };
    // A family is placed by its original clip's time, or by its newest trim when the
    // original is gone.
    let mut placed: HashMap<(String, String), (bool, SystemTime)> = HashMap::new();
    for c in clips.iter() {
        let (key, trimmed) = family(c);
        let entry = placed.entry(key).or_insert((trimmed, c.modified));
        if !trimmed {
            *entry = (false, c.modified);
        } else if entry.0 && c.modified > entry.1 {
            entry.1 = c.modified;
        }
    }
    clips.sort_by(|a, b| {
        let (ka, ta) = family(a);
        let (kb, tb) = family(b);
        let pa = placed[&ka].1;
        let pb = placed[&kb].1;
        pb.cmp(&pa)
            .then_with(|| ka.cmp(&kb))
            .then_with(|| ta.cmp(&tb))
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Path of a clip relative to the clips folder, with forward slashes ("Game/clip.mp4").
/// Used to remember favourites independently of where the clips folder lives.
pub fn relative_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Clips to delete so the library fits in `limit_bytes`: oldest first, never favourites.
pub fn over_limit<'a>(
    clips: &'a [Clip],
    limit_bytes: u64,
    is_favorite: impl Fn(&Clip) -> bool,
) -> Vec<&'a Clip> {
    let mut total: u64 = clips.iter().map(|c| c.size_bytes).sum();
    if total <= limit_bytes {
        return Vec::new();
    }
    let mut oldest: Vec<&Clip> = clips.iter().filter(|c| !is_favorite(c)).collect();
    oldest.sort_by_key(|c| c.modified);
    let mut out = Vec::new();
    for c in oldest {
        if total <= limit_bytes {
            break;
        }
        total = total.saturating_sub(c.size_bytes);
        out.push(c);
    }
    out
}

/// A folder name for a game: Windows-safe, short, and never empty.
pub fn game_folder_name(raw: &str) -> Option<String> {
    let trimmed: String = raw.trim().chars().take(48).collect();
    if trimmed.is_empty() {
        return None;
    }
    let name = sanitize_file_name(&trimmed);
    (!name.is_empty() && name != "clip").then_some(name)
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
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim().to_string();
    if trimmed.is_empty() {
        "clip".into()
    } else {
        trimmed
    }
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

    #[test]
    fn trims_sit_next_to_their_original() {
        let t = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let clip = |n: &str, secs| Clip {
            path: PathBuf::from(n),
            name: n.into(),
            group: String::new(),
            size_bytes: 1,
            modified: t(secs),
        };
        let mut clips = vec![
            clip("A.mp4", 100),
            clip("B.mp4", 200),
            clip("C.mp4", 300),
            clip("A-trim.mp4", 900),
            clip("A-trim (2).mp4", 950),
            clip("A-trim-trim.mp4", 960),
            clip("Gone-trim.mp4", 250),
        ];
        sort_clips(&mut clips);
        let names: Vec<_> = clips.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "C.mp4",
                "Gone-trim.mp4",
                "B.mp4",
                "A.mp4",
                "A-trim-trim.mp4",
                "A-trim (2).mp4",
                "A-trim.mp4"
            ]
        );
        assert_eq!(
            trim_family("My Clip (2).mp4"),
            ("my clip (2)".into(), false)
        );
    }

    #[test]
    fn storage_limit_keeps_favourites_and_newest() {
        let t = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let clip = |n: &str, secs, mb: u64| Clip {
            path: PathBuf::from(n),
            name: n.into(),
            group: String::new(),
            size_bytes: mb * 1_000_000,
            modified: t(secs),
        };
        let clips = vec![clip("a", 1, 400), clip("b", 2, 400), clip("c", 3, 400)];
        let gone = over_limit(&clips, 900_000_000, |c| c.name == "a");
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].name, "b");
        assert!(over_limit(&clips, 2_000_000_000, |_| false).is_empty());
        assert_eq!(
            relative_key(Path::new("/c"), Path::new("/c/Game/x.mp4")),
            "Game/x.mp4"
        );
        assert_eq!(
            game_folder_name("  VALORANT  ").as_deref(),
            Some("VALORANT")
        );
        assert_eq!(game_folder_name("a:b"), Some("a_b".into()));
        assert_eq!(game_folder_name("   "), None);
    }

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
        let mut names: Vec<_> = clips
            .iter()
            .map(|c| (c.group.as_str(), c.name.as_str()))
            .collect();
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
        assert_eq!(
            human_age(now, now - Duration::from_secs(200_000)),
            "2 d ago"
        );
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
