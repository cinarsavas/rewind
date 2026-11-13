//! Tiny celebration effects played when a switch turns on.
//!
//! Everything here is drawn with a handful of plain egui shapes, so it stays cheap on the
//! friend's HD 5650 laptop. Effects only run while animations are on and the low-power UI is
//! off; otherwise the queue is ignored and nothing is painted.

use std::cell::{Cell, RefCell};
use std::f32::consts::{PI, TAU};
use std::time::Instant;

use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, LayerId, Order, Pos2, Rect, Shape, Stroke};

use rewind_core::config::EffectStyle;

use super::theme::pal;

thread_local! {
    static STYLE: Cell<EffectStyle> = const { Cell::new(EffectStyle::Shine) };
    static QUEUE: RefCell<Vec<(EffectStyle, Pos2)>> = const { RefCell::new(Vec::new()) };
}

/// The celebration every switch should use. Set once per frame from the applied config.
pub fn set_style(style: EffectStyle) {
    STYLE.with(|s| s.set(style));
}

/// Queue a celebration at `at` using the style currently selected in Settings.
pub fn queue_current(at: Pos2) {
    let style = STYLE.with(|s| s.get());
    if style != EffectStyle::None {
        QUEUE.with(|q| q.borrow_mut().push((style, at)));
    }
}

/// Queue a specific celebration. Used by the "Try it" preview so it can show the option
/// under the cursor before it is applied.
pub fn queue(style: EffectStyle, at: Pos2) {
    if style != EffectStyle::None {
        QUEUE.with(|q| q.borrow_mut().push((style, at)));
    }
}

/// Take everything queued since the last frame.
pub fn take_queued() -> Vec<(EffectStyle, Pos2)> {
    QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// One live celebration. Kept small and copy-free apart from the position.
pub struct FxEvent {
    pub style: EffectStyle,
    pub at: Pos2,
    start: Instant,
    seed: u32,
}

impl FxEvent {
    pub fn new(style: EffectStyle, at: Pos2, seed: u32) -> Self {
        Self { style, at, start: Instant::now(), seed }
    }

    fn age(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// How long this style lives, in seconds.
    pub fn duration(&self) -> f32 {
        match self.style {
            EffectStyle::None => 0.0,
            EffectStyle::Shine => 0.62,
            EffectStyle::Sparkle => 0.85,
            EffectStyle::Creeper => 1.25,
            EffectStyle::Ripple => 0.9,
        }
    }

    pub fn expired(&self) -> bool {
        self.age() >= self.duration()
    }
}

/// Paint every live event on a foreground layer so it floats above the widgets.
pub fn draw(ctx: &egui::Context, events: &[FxEvent]) {
    if events.is_empty() {
        return;
    }
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::unique("rewind-fx")));
    for ev in events {
        let span = ev.duration();
        if span <= 0.0 {
            continue;
        }
        let t = (ev.age() / span).clamp(0.0, 1.0);
        match ev.style {
            EffectStyle::None => {}
            EffectStyle::Shine => shine(&painter, ev, t),
            EffectStyle::Sparkle => sparkle(&painter, ev, t),
            EffectStyle::Creeper => creeper(&painter, ev, t),
            EffectStyle::Ripple => ripple(&painter, ev, t),
        }
    }
}

// ------------------------------------------------------------------ painters

