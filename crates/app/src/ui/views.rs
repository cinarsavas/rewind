use eframe::egui::{self, pos2, vec2, Align, Color32, CornerRadius, Event, Key, Layout, Margin, RichText, Sense, Stroke};
use rewind_core::config::{output_size, CaptureMode, Codec, Container, Corner, DisplayMethod, EffectStyle, Preset, Quality, Resolution, FPS_CHOICES, FPS_LABELS};
use rewind_core::estimate;
use rewind_core::hotkey::Hotkey;
use rewind_core::library::{human_size, Clip};

use super::effects;
use super::theme::*;
use super::widgets::*;
use super::{row_reveal_at, Page, Phase, RewindApp, TrimMode, TrimState};
use crate::packs::IndexState;
use crate::thumbs::Thumbs;
use crate::voice::VoiceStatus;

pub fn draw(app: &mut RewindApp, root: &mut egui::Ui) {
    let ctx = root.ctx().clone();
    let ctx = &ctx;
    match &app.phase {
        Phase::Installing => return splash(root, "Setting things up", "Downloading the OBS capture runtime. This only happens once (about 100 MB)."),
        Phase::Starting { .. } => return splash(root, "Starting", "Spinning up the capture engine…"),
        Phase::Failed(msg) => {
            let msg = msg.clone();
            return failure(root, &msg);
        }
        Phase::Ready => {}
    }

    egui::Panel::left("nav")
        .exact_size(236.0)
        .frame(egui::Frame::new().fill(pal().nav).inner_margin(Margin::same(18)))
        .show(root, |ui| nav(app, ui));

    let dirty = app.draft != app.cfg;
    if app.page == Page::Settings && dirty {
        egui::Panel::bottom("apply_bar")
            .frame(egui::Frame::new().fill(pal().nav).stroke(Stroke::new(1.0, pal().stroke)).inner_margin(Margin::symmetric(28, 12)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("You have unsaved changes").color(pal().warn));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if accent_pill(ui, "Apply", true).clicked() {
                            app.apply_settings();
                        }
                        if text_button(ui, None, "Revert", true).clicked() {
                            app.draft = app.cfg.clone();
                            app.capturing_hotkey = None;
                        }
                    });
                });
            });
    }

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(pal().panel).inner_margin(Margin::symmetric(28, 24)))
        .show(root, |ui| {
            let smooth = app.cfg.smooth_scroll;
            if smooth {
                // Eat the wheel only when the pointer is over the page content (not the nav,
                // dialogs or the apply bar) and glide toward the new offset instead of
                // jumping. A separate toggle from animations, as requested.
                let region = ui.available_rect_before_wrap();
                let over_page = ui.rect_contains_pointer(region)
                    && ui
                        .ctx()
                        .pointer_latest_pos()
                        .and_then(|p| ui.ctx().layer_id_at(p))
                        == Some(egui::LayerId::background());
                let wheel = if over_page {
                    ui.input_mut(|i| {
                        let d = i.smooth_scroll_delta.y;
                        if d != 0.0 {
                            i.smooth_scroll_delta.y = 0.0;
                        }
                        d
                    })
                } else {
                    0.0
                };
                if wheel != 0.0 {
                    // egui reports wheel-up as a positive delta, but the scroll offset grows
                    // downward, so subtract instead of adding. Getting this backwards was
                    // what made scrolling feel inverted.
                    app.scroll_target = (app.scroll_target - wheel).clamp(0.0, app.scroll_max);
                    ui.ctx().request_repaint();
                }
                let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.05);
                // Gentle exponential glide: quick enough to feel responsive, slow enough
                // that a single notch lands softly instead of snapping.
                let k = 1.0 - (-dt * 13.0).exp();
                app.scroll_pos += (app.scroll_target - app.scroll_pos) * k;
                if (app.scroll_target - app.scroll_pos).abs() < 0.12 {
                    app.scroll_pos = app.scroll_target;
                }
            }

            let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if smooth {
                area = area.vertical_scroll_offset(app.scroll_pos);
            }
            let out = area.show(ui, |ui| {
                let t = app.page_anim();
                let dir = app.page_dir();
                column(ui, dir * (1.0 - t) * 9.0, |ui| {
                    ui.set_opacity(t);
                    ui.add_space((1.0 - t) * 12.0);
                    match app.page {
                        Page::Replay => replay_page(app, ui, ctx),
                        Page::Clips => clips_page(app, ui),
                        Page::Settings => settings_page(app, ui, ctx),
                        Page::Addons => addons_page(app, ui),
                        Page::About => about_page(ui),
                    }
                });
            });
            if smooth {
                app.scroll_max = (out.content_size.y - out.inner_rect.height()).max(0.0);
                app.scroll_target = app.scroll_target.clamp(0.0, app.scroll_max);
                // If the user dragged the scrollbar or used the keyboard, follow the real
                // offset instead of fighting it.
                let actual = out.state.clamped_offset().y;
                if (actual - app.scroll_pos).abs() > 0.5 {
                    app.scroll_pos = actual;
                    app.scroll_target = actual;
                }
            }
        });

    trim_window(app, ctx);
    delete_window(app, ctx);
    toast(app, ctx);
}

/// Centre a content column of at most 880 px, offset sideways for the page transition.
fn column(ui: &mut egui::Ui, slide: f32, add: impl FnOnce(&mut egui::Ui)) {
    let avail = ui.available_width();
    let w = avail.min(880.0);
    ui.horizontal_top(|ui| {
        ui.add_space((((avail - w) / 2.0) + slide).max(0.0));
        ui.vertical(|ui| {
            ui.set_width(w - 12.0);
            add(ui);
            ui.add_space(24.0);
        });
    });
}

fn splash(root: &mut egui::Ui, title: &str, body: &str) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(pal().nav)).show(root, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.34);
            ui.add(egui::Spinner::new().size(36.0).color(pal().accent));
            ui.add_space(16.0);
            ui.label(bold(title).size(24.0));
            ui.label(muted(body));
        });
    });
}

