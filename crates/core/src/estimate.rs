//! Back-of-envelope numbers shown in the UI so settings don't surprise people.

/// Approximate RAM (MB) the rolling buffer needs: video + audio bitrate over the buffer
/// length, plus ~10% for container/keyframe overhead.
pub fn buffer_ram_mb(video_kbps: u32, audio_kbps: u32, secs: u32) -> u32 {
    let kbit = (video_kbps as u64 + audio_kbps as u64) * secs as u64;
    let mb = kbit as f64 / 8.0 / 1000.0 * 1.10;
    mb.ceil() as u32
}

/// Approximate size of one saved clip in MB (same maths, without overhead padding).
pub fn clip_size_mb(video_kbps: u32, audio_kbps: u32, secs: u32) -> u32 {
    let kbit = (video_kbps as u64 + audio_kbps as u64) * secs as u64;
    (kbit as f64 / 8.0 / 1000.0).ceil() as u32
}

/// A sensible starting bitrate (kbps) for a given output height and frame rate.
pub fn suggested_bitrate_kbps(height: u32, fps: u32) -> u32 {
    let base = match height {
        0..=720 => 8_000,
        721..=1080 => 16_000,
        1081..=1440 => 28_000,
        _ => 50_000,
    };
    let scaled = if fps > 60 { base as f32 * (1.0 + (fps as f32 - 60.0) / 180.0) } else { base as f32 };
    (scaled as u32 / 500) * 500
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_estimate_is_sane() {
        // 20 Mbps video + 192 kbps audio for 60 s ≈ 152 MB (+10%).
        let mb = buffer_ram_mb(20_000, 192, 60);
        assert!((160..=175).contains(&mb), "got {mb}");
    }

    #[test]
    fn clip_size_matches_bitrate() {
        assert_eq!(clip_size_mb(8_000, 0, 8), 8);
    }

    #[test]
    fn suggestions_scale_with_resolution_and_fps() {
        assert!(suggested_bitrate_kbps(1080, 60) < suggested_bitrate_kbps(1440, 60));
        assert!(suggested_bitrate_kbps(1080, 60) < suggested_bitrate_kbps(1080, 240));
        assert_eq!(suggested_bitrate_kbps(1080, 60), 16_000);
    }
}