/// A soft diagonal glint sweeping across the switch, with a brief halo.
fn shine(p: &egui::Painter, ev: &FxEvent, t: f32) {
    let accent = pal().accent;
    let glow = pal().on_accent;
    let e = ease_out_cubic(t);
    let fade = (1.0 - t).powf(1.3);

    // Breathing halo behind the sweep.
    p.circle_filled(ev.at, 24.0 + 20.0 * e, accent.gamma_multiply(fade * 0.12));

    // Slanted light band travelling left to right.
    let x = ev.at.x - 54.0 + 108.0 * e;
    let half = 9.0;
    let slant = 13.0;
    let top = ev.at.y - 22.0;
    let bot = ev.at.y + 22.0;
    let band = vec![
        pos2(x - half, top),
        pos2(x + half, top),
        pos2(x + half + slant, bot),
        pos2(x - half + slant, bot),
    ];
    p.add(Shape::convex_polygon(band, glow.gamma_multiply(fade * 0.55), Stroke::NONE));

    // Bright core line so the glint reads on light themes too.
    p.line_segment(
        [pos2(x + slant * 0.5, top + 3.0), pos2(x + slant * 0.5, bot - 3.0)],
        Stroke::new(2.0, accent.gamma_multiply(fade * 0.5)),
    );

    // Two tiny sparks at the leading edge.
    let spark = accent.gamma_multiply(fade * 0.9);
    let sy = ev.at.y - 14.0 + 8.0 * e;
    p.line_segment([pos2(x + 12.0, sy - 3.0), pos2(x + 12.0, sy + 3.0)], Stroke::new(1.2, spark));
    p.line_segment([pos2(x + 9.0, sy), pos2(x + 15.0, sy)], Stroke::new(1.2, spark));
}

/// A small burst of four-point stars that drift out and fall gently.
fn sparkle(p: &egui::Painter, ev: &FxEvent, t: f32) {
    const N: usize = 14;
    let accent = pal().accent;
    let bright = pal().on_accent;
    let e = ease_out_cubic(t);
    let fade = (1.0 - t).powf(1.2);

    for i in 0..N {
        let a = (i as f32 / N as f32) * TAU + hash(ev.seed, i) * 0.9;
        let speed = 24.0 + hash(ev.seed ^ 0x51, i) * 34.0;
        let dist = speed * e;
        let fall = 26.0 * t * t;
        let pos = pos2(ev.at.x + a.cos() * dist, ev.at.y + a.sin() * dist + fall);
        let size = (2.4 * (1.0 - t) + 0.7) * (0.7 + hash(ev.seed ^ 0x9e, i) * 0.7);
        let col = if i % 3 == 0 { bright } else { accent }.gamma_multiply(fade);
        p.line_segment([pos2(pos.x - size * 1.8, pos.y), pos2(pos.x + size * 1.8, pos.y)], Stroke::new(1.0, col));
        p.line_segment([pos2(pos.x, pos.y - size * 1.8), pos2(pos.x, pos.y + size * 1.8)], Stroke::new(1.0, col));
        if i % 4 == 0 {
            p.circle_filled(pos, size * 0.5, col);
        }
    }

    // A short central pop before the stars spread out.
    let pop = ((1.0 - t * 2.2).max(0.0)).powf(2.0);
    p.circle_filled(ev.at, 9.0 * pop, accent.gamma_multiply(0.55 * pop));
}

