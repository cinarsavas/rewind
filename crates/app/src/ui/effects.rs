//! Tiny celebration effects played when a switch turns on.
//!
//! Everything here is drawn with a handful of plain egui shapes, so it stays cheap on the
//! friend's HD 5650 laptop. Effects only run while animations are on and the low-power UI is
//! off; otherwise the queue is ignored and nothing is painted.

use std::cell::{Cell, RefCell};
use std::f32::consts::{PI, TAU};
use std::time::Instant;

use eframe::egui::{
    self, pos2, vec2, Color32, CornerRadius, LayerId, Order, Pos2, Rect, Shape, Stroke,
};

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
        Self {
            style,
            at,
            start: Instant::now(),
            seed,
        }
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
            EffectStyle::Confetti => 1.15,
            EffectStyle::Comet => 0.72,
            EffectStyle::Bloom => 0.85,
            EffectStyle::Bolt => 0.55,
        }
    }

    pub fn expired(&self) -> bool {
        self.age() >= self.duration()
    }
}

/// Keep a celebration fully on screen even when its switch sits near an edge. The effect
/// radius is about 70 points, so leave that much room.
pub fn clamp_to_screen(ctx: &egui::Context, at: Pos2) -> Pos2 {
    const MARGIN: f32 = 72.0;
    let r = ctx.content_rect();
    let x = if r.width() > MARGIN * 2.0 {
        at.x.clamp(r.left() + MARGIN, r.right() - MARGIN)
    } else {
        r.center().x
    };
    let y = if r.height() > MARGIN * 2.0 {
        at.y.clamp(r.top() + MARGIN, r.bottom() - MARGIN)
    } else {
        r.center().y
    };
    pos2(x, y)
}

/// Paint every live event on a foreground layer so it floats above the widgets.
pub fn draw(ctx: &egui::Context, events: &[FxEvent]) {
    if events.is_empty() {
        return;
    }
    // A stable layer id: a fresh `Id::unique` every frame would register a new layer each
    // frame and grow egui's layer table for as long as an effect is playing.
    let painter = ctx.layer_painter(LayerId::new(
        Order::Foreground,
        egui::Id::unique("rewind-fx"),
    ));
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
            EffectStyle::Confetti => confetti(&painter, ev, t),
            EffectStyle::Comet => comet(&painter, ev, t),
            EffectStyle::Bloom => bloom(&painter, ev, t),
            EffectStyle::Bolt => bolt(&painter, ev, t),
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
    p.add(Shape::convex_polygon(
        band,
        glow.gamma_multiply(fade * 0.55),
        Stroke::NONE,
    ));

    // Bright core line so the glint reads on light themes too.
    p.line_segment(
        [
            pos2(x + slant * 0.5, top + 3.0),
            pos2(x + slant * 0.5, bot - 3.0),
        ],
        Stroke::new(2.0, accent.gamma_multiply(fade * 0.5)),
    );

    // Two tiny sparks at the leading edge.
    let spark = accent.gamma_multiply(fade * 0.9);
    let sy = ev.at.y - 14.0 + 8.0 * e;
    p.line_segment(
        [pos2(x + 12.0, sy - 3.0), pos2(x + 12.0, sy + 3.0)],
        Stroke::new(1.2, spark),
    );
    p.line_segment(
        [pos2(x + 9.0, sy), pos2(x + 15.0, sy)],
        Stroke::new(1.2, spark),
    );
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
        p.line_segment(
            [
                pos2(pos.x - size * 1.8, pos.y),
                pos2(pos.x + size * 1.8, pos.y),
            ],
            Stroke::new(1.0, col),
        );
        p.line_segment(
            [
                pos2(pos.x, pos.y - size * 1.8),
                pos2(pos.x, pos.y + size * 1.8),
            ],
            Stroke::new(1.0, col),
        );
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
            p.circle_filled(
                ev.at,
                18.0 + 12.0 * charge,
                Color32::WHITE.gamma_multiply(0.16 * charge),
            );
        }
    } else {
        let k = ((t - flash_at) / (1.0 - flash_at)).clamp(0.0, 1.0);
        let fade = (1.0 - k).powf(1.1);

        // White flash and shockwave ring.
        p.circle_filled(
            ev.at,
            20.0 + 26.0 * k,
            Color32::WHITE.gamma_multiply(0.28 * fade * fade),
        );
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
            p.rect_filled(
                Rect::from_center_size(pos, vec2(s, s)),
                CornerRadius::ZERO,
                col,
            );
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
        p.circle_stroke(
            ev.at,
            r,
            Stroke::new(2.6 * (1.0 - k) + 0.5, accent.gamma_multiply(fade)),
        );
    }
    p.circle_filled(
        ev.at,
        3.5 * (1.0 - t),
        accent.gamma_multiply((1.0 - t) * 0.8),
    );
}