fn failure(root: &mut egui::Ui, msg: &str) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(pal().nav)).show(root, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.28);
            ui.label(bold("Rewind couldn't start").size(24.0).color(pal().danger));
            ui.add_space(6.0);
            ui.set_max_width(580.0);
            ui.label(RichText::new(msg).color(pal().muted));
            ui.add_space(10.0);
            ui.label(muted("Check your internet connection (needed once to fetch OBS) and restart the app."));
        });
    });
}

// ---------------------------------------------------------------- navigation

fn nav(app: &mut RewindApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(10), pal().accent);
        paint_icon(ui.painter(), Icon::Record, rect.center(), 20.0, pal().on_accent);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(bold("Rewind").size(19.0));
            ui.label(muted("Instant replay"));
        });
    });
    ui.add_space(22.0);

    for (page, icon, name) in [
        (Page::Replay, Icon::Record, "Replay"),
        (Page::Clips, Icon::Clips, "Clips"),
        (Page::Settings, Icon::Sliders, "Settings"),
        (Page::Addons, Icon::Addons, "Add-ons"),
        (Page::About, Icon::Info, "About"),
    ] {
        if nav_item(ui, icon, name, app.page == page).clicked() {
            app.set_page(page);
            if page == Page::Clips {
                app.refresh_clips(true);
            }
        }
    }

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.label(muted(format!("v{}", env!("CARGO_PKG_VERSION"))));
        ui.add_space(6.0);
        let active = app.buffer_active();
        egui::Frame::new()
            .fill(pal().card)
            .stroke(Stroke::new(1.0, pal().stroke))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    if active {
                        let t = ui.input(|i| i.time) as f32;
                        let k = (t * 2.2).sin() * 0.5 + 0.5;
                        ui.painter().circle_filled(r.center(), 5.0 + 2.0 * k, pal().accent.gamma_multiply(0.2));
                    }
                    ui.painter().circle_filled(r.center(), 4.0, if active { pal().accent } else { pal().faint });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(bold(if active { "Replay on" } else { "Replay off" }).size(13.5));
                        ui.label(muted(if active { format!("Last {} s", app.cfg.buffer_secs) } else { "Not recording".into() }));
                    });
                });
            });
    });
}

fn nav_item(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id.with("selected"), selected, anim_time(0.22));
    let hover = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered() && !selected, anim_time(0.18));
    if hover > 0.0 {
        let c = pal().card;
        let hover_fill = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (215.0 * hover) as u8);
        ui.painter().rect_filled(rect, CornerRadius::same(11), hover_fill);
        let h = 10.0 * hover;
        ui.painter().rect_filled(
            egui::Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, h)),
            CornerRadius::same(2),
            pal().accent.gamma_multiply(0.55),
        );
    }
    if t > 0.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(11), pal().accent_tint.gamma_multiply(t));
        let bar_h = 20.0 * t;
        ui.painter().rect_filled(egui::Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, bar_h)), CornerRadius::same(2), pal().accent);
    }
    let col = if selected {
        pal().accent
    } else {
        pal().muted.lerp_to_gamma(pal().text, hover)
    };
    // A tiny slide makes the row feel alive without moving the layout.
    let dx = 2.0 * hover;
    paint_icon(ui.painter(), icon, pos2(rect.left() + 26.0 + dx, rect.center().y), 18.0, col);
    ui.painter().text(pos2(rect.left() + 48.0 + dx, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::new(15.0, semibold_family()), col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn page_header(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.label(RichText::new(title).heading());
    ui.label(muted(sub));
    ui.add_space(10.0);
}

fn mmss(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

// ---------------------------------------------------------------- replay page

fn resolution_text(app: &RewindApp) -> String {
    let (bw, bh) = app.monitors.get(app.cfg.monitor_index).or_else(|| app.monitors.first()).map(|m| (m.width, m.height)).unwrap_or((1920, 1080));
    let (w, h) = output_size(bw, bh, app.cfg.resolution);
    format!("{w}×{h}")
}

fn audio_text(app: &RewindApp) -> &'static str {
    match (app.cfg.audio.desktop_enabled, app.cfg.audio.mic_enabled) {
        (true, true) => "Game audio + mic",
        (true, false) => "Game audio",
        (false, true) => "Mic only",
        (false, false) => "No audio",
    }
}

fn replay_page(app: &mut RewindApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    page_header(ui, "Instant Replay", "Always recording in the background. One key keeps the moment you just had.");
    let active = app.buffer_active();
    let secs = app.cfg.buffer_secs;
    let elapsed = app.buffer_started_at.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
    let have = if active { elapsed.min(secs as f32) } else { 0.0 };
    // Show what is actually encoding, not just what was requested. A codec that fell back
    // to x264 should say "H.264 · CPU" rather than claim HEVC.
    let encoder_label = app
        .engine
        .as_ref()
        .map(|e| e.encoder_label.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| app.cfg.codec.short_label().to_string());

    egui::Frame::new()
        .fill(if active { pal().hero_on } else { pal().card })
        .stroke(Stroke::new(1.0, if active { pal().accent_dim } else { pal().stroke }))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::same(26))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (big, small) = if active { (mmss(have), format!("of {}", mmss(secs as f32))) } else { ("Off".to_string(), "not recording".to_string()) };
                ring(ui, 176.0, have / secs as f32, active, &big, &small);
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.label(bold(if active { "RECORDING" } else { "PAUSED" }).size(12.0).color(if active { pal().accent } else { pal().faint }));
                    ui.label(bold(if active { format!("Keeping the last {secs} seconds") } else { "Instant Replay is off".to_string() }).size(23.0));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        chip(ui, &encoder_label);
                        chip(ui, &resolution_text(app));
                        chip(ui, &format!("{} fps", app.cfg.fps));
                        chip(ui, &format!("{} Mbps", app.cfg.bitrate_kbps / 1000));
                        chip(ui, audio_text(app));
                    });
                    ui.add_space(6.0);
                    let label = if app.saving { "Saving…".to_string() } else { format!("Save last {secs} s") };
                    let key = app.cfg.save_hotkey.to_string();
                    if primary_button(ui, &label, Some(&key), active && !app.saving).clicked() {
                        app.save_replay(ctx);
                    }
                    ui.horizontal(|ui| {
                        if text_button(ui, None, if active { "Turn off" } else { "Turn on" }, true).clicked() {
                            app.set_buffer(!active);
                        }
                        if let Some(p) = app.last_saved.clone() {
                            if text_button(ui, Some(Icon::Play), "Play last clip", true).clicked() {
                                let _ = opener::open(&p);
                            }
                        }
                    });
                });
            });
        });

    ui.add_space(4.0);
    let ram = estimate::buffer_ram_mb(app.cfg.bitrate_kbps, app.cfg.audio.bitrate_kbps, secs);
    let clip = estimate::clip_size_mb(app.cfg.bitrate_kbps, app.cfg.audio.bitrate_kbps, secs);
    let cols = [
        ("MEMORY IN USE", format!("~{ram} MB"), "while the buffer is full".to_string()),
        ("CLIP SIZE", format!("~{clip} MB"), format!("for {secs} seconds")),
        ("THIS APP", format!("{:.0}% CPU", app.perf.cpu_pct), format!("{} MB · {encoder_label}", app.perf.mem_mb)),
    ];
    ui.columns(3, |c| {
        for (i, (name, value, hint)) in cols.iter().enumerate() {
            card(&mut c[i], |ui| {
                ui.label(bold(*name).size(11.5).color(pal().faint));
                ui.label(bold(value.clone()).size(21.0));
                ui.label(muted(hint.clone()));
            });
        }
    });

    // Hotkey problems are real warnings; encoder/capture notes are calm information.
    for w in &app.hotkey_errors {
        egui::Frame::new()
            .fill(pal().warn_bg)
            .stroke(Stroke::new(1.0, pal().warn.gamma_multiply(0.7)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("⚠  {w}")).color(pal().warn));
            });
    }
    if let Some(engine) = &app.engine {
        for note in &engine.notes {
            egui::Frame::new()
                .fill(pal().accent_bg)
                .stroke(Stroke::new(1.0, pal().accent_dim))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(note).color(pal().accent));
                });
        }
    }

    let cpu_encoding = app.engine.as_ref().map(|e| e.cpu_encoding).unwrap_or(false);
    if cpu_encoding && !app.cfg.low_power_ui {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(bold("Recording on the CPU"));
                    ui.label(muted("That is normal on laptops without a hardware video encoder, and the clips still look good. If recording feels heavy, the Low-end preset keeps it light."));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if accent_pill(ui, "Apply Low-end preset", true).clicked() {
                        app.draft.apply_preset(Preset::LowEnd);
                        app.apply_settings();
                    }
                });
            });
        });
    }

    // Recent clips
    app.refresh_clips(false);
    app.sync_clip_rows();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(bold("Recent clips").size(16.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if text_button(ui, None, "View all", true).clicked() {
                app.set_page(Page::Clips);
            }
        });
    });
    if app.clip_rows.is_empty() {
        let hint = format!("Press {} while Instant Replay is on to save your first one.", app.cfg.save_hotkey);
        card(ui, |ui| {
            ui.label(bold("No clips yet"));
            ui.label(muted(hint));
        });
        return;
    }
    let (want_thumb, can_trim) = (app.cfg.thumbnails, app.ffmpeg.is_some());
    let mut action: Option<(usize, ClipAction)> = None;
    for i in 0..app.clip_rows.len().min(3) {
        let reveal = app.row_reveal(i);
        let row = &app.clip_rows[i];
        if let Some(a) = clip_row(ui, &row.clip, &row.meta, &mut app.thumbs, want_thumb, can_trim, reveal) {
            action = Some((i, a));
        }
    }
    if let Some((i, a)) = action {
        let clip = app.clip_rows[i].clip.clone();
        apply_clip_action(app, a, &clip);
    }
}

