//! Tiny release-version comparison for the GitHub update check.
//!
//! Only the numeric `major.minor.patch` part matters; a leading `v`, pre-release suffixes
//! like `-beta.1` and build metadata are ignored so `v0.3.0` beats `0.2.1`.

/// Split a tag or version into numeric components, ignoring `v`, suffixes and metadata.
fn parts(v: &str) -> Vec<u64> {
    let core = v.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next().unwrap_or("");
    core.split('.')
        .map(|p| p.trim().parse::<u64>().unwrap_or(0))
        .collect()
}

/// True when `latest` is a strictly newer release than `current`.
pub fn newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parts(latest), parts(current));
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Find `file`'s SHA-256 in a `sha256sum` listing (`<hash>  <name>` or `<hash> *<name>`).
pub fn checksum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut it = line.split_whitespace();
        let hash = it.next()?;
        let name = it.next()?.trim_start_matches('*');
        (name == file && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_checksums() {
        let h = "a".repeat(64);
        let sums = format!(
            "{h}  Rewind-Setup-0.3.2.exe\n{}  Rewind-portable-x64.zip\n",
            "B".repeat(64)
        );
        assert_eq!(
            super::checksum_for(&sums, "Rewind-Setup-0.3.2.exe"),
            Some(h)
        );
        assert_eq!(
            super::checksum_for(&sums, "Rewind-portable-x64.zip"),
            Some("b".repeat(64))
        );
        assert_eq!(super::checksum_for(&sums, "missing.zip"), None);
        assert_eq!(super::checksum_for("short  a.zip", "a.zip"), None);
    }

    use super::*;

    #[test]
    fn compares_numeric_parts() {
        assert!(newer("0.3.0", "0.2.1"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("0.2.1", "0.2.1"));
        assert!(!newer("0.2.0", "0.2.1"));
        assert!(!newer("v0.2.1", "0.2.1"));
    }

    #[test]
    fn ignores_suffixes_and_missing_parts() {
        assert!(newer("0.3.0-beta.1", "0.2.9"));
        assert!(!newer("0.2.1-rc1", "0.2.1"));
        assert!(newer("1", "0.9.9"));
        assert!(!newer("nonsense", "0.0.1"));
    }
}
