//! Check GitHub for a newer release. Runs off the UI thread and never blocks startup.

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub notes: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
}

/// Ask the GitHub release API for the latest release. `Ok(None)` means "already current".
pub fn check(repo: &str) -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let text = crate::download::fetch_github_api(&url)?;
    let release: Release = serde_json::from_str(&text)
        .map_err(|e| format!("couldn't read the update response: {e}"))?;
    let latest = release
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    if latest.is_empty() {
        return Err("the latest release has no version tag".into());
    }
    if rewind_core::version::newer(&latest, env!("CARGO_PKG_VERSION")) {
        Ok(Some(UpdateInfo {
            version: latest,
            url: release.html_url,
            notes: release.body,
        }))
    } else {
        Ok(None)
    }
}