#[derive(Clone, Copy)]
enum ClipAction {
    Delete,
    Trim,
    Reveal,
    Play,
}

fn apply_clip_action(app: &mut RewindApp, action: ClipAction, clip: &Clip) {
    match action {
        ClipAction::Delete => app.confirm_delete = Some(clip.clone()),
        ClipAction::Trim => {
            app.trim = Some(TrimState { clip: clip.clone(), mode: TrimMode::LastSeconds, last_secs: 15.0, start: 0.0, end: 15.0, running: false })
        }
        ClipAction::Reveal => {
            let _ = opener::reveal(&clip.path);
        }
        ClipAction::Play => {
            let _ = opener::open(&clip.path);
        }
    }
}

/// One clip row. Borrows the row and the thumbnail cache instead of the whole app, so the
/// caller can iterate the library without cloning it every frame.
fn clip_row(
    ui: &mut egui::Ui,
    clip: &Clip,
    meta: &str,
    thumbs: &mut Thumbs,
    want_thumb: bool,
    can_trim: bool,
    reveal: f32,
) -> Option<ClipAction> {
    let mut action = None;
    let prev_opacity = ui.opacity();
    if reveal < 1.0 {
        ui.multiply_opacity(reveal.max(0.0));
    }
    let resp = egui::Frame::new()
        .fill(pal().card)
        .stroke(Stroke::new(1.0, pal().stroke))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let size = if want_thumb { vec2(112.0, 63.0) } else { vec2(46.0, 46.0) };
                let (r, _) = ui.allocate_exact_size(size, Sense::hover());
                let tex = if want_thumb { thumbs.get(ui.ctx(), &clip.path, clip.modified) } else { None };
                if let Some(tex) = tex {
                    egui::Image::new((tex.id(), size)).corner_radius(CornerRadius::same(10)).paint_at(ui, r);
                } else {
                    ui.painter().rect_filled(r, CornerRadius::same(if want_thumb { 10 } else { 12 }), pal().deep);
                    paint_icon(ui.painter(), Icon::Play, r.center(), 18.0, pal().accent);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(bold(&clip.name).size(14.5));
                    ui.label(muted(meta));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Trash, "Move to Recycle Bin", pal().danger).clicked() {
                        action = Some(ClipAction::Delete);
                    }
                    let trim = ui.add_enabled_ui(can_trim, |ui| icon_button(ui, Icon::Scissors, if can_trim { "Trim" } else { "Needs ffmpeg next to Rewind.exe or on PATH" }, pal().muted)).inner;
                    if can_trim && trim.clicked() {
                        action = Some(ClipAction::Trim);
                    }
                    if icon_button(ui, Icon::Folder, "Show in folder", pal().muted).clicked() {
                        action = Some(ClipAction::Reveal);
                    }
                    if icon_button(ui, Icon::Play, "Play", pal().accent).clicked() {
                        action = Some(ClipAction::Play);
                    }
                });
            });
        })
        .response;
    if reveal < 1.0 {
        ui.set_opacity(prev_opacity);
    }

    // Hover gets a soft accent border plus a faint outer glow. The fill stays put so there
    // is no layout cost; the glow is a wide translucent stroke, drawn only while hovered.
    let hover = ui.ctx().animate_bool_with_time(resp.id.with("row-hover"), ui.rect_contains_pointer(resp.rect), anim_time(0.18));
    if hover > 0.0 {
        if hover < 1.0 {
            ui.painter().rect_stroke(
                resp.rect.expand(1.5),
                CornerRadius::same(15),
                Stroke::new(4.0, pal().accent.gamma_multiply(0.09 * hover)),
                egui::StrokeKind::Outside,
            );
        }
        ui.painter().rect_stroke(
            resp.rect,
            CornerRadius::same(14),
            Stroke::new(1.0, pal().stroke.lerp_to_gamma(pal().accent, 0.55 * hover)),
            egui::StrokeKind::Inside,
        );
    }
    action
}

