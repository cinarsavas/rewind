use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText,
    Stroke, TextStyle,
};

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rewind_core::theme::{parse_hex, Palette, Rgb, ThemeLayout};

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
            nav: c(p.nav),
            panel: c(p.panel),
            card: c(p.card),
            card_hi: c(p.card_hi),
            stroke: c(p.stroke),
            text: c(p.text),
            muted: c(p.muted),
            faint: c(p.faint),
            accent: c(p.accent),
            accent_dim: c(p.accent_dim),
            on_accent: c(p.on_accent),
            danger: c(p.danger),
            warn: c(p.warn),
            deep: c(p.deep),
            accent_tint: c(p.accent_tint),
            hero_on: c(p.hero_on),
            danger_bg: c(p.danger_bg),
            accent_bg: c(p.accent_bg),
            warn_bg: c(p.warn_bg),
            hover: c(p.hover),
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
    static LAYOUT: RefCell<Layout> = RefCell::new(Layout::default());
}

/// Resolved layout and background for the active theme.
#[derive(Clone)]
pub struct Layout {
    pub nav_width: f32,
    pub content_width: f32,
    pub page_margin: f32,
    pub card_radius: u8,
    pub background_dim: f32,
    pub background_color: Option<Color32>,
    pub background_top: Option<Color32>,
    pub background_bottom: Option<Color32>,
    pub background: Option<egui::TextureHandle>,
}

impl Default for Layout {
    fn default() -> Self {
        let d = ThemeLayout::default();
        Self {
            nav_width: d.nav_width,
            content_width: d.content_width,
            page_margin: d.page_margin,
            card_radius: d.card_radius,
            background_dim: d.background_dim,
            background_color: None,
            background_top: None,
            background_bottom: None,
            background: None,
        }
    }
}

impl Layout {
    /// True when a background is painted behind the panels, so they should let it show.
    pub fn has_background(&self) -> bool {
        self.background.is_some()
            || (self.background_top.is_some() && self.background_bottom.is_some())
            || self.background_color.is_some()
    }
}

/// The active layout. Cheap to clone (the texture handle is reference counted).
pub fn layout() -> Layout {
    LAYOUT.with(|l| l.borrow().clone())
}

pub fn set_layout(l: Layout) {
    LAYOUT.with(|cell| *cell.borrow_mut() = l);
}

fn color(s: &str) -> Option<Color32> {
    parse_hex(s).map(c)
}

/// Turn a theme's layout overrides plus its optional background image into a [`Layout`].
/// Images are decoded once, when the theme is applied, never per frame.
pub fn resolve_layout(
    spec: &ThemeLayout,
    image_path: Option<&std::path::Path>,
    ctx: &egui::Context,
) -> Layout {
    let background = image_path.and_then(|path| {
        let img = image::open(path).ok()?.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let pixels = img.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        Some(ctx.load_texture(
            "rewind-theme-background",
            color_image,
            egui::TextureOptions::LINEAR,
        ))
    });
    Layout {
        nav_width: spec.nav_width,
        content_width: spec.content_width,
        page_margin: spec.page_margin,
        card_radius: spec.card_radius,
        background_dim: spec.background_dim,
        background_color: color(&spec.background_color),
        background_top: color(&spec.background_top),
        background_bottom: color(&spec.background_bottom),
        background,
    }
}

