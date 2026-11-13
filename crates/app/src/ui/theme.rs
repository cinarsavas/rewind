use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, Stroke, TextStyle,
};

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rewind_core::theme::{Palette, Rgb};

/// The active colours, resolved to egui types. Read with [`pal`].
#[derive(Clone, Copy, PartialEq)]
pub struct Pal {
    pub nav: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hi: Color32,
    pub stroke: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_dim: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub warn: Color32,
    pub deep: Color32,
    pub accent_tint: Color32,
    pub hero_on: Color32,
    pub danger_bg: Color32,
    pub accent_bg: Color32,
    pub warn_bg: Color32,
    pub hover: Color32,
}

fn c(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

impl From<&Palette> for Pal {
    fn from(p: &Palette) -> Pal {
        Pal {
            nav: c(p.nav), panel: c(p.panel), card: c(p.card), card_hi: c(p.card_hi), stroke: c(p.stroke),
            text: c(p.text), muted: c(p.muted), faint: c(p.faint), accent: c(p.accent),
            accent_dim: c(p.accent_dim), on_accent: c(p.on_accent), danger: c(p.danger), warn: c(p.warn),
            deep: c(p.deep), accent_tint: c(p.accent_tint), hero_on: c(p.hero_on),
            danger_bg: c(p.danger_bg), accent_bg: c(p.accent_bg), warn_bg: c(p.warn_bg), hover: c(p.hover),
        }
    }
}

impl Pal {
    /// Blend two palettes in gamma space. Used for the theme cross-fade.
    fn lerp(self, other: Pal, t: f32) -> Pal {
        let m = |a: Color32, b: Color32| a.lerp_to_gamma(b, t);
        Pal {
            nav: m(self.nav, other.nav),
            panel: m(self.panel, other.panel),
            card: m(self.card, other.card),
            card_hi: m(self.card_hi, other.card_hi),
            stroke: m(self.stroke, other.stroke),
            text: m(self.text, other.text),
            muted: m(self.muted, other.muted),
            faint: m(self.faint, other.faint),
            accent: m(self.accent, other.accent),
            accent_dim: m(self.accent_dim, other.accent_dim),
            on_accent: m(self.on_accent, other.on_accent),
            danger: m(self.danger, other.danger),
            warn: m(self.warn, other.warn),
            deep: m(self.deep, other.deep),
            accent_tint: m(self.accent_tint, other.accent_tint),
            hero_on: m(self.hero_on, other.hero_on),
            danger_bg: m(self.danger_bg, other.danger_bg),
            accent_bg: m(self.accent_bg, other.accent_bg),
            warn_bg: m(self.warn_bg, other.warn_bg),
            hover: m(self.hover, other.hover),
        }
    }
}

thread_local! {
    static PAL: Cell<Pal> = Cell::new(Pal::from(&Palette::default_dark()));
    static TWEEN: RefCell<Option<Tween>> = const { RefCell::new(None) };
}

struct Tween {
    from: Pal,
    to: Pal,
    start: Instant,
    secs: f32,
}

/// Current palette (UI thread only, which is the only place it's used).
pub fn pal() -> Pal {
    PAL.with(|p| p.get())
}

pub fn set_palette(p: &Palette) {
    TWEEN.with(|t| *t.borrow_mut() = None);
    PAL.with(|cell| cell.set(Pal::from(p)));
}

/// Switch palettes with a short cross-fade instead of a hard cut. Falls back to an
/// instant switch when animations are off.
pub fn fade_to(ctx: &egui::Context, p: &Palette) {
    let to = Pal::from(p);
    let from = pal();
    if !animations_on() || from == to {
        set_palette(p);
        return;
    }
    TWEEN.with(|t| *t.borrow_mut() = Some(Tween { from, to, start: Instant::now(), secs: 0.24 }));
    ctx.request_repaint();
}

/// Advance a running palette cross-fade. Returns true while it is still running.
pub fn tick_tween() -> bool {
    TWEEN.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(tw) = slot.as_mut() else { return false };
        let k = (tw.start.elapsed().as_secs_f32() / tw.secs).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - k).powi(3);
        let mixed = tw.from.lerp(tw.to, eased);
        PAL.with(|p| p.set(mixed));
        if k >= 1.0 {
            *slot = None;
            return false;
        }
        true
    })
}

pub fn tweening() -> bool {
    TWEEN.with(|t| t.borrow().is_some())
}

static ANIMATIONS: AtomicBool = AtomicBool::new(true);

pub fn set_animations(ctx: &egui::Context, on: bool) {
    ANIMATIONS.store(on, Ordering::Relaxed);
    if !on {
        // Jump straight to the target palette instead of finishing a cross-fade.
        let target = TWEEN.with(|t| t.borrow_mut().take().map(|tw| tw.to));
        if let Some(target) = target {
            PAL.with(|p| p.set(target));
        }
    }
    ctx.global_style_mut(|s| s.animation_time = if on { 0.20 } else { 0.0 });
}

pub fn animations_on() -> bool {
    ANIMATIONS.load(Ordering::Relaxed)
}

/// Duration for a transition: 0 when animations are switched off, so state changes snap.
pub fn anim_time(secs: f32) -> f32 {
    if animations_on() { secs } else { 0.0 }
}

pub fn semibold_family() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn install_fonts(ctx: &egui::Context) {
    // Inter (SIL OFL 1.1, see assets/Inter-LICENSE.txt) for a clean, modern look.
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("inter".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/Inter-Regular.ttf"))));
    fonts.font_data.insert("inter-semibold".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/Inter-SemiBold.ttf"))));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".into());
    let mut semi = vec!["inter-semibold".to_string()];
    semi.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(semibold_family(), semi);
    ctx.set_fonts(fonts);
}

/// (Re)build egui's visuals from the active palette. Cheap; call again after a theme change.
pub fn apply_visuals(ctx: &egui::Context) {
    let p = pal();
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = p.panel;
    v.window_fill = p.card;
    v.extreme_bg_color = p.deep;
    v.faint_bg_color = p.card;
    v.override_text_color = Some(p.text);
    v.window_corner_radius = CornerRadius::same(16);
    v.window_stroke = Stroke::new(1.0, p.stroke);
    v.window_shadow = egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(120) };
    v.popup_shadow = v.window_shadow;
    v.selection.bg_fill = p.accent_dim;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;

    let r = CornerRadius::same(10);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.stroke);
    v.widgets.inactive.bg_fill = p.card_hi;
    v.widgets.inactive.weak_bg_fill = p.card_hi;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.stroke);
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.hover);
    v.widgets.active.bg_fill = p.accent_dim;
    v.widgets.active.weak_bg_fill = p.accent_dim;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);

    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.interact_size.y = 34.0;
    style.spacing.slider_width = 200.0;
    style.spacing.combo_width = 200.0;
    style.animation_time = if animations_on() { 0.16 } else { 0.0 };
    style.text_styles.insert(TextStyle::Heading, FontId::new(28.0, semibold_family()));
    style.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
    style.text_styles.insert(TextStyle::Button, FontId::proportional(14.5));
    style.text_styles.insert(TextStyle::Small, FontId::proportional(12.5));
    ctx.set_global_style(style);
}

pub fn bold(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).family(semibold_family())
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(pal().muted).size(13.0)
}

pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(pal().card)
        .stroke(Stroke::new(1.0, pal().stroke))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}