// ---------------------------------------------------------------- clips page

fn clips_page(app: &mut RewindApp, ui: &mut egui::Ui) {
    app.refresh_clips(false);
    app.sync_clip_rows();
    app.sync_filter();
    let header = format!("{} clips · {} · {}", app.clips.len(), human_size(app.clips_total_bytes), app.cfg.output_dir.display());
    page_header(ui, "Clips", &header);
    ui.horizontal(|ui| {
        let resp = ui.add(egui::TextEdit::singleline(&mut app.clip_filter).hint_text("Search clips").desired_width(280.0).margin(Margin::symmetric(12, 8)));
        if resp.changed() {
            app.mark_filter_dirty();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, Icon::Folder, "Open clips folder", pal().muted).clicked() {
                let _ = std::fs::create_dir_all(&app.cfg.output_dir);
                let _ = opener::open(&app.cfg.output_dir);
            }
            if icon_button(ui, Icon::Refresh, "Refresh", pal().muted).clicked() {
                app.refresh_clips(true);
            }
        });
    });
    if app.filter_idx.is_empty() {
        let (title, hint) = if app.clips.is_empty() {
            ("No clips yet", format!("Press {} while Instant Replay is on to save your first one.", app.cfg.save_hotkey))
        } else {
            ("Nothing matches that search", String::new())
        };
        card(ui, |ui| {
            ui.label(bold(title));
            if !hint.is_empty() {
                ui.label(muted(hint));
            }
        });
        return;
    }
    let (want_thumb, can_trim) = (app.cfg.thumbnails, app.ffmpeg.is_some());
    let animate = animations_on() && !app.cfg.low_power_ui;
    let shown = app.list_shown;
    let mut action: Option<(usize, ClipAction)> = None;
    for (pos, &i) in app.filter_idx.iter().enumerate() {
        let reveal = row_reveal_at(shown, pos, animate);
        let row = &app.clip_rows[i];
        if let Some(a) = clip_row(ui, &row.clip, &row.meta, &mut app.thumbs, want_thumb, can_trim, reveal) {
            action = Some((i, a));
        }
    }
    if let Some((i, a)) = action {
        let clip = app.clip_rows[i].clip.clone();
        apply_clip_action(app, a, &clip);
    }
}

// ---------------------------------------------------------------- settings page

const CAPTURE_OPTS: [(CaptureMode, &str); 2] = [(CaptureMode::Desktop, "Whole screen"), (CaptureMode::GameOnly, "Games only")];
const DISPLAY_OPTS: [(DisplayMethod, &str); 2] = [(DisplayMethod::Dxgi, "DXGI"), (DisplayMethod::Wgc, "WGC")];
const EFFECT_OPTS: [(EffectStyle, &str); 5] = [
    (EffectStyle::None, "Off"),
    (EffectStyle::Shine, "Shine"),
    (EffectStyle::Sparkle, "Sparkle"),
    (EffectStyle::Creeper, "Creeper"),
    (EffectStyle::Ripple, "Ripple"),
];
const CONTAINER_OPTS: [(Container, &str); 2] = [(Container::Mp4, "MP4"), (Container::Mkv, "MKV")];

fn key_name(k: Key) -> Option<String> {
    Some(match k {
        Key::Escape => return None,
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        other => other.name().to_string(),
    })
}

/// Click, then press the new combination. Esc cancels.
fn hotkey_field(app: &mut RewindApp, ui: &mut egui::Ui, which: usize) {
    let capturing = app.capturing_hotkey == Some(which);
    let current = if which == 0 { app.draft.save_hotkey.clone() } else { app.draft.toggle_hotkey.clone() };
    let label = if capturing { "Press the new keys…".to_string() } else { current.to_string() };
    if text_button(ui, None, &label, true).clicked() {
        app.capturing_hotkey = if capturing { None } else { Some(which) };
        app.hotkey_capture_error = None;
    }
    if !capturing {
        return;
    }
    ui.ctx().request_repaint();
    let events = ui.input(|i| i.events.clone());
    for ev in events {
        if let Event::Key { key, pressed: true, modifiers, .. } = ev {
            if key == Key::Escape && !modifiers.any() {
                app.capturing_hotkey = None;
                return;
            }
            let Some(name) = key_name(key) else { continue };
            let mut parts: Vec<String> = Vec::new();
            if modifiers.ctrl {
                parts.push("Ctrl".into());
            }
            if modifiers.shift {
                parts.push("Shift".into());
            }
            if modifiers.alt {
                parts.push("Alt".into());
            }
            parts.push(name);
            match parts.join("+").parse::<Hotkey>() {
                Ok(h) => {
                    if which == 0 {
                        app.draft.save_hotkey = h;
                    } else {
                        app.draft.toggle_hotkey = h;
                    }
                    app.capturing_hotkey = None;
                    app.hotkey_capture_error = None;
                }
                Err(e) => app.hotkey_capture_error = Some(e.to_string()),
            }
        }
    }
    if let Some(e) = &app.hotkey_capture_error {
        ui.label(RichText::new(e).color(pal().danger).size(12.5));
    }
}

