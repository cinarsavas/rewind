//! Lossless (stream-copy) trimming by driving an `ffmpeg` executable.
//! Pure argument building lives here so it can be tested without ffmpeg installed.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct TrimJob {
    pub input: PathBuf,
    pub output: PathBuf,
    pub start_secs: f64,
    pub end_secs: f64,
    /// If set, keep only the last N seconds of the clip (the clip's length isn't needed).
    /// `start_secs`/`end_secs` are ignored in that case.
    pub last_secs: Option<f64>,
    /// Zero-based audio track indices to keep. Empty means "keep all".
    pub keep_audio: Vec<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TrimError {
    RangeTooShort,
    NegativeStart,
    SameFile,
}

impl std::fmt::Display for TrimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrimError::RangeTooShort => write!(f, "the selected range must be at least 0.1 s long"),
            TrimError::NegativeStart => write!(f, "start time can't be negative"),
            TrimError::SameFile => write!(f, "output would overwrite the input; choose a different name"),
        }
    }
}

impl std::error::Error for TrimError {}

impl TrimJob {
    pub fn validate(&self) -> Result<(), TrimError> {
        if self.input == self.output {
            return Err(TrimError::SameFile);
        }
        if let Some(n) = self.last_secs {
            return if n < 0.1 { Err(TrimError::RangeTooShort) } else { Ok(()) };
        }
        if self.start_secs < 0.0 {
            return Err(TrimError::NegativeStart);
        }
        if self.end_secs - self.start_secs < 0.1 {
            return Err(TrimError::RangeTooShort);
        }
        Ok(())
    }

    /// Arguments for `ffmpeg`. `-ss` before `-i` makes seeking fast; with stream copy the
    /// cut lands on the nearest earlier keyframe, which is the price of not re-encoding.
    pub fn ffmpeg_args(&self) -> Vec<String> {
        let mut a: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-y".into()];
        match self.last_secs {
            Some(n) => a.extend(["-sseof".into(), format!("-{n:.3}")]),
            None => a.extend(["-ss".into(), format!("{:.3}", self.start_secs)]),
        }
        a.extend(["-i".into(), self.input.to_string_lossy().into_owned()]);
        if self.last_secs.is_none() {
            a.extend(["-t".into(), format!("{:.3}", self.end_secs - self.start_secs)]);
        }
        a.extend(["-map".into(), "0:v:0".into()]);
        if self.keep_audio.is_empty() {
            a.extend(["-map".into(), "0:a?".into()]);
        } else {
            for i in &self.keep_audio {
                a.extend(["-map".into(), format!("0:a:{i}")]);
            }
        }
        a.extend([
            "-c".into(),
            "copy".into(),
            "-avoid_negative_ts".into(),
            "make_zero".into(),
            self.output.to_string_lossy().into_owned(),
        ]);
        a
    }
}

/// Default output name: `<stem>-trim.<ext>` next to the input.
pub fn default_output(input: &Path) -> PathBuf {
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "clip".into());
    let ext = input.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "mp4".into());
    input.with_file_name(format!("{stem}-trim.{ext}"))
}

/// Look for ffmpeg next to the app first, then on PATH.
pub fn find_ffmpeg(app_dir: Option<&Path>) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    if let Some(dir) = app_dir {
        let p = dir.join(exe);
        if p.is_file() {
            return Some(p);
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|d| d.join(exe)).find(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(start: f64, end: f64) -> TrimJob {
        TrimJob {
            input: "in.mp4".into(),
            output: "out.mp4".into(),
            start_secs: start,
            end_secs: end,
            last_secs: None,
            keep_audio: vec![],
        }
    }

    #[test]
    fn builds_stream_copy_args() {
        let args = job(2.5, 12.0).ffmpeg_args();
        let joined = args.join(" ");
        assert!(joined.contains("-ss 2.500 -i in.mp4 -t 9.500"), "{joined}");
        assert!(joined.contains("-c copy"));
        assert!(joined.ends_with("out.mp4"));
        assert!(joined.contains("-map 0:a?"));
    }

    #[test]
    fn maps_only_selected_audio_tracks() {
        let mut j = job(0.0, 5.0);
        j.keep_audio = vec![0, 2];
        let joined = j.ffmpeg_args().join(" ");
        assert!(joined.contains("-map 0:a:0 -map 0:a:2"));
        assert!(!joined.contains("0:a?"));
    }

    #[test]
    fn validates_ranges_and_paths() {
        assert_eq!(job(5.0, 5.05).validate(), Err(TrimError::RangeTooShort));
        assert_eq!(job(-1.0, 5.0).validate(), Err(TrimError::NegativeStart));
        let mut same = job(0.0, 5.0);
        same.output = same.input.clone();
        assert_eq!(same.validate(), Err(TrimError::SameFile));
        assert!(job(0.0, 5.0).validate().is_ok());
    }

    #[test]
    fn last_seconds_uses_sseof_and_no_duration() {
        let mut j = job(0.0, 0.0);
        j.last_secs = Some(15.0);
        assert!(j.validate().is_ok());
        let joined = j.ffmpeg_args().join(" ");
        assert!(joined.contains("-sseof -15.000 -i in.mp4"), "{joined}");
        assert!(!joined.contains(" -t "));
        j.last_secs = Some(0.01);
        assert_eq!(j.validate(), Err(TrimError::RangeTooShort));
    }

    #[test]
    fn default_output_sits_beside_input() {
        assert_eq!(default_output(Path::new("/clips/a b.mkv")), PathBuf::from("/clips/a b-trim.mkv"));
    }
}