/// Paint the theme background across the whole window. Called once per frame before the UI.
pub fn paint_background(ui: &egui::Ui) {
    let l = layout();
    if !l.has_background() {
        return;
    }
    let rect = ui.max_rect();
    let painter = ui.painter();
    if let Some(tex) = &l.background {
        // Scale the image to cover the window without stretching it out of proportion.
        let size = tex.size_vec2();
        let scale = (rect.width() / size.x.max(1.0)).max(rect.height() / size.y.max(1.0));
        let dst = egui::Rect::from_center_size(rect.center(), size * scale);
        painter.image(
            tex.id(),
            dst,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if l.background_dim > 0.0 {
            let a = (l.background_dim * 255.0).round().clamp(0.0, 255.0) as u8;
            painter.rect_filled(rect, 0, Color32::from_black_alpha(a));
        }
    } else if let (Some(top), Some(bottom)) = (l.background_top, l.background_bottom) {
        let mut mesh = egui::Mesh::default();
        mesh.vertices
            .push(egui::epaint::Vertex::untextured(rect.left_top(), top));
        mesh.vertices
            .push(egui::epaint::Vertex::untextured(rect.right_top(), top));
        mesh.vertices.push(egui::epaint::Vertex::untextured(
            rect.right_bottom(),
            bottom,
        ));
        mesh.vertices
            .push(egui::epaint::Vertex::untextured(rect.left_bottom(), bottom));
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        painter.add(egui::Shape::mesh(mesh));
    } else if let Some(c) = l.background_color {
        painter.rect_filled(rect, 0, c);
    }
}

/// Fill for the navigation bar and bottom bars. Slightly translucent over a background so the
/// theme image stays visible without making the labels unreadable.
pub fn nav_fill() -> Color32 {
    let c = pal().nav;
    if layout().has_background() {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 214)
    } else {
        c
    }
}

/// Fill for the scrolling content area. Transparent over a background, solid otherwise.
pub fn content_fill() -> Color32 {
    if layout().has_background() {
        Color32::TRANSPARENT
    } else {
        pal().panel
    }
}

pub fn card_radius() -> u8 {
    layout().card_radius
}

fn rgb_u32(c: Color32) -> u32 {
    ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | (c.b() as u32)
}

/// Palette-derived colours for the native over-the-game toast.
pub fn toast_colors() -> crate::overlay::ToastColors {
    let p = pal();
    crate::overlay::ToastColors {
        card: rgb_u32(p.card),
        border: rgb_u32(p.stroke),
        title: rgb_u32(p.text),
        subtitle: rgb_u32(p.muted),
        success: rgb_u32(p.accent),
        // Neutral status toasts use the theme's warning colour so they stay distinct from
        // success and error without inventing a colour the theme doesn't define.
        info: rgb_u32(p.warn),
        error: rgb_u32(p.danger),
    }
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
    TWEEN.with(|t| {
        *t.borrow_mut() = Some(Tween {
            from,
            to,
            start: Instant::now(),
            secs: 0.24,
        })
    });
    ctx.request_repaint();
}

/// Advance a running palette cross-fade. Returns true while it is still running.
pub fn tick_tween() -> bool {
    TWEEN.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(tw) = slot.as_mut() else {
            return false;
        };
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

static EXTRA_MOTION: AtomicBool = AtomicBool::new(true);

/// The "Extra animations" setting (also off when low-power mode is on).
pub fn set_extra_motion(on: bool) {
    EXTRA_MOTION.store(on, Ordering::Relaxed);
}

/// Extra motion: hover zoom, star pop, new-clip glow. Only with animations on.
pub fn extra_motion() -> bool {
    animations_on() && EXTRA_MOTION.load(Ordering::Relaxed)
}

/// Duration for a transition: 0 when animations are switched off, so state changes snap.
pub fn anim_time(secs: f32) -> f32 {
    if animations_on() {
        secs
    } else {
        0.0
    }
}

pub fn semibold_family() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn install_fonts(ctx: &egui::Context) {
    // Inter (SIL OFL 1.1, see assets/Inter-LICENSE.txt) for a clean, modern look.
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/Inter-SemiBold.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".into());
    let mut semi = vec!["inter-semibold".to_string()];
    semi.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
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
    v.window_corner_radius = CornerRadius::same(card_radius());
    v.window_stroke = Stroke::new(1.0, p.stroke);
    v.window_shadow = egui::Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    v.popup_shadow = v.window_shadow;
    v.selection.bg_fill = p.accent_dim;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;

    let r = CornerRadius::same(10);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
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
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(28.0, semibold_family()));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.5));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.5));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.5));
    ctx.set_global_style(style);
}

pub fn bold(text: impl AsRef<str>) -> RichText {
    RichText::new(super::i18n::t(text.as_ref())).family(semibold_family())
}

pub fn muted(text: impl AsRef<str>) -> RichText {
    RichText::new(super::i18n::t(text.as_ref()))
        .color(pal().muted)
        .size(13.0)
}

pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(pal().card)
        .stroke(Stroke::new(1.0, pal().stroke))
        .corner_radius(CornerRadius::same(card_radius()))
        .inner_margin(Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}