fn settings_page(app: &mut RewindApp, ui: &mut egui::Ui, _ctx: &egui::Context) {
    page_header(ui, "Settings", "Changes apply when you press Apply. Video changes restart the buffer for a moment.");

    section_title(ui, "PERFORMANCE");
    ui.columns(3, |c| {
        for (i, p) in Preset::ALL.iter().enumerate() {
            if preset_card(&mut c[i], p.label(), p.blurb()) {
                app.draft.apply_preset(*p);
            }
        }
    });
    card(ui, |ui| {
        let d = &mut app.draft;
        setting_row(ui, "Smooth animations", "Fades and slides in the interface. Turn off on slow PCs.", false, |ui| {
            toggle(ui, &mut d.animations);
        });
        setting_row(ui, "Smooth scrolling", "Glides the page between wheel notches instead of jumping.", false, |ui| {
            toggle(ui, &mut d.smooth_scroll);
        });
        setting_row(ui, "Low-power interface", "No pulsing, slower refresh, cheaper video scaling.", false, |ui| {
            toggle(ui, &mut d.low_power_ui);
        });
        setting_row(ui, "Toggle effects", d.effects.blurb(), false, |ui| {
            segmented(ui, &mut d.effects, &EFFECT_OPTS);
            let at = ui.available_rect_before_wrap().right_center();
            if text_button(ui, Some(Icon::Play), "Try it", true).clicked() {
                effects::queue(d.effects, at);
            }
        });
        setting_row(ui, "Clip thumbnails", "Uses Windows' own thumbnail cache; MP4 works best.", true, |ui| {
            toggle(ui, &mut d.thumbnails);
        });
    });

    section_title(ui, "VIDEO");
    card(ui, |ui| {
        let d = &mut app.draft;
        setting_row(ui, "Capture", "Whole screen follows you into fullscreen games.", false, |ui| {
            segmented(ui, &mut d.capture_mode, &CAPTURE_OPTS);
        });
        setting_row(ui, "Screen capture method", d.display_method.blurb(), false, |ui| {
            segmented(ui, &mut d.display_method, &DISPLAY_OPTS);
        });
        setting_row(ui, "Monitor", "", false, |ui| {
            let current = app.monitors.get(d.monitor_index).map(|m| m.label.clone()).unwrap_or_else(|| "Primary".into());
            egui::ComboBox::from_id_salt("monitor").width(280.0).selected_text(current).show_ui(ui, |ui| {
                for (i, m) in app.monitors.iter().enumerate() {
                    ui.selectable_value(&mut d.monitor_index, i, &m.label);
                }
            });
        });
        setting_row(ui, "Resolution", "Never upscales.", false, |ui| {
            // A stack array: no heap allocation per frame.
            let items: [(Resolution, &str); Resolution::ALL.len()] =
                std::array::from_fn(|i| (Resolution::ALL[i], if Resolution::ALL[i] == Resolution::Native { "Native" } else { Resolution::ALL[i].label() }));
            segmented(ui, &mut d.resolution, &items);
        });
        setting_row(ui, "Frame rate", "", false, |ui| {
            let items: [(u32, &str); FPS_CHOICES.len()] = std::array::from_fn(|i| (FPS_CHOICES[i], FPS_LABELS[i]));
            segmented(ui, &mut d.fps, &items);
        });
        setting_row(ui, "Codec", "HEVC and AV1 give smaller files if your GPU supports them.", false, |ui| {
            let items: [(Codec, &str); Codec::ALL.len()] = std::array::from_fn(|i| (Codec::ALL[i], Codec::ALL[i].short_label()));
            segmented(ui, &mut d.codec, &items);
        });
        setting_row(ui, "Encoder quality", "", false, |ui| {
            let items: [(Quality, &str); Quality::ALL.len()] = std::array::from_fn(|i| (Quality::ALL[i], Quality::ALL[i].label()));
            segmented(ui, &mut d.quality, &items);
        });
        setting_row(ui, "Bitrate", "Higher looks better and makes bigger files.", false, |ui| {
            if text_button(ui, None, "Suggest", true).clicked() {
                let (bw, bh) = app.monitors.get(d.monitor_index).map(|m| (m.width, m.height)).unwrap_or((1920, 1080));
                let (_, h) = output_size(bw, bh, d.resolution);
                d.bitrate_kbps = estimate::suggested_bitrate_kbps(h, d.fps);
            }
            let mut mbps = d.bitrate_kbps as f32 / 1000.0;
            ui.add(egui::Slider::new(&mut mbps, 2.0..=100.0).suffix(" Mbps").trailing_fill(true));
            d.bitrate_kbps = (mbps * 1000.0) as u32;
        });
        setting_row(ui, "Show mouse cursor", "", true, |ui| {
            toggle(ui, &mut d.capture_cursor);
        });
    });

    section_title(ui, "AUDIO");
    card(ui, |ui| {
        let d = &mut app.draft;
        setting_row(ui, "Game & desktop sound", "Everything you hear from your speakers.", false, |ui| {
            toggle(ui, &mut d.audio.desktop_enabled);
            let mut pct = d.audio.desktop_volume * 100.0;
            ui.add_enabled(d.audio.desktop_enabled, egui::Slider::new(&mut pct, 0.0..=200.0).suffix("%").trailing_fill(true));
            d.audio.desktop_volume = pct / 100.0;
        });
        setting_row(ui, "Microphone", "Mixed into the same track.", false, |ui| {
            toggle(ui, &mut d.audio.mic_enabled);
            let mut pct = d.audio.mic_volume * 100.0;
            ui.add_enabled(d.audio.mic_enabled, egui::Slider::new(&mut pct, 0.0..=400.0).suffix("%").trailing_fill(true));
            d.audio.mic_volume = pct / 100.0;
        });
        setting_row(ui, "Audio quality", "", true, |ui| {
            ui.add(egui::Slider::new(&mut d.audio.bitrate_kbps, 96..=320).suffix(" kbps").trailing_fill(true));
        });
    });

    section_title(ui, "REPLAY BUFFER");
    card(ui, |ui| {
        let d = &mut app.draft;
        let need = estimate::buffer_ram_mb(d.bitrate_kbps, d.audio.bitrate_kbps, d.buffer_secs);
        let hint = if need > d.buffer_max_mb { format!("Needs ~{need} MB, over your limit, so it will keep less than this.") } else { format!("Uses about {need} MB of RAM.") };
        setting_row(ui, "Replay length", &hint, false, |ui| {
            ui.add(egui::Slider::new(&mut d.buffer_secs, 10..=600).suffix(" s").logarithmic(true).trailing_fill(true));
        });
        setting_row(ui, "Memory limit", "", false, |ui| {
            ui.add(egui::Slider::new(&mut d.buffer_max_mb, 256..=8192).suffix(" MB").logarithmic(true).trailing_fill(true));
        });
        setting_row(ui, "File format", "", false, |ui| {
            segmented(ui, &mut d.container, &CONTAINER_OPTS);
        });
        setting_row(ui, "Save folder", "", true, |ui| {
            if icon_button(ui, Icon::Folder, "Open folder", pal().muted).clicked() {
                let _ = std::fs::create_dir_all(&d.output_dir);
                let _ = opener::open(&d.output_dir);
            }
            let mut path = d.output_dir.display().to_string();
            if ui.add(egui::TextEdit::singleline(&mut path).desired_width(330.0).margin(Margin::symmetric(10, 7))).changed() {
                d.output_dir = path.into();
            }
        });
    });

    section_title(ui, "NOTIFICATIONS");
    card(ui, |ui| {
        let d = &mut app.draft;
        setting_row(ui, "Show notifications over games", "Hidden from your recordings. Needs borderless or windowed games.", false, |ui| {
            toggle(ui, &mut d.overlay_enabled);
        });
        setting_row(ui, "Position", "", false, |ui| {
            let items: [(Corner, &str); Corner::ALL.len()] = std::array::from_fn(|i| (Corner::ALL[i], Corner::ALL[i].label()));
            segmented(ui, &mut d.overlay_corner, &items);
        });
        setting_row(ui, "Duration", "", false, |ui| {
            ui.add(egui::Slider::new(&mut d.overlay_secs, 1..=10).suffix(" s").trailing_fill(true));
        });
        let mut preview = false;
        setting_row(ui, "Play a sound when a clip is saved", "Also works in exclusive-fullscreen games, where the overlay can't draw.", true, |ui| {
            if text_button(ui, None, "Preview", true).clicked() {
                preview = true;
            }
            toggle(ui, &mut d.sound_on_save);
        });
        if preview {
            app.preview_overlay();
            if app.draft.sound_on_save {
                crate::platform::beep();
            }
        }
    });

    section_title(ui, "HOTKEYS");
    card(ui, |ui| {
        setting_row(ui, "Save replay", "Click, then press the keys you want.", false, |ui| hotkey_field(app, ui, 0));
        setting_row(ui, "Turn replay on/off", "", true, |ui| hotkey_field(app, ui, 1));
    });

    section_title(ui, "GENERAL");
    card(ui, |ui| {
        let d = &mut app.draft;
        setting_row(ui, "Start Instant Replay when Rewind opens", "", false, |ui| {
            toggle(ui, &mut d.start_buffer_on_launch);
        });
        setting_row(ui, "Closing the window keeps Rewind in the tray", "Recording and hotkeys keep working.", false, |ui| {
            toggle(ui, &mut d.close_to_tray);
        });
        setting_row(ui, "Start hidden in the tray", "", false, |ui| {
            toggle(ui, &mut d.start_minimized);
        });
        setting_row(ui, "Start with Windows", "", true, |ui| {
            toggle(ui, &mut d.launch_at_login);
        });
    });
    ui.add_space(40.0);
}

