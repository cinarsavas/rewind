//! Custom-painted widgets: icons, switches, segmented controls, setting rows, progress ring.

use std::f32::consts::{PI, TAU};

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, Ui,
};

use super::theme::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Record,
    Clips,
    Sliders,
    Info,
    Play,
    Folder,
    Trash,
    Refresh,
    Scissors,
    Addons,
    Download,
    Check,
    Warning,
}

pub fn paint_icon(p: &egui::Painter, icon: Icon, c: Pos2, size: f32, color: Color32) {
    let s = size / 2.0;
    let st = Stroke::new((size / 11.0).max(1.4), color);
    match icon {
        Icon::Record => {
            p.circle_stroke(c, s * 0.92, st);
            p.circle_filled(c, s * 0.42, color);
        }
        Icon::Clips => {
            let r = Rect::from_center_size(c, vec2(size * 0.95, size * 0.74));
            p.rect_stroke(r, CornerRadius::same((size * 0.2) as u8), st, StrokeKind::Inside);
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - s * 0.22, c.y - s * 0.34), pos2(c.x + s * 0.4, c.y), pos2(c.x - s * 0.22, c.y + s * 0.34)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Sliders => {
            for (i, (y, k)) in [(-0.55, 0.3), (0.0, -0.35), (0.55, 0.15)].into_iter().enumerate() {
                let yy = c.y + y * s;
                p.line_segment([pos2(c.x - s * 0.95, yy), pos2(c.x + s * 0.95, yy)], st);
                p.circle_filled(pos2(c.x + k * s, yy), s * 0.26, color);
                let _ = i;
            }
        }
        Icon::Info => {
            p.circle_stroke(c, s * 0.92, st);
            p.circle_filled(pos2(c.x, c.y - s * 0.4), s * 0.1, color);
            p.line_segment([pos2(c.x, c.y - s * 0.1), pos2(c.x, c.y + s * 0.45)], st);
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - s * 0.4, c.y - s * 0.62), pos2(c.x + s * 0.7, c.y), pos2(c.x - s * 0.4, c.y + s * 0.62)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Folder => {
            let pts = vec![
                pos2(c.x - s * 0.9, c.y - s * 0.55),
                pos2(c.x - s * 0.25, c.y - s * 0.55),
                pos2(c.x + s * 0.0, c.y - s * 0.25),
                pos2(c.x + s * 0.9, c.y - s * 0.25),
                pos2(c.x + s * 0.9, c.y + s * 0.6),
                pos2(c.x - s * 0.9, c.y + s * 0.6),
            ];
            p.add(Shape::closed_line(pts, st));
        }
        Icon::Trash => {
            p.line_segment([pos2(c.x - s * 0.8, c.y - s * 0.5), pos2(c.x + s * 0.8, c.y - s * 0.5)], st);
            p.line_segment([pos2(c.x - s * 0.25, c.y - s * 0.75), pos2(c.x + s * 0.25, c.y - s * 0.75)], st);
            p.add(Shape::closed_line(
                vec![pos2(c.x - s * 0.6, c.y - s * 0.5), pos2(c.x - s * 0.5, c.y + s * 0.8), pos2(c.x + s * 0.5, c.y + s * 0.8), pos2(c.x + s * 0.6, c.y - s * 0.5)],
                st,
            ));
        }
        Icon::Refresh => {
            let pts: Vec<Pos2> = (0..=22).map(|i| {
                let a = -PI * 0.15 + (i as f32 / 22.0) * PI * 1.6;
                pos2(c.x + a.cos() * s * 0.78, c.y + a.sin() * s * 0.78)
            }).collect();
            let end = *pts.last().unwrap();
            p.add(Shape::line(pts, st));
            p.add(Shape::convex_polygon(vec![pos2(end.x - s * 0.1, end.y - s * 0.5), pos2(end.x + s * 0.45, end.y + s * 0.05), pos2(end.x - s * 0.5, end.y + s * 0.15)], color, Stroke::NONE));
        }
        Icon::Download => {
            p.line_segment([pos2(c.x, c.y - s * 0.75), pos2(c.x, c.y + s * 0.35)], st);
            p.add(Shape::line(vec![pos2(c.x - s * 0.45, c.y - s * 0.1), pos2(c.x, c.y + s * 0.4), pos2(c.x + s * 0.45, c.y - s * 0.1)], st));
            p.line_segment([pos2(c.x - s * 0.8, c.y + s * 0.8), pos2(c.x + s * 0.8, c.y + s * 0.8)], st);
        }
        Icon::Addons => {
            let q = s * 0.42;
            for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0)] {
                let r = Rect::from_center_size(pos2(c.x + dx * s * 0.5, c.y + dy * s * 0.5), vec2(q * 1.5, q * 1.5));
                p.rect_stroke(r, CornerRadius::same(2), st, StrokeKind::Inside);
            }
            p.line_segment([pos2(c.x + s * 0.5, c.y + s * 0.15), pos2(c.x + s * 0.5, c.y + s * 0.85)], st);
            p.line_segment([pos2(c.x + s * 0.15, c.y + s * 0.5), pos2(c.x + s * 0.85, c.y + s * 0.5)], st);
        }
        Icon::Scissors => {
            p.circle_stroke(pos2(c.x - s * 0.45, c.y + s * 0.5), s * 0.28, st);
            p.circle_stroke(pos2(c.x + s * 0.45, c.y + s * 0.5), s * 0.28, st);
            p.line_segment([pos2(c.x - s * 0.3, c.y + s * 0.25), pos2(c.x + s * 0.7, c.y - s * 0.75)], st);
            p.line_segment([pos2(c.x + s * 0.3, c.y + s * 0.25), pos2(c.x - s * 0.7, c.y - s * 0.75)], st);
        }
        Icon::Check => {
            p.add(Shape::line(
                vec![pos2(c.x - s * 0.62, c.y + s * 0.02), pos2(c.x - s * 0.15, c.y + s * 0.5), pos2(c.x + s * 0.66, c.y - s * 0.5)],
                Stroke::new((size / 8.5).max(1.6), color),
            ));
        }
        Icon::Warning => {
            p.add(Shape::closed_line(
                vec![pos2(c.x, c.y - s * 0.82), pos2(c.x + s * 0.9, c.y + s * 0.68), pos2(c.x - s * 0.9, c.y + s * 0.68)],
                st,
            ));
            p.line_segment([pos2(c.x, c.y - s * 0.18), pos2(c.x, c.y + s * 0.22)], st);
            p.circle_filled(pos2(c.x, c.y + s * 0.48), s * 0.1, color);
        }
    }
}

