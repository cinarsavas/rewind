//! Pure maths for the on-screen notification: placement, easing and anti-aliased
//! rounded-rectangle coverage. The Win32 drawing code lives in the app crate.

use crate::config::Corner;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastSpec {
    pub kind: ToastKind,
    pub title: String,
    pub subtitle: String,
}

/// Top-left position (in pixels) for a `w`×`h` toast tucked into `corner` of `monitor`.
pub fn place(corner: Corner, monitor: Rect, w: i32, h: i32, margin: i32) -> (i32, i32) {
    let left = monitor.x + margin;
    let right = monitor.x + monitor.w - w - margin;
    let top = monitor.y + margin;
    let bottom = monitor.y + monitor.h - h - margin;
    match corner {
        Corner::TopLeft => (left, top),
        Corner::TopRight => (right, top),
        Corner::BottomLeft => (left, bottom),
        Corner::BottomRight => (right, bottom),
    }
}

const FADE_IN: f32 = 0.18;
const FADE_OUT: f32 = 0.35;

/// Opacity 0..=1 at `t` seconds into a toast that lasts `total` seconds.
pub fn alpha_at(t: f32, total: f32) -> f32 {
    if t <= 0.0 || t >= total {
        return 0.0;
    }
    let fade_in = (t / FADE_IN).min(1.0);
    let fade_out = ((total - t) / FADE_OUT).min(1.0);
    let ease = |x: f32| x * x * (3.0 - 2.0 * x);
    ease(fade_in) * ease(fade_out)
}

/// How far (0..=1) the toast still has to slide into place; 1 at the start, 0 once settled.
pub fn slide_remaining(t: f32) -> f32 {
    let k = (t / FADE_IN).clamp(0.0, 1.0);
    (1.0 - k).powi(3)
}

/// Anti-aliased coverage (0..=1) of the pixel centred at (`px`,`py`) by a rounded rectangle
/// spanning (0,0)–(`w`,`h`) with corner radius `r`.
pub fn rounded_rect_coverage(px: f32, py: f32, w: f32, h: f32, r: f32) -> f32 {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (dx, dy) = ((px - cx).abs() - (cx - r), (py - cy).abs() - (cy - r));
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    let inside = dx.max(dy).min(0.0);
    let signed_distance = outside + inside - r;
    (0.5 - signed_distance).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect { x: 100, y: 50, w: 1920, h: 1080 };

    #[test]
    fn places_in_each_corner_with_margin() {
        assert_eq!(place(Corner::TopLeft, MON, 300, 80, 20), (120, 70));
        assert_eq!(place(Corner::TopRight, MON, 300, 80, 20), (100 + 1920 - 300 - 20, 70));
        assert_eq!(place(Corner::BottomLeft, MON, 300, 80, 20), (120, 50 + 1080 - 80 - 20));
        assert_eq!(place(Corner::BottomRight, MON, 300, 80, 20), (1700, 1030));
    }

    #[test]
    fn alpha_fades_in_holds_and_fades_out() {
        assert_eq!(alpha_at(0.0, 3.0), 0.0);
        assert!(alpha_at(0.05, 3.0) > 0.0 && alpha_at(0.05, 3.0) < 1.0);
        assert_eq!(alpha_at(1.5, 3.0), 1.0);
        assert!(alpha_at(2.9, 3.0) < 0.5);
        assert_eq!(alpha_at(3.0, 3.0), 0.0);
        assert_eq!(alpha_at(5.0, 3.0), 0.0);
    }

    #[test]
    fn short_toast_never_exceeds_one() {
        for i in 0..100 {
            let a = alpha_at(i as f32 * 0.01, 0.5);
            assert!((0.0..=1.0).contains(&a));
        }
    }

    #[test]
    fn slide_settles() {
        assert_eq!(slide_remaining(0.0), 1.0);
        assert_eq!(slide_remaining(1.0), 0.0);
        assert!(slide_remaining(0.09) < 0.2);
    }

    #[test]
    fn coverage_is_opaque_inside_clear_outside_and_soft_on_corners() {
        let (w, h, r) = (100.0, 40.0, 12.0);
        assert_eq!(rounded_rect_coverage(50.0, 20.0, w, h, r), 1.0);
        assert_eq!(rounded_rect_coverage(-5.0, 20.0, w, h, r), 0.0);
        // The extreme top-left pixel is outside the rounded corner.
        assert!(rounded_rect_coverage(0.5, 0.5, w, h, r) < 0.1);
        // A pixel on the straight edge is (nearly) fully covered.
        assert!(rounded_rect_coverage(50.0, 0.5, w, h, r) > 0.95);
        // Somewhere along the curve gives partial coverage.
        let partial = (0..12)
            .map(|i| rounded_rect_coverage(i as f32 + 0.5, 3.5, w, h, r))
            .any(|c| c > 0.05 && c < 0.95);
        assert!(partial);
    }
}