// ---------------------------------------------------------------- about

fn about_page(ui: &mut egui::Ui) {
    page_header(ui, "About Rewind", "A small instant-replay recorder written in Rust.");
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(bold(format!("Version {}", env!("CARGO_PKG_VERSION"))));
            ui.label(muted("· made by pro82xw"));
        });
        ui.add_space(4.0);
        ui.label(muted("Capture, encoding and the replay buffer are provided by libobs from OBS Studio (GPL-3.0-or-later), driven through the libobs-rs bindings. Rewind is licensed GPL-3.0-or-later and isn't affiliated with NVIDIA or the OBS Project."));
        ui.add_space(4.0);
        ui.label(muted("Clips never leave your PC. The only network access is the one-time download of the OBS runtime."));
        ui.add_space(4.0);
        ui.label(muted("Typeface: Inter, SIL Open Font License 1.1."));
    });
}

// ---------------------------------------------------------------- dialogs

fn trim_window(app: &mut RewindApp, ctx: &egui::Context) {
    let Some(state) = &mut app.trim else { return };
    let mut open = true;
    let mut run = false;
    let mut cancel = false;
    egui::Window::new("Trim clip").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.label(bold(&state.clip.name));
        ui.label(muted("Lossless cut with no re-encoding, so the start snaps to the nearest keyframe."));
        ui.add_space(4.0);
        ui.radio_value(&mut state.mode, TrimMode::LastSeconds, "Keep only the last…");
        ui.add_enabled_ui(state.mode == TrimMode::LastSeconds, |ui| {
            ui.add(egui::DragValue::new(&mut state.last_secs).range(1.0..=3600.0).suffix(" s"));
        });
        ui.radio_value(&mut state.mode, TrimMode::Range, "Keep from … to …");
        ui.add_enabled_ui(state.mode == TrimMode::Range, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut state.start).range(0.0..=36000.0).suffix(" s"));
                ui.label("to");
                ui.add(egui::DragValue::new(&mut state.end).range(0.0..=36000.0).suffix(" s"));
            });
        });
        ui.label(muted("A new file is saved next to the original."));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if state.running {
                ui.add(egui::Spinner::new());
                ui.label("Trimming…");
            } else {
                if accent_pill(ui, "Trim", true).clicked() {
                    run = true;
                }
                if text_button(ui, None, "Cancel", true).clicked() {
                    cancel = true;
                }
            }
        });
    });
    if run {
        app.run_trim(ctx);
    }
    if cancel || !open {
        app.trim = None;
    }
}