/// Square icon button with a hover background and a press compression.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    if resp.hovered() || pressed {
        let fill = if pressed { pal().accent_tint } else { pal().card_hi };
        ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    }
    let col = if pressed { pal().accent } else if resp.hovered() { color } else { color.gamma_multiply(0.8) };
    let size = if pressed { 15.5 } else if resp.hovered() { 18.0 } else { 17.0 };
    paint_icon(ui.painter(), icon, rect.center(), size, col);
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Text + icon button used for small actions ("Trim", "Open folder").
pub fn text_button(ui: &mut Ui, icon: Option<Icon>, label: &str, enabled: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(14.0), pal().text);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 28.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hovered = resp.hovered() && enabled;
    let pressed = resp.is_pointer_button_down_on() && enabled;
    let fill = if pressed { pal().accent_tint } else if hovered { pal().hover } else { pal().card_hi };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    ui.painter().rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0, pal().stroke), StrokeKind::Inside);
    let col = if enabled { pal().text } else { pal().faint };
    let mut x = rect.left() + 14.0;
    if let Some(i) = icon {
        paint_icon(ui.painter(), i, pos2(x + 7.0, rect.center().y), 15.0, col);
        x += 22.0;
    }
    ui.painter().text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), col);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// The big mint call-to-action button. Optionally shows the hotkey as a key-cap on the right.