/// The classic 8x8 creeper face, drawn as solid pixels.
fn draw_creeper(p: &egui::Painter, c: Pos2, px: f32, green: Color32, dark: Color32, alpha: f32) {
    const FACE: [u8; 64] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 1, 1, 0,
        0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0,
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

/// Colourful paper thrown up and out, then pulled down by gravity.
fn confetti(p: &egui::Painter, ev: &FxEvent, t: f32) {
    const N: usize = 26;
    let accent = pal().accent;
    let bright = pal().on_accent;
    let cols = [accent, bright, pal().warn, pal().accent_dim];
    let e = ease_out_cubic(t);
    let fade = (1.0 - t).powf(0.85);

    for i in 0..N {
        let a = -PI / 2.0 + (hash(ev.seed, i) - 0.5) * 2.4;
        let speed = 55.0 + hash(ev.seed ^ 0x31, i) * 90.0;
        let d = speed * e;
        let fall = 120.0 * t * t;
        let pos = pos2(ev.at.x + a.cos() * d, ev.at.y + a.sin() * d + fall);
        let s = (4.6 - 2.4 * t) * (0.65 + hash(ev.seed ^ 0x77, i) * 0.8);
        let rot = hash(ev.seed ^ 0xa5, i) * TAU + t * (4.0 + hash(ev.seed ^ 0x13, i) * 6.0);
        let col = cols[i % cols.len()].gamma_multiply(fade);
        let (sin, cos) = rot.sin_cos();
        let (hx, hy) = (s, s * 0.55);
        let (ax, ay) = (cos * hx, sin * hx);
        let (bx, by) = (-sin * hy, cos * hy);
        let pts = vec![
            pos2(pos.x + ax + bx, pos.y + ay + by),
            pos2(pos.x - ax + bx, pos.y - ay + by),
            pos2(pos.x - ax - bx, pos.y - ay - by),
            pos2(pos.x + ax - bx, pos.y + ay - by),
        ];
        p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    }

    let pop = (1.0 - t * 2.5).max(0.0);
    p.circle_filled(ev.at, 10.0 * pop, bright.gamma_multiply(0.35 * pop));
}

/// A bright head with a tapering tail, sweeping across the control.
fn comet(p: &egui::Painter, ev: &FxEvent, t: f32) {
    let accent = pal().accent;
    let bright = pal().on_accent;
    let e = ease_out_cubic(t);
    let fade = (1.0 - t).powf(1.2);
    let dir = vec2(1.0, 0.45);
    let start = ev.at - dir * 46.0;
    let end = ev.at + dir * 46.0;
    let head = pos2(
        start.x + (end.x - start.x) * e,
        start.y + (end.y - start.y) * e,
    );

    for i in 0..7 {
        let k = i as f32 / 7.0;
        let tail = pos2(
            head.x - dir.x * (6.0 + k * 34.0),
            head.y - dir.y * (6.0 + k * 34.0),
        );
        let w = (1.0 - k) * 3.2 * fade + 0.4;
        p.line_segment(
            [head, tail],
            Stroke::new(w, accent.gamma_multiply(fade * (1.0 - k) * 0.7)),
        );
    }

    p.circle_filled(head, 10.0 * fade, accent.gamma_multiply(fade * 0.18));
    p.circle_filled(head, 3.4 * fade + 1.0, bright.gamma_multiply(fade));

    for i in 0..4 {
        let k = ((t - i as f32 * 0.08) / 0.35).clamp(0.0, 1.0);
        if k <= 0.0 {
            continue;
        }
        let pos = pos2(
            head.x - dir.x * (14.0 + i as f32 * 9.0),
            head.y - dir.y * (14.0 + i as f32 * 9.0) + 8.0 * k,
        );
        p.circle_filled(pos, 1.6 * (1.0 - k), accent.gamma_multiply((1.0 - k) * 0.6));
    }
}

/// Petals unfolding from a bright centre.
fn bloom(p: &egui::Painter, ev: &FxEvent, t: f32) {
    const PETALS: usize = 7;
    let accent = pal().accent;
    let bright = pal().on_accent;
    let open = ease_out_cubic((t / 0.55).clamp(0.0, 1.0));
    let fade = (1.0 - t).powf(0.9);

    for i in 0..PETALS {
        let a = (i as f32 / PETALS as f32) * TAU - PI / 2.0 + hash(ev.seed, i) * 0.25;
        let dist = 4.0 + 17.0 * open;
        let c = pos2(ev.at.x + a.cos() * dist, ev.at.y + a.sin() * dist);
        let size = (6.0 - 2.5 * t) * (0.75 + hash(ev.seed ^ 0x4b, i) * 0.5);
        let (rs, rc) = (a - PI / 2.0).sin_cos();
        let mut pts = Vec::with_capacity(12);
        for j in 0..12 {
            let ang = (j as f32 / 12.0) * TAU;
            let (lx, ly) = (size * 0.55 * ang.cos(), size * ang.sin());
            pts.push(pos2(c.x + lx * rc - ly * rs, c.y + lx * rs + ly * rc));
        }
        let col = if i % 3 == 0 {
            bright
        } else if i % 3 == 1 {
            accent
        } else {
            pal().accent_dim
        };
        p.add(Shape::convex_polygon(
            pts,
            col.gamma_multiply(fade * 0.65),
            Stroke::NONE,
        ));
    }

    p.circle_filled(ev.at, 9.0 * open, bright.gamma_multiply(fade * 0.7));
    p.circle_filled(ev.at, 4.0 * open, bright.gamma_multiply(fade));
}

/// A quick jagged spark of lightning.
fn bolt(p: &egui::Painter, ev: &FxEvent, t: f32) {
    let accent = pal().accent;
    let bright = pal().on_accent;
    let flash = (1.0 - t).powf(2.2);
    let scale = 0.85 + 0.35 * ease_out_cubic(t);
    let pts: Vec<Pos2> = [
        (-5.0, -26.0),
        (4.0, -7.0),
        (-3.0, -5.0),
        (7.0, 12.0),
        (1.0, 10.0),
        (6.0, 27.0),
    ]
    .iter()
    .map(|(x, y)| pos2(ev.at.x + x * scale, ev.at.y + y * scale))
    .collect();

    p.add(Shape::line(
        pts.clone(),
        Stroke::new(7.0 * flash + 1.0, accent.gamma_multiply(0.10 * flash)),
    ));
    p.add(Shape::line(
        pts.clone(),
        Stroke::new(3.4 * flash + 0.6, accent.gamma_multiply(0.8 * flash)),
    ));
    p.add(Shape::line(
        pts,
        Stroke::new(1.2 * flash + 0.3, bright.gamma_multiply(flash)),
    ));
    p.circle_filled(ev.at, 16.0 * flash, accent.gamma_multiply(0.10 * flash));
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