fn delete_window(app: &mut RewindApp, ctx: &egui::Context) {
    let Some(clip) = app.confirm_delete.clone() else { return };
    let mut keep_open = true;
    let mut keep_it = false;
    let mut delete = false;
    egui::Window::new("Delete clip?").collapsible(false).resizable(false).open(&mut keep_open).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.label(bold(&clip.name));
        ui.label(muted("It will be moved to the Recycle Bin."));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if danger_pill(ui, "Delete").clicked() {
                delete = true;
            }
            if text_button(ui, None, "Keep it", true).clicked() {
                keep_it = true;
            }
        });
    });
    if delete {
        match trash::delete(&clip.path) {
            Ok(()) => app.notify("Moved to Recycle Bin", false),
            Err(e) => app.notify(format!("Couldn't delete: {e}"), true),
        }
        app.confirm_delete = None;
        app.refresh_clips(true);
    } else if !keep_open || keep_it {
        app.confirm_delete = None;
    }
}

fn toast(app: &RewindApp, ctx: &egui::Context) {
    let Some(t) = &app.toast else { return };
    // Reuse the easing that already drives the over-the-game toast: slide up on entry,
    // fade in and out. The loop's repaint cadence follows [`RewindApp::toast_alpha`].
    let elapsed = t.at.elapsed().as_secs_f32();
    let alpha = app.toast_alpha();
    let slide = rewind_core::toast::slide_remaining(elapsed) * 12.0;
    egui::Area::new(egui::Id::unique("toast"))
        .anchor(egui::Align2::RIGHT_BOTTOM, [-24.0, -24.0 + slide])
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            egui::Frame::new()
                .fill(if t.is_error { pal().danger_bg } else { pal().accent_bg })
                .stroke(Stroke::new(1.0, if t.is_error { pal().danger } else { pal().accent_dim }))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.horizontal(|ui| {
                        let col = if t.is_error { pal().danger } else { pal().accent };
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 10.0, col.gamma_multiply(0.18));
                        paint_icon(ui.painter(), if t.is_error { Icon::Warning } else { Icon::Check }, r.center(), 12.0, col);
                        ui.label(RichText::new(&t.text).color(col));
                    });
                });
        });
}

fn preset_card(ui: &mut egui::Ui, title: &str, blurb: &str) -> bool {
    let size = vec2(ui.available_width(), 82.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), anim_time(0.16));
    ui.painter().rect_filled(rect, CornerRadius::same(14), pal().card.lerp_to_gamma(pal().card_hi, t));
    ui.painter().rect_stroke(rect, CornerRadius::same(14), Stroke::new(1.0, pal().stroke.lerp_to_gamma(pal().accent, 0.6 * t)), egui::StrokeKind::Inside);
    ui.painter().text(pos2(rect.left() + 16.0, rect.top() + 18.0), egui::Align2::LEFT_CENTER, title, egui::FontId::new(15.5, semibold_family()), pal().text);
    let galley = ui.painter().layout(blurb.to_string(), egui::FontId::proportional(12.5), pal().muted, rect.width() - 32.0);
    ui.painter().galley(pos2(rect.left() + 16.0, rect.top() + 36.0), galley, pal().muted);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn swatch(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 12.0, color);
    ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(1.0, pal().stroke));
}

fn status_text(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).color(color).size(13.0));
}