pub fn primary_button(ui: &mut Ui, label: &str, keycap: Option<&str>, enabled: bool) -> Response {
    let size = vec2(ui.available_width(), 52.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() && enabled, anim_time(0.16));
    let pressed = resp.is_pointer_button_down_on() && enabled;
    let base = if enabled { pal().accent } else { pal().hover };
    let fill = if enabled {
        let f = base.lerp_to_gamma(Color32::WHITE, 0.12 * t);
        if pressed { f.lerp_to_gamma(Color32::BLACK, 0.10) } else { f }
    } else {
        base
    };
    ui.painter().rect_filled(rect, CornerRadius::same(14), fill);
    let fg = if enabled { pal().on_accent } else { pal().faint };
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::new(16.5, semibold_family()), fg);
    let center = rect.center() + vec2(0.0, if pressed { 1.0 } else { 0.0 });
    ui.painter().galley(pos2(center.x - galley.size().x / 2.0, center.y - galley.size().y / 2.0), galley, fg);
    if let Some(k) = keycap {
        let kg = ui.painter().layout_no_wrap(k.to_string(), FontId::proportional(12.5), fg);
        let kr = Rect::from_min_size(pos2(rect.right() - kg.size().x - 34.0, rect.center().y - 12.0), vec2(kg.size().x + 18.0, 24.0));
        ui.painter().rect_filled(kr, CornerRadius::same(7), if enabled { Color32::from_black_alpha(36) } else { Color32::from_black_alpha(40) });
        ui.painter().galley(pos2(kr.left() + 9.0, kr.center().y - kg.size().y / 2.0), kg, fg);
    }
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// iOS-style switch.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(42.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
        if *on {
            super::effects::queue_current(rect.center());
        }
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, anim_time(0.18));
    let track = pal().card_hi.lerp_to_gamma(pal().accent, t);
    ui.painter().rect_filled(rect, CornerRadius::same(12), track);
    if t < 1.0 {
        ui.painter().rect_stroke(rect, CornerRadius::same(12), Stroke::new(1.0, pal().stroke.gamma_multiply(1.0 - t)), StrokeKind::Inside);
    }
    let x = egui::lerp((rect.left() + 12.0)..=(rect.right() - 12.0), t);
    // A soft halo behind the knob while it is travelling, so the switch feels springy.
    if t > 0.0 && t < 1.0 {
        let halo = (t * PI).sin();
        ui.painter().circle_filled(pos2(x, rect.center().y), 12.0 + 4.0 * halo, pal().accent.gamma_multiply(0.18 * halo));
    }
    ui.painter().circle_filled(pos2(x, rect.center().y), 8.5, if *on { pal().on_accent } else { pal().muted });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A row of mutually exclusive pills. The highlight slides between options when animations
/// are on. Returns true when the selection changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    let font = FontId::proportional(13.5);
    let galleys: Vec<_> = options.iter().map(|(_, l)| ui.painter().layout_no_wrap((*l).to_string(), font.clone(), pal().text)).collect();
    let total_w: f32 = galleys.iter().map(|g| g.size().x + 26.0).sum::<f32>() + 8.0;
    let (rect, bg) = ui.allocate_exact_size(vec2(total_w, 36.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(11), pal().deep);
    ui.painter().rect_stroke(rect, CornerRadius::same(11), Stroke::new(1.0, pal().stroke), StrokeKind::Inside);

    let mut segs: Vec<Rect> = Vec::with_capacity(options.len());
    let mut x = rect.left() + 4.0;
    for g in &galleys {
        let w = g.size().x + 26.0;
        segs.push(Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(w, 28.0)));
        x += w;
    }
    let selected = options.iter().position(|(o, _)| *o == *value);
    if let Some(si) = selected {
        let target = segs[si];
        let ax = ui.ctx().animate_value_with_time(bg.id.with("pill-x"), target.left(), anim_time(0.20));
        let aw = ui.ctx().animate_value_with_time(bg.id.with("pill-w"), target.width(), anim_time(0.20));
        let pill = Rect::from_min_size(pos2(ax, target.top()), vec2(aw, target.height()));
        ui.painter().rect_filled(pill, CornerRadius::same(8), pal().accent_dim);
        ui.painter().rect_stroke(pill, CornerRadius::same(8), Stroke::new(1.0, pal().accent.gamma_multiply(0.6)), StrokeKind::Inside);
    }
    for (i, ((opt, _), g)) in options.iter().zip(galleys).enumerate() {
        let seg = segs[i];
        let resp = ui.interact(seg, bg.id.with(("seg", i)), Sense::click());
        let is_sel = selected == Some(i);
        if !is_sel && resp.hovered() {
            ui.painter().rect_filled(seg, CornerRadius::same(8), pal().card_hi);
        }
        let col = if is_sel { pal().accent } else if resp.hovered() { pal().text } else { pal().muted };
        ui.painter().galley(pos2(seg.left() + 13.0, seg.center().y - g.size().y / 2.0), g, col);
        if resp.clicked() && !is_sel {
            *value = *opt;
            changed = true;
        }
    }
    changed
}

/// Section heading above a card.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(bold(text).size(13.0).color(pal().muted));
    ui.add_space(-2.0);
}

/// One settings row: title + hint on the left, the control on the right.
pub fn setting_row(ui: &mut Ui, title: &str, hint: &str, last: bool, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(48.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            // Centre title-only rows against their control.
            ui.add_space(if hint.is_empty() { 13.0 } else { 5.0 });
            ui.label(RichText::new(title).size(14.5));
            if !hint.is_empty() {
                ui.label(muted(hint));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
    if !last {
        let y = ui.cursor().top();
        let r = ui.max_rect();
        ui.painter().line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0, pal().stroke.gamma_multiply(0.7)));
        ui.add_space(2.0);
    }
}