/// A pixel creeper that swells, flashes, then bursts into green squares.
fn creeper(p: &egui::Painter, ev: &FxEvent, t: f32) {
    let green = Color32::from_rgb(78, 156, 44);
    let dark = Color32::from_rgb(20, 30, 16);
    let flash_at = 0.42;

    if t < flash_at {
        let k = (t / flash_at).clamp(0.0, 1.0);
        let scale = 0.55 + 0.45 * ease_out_back(k);
        let bob = (k * PI).sin() * 1.5;
        draw_creeper(p, ev.at + vec2(0.0, bob), 3.8 * scale, green, dark, 1.0);
        let charge = ((k - 0.65) / 0.35).clamp(0.0, 1.0);
        if charge > 0.0 {
            p.circle_filled(ev.at, 18.0 + 12.0 * charge, Color32::WHITE.gamma_multiply(0.16 * charge));
        }
    } else {
        let k = ((t - flash_at) / (1.0 - flash_at)).clamp(0.0, 1.0);
        let fade = (1.0 - k).powf(1.1);

        // White flash and shockwave ring.
        p.circle_filled(ev.at, 20.0 + 26.0 * k, Color32::WHITE.gamma_multiply(0.28 * fade * fade));
        p.circle_stroke(
            ev.at,
            8.0 + 44.0 * ease_out_cubic(k),
            Stroke::new(2.5 * fade + 0.5, green.gamma_multiply(fade * 0.8)),
        );

        // Square blocks flying out with a little gravity.
        for i in 0..18 {
            let a = (i as f32 / 18.0) * TAU + hash(ev.seed, i) * 0.7;
            let speed = 30.0 + hash(ev.seed ^ 0x2d, i) * 44.0;
            let d = speed * ease_out_cubic(k);
            let pos = pos2(ev.at.x + a.cos() * d, ev.at.y + a.sin() * d + 34.0 * k * k);
            let s = (3.2 * fade + 0.9) * (0.7 + hash(ev.seed ^ 0x7a, i) * 0.8);
            let col = if i % 4 == 0 { dark } else { green }.gamma_multiply(fade);
            p.rect_filled(Rect::from_center_size(pos, vec2(s, s)), CornerRadius::ZERO, col);
        }
    }
}

/// Calm rings expanding out from the switch.
fn ripple(p: &egui::Painter, ev: &FxEvent, t: f32) {
    let accent = pal().accent;
    for i in 0..3 {
        let offset = i as f32 * 0.13;
        let k = ((t - offset) / (1.0 - offset)).clamp(0.0, 1.0);
        if k <= 0.0 {
            continue;
        }
        let r = 5.0 + 32.0 * ease_out_cubic(k);
        let fade = (1.0 - k) * (0.75 - i as f32 * 0.16);
        p.circle_stroke(ev.at, r, Stroke::new(2.6 * (1.0 - k) + 0.5, accent.gamma_multiply(fade)));
    }
    p.circle_filled(ev.at, 3.5 * (1.0 - t), accent.gamma_multiply((1.0 - t) * 0.8));
}

/// The classic 8x8 creeper face, drawn as solid pixels.
fn draw_creeper(p: &egui::Painter, c: Pos2, px: f32, green: Color32, dark: Color32, alpha: f32) {
    const FACE: [u8; 64] = [
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 1, 1, 0, 0, 1, 1, 0,
        0, 1, 1, 0, 0, 1, 1, 0,
        0, 0, 0, 1, 1, 0, 0, 0,
        0, 0, 1, 1, 1, 1, 0, 0,
        0, 0, 1, 1, 1, 1, 0, 0,
        0, 0, 1, 0, 0, 1, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let half = px * 4.0;
    let origin = pos2(c.x - half, c.y - half);
    p.rect_filled(
        Rect::from_center_size(c, vec2(px * 8.0 + 4.0, px * 8.0 + 4.0)),
        CornerRadius::same(3),
        Color32::BLACK.gamma_multiply(0.22 * alpha),
    );
    for row in 0..8 {
        for col in 0..8 {
            let on = FACE[row * 8 + col] == 1;
            let rect = Rect::from_min_size(
                pos2(origin.x + col as f32 * px, origin.y + row as f32 * px),
                vec2(px, px),
            );
            let colour = if on { dark } else { green };
            p.rect_filled(rect, CornerRadius::ZERO, colour.gamma_multiply(alpha));
        }
    }
}

// ------------------------------------------------------------------ helpers

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_out_back(t: f32) -> f32 {
    const C1: f32 = 1.70158;
    const C3: f32 = C1 + 1.0;
    let u = t - 1.0;
    1.0 + C3 * u * u * u + C1 * u * u
}

/// Deterministic 0..1 noise so each burst looks slightly different without a rand crate.
fn hash(seed: u32, i: usize) -> f32 {
    let mut x = seed ^ (i as u32).wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x & 0xFFFF) as f32 / 65535.0
}