/// Download / progress / remove control shared by every add-on row.
fn pack_controls(app: &mut RewindApp, ui: &mut egui::Ui, key: &str, installed: bool, can_download: bool, on_download: impl FnOnce(&mut RewindApp), on_remove: impl FnOnce(&mut RewindApp)) {
    if let Some(p) = app.packs.jobs.get(key) {
        let f = p.fraction();
        if text_button(ui, None, "Cancel", true).clicked() {
            p.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        progress_bar(ui, f, 140.0);
    } else if installed {
        if text_button(ui, None, "Remove", true).clicked() {
            on_remove(app);
        }
        status_text(ui, "Installed", pal().accent);
    } else if ui.add_enabled_ui(can_download, |ui| text_button(ui, Some(Icon::Download), "Download", can_download)).inner.clicked() && can_download {
        on_download(app);
    }
}

fn addons_page(app: &mut RewindApp, ui: &mut egui::Ui) {
    page_header(ui, "Add-ons", "Optional extras. Nothing here is installed by default, so Rewind stays small.");

    // Index status
    match app.packs.index_state.clone() {
        IndexState::Loading => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label(muted("Checking for add-ons…"));
            });
        }
        IndexState::Failed(e) => {
            egui::Frame::new()
                .fill(pal().warn_bg)
                .stroke(Stroke::new(1.0, pal().warn.gamma_multiply(0.7)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Couldn't load the add-on list: {e}")).color(pal().warn));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if text_button(ui, None, "Retry", true).clicked() {
                                app.packs.request_index();
                            }
                        });
                    });
                });
        }
        IndexState::Idle => app.packs.request_index(),
        IndexState::Ready => {}
    }

    // ---- Voice
    section_title(ui, "VOICE CLIPPING");
    let index = app.packs.index.clone();
    let runtime_ready = app.packs.runtime_installed();
    card(ui, |ui| {
        setting_row(ui, "Speech engine", "About 15 MB. Recognition runs on this PC; no audio is sent anywhere.", false, |ui| {
            let can = index.as_ref().and_then(|i| i.voice_runtime.as_ref()).is_some();
            pack_controls(app, ui, crate::packs::KEY_RUNTIME, runtime_ready, can, |a| a.packs.install_runtime(), |a| {
                a.packs.remove_runtime();
                a.sync_voice();
            });
        });
        let lang = app.draft.voice.language.clone();
        let voices = index.as_ref().map(|i| i.voice.clone()).unwrap_or_default();
        setting_row(ui, "Language", "A small language model, about 40 MB each.", false, |ui| {
            let model_ready = app.packs.model_installed(&lang);
            let pack = voices.iter().find(|v| v.id == lang).cloned();
            let key = format!("voice-{lang}");
            let p2 = pack.clone();
            pack_controls(app, ui, &key, model_ready, pack.is_some(), move |a| {
                if let Some(p) = p2 {
                    a.packs.install_voice(&p);
                }
            }, |a| {
                a.packs.remove_voice(&lang);
                a.sync_voice();
            });
            let current = voices.iter().find(|v| v.id == app.draft.voice.language).map(|v| v.name.clone()).unwrap_or_else(|| app.draft.voice.language.clone());
            egui::ComboBox::from_id_salt("voice-lang").width(190.0).selected_text(current).show_ui(ui, |ui| {
                for v in &voices {
                    ui.selectable_value(&mut app.draft.voice.language, v.id.clone(), &v.name);
                }
            });
        });
        let ready = runtime_ready && app.packs.model_installed(&app.draft.voice.language);
        setting_row(ui, "Save a clip when I say the phrase", if ready { "" } else { "Download the engine and a language first." }, false, |ui| {
            ui.add_enabled_ui(ready, |ui| {
                toggle(ui, &mut app.draft.voice.enabled);
            });
        });
        setting_row(ui, "Trigger phrase", "Use words your language model knows, like \"clip that\".", false, |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.draft.voice.phrase).desired_width(220.0).margin(Margin::symmetric(10, 7)));
        });
        setting_row(ui, "Sensitivity", "Higher picks up quieter speech but wakes more often.", false, |ui| {
            ui.add(egui::Slider::new(&mut app.draft.voice.sensitivity, 0.0..=1.0).show_value(false).trailing_fill(true));
        });
        setting_row(ui, "Status", "", true, |ui| {
            match app.voice_handle().map(|v| (v.shared.status(), v.shared.level())) {
                Some((VoiceStatus::Listening, level)) => {
                    progress_bar(ui, (level * 6.0).clamp(0.0, 1.0), 120.0);
                    status_text(ui, "Listening", pal().accent);
                }
                Some((VoiceStatus::Starting, _)) => status_text(ui, "Starting…", pal().muted),
                Some((VoiceStatus::Error(e), _)) => status_text(ui, &e, pal().danger),
                Some((VoiceStatus::Stopped, _)) | None => status_text(ui, if app.cfg.voice.enabled { "Not running" } else { "Off" }, pal().muted),
            }
        });
    });
    ui.label(muted("Tip: press Apply after changing the phrase or language. Say it a little after you finish the moment you want to keep."));

    // ---- Themes
    section_title(ui, "THEMES");
    card(ui, |ui| {
        let current = app.draft.theme.clone();
        // Built-in default
        ui.horizontal(|ui| {
            swatch(ui, Color32::from_rgb(0x4a, 0xd6, 0xa7));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(bold("Mint (default)"));
                ui.label(muted("Built in"));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if current == "default" {
                    status_text(ui, "In use", pal().accent);
                } else if text_button(ui, None, "Use", true).clicked() {
                    app.draft.theme = "default".into();
                }
            });
        });
        let installed = app.packs.themes.clone();
        for t in &installed {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let a = t.palette.accent;
                swatch(ui, Color32::from_rgb(a[0], a[1], a[2]));
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(bold(&t.name));
                    ui.label(muted(if t.author.is_empty() { "Installed".to_string() } else { format!("by {}", t.author) }));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if text_button(ui, None, "Remove", true).clicked() {
                        if app.draft.theme == t.id {
                            app.draft.theme = "default".into();
                        }
                        app.packs.remove_theme(&t.id);
                    }
                    if current == t.id {
                        status_text(ui, "In use", pal().accent);
                    } else if text_button(ui, None, "Use", true).clicked() {
                        app.draft.theme = t.id.clone();
                    }
                });
            });
        }
        let available: Vec<_> = index.as_ref().map(|i| i.theme.iter().filter(|t| !installed.iter().any(|x| x.id == t.id)).cloned().collect()).unwrap_or_default();
        for t in available {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                swatch(ui, rewind_core::theme::parse_hex(&t.swatch).map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(pal().faint));
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(bold(&t.name));
                    ui.label(muted(if t.author.is_empty() { "Optional download".to_string() } else { format!("by {} · optional download", t.author) }));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let key = format!("theme-{}", t.id);
                    pack_controls(app, ui, &key, false, true, |a| a.packs.install_theme(&t), |_| {});
                });
            });
        }
    });
    ui.label(muted("Make your own: drop a theme .toml file into the themes folder, or start from the example."));
    ui.horizontal(|ui| {
        if text_button(ui, Some(Icon::Folder), "Open themes folder", true).clicked() {
            let dir = crate::ui::themes_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = opener::open(dir);
        }
        if text_button(ui, Some(Icon::Addons), "Create example theme", true).clicked() {
            create_example_theme(app);
        }
    });
    ui.label(muted("The example is a fully commented copy of the default theme. Rename it, change the colours and press Apply."));
}

/// The example theme lives in docs/ so it is both readable on disk and embedded here.
const EXAMPLE_THEME: &str = include_str!("../../../../docs/theme-template.toml");

fn create_example_theme(app: &mut RewindApp) {
    let dir = crate::ui::themes_dir();
    let _ = std::fs::create_dir_all(&dir);
    let mut name = "example-theme".to_string();
    let mut n = 1;
    while dir.join(format!("{name}.toml")).exists() {
        n += 1;
        name = format!("example-theme-{n}");
    }
    match std::fs::write(dir.join(format!("{name}.toml")), EXAMPLE_THEME) {
        Ok(()) => {
            app.packs.scan_themes();
            app.notify(format!("Created {name}.toml in the themes folder"), false);
        }
        Err(e) => app.notify(format!("Couldn't create the example theme: {e}"), true),
    }
}
