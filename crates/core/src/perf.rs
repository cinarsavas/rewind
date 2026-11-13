//! Tiny helpers for the "how heavy is Rewind?" readout.

/// Process CPU use as a percentage of the *whole machine* (like Task Manager's CPU column).
/// `cpu_100ns` is the growth in kernel+user time, `wall_100ns` the elapsed wall-clock time,
/// both in 100-nanosecond units.
pub fn cpu_percent(cpu_100ns: u64, wall_100ns: u64, logical_cores: usize) -> f32 {
    if wall_100ns == 0 || logical_cores == 0 {
        return 0.0;
    }
    let pct = cpu_100ns as f64 / (wall_100ns as f64 * logical_cores as f64) * 100.0;
    pct.clamp(0.0, 100.0) as f32
}

/// Exponential smoothing so the readout doesn't flicker.
pub fn smooth(previous: f32, sample: f32, alpha: f32) -> f32 {
    previous + (sample - previous) * alpha.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_busy_core_of_four_is_25_percent() {
        assert!((cpu_percent(10_000_000, 10_000_000, 4) - 25.0).abs() < 0.01);
    }

    #[test]
    fn handles_zero_and_overflow() {
        assert_eq!(cpu_percent(5, 0, 4), 0.0);
        assert_eq!(cpu_percent(5, 5, 0), 0.0);
        assert_eq!(cpu_percent(1_000_000_000, 10, 2), 100.0);
    }

    #[test]
    fn smoothing_moves_toward_the_sample() {
        assert_eq!(smooth(0.0, 10.0, 0.5), 5.0);
        assert_eq!(smooth(3.0, 3.0, 0.5), 3.0);
        assert_eq!(smooth(0.0, 10.0, 5.0), 10.0);
    }
}
