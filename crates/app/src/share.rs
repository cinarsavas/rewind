//! Quick sharing from the Clips page: put the clip itself on the clipboard (paste it into
//! Discord, WhatsApp or a folder), or first make a smaller copy that fits Discord's free
//! upload limit.

use std::path::{Path, PathBuf};

use rewind_core::trim::{parse_ffmpeg_duration, share_bitrate_kbps};

/// Discord's free upload limit is 10 MB; aim a little under it.
pub const SMALL_TARGET_MB: f64 = 9.5;
const AUDIO_KBPS: u32 = 96;

/// Copy the clip file to the clipboard, like Explorer's Copy.
pub fn copy_file(path: &Path) -> Result<(), String> {
    crate::platform::copy_files_to_clipboard(&[path.to_path_buf()])
}

fn hidden(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS
        cmd.creation_flags(0x0800_0000 | 0x0000_4000);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Make a 720p copy of `src` that stays under [`SMALL_TARGET_MB`]. Blocking; call from a
/// worker thread. The copy goes to a temp folder, never next to your clips.
pub fn make_small_copy(ffmpeg: &Path, src: &Path) -> Result<PathBuf, String> {
    // ffmpeg prints the duration when given only an input; it exits with an error, which
    // is expected here.
    let mut probe = std::process::Command::new(ffmpeg);
    probe.args(["-hide_banner", "-nostdin", "-i"]).arg(src);
    hidden(&mut probe);
    let probe = probe
        .output()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    let secs = parse_ffmpeg_duration(&String::from_utf8_lossy(&probe.stderr));
    let kbps = share_bitrate_kbps(secs, SMALL_TARGET_MB, AUDIO_KBPS);

    let dir = std::env::temp_dir().join("rewind-share");
    std::fs::create_dir_all(&dir).map_err(|e| format!("temp folder: {e}"))?;
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clip".into());
    let out = dir.join(format!("{stem} (small).mp4"));
    let _ = std::fs::remove_file(&out);

    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
        .arg(src)
        .args([
            "-vf",
            "scale=-2:'min(720,ih)'",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-b:v",
            &format!("{kbps}k"),
            "-maxrate",
            &format!("{kbps}k"),
            "-bufsize",
            &format!("{}k", kbps * 2),
            "-c:a",
            "aac",
            "-b:a",
            &format!("{AUDIO_KBPS}k"),
            "-movflags",
            "+faststart",
        ])
        .arg(&out);
    hidden(&mut cmd);
    let result = cmd
        .output()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    if !result.status.success() {
        let err = String::from_utf8_lossy(&result.stderr);
        return Err(err.lines().last().unwrap_or("ffmpeg failed").to_string());
    }
    Ok(out)
}
