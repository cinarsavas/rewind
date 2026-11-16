//! Check GitHub for a newer release and install it. Runs off the UI thread and never blocks
//! startup.
//!
//! Installing works for both kinds of install:
//! - **Installer** (an Inno Setup uninstaller sits next to the exe): download the new
//!   `Rewind-Setup-*.exe`, then run it silently once Rewind has exited. The installer starts
//!   Rewind again when it finishes (`/RELAUNCH`).
//! - **Portable** (no uninstaller): download `Rewind-portable-x64.zip`, extract it, and copy it
//!   over the app folder once Rewind has exited, then start Rewind again.
//!
//! Every file is verified against the release's `SHA256SUMS.txt` before it is used. The
//! hand-over is a small hidden PowerShell process, because a running exe can't overwrite
//! itself; it waits for Rewind to quit (and ends it if it hangs) before touching any file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use crate::download::{self, Progress};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub notes: String,
    pub installer: Option<Asset>,
    pub portable: Option<Asset>,
    pub sums: Option<Asset>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// Ask the GitHub release API for the latest release. `Ok(None)` means "already current".
pub fn check(repo: &str) -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let text = download::fetch_github_api(&url)?;
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
    if !rewind_core::version::newer(&latest, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let find = |pred: &dyn Fn(&str) -> bool| {
        release
            .assets
            .iter()
            .find(|a| pred(&a.name))
            .map(|a| Asset {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
            })
    };
    Ok(Some(UpdateInfo {
        version: latest,
        url: release.html_url,
        notes: release.body,
        installer: find(&|n| n.starts_with("Rewind-Setup-") && n.ends_with(".exe")),
        portable: find(&|n| n == "Rewind-portable-x64.zip"),
        sums: find(&|n| n == "SHA256SUMS.txt"),
    }))
}

/// Folder the running exe lives in.
fn app_dir() -> Result<(PathBuf, PathBuf), String> {
    let exe = std::env::current_exe().map_err(|e| format!("can't find Rewind.exe: {e}"))?;
    let dir = exe
        .parent()
        .ok_or("can't find Rewind's folder")?
        .to_path_buf();
    Ok((exe, dir))
}

/// True when Rewind was installed with the setup program (it leaves `unins000.exe` behind).
/// Looked up once per run; the About page asks every frame.
pub fn is_installed_copy() -> bool {
    static INSTALLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *INSTALLED.get_or_init(|| {
        let Ok((_, dir)) = app_dir() else {
            return false;
        };
        std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten().any(|e| {
                    let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                    n.starts_with("unins") && n.ends_with(".exe")
                })
            })
            .unwrap_or(false)
    })
}

/// Whether this release can be installed from inside the app (it has the right file and the
/// checksum list). If not, the UI falls back to the release page.
pub fn can_install(info: &UpdateInfo) -> bool {
    info.sums.is_some()
        && if is_installed_copy() {
            info.installer.is_some()
        } else {
            info.portable.is_some()
        }
}

/// Single-quote a string for PowerShell.
fn ps(s: &Path) -> String {
    format!("'{}'", s.to_string_lossy().replace('\'', "''"))
}

/// Download and verify the update, then start the hidden helper that installs it as soon as
/// Rewind exits. On `Ok`, the caller must quit the app; the helper ends it after 20 s anyway.
pub fn prepare(info: &UpdateInfo, progress: &Arc<Progress>) -> Result<(), String> {
    let sums = info
        .sums
        .as_ref()
        .ok_or("this release has no checksum list")?;
    let sums_text = download::fetch_text(&sums.url)?;
    let (exe, dir) = app_dir()?;
    let work = std::env::temp_dir()
        .join("rewind-update")
        .join(&info.version);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("creating a temp folder: {e}"))?;
    let pid = std::process::id();
    let log = work.join("update.log");

    let installer = is_installed_copy();
    let asset = if installer {
        info.installer.as_ref()
    } else {
        info.portable.as_ref()
    }
    .ok_or("this release doesn't have a file for this kind of install")?;
    let sha = rewind_core::version::checksum_for(&sums_text, &asset.name)
        .ok_or_else(|| format!("{} isn't in the checksum list", asset.name))?;
    let file = work.join(&asset.name);
    download::download_verified(&asset.url, &file, &sha, progress)?;

    // Wait for this Rewind to exit (ending it if it hangs), then install.
    let wait = format!(
        "Wait-Process -Id {pid} -Timeout 20 -ErrorAction SilentlyContinue; \
         Stop-Process -Id {pid} -Force -ErrorAction SilentlyContinue; \
         Start-Sleep -Milliseconds 800; "
    );
    let script = if installer {
        format!(
            "{wait}Start-Process -FilePath {} -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-','/RELAUNCH'",
            ps(&file)
        )
    } else {
        let files = work.join("files");
        download::unzip(&file, &files)?;
        if !files.join("Rewind.exe").is_file() {
            return Err("the update archive doesn't contain Rewind.exe".into());
        }
        // Copy over the app folder, then start Rewind again whatever happened, so a failed
        // copy never leaves the user without a running app.
        format!(
            "{wait}try {{ Copy-Item -Path (Join-Path {} '*') -Destination {} -Recurse -Force -ErrorAction Stop }} \
             catch {{ $_ | Out-File -FilePath {} -Append }} \
             finally {{ Start-Process -FilePath {} -ArgumentList '--updated' }}",
            ps(&files),
            ps(&dir),
            ps(&log),
            ps(&dir.join(exe.file_name().unwrap_or(std::ffi::OsStr::new("Rewind.exe")))),
        )
    };

    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &script,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP: no console, and the helper outlives Rewind.
        cmd.creation_flags(0x0800_0000 | 0x0000_0200);
    }
    cmd.spawn()
        .map_err(|e| format!("couldn't start the updater: {e}"))?;
    Ok(())
}