pub fn chip(ui: &mut Ui, text: &str) {
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.5), pal().muted);
    let (rect, _) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 26.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(13), pal().deep);
    ui.painter().rect_stroke(rect, CornerRadius::same(13), Stroke::new(1.0, pal().stroke), StrokeKind::Inside);
    ui.painter().galley(pos2(rect.left() + 10.0, rect.center().y - g.size().y / 2.0), g, pal().muted);
}

/// Circular progress ring with big text inside. `fraction` 0..=1.
pub fn ring(ui: &mut Ui, size: f32, fraction: f32, active: bool, big: &str, small: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let fraction = ui.ctx().animate_value_with_time(resp.id.with("ring"), fraction, anim_time(0.6));
    let c = rect.center();
    let r = size / 2.0 - 8.0;
    let p = ui.painter();
    if active && animations_on() {
        // Soft breathing halo behind the ring. The loop's 10 fps pulse cadence drives it.
        let t = ui.input(|i| i.time) as f32;
        let k = (t * 2.2).sin() * 0.5 + 0.5;
        p.circle_stroke(c, r + 6.0, Stroke::new(5.0, pal().accent.gamma_multiply(0.04 + 0.035 * k)));
    }
    p.circle_stroke(c, r, Stroke::new(9.0, pal().deep));
    p.circle_stroke(c, r, Stroke::new(1.0, pal().stroke));
    let frac = fraction.clamp(0.0, 1.0);
    if frac > 0.002 {
        let n = ((frac * 90.0) as usize).max(2);
        let pts: Vec<Pos2> = (0..=n).map(|i| {
            let a = -PI / 2.0 + frac * TAU * (i as f32 / n as f32);
            pos2(c.x + a.cos() * r, c.y + a.sin() * r)
        }).collect();
        let col = if active { pal().accent } else { pal().faint };
        p.add(Shape::line(pts.clone(), Stroke::new(9.0, col)));
        p.circle_filled(pts[0], 4.5, col);
        p.circle_filled(*pts.last().unwrap(), 4.5, col);
    }
    p.text(pos2(c.x, c.y - 6.0), Align2::CENTER_CENTER, big, FontId::new(size * 0.2, semibold_family()), pal().text);
    p.text(pos2(c.x, c.y + size * 0.14), Align2::CENTER_CENTER, small, FontId::proportional(12.5), pal().muted);
}

/// Compact mint button for footers and dialogs.
pub fn accent_pill(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::new(14.5, semibold_family()), pal().on_accent);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 40.0, 36.0), if enabled { Sense::click() } else { Sense::hover() });
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() && enabled, anim_time(0.16));
    let pressed = resp.is_pointer_button_down_on() && enabled;
    let fill = if enabled {
        let f = pal().accent.lerp_to_gamma(Color32::WHITE, 0.12 * t);
        if pressed { f.lerp_to_gamma(Color32::BLACK, 0.10) } else { f }
    } else {
        pal().hover
    };
    ui.painter().rect_filled(rect, CornerRadius::same(11), fill);
    let fg = if enabled { pal().on_accent } else { pal().faint };
    let y = rect.center().y + if pressed { 1.0 } else { 0.0 };
    ui.painter().galley(pos2(rect.center().x - g.size().x / 2.0, y - g.size().y / 2.0), g, fg);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// Tinted pill for destructive actions in dialogs.
pub fn danger_pill(ui: &mut Ui, label: &str) -> Response {
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::new(14.5, semibold_family()), pal().danger);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 40.0, 36.0), Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    let fill = if resp.hovered() { pal().danger_bg.lerp_to_gamma(pal().danger, 0.15) } else { pal().danger_bg };
    ui.painter().rect_filled(rect, CornerRadius::same(11), if pressed { fill.lerp_to_gamma(Color32::BLACK, 0.10) } else { fill });
    let y = rect.center().y + if pressed { 1.0 } else { 0.0 };
    ui.painter().galley(pos2(rect.center().x - g.size().x / 2.0, y - g.size().y / 2.0), g, pal().danger);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Thin rounded progress bar.
pub fn progress_bar(ui: &mut Ui, fraction: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(4), pal().deep);
    let f = ui.ctx().animate_value_with_time(ui.next_auto_id(), fraction.clamp(0.0, 1.0), anim_time(0.2));
    if f > 0.0 {
        let fill = Rect::from_min_size(rect.min, vec2((rect.width() * f).max(8.0), rect.height()));
        ui.painter().rect_filled(fill, CornerRadius::same(4), pal().accent);
    }
}
