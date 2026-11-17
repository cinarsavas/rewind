use std::fs;

use eframe::egui::{
    self, pos2, vec2, Align, Color32, CornerRadius, Event, Key, Layout, Margin, RichText, Sense,
    Stroke,
};
use rewind_core::config::{
    output_size, CaptureMode, Codec, Container, Corner, DisplayMethod, EffectStyle, Preset,
    Quality, Resolution, FPS_CHOICES, FPS_LABELS,
};
use rewind_core::estimate;
use rewind_core::hotkey::Hotkey;
use rewind_core::i18n::{default_phrase, Lang};
use rewind_core::library::{human_size, sanitize_file_name, unique_path, Clip};

use super::effects;
use super::i18n::{t, tf};
use super::theme::*;
use super::widgets::*;
use super::{row_reveal_at, Page, Phase, RenameState, RewindApp, TrimMode, TrimState, UpdateState};
use crate::audio::AudioDevice;
use crate::packs::IndexState;
use crate::thumbs::Thumbs;
use crate::voice::VoiceStatus;
use rewind_core::voice::Verdict;

pub fn draw(app: &mut RewindApp, root: &mut egui::Ui) {
    let ctx = root.ctx().clone();
    let ctx = &ctx;
    paint_background(root);
    let l = layout();
    match &app.phase {
        Phase::Installing => {
            return splash(
                root,
                "Setting things up",
                "Downloading the OBS capture runtime. This only happens once (about 100 MB).",
            )
        }
        Phase::Starting { .. } => {
            return splash(root, "Starting", "Spinning up the capture engine…")
        }
        Phase::Failed(msg) => {
            let msg = msg.clone();
            return failure(root, &msg);
        }
        Phase::Ready => {}
    }

    egui::Panel::left("nav")
        .exact_size(l.nav_width)
        .frame(
            egui::Frame::new()
                .fill(nav_fill())
                .inner_margin(Margin::same(18)),
        )
        .show(root, |ui| nav(app, ui));

    let dirty = app.draft != app.cfg;
    if dirty {
        egui::Panel::bottom("apply_bar")
            .frame(
                egui::Frame::new()
                    .fill(nav_fill())
                    .stroke(Stroke::new(1.0, pal().stroke))
                    .inner_margin(Margin::symmetric(l.page_margin.round() as i8, 12)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("You have unsaved changes")).color(pal().warn));
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
        .frame(
            egui::Frame::new()
                .fill(content_fill())
                .inner_margin(Margin::symmetric(l.page_margin.round() as i8, 24)),
        )
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
                // The Clips grid uses the whole window; text pages keep a readable column.
                let max_w = if app.page == Page::Clips {
                    layout().content_width.max(1700.0)
                } else {
                    layout().content_width
                };
                column(ui, max_w, dir * (1.0 - t) * 9.0, |ui| {
                    ui.set_opacity(t);
                    ui.add_space((1.0 - t) * 12.0);
                    match app.page {
                        Page::Replay => replay_page(app, ui, ctx),
                        Page::Clips => clips_page(app, ui),
                        Page::Settings => settings_page(app, ui, ctx),
                        Page::Addons => addons_page(app, ui),
                        Page::About => about_page(app, ui),
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
    rename_window(app, ctx);
    delete_window(app, ctx);
    toast(app, ctx);
}

/// Centre a content column of at most `content_width`, offset sideways for the page transition.
fn column(ui: &mut egui::Ui, max_w: f32, slide: f32, add: impl FnOnce(&mut egui::Ui)) {
    let avail = ui.available_width();
    let w = avail.min(max_w);
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
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(nav_fill()))
        .show(root, |ui| {
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
    egui::CentralPanel::default().frame(egui::Frame::new().fill(nav_fill())).show(root, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.28);
            ui.label(bold("Rewind couldn't start").size(24.0).color(pal().danger));
            ui.add_space(6.0);
            ui.set_max_width(580.0);
            ui.label(RichText::new(msg).color(pal().muted));
            ui.add_space(6.0);
            if text_button(ui, Some(Icon::Copy), "Copy error", true).clicked() {
                ui.ctx().copy_text(msg.to_string());
            }
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
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), pal().accent);
        paint_icon(
            ui.painter(),
            Icon::Record,
            rect.center(),
            20.0,
            pal().on_accent,
        );
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
                        ui.painter().circle_filled(
                            r.center(),
                            5.0 + 2.0 * k,
                            pal().accent.gamma_multiply(0.2),
                        );
                    }
                    ui.painter().circle_filled(
                        r.center(),
                        4.0,
                        if active { pal().accent } else { pal().faint },
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(bold(if active { "Replay on" } else { "Replay off" }).size(13.5));
                        ui.label(muted(if active {
                            tf(
                                "Last {secs} s",
                                &[("secs", &app.cfg.buffer_secs.to_string())],
                            )
                        } else {
                            "Not recording".to_string()
                        }));
                    });
                });
            });
    });
}

fn nav_item(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let sel = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("selected"), selected, anim_time(0.22));
    let hover = ui.ctx().animate_bool_with_time(
        resp.id.with("hover"),
        resp.hovered() && !selected,
        anim_time(0.18),
    );
    if hover > 0.0 {
        let c = pal().card;
        let hover_fill =
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (215.0 * hover) as u8);
        ui.painter()
            .rect_filled(rect, CornerRadius::same(11), hover_fill);
        let h = 10.0 * hover;
        ui.painter().rect_filled(
            egui::Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, h)),
            CornerRadius::same(2),
            pal().accent.gamma_multiply(0.55),
        );
    }
    if sel > 0.0 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(11),
            pal().accent_tint.gamma_multiply(sel),
        );
        let bar_h = 20.0 * sel;
        ui.painter().rect_filled(
            egui::Rect::from_center_size(
                pos2(rect.left() + 1.5, rect.center().y),
                vec2(3.0, bar_h),
            ),
            CornerRadius::same(2),
            pal().accent,
        );
    }
    let col = if selected {
        pal().accent
    } else {
        pal().muted.lerp_to_gamma(pal().text, hover)
    };
    // A tiny slide makes the row feel alive without moving the layout.
    let dx = 2.0 * hover;
    paint_icon(
        ui.painter(),
        icon,
        pos2(rect.left() + 26.0 + dx, rect.center().y),
        18.0,
        col,
    );
    ui.painter().text(
        pos2(rect.left() + 48.0 + dx, rect.center().y),
        egui::Align2::LEFT_CENTER,
        t(label),
        egui::FontId::new(15.0, semibold_family()),
        col,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn page_header(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.label(RichText::new(t(title)).heading());
    ui.label(muted(sub));
    ui.add_space(10.0);
}

fn mmss(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

// ---------------------------------------------------------------- replay page

fn resolution_text(app: &RewindApp) -> String {
    let (bw, bh) = app
        .monitors
        .get(app.cfg.monitor_index)
        .or_else(|| app.monitors.first())
        .map(|m| (m.width, m.height))
        .unwrap_or((1920, 1080));
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
    page_header(
        ui,
        "Instant Replay",
        "Always recording in the background. One key keeps the moment you just had.",
    );
    let active = app.buffer_active();
    let secs = app.cfg.buffer_secs;
    let elapsed = app
        .buffer_started_at
        .map(|t| t.elapsed().as_secs_f32())
        .unwrap_or(0.0);
    let have = if active {
        elapsed.min(secs as f32)
    } else {
        0.0
    };
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
        .stroke(Stroke::new(
            1.0,
            if active {
                pal().accent_dim
            } else {
                pal().stroke
            },
        ))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::same(26))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (big, small) = if active {
                    (mmss(have), tf("of {time}", &[("time", &mmss(secs as f32))]))
                } else {
                    ("Off".to_string(), "not recording".to_string())
                };
                ring(ui, 176.0, have / secs as f32, active, &big, &small);
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.label(
                        bold(if active { "RECORDING" } else { "PAUSED" })
                            .size(12.0)
                            .color(if active { pal().accent } else { pal().faint }),
                    );
                    ui.label(
                        bold(if active {
                            tf(
                                "Keeping the last {secs} seconds",
                                &[("secs", &secs.to_string())],
                            )
                        } else {
                            "Instant Replay is off".to_string()
                        })
                        .size(23.0),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        chip(ui, &encoder_label);
                        chip(ui, &resolution_text(app));
                        chip(ui, &format!("{} fps", app.cfg.fps));
                        chip(ui, &format!("{} Mbps", app.cfg.bitrate_kbps / 1000));
                        chip(ui, audio_text(app));
                    });
                    ui.add_space(6.0);
                    let label = if app.is_saving() {
                        "Saving…".to_string()
                    } else {
                        tf("Save last {secs} s", &[("secs", &secs.to_string())])
                    };
                    let key = app.cfg.save_hotkey.to_string();
                    if primary_button(ui, &label, Some(&key), active && !app.is_saving()).clicked()
                    {
                        app.save_replay(ctx);
                    }
                    ui.horizontal(|ui| {
                        if text_button(ui, None, if active { "Turn off" } else { "Turn on" }, true)
                            .clicked()
                        {
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
        (
            "MEMORY IN USE",
            format!("~{ram} MB"),
            "while the buffer is full".to_string(),
        ),
        (
            "CLIP SIZE",
            format!("~{clip} MB"),
            tf("for {secs} seconds", &[("secs", &secs.to_string())]),
        ),
        (
            "THIS APP",
            format!("{:.0}% CPU", app.perf.cpu_pct),
            format!("{} MB · {encoder_label}", app.perf.mem_mb),
        ),
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

    // Hotkey problems are real warnings; encoder/capture notes are calm information. Both
    // have a copy button so they can be pasted into a bug report.
    let mut copy: Option<String> = None;
    for w in &app.hotkey_errors {
        egui::Frame::new()
            .fill(pal().warn_bg)
            .stroke(Stroke::new(1.0, pal().warn.gamma_multiply(0.7)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("⚠  {}", super::i18n::tp(w))).color(pal().warn));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Copy, "Copy", pal().warn).clicked() {
                            copy = Some(super::i18n::tp(w));
                        }
                    });
                });
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
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(super::i18n::tp(note)).color(pal().accent),
                            )
                            .wrap(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icon_button(ui, Icon::Copy, "Copy", pal().accent).clicked() {
                                copy = Some(note.clone());
                            }
                        });
                    });
                });
        }
    }
    if let Some(text) = copy {
        app.copy_to_clipboard(text);
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
        let hint = tf(
            "Press {key} while Instant Replay is on to save your first one.",
            &[("key", &app.cfg.save_hotkey.to_string())],
        );
        card(ui, |ui| {
            ui.label(bold("No clips yet"));
            ui.label(muted(hint));
        });
        return;
    }
    let preview_exe = app.preview_ffmpeg();
    let opts = RowOpts {
        want_thumb: app.cfg.thumbnails,
        can_trim: app.ffmpeg.is_some(),
        permanent: app.cfg.delete_permanently,
        zoom: 1.0,
        preview: preview_exe.as_deref(),
        prefetch: ui.ctx().input(|i| i.viewport().focused.unwrap_or(false)),
    };
    let mut action: Option<(usize, ClipAction)> = None;
    let mut any_preview = false;
    for i in 0..app.clip_rows.len().min(3) {
        let reveal = app.row_reveal(i);
        let row = &app.clip_rows[i];
        let (a, previewing) = clip_row(
            ui,
            &row.clip,
            &row.meta,
            &mut app.thumbs,
            &mut app.previews,
            opts,
            reveal,
        );
        any_preview |= previewing;
        if let Some(a) = a {
            action = Some((i, a));
        }
    }
    if !any_preview {
        app.previews.unhover();
    }
    if let Some((i, a)) = action {
        let clip = app.clip_rows[i].clip.clone();
        apply_clip_action(app, a, &clip);
    }
}

#[derive(Clone, Copy)]
enum ClipAction {
    Delete,
    Rename,
    Trim,
    Reveal,
    Play,
    Favorite,
    Share,
    ShareSmall,
}

fn apply_clip_action(app: &mut RewindApp, action: ClipAction, clip: &Clip) {
    match action {
        ClipAction::Delete => {
            if app.cfg.confirm_delete {
                app.confirm_delete = Some(clip.clone());
            } else {
                delete_clip_now(app, clip);
            }
        }
        ClipAction::Rename => {
            let stem = clip
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| clip.name.clone());
            app.rename = Some(RenameState {
                clip: clip.clone(),
                stem,
                error: None,
                focus: true,
            });
        }
        ClipAction::Trim => {
            app.trim = Some(TrimState {
                clip: clip.clone(),
                mode: TrimMode::LastSeconds,
                last_secs: 15.0,
                start: 0.0,
                end: 15.0,
                running: false,
            })
        }
        ClipAction::Reveal => {
            let _ = opener::reveal(&clip.path);
        }
        ClipAction::Play => {
            let _ = opener::open(&clip.path);
        }
        ClipAction::Favorite => app.toggle_favorite(clip),
        ClipAction::Share => app.share_copy(clip),
        ClipAction::ShareSmall => app.share_small(clip),
    }
}

/// Delete a clip (Recycle Bin, or for good when "Delete permanently" is on) and refresh
/// the list. Uses the applied config, not the draft, so an unapplied toggle never decides
/// whether a file is recoverable.
fn delete_clip_now(app: &mut RewindApp, clip: &Clip) {
    let result = if app.cfg.delete_permanently {
        std::fs::remove_file(&clip.path).map_err(|e| e.to_string())
    } else {
        trash::delete(&clip.path).map_err(|e| e.to_string())
    };
    if result.is_ok() {
        app.favorite_moved(&clip.path, None);
    }
    match result {
        Ok(()) if app.cfg.delete_permanently => app.notify(t("Clip deleted"), false),
        Ok(()) => app.notify(t("Moved to Recycle Bin"), false),
        Err(e) => app.notify(tf("Couldn't delete: {e}", &[("e", &e)]), true),
    }
    app.refresh_clips(true);
}

/// How clip rows are drawn; the same for every row on a page.
#[derive(Clone, Copy)]
struct RowOpts<'a> {
    want_thumb: bool,
    can_trim: bool,
    permanent: bool,
    /// Thumbnail size multiplier (Clips page zoom).
    zoom: f32,
    /// ffmpeg from the hover-preview add-on, when it's installed and switched on.
    preview: Option<&'a std::path::Path>,
    /// Prepare previews of drawn rows in the background (only while Rewind is in front).
    prefetch: bool,
}

/// One clip row. Borrows the row and the thumbnail cache instead of the whole app, so the
/// caller can iterate the library without cloning it every frame. Returns the clicked action
/// and whether the thumbnail is being hovered for a preview.
fn clip_row(
    ui: &mut egui::Ui,
    clip: &Clip,
    meta: &str,
    thumbs: &mut Thumbs,
    previews: &mut crate::preview::Previews,
    opts: RowOpts,
    reveal: f32,
) -> (Option<ClipAction>, bool) {
    let RowOpts {
        want_thumb,
        can_trim,
        permanent,
        zoom,
        preview,
        prefetch,
    } = opts;
    let mut previewing = false;
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
                let size = if want_thumb {
                    vec2(112.0, 63.0) * zoom
                } else {
                    vec2(46.0, 46.0)
                };
                let (r, thumb_resp) = ui.allocate_exact_size(size, Sense::hover());
                let tex = if want_thumb {
                    thumbs.get(ui.ctx(), &clip.path, clip.modified)
                } else {
                    None
                };
                if let Some(tex) = tex {
                    egui::Image::new((tex.id(), size))
                        .corner_radius(CornerRadius::same(10))
                        .paint_at(ui, r);
                } else {
                    ui.painter().rect_filled(
                        r,
                        CornerRadius::same(if want_thumb { 10 } else { 12 }),
                        pal().deep,
                    );
                    paint_icon(ui.painter(), Icon::Play, r.center(), 18.0, pal().accent);
                }
                if let (Some(ffmpeg), true, false) =
                    (preview, prefetch && want_thumb, thumb_resp.hovered())
                {
                    previews.prefetch(
                        &clip.path,
                        clip.modified,
                        ffmpeg,
                        size.x * ui.ctx().pixels_per_point(),
                    );
                }
                if let (Some(ffmpeg), true) = (preview, want_thumb && thumb_resp.hovered()) {
                    previewing = true;
                    let ctx = ui.ctx().clone();
                    match previews.hover(
                        &ctx,
                        &clip.path,
                        clip.modified,
                        ffmpeg,
                        size.x * ctx.pixels_per_point(),
                    ) {
                        crate::preview::Frame::Ready(tex, progress) => {
                            egui::Image::new((tex.id(), size))
                                .corner_radius(CornerRadius::same(10))
                                .paint_at(ui, r);
                            let bar = egui::Rect::from_min_max(
                                pos2(r.left() + 6.0, r.bottom() - 6.0),
                                pos2(r.right() - 6.0, r.bottom() - 3.0),
                            );
                            ui.painter().rect_filled(
                                bar,
                                CornerRadius::same(2),
                                Color32::from_black_alpha(120),
                            );
                            ui.painter().rect_filled(
                                egui::Rect::from_min_max(
                                    bar.min,
                                    pos2(bar.left() + bar.width() * progress, bar.bottom()),
                                ),
                                CornerRadius::same(2),
                                pal().accent,
                            );
                        }
                        crate::preview::Frame::Loading => {
                            ui.painter().rect_filled(
                                r,
                                CornerRadius::same(10),
                                Color32::from_black_alpha(110),
                            );
                            let t = ui.input(|i| i.time) as f32;
                            for k in 0..3 {
                                let a = ((t * 3.0 - k as f32 * 0.6).sin() * 0.5 + 0.5) * 0.8 + 0.2;
                                ui.painter().circle_filled(
                                    pos2(r.center().x + (k as f32 - 1.0) * 9.0, r.center().y),
                                    2.6,
                                    pal().accent.gamma_multiply(a),
                                );
                            }
                        }
                        crate::preview::Frame::Failed(e) => {
                            thumb_resp
                                .clone()
                                .on_hover_text(format!("{}: {e}", t("No preview")));
                        }
                    }
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(bold(&clip.name).size(14.5));
                    ui.label(muted(meta));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let tip = if permanent {
                        "Delete permanently"
                    } else {
                        "Move to Recycle Bin"
                    };
                    if icon_button(ui, Icon::Trash, tip, pal().danger).clicked() {
                        action = Some(ClipAction::Delete);
                    }
                    if icon_button(ui, Icon::Pencil, "Rename", pal().muted).clicked() {
                        action = Some(ClipAction::Rename);
                    }
                    let trim = ui
                        .add_enabled_ui(can_trim, |ui| {
                            icon_button(
                                ui,
                                Icon::Scissors,
                                if can_trim {
                                    "Trim"
                                } else {
                                    "Needs ffmpeg next to Rewind.exe or on PATH"
                                },
                                pal().muted,
                            )
                        })
                        .inner;
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
    let action = (action, previewing);

    // Hover gets a soft accent border plus a faint outer glow. The fill stays put so there
    // is no layout cost; the glow is a wide translucent stroke, drawn only while hovered.
    let hover = ui.ctx().animate_bool_with_time(
        resp.id.with("row-hover"),
        ui.rect_contains_pointer(resp.rect),
        anim_time(0.18),
    );
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
    let header = format!(
        "{} · {} · {}",
        tf("{n} clips", &[("n", &app.clips.len().to_string())]),
        human_size(app.clips_total_bytes),
        app.cfg.output_dir.display()
    );
    page_header(ui, "Clips", &header);
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.clip_filter)
                .hint_text(t("Search clips"))
                .desired_width(280.0)
                .margin(Margin::symmetric(12, 8)),
        );
        if resp.changed() {
            app.mark_filter_dirty();
        }
        let fav_color = if app.favorites_only {
            pal().warn
        } else {
            pal().muted
        };
        let fav_icon = if app.favorites_only {
            Icon::StarFilled
        } else {
            Icon::Star
        };
        if icon_button(ui, fav_icon, "Show only starred clips", fav_color).clicked() {
            app.favorites_only = !app.favorites_only;
            app.mark_filter_dirty();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if app.cfg.thumbnails {
                // Thumbnail size. Saved right away (no Apply needed); Ctrl + mouse wheel on
                // the list does the same.
                let before = app.cfg.clip_zoom;
                let mut zoom = before;
                let zr = ui
                    .add(
                        egui::Slider::new(&mut zoom, 0.75..=2.5)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .on_hover_text(t("Thumbnail size (Ctrl + mouse wheel)"));
                ui.label(muted("Size"));
                let wheel = ui.input(|i| {
                    if i.modifiers.command {
                        i.zoom_delta()
                    } else {
                        1.0
                    }
                });
                if wheel != 1.0 {
                    zoom = (zoom * wheel).clamp(0.75, 2.5);
                }
                if zoom != before {
                    app.cfg.clip_zoom = zoom;
                    app.draft.clip_zoom = zoom;
                }
                if zr.drag_stopped() || (zoom != before && !zr.dragged()) {
                    app.save_quietly();
                }
            }
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
            (
                "No clips yet",
                tf(
                    "Press {key} while Instant Replay is on to save your first one.",
                    &[("key", &app.cfg.save_hotkey.to_string())],
                ),
            )
        } else if app.favorites_only {
            (
                "No starred clips",
                t("Star a clip to keep it here. Starred clips are never auto-deleted.").to_string(),
            )
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
    let preview_exe = app.preview_ffmpeg();
    let opts = RowOpts {
        want_thumb: app.cfg.thumbnails,
        can_trim: app.ffmpeg.is_some(),
        permanent: app.cfg.delete_permanently,
        zoom: app.cfg.clip_zoom,
        preview: preview_exe.as_deref(),
        prefetch: ui.ctx().input(|i| i.viewport().focused.unwrap_or(false)),
    };
    let animate = animations_on() && !app.cfg.low_power_ui;
    let shown = app.list_shown;

    // A grid of cards: thumbnail on top, then the name and the buttons each get the full
    // card width, so long names and the action icons never fight for space. The Size slider
    // sets the card width; the column count follows the window.
    const GAP: f32 = 14.0;
    // Leave room for the scroll bar so the last column is never covered or cut off.
    let avail = (ui.available_width() - 6.0).max(200.0);
    let wanted = (250.0 * app.cfg.clip_zoom).max(230.0);
    // The nearest whole number of columns, so cards stay close to the chosen size instead
    // of stretching to almost twice as wide when the window is a bit too narrow for one more.
    let cols = (((avail + GAP) / (wanted + GAP)).round() as usize).max(1);
    // Never squeeze cards below a usable width.
    let cols = cols.min((((avail + GAP) / (220.0 + GAP)).floor() as usize).max(1));
    let card_w = ((avail - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    let total = app.filter_idx.len();
    let rows = total.div_ceil(cols);

    let mut any_preview = false;
    let mut action: Option<(usize, ClipAction)> = None;
    // Rows scrolled out of view become empty space of the same height, so a big library
    // costs the same per frame as a small one and only visible cards load thumbnails.
    let mut row_h = app.clip_row_h;
    for r in 0..rows {
        let top = ui.cursor().top();
        if row_h > 0.0 {
            let slot = egui::Rect::from_min_size(pos2(ui.cursor().left(), top), vec2(avail, row_h));
            if !ui.is_rect_visible(slot) {
                ui.add_space(row_h);
                continue;
            }
        }
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for c in 0..cols {
                let pos = r * cols + c;
                let Some(&i) = app.filter_idx.get(pos) else {
                    break;
                };
                let reveal = row_reveal_at(shown, pos, animate);
                let row = &app.clip_rows[i];
                let key = rewind_core::library::relative_key(&app.cfg.output_dir, &row.clip.path);
                let fav = app.cfg.favorites.iter().any(|f| *f == key);
                let sharing = app.sharing.as_deref() == Some(row.clip.name.as_str());
                let (a, previewing) = clip_card(
                    ui,
                    card_w,
                    &row.clip,
                    &row.meta,
                    fav,
                    sharing,
                    &mut app.thumbs,
                    &mut app.previews,
                    opts,
                    reveal,
                );
                any_preview |= previewing;
                if let Some(a) = a {
                    action = Some((i, a));
                }
            }
        });
        ui.add_space(GAP - ui.spacing().item_spacing.y);
        row_h = ui.cursor().top() - top;
    }
    app.clip_row_h = row_h;
    if !any_preview {
        app.previews.unhover();
    }
    if let Some((i, a)) = action {
        let clip = app.clip_rows[i].clip.clone();
        apply_clip_action(app, a, &clip);
    }
}

/// One clip in the Clips grid. Returns the chosen action and whether the thumbnail is
/// being hovered for a preview.
#[allow(clippy::too_many_arguments)]
fn clip_card(
    ui: &mut egui::Ui,
    width: f32,
    clip: &Clip,
    meta: &str,
    favorite: bool,
    sharing: bool,
    thumbs: &mut Thumbs,
    previews: &mut crate::preview::Previews,
    opts: RowOpts,
    reveal: f32,
) -> (Option<ClipAction>, bool) {
    let mut action = None;
    let mut previewing = false;
    let prev_opacity = ui.opacity();
    if reveal < 1.0 {
        ui.multiply_opacity(reveal.max(0.0));
    }
    // 10 px margin and 1 px border on each side.
    let inner = (width - 22.0).max(60.0);
    let resp = ui
        .allocate_ui_with_layout(
            vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(pal().card)
                    .stroke(Stroke::new(1.0, pal().stroke))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(inner);
                        ui.set_max_width(inner);
                        ui.spacing_mut().item_spacing.y = 6.0;

                        // ---- thumbnail (click to play, hover to preview)
                        let size = if opts.want_thumb {
                            vec2(inner, (inner * 9.0 / 16.0).round())
                        } else {
                            vec2(inner, 64.0)
                        };
                        let (r, thumb) = ui.allocate_exact_size(size, Sense::click());
                        // Extra animation: the picture zooms in a little under the pointer.
                        let motion = extra_motion();
                        let zoom = if motion && opts.want_thumb {
                            ui.ctx().animate_bool_with_time(
                                egui::Id::unique(("clip-zoom", &clip.path)),
                                thumb.hovered(),
                                0.22,
                            )
                        } else {
                            0.0
                        };
                        let uv = {
                            let z = 1.0 + 0.06 * zoom * zoom * (3.0 - 2.0 * zoom);
                            let h = 0.5 / z;
                            egui::Rect::from_min_max(pos2(0.5 - h, 0.5 - h), pos2(0.5 + h, 0.5 + h))
                        };
                        let tex = if opts.want_thumb {
                            thumbs.get(ui.ctx(), &clip.path, clip.modified)
                        } else {
                            None
                        };
                        match &tex {
                            Some(tex) => {
                                egui::Image::new((tex.id(), size))
                                    .uv(uv)
                                    .corner_radius(CornerRadius::same(10))
                                    .paint_at(ui, r);
                            }
                            None => {
                                ui.painter()
                                    .rect_filled(r, CornerRadius::same(10), pal().deep);
                                paint_icon(
                                    ui.painter(),
                                    Icon::Play,
                                    r.center(),
                                    22.0,
                                    pal().accent,
                                );
                            }
                        }
                        if let (Some(ffmpeg), true, false) = (
                            opts.preview,
                            opts.prefetch && opts.want_thumb,
                            thumb.hovered(),
                        ) {
                            previews.prefetch(
                                &clip.path,
                                clip.modified,
                                ffmpeg,
                                size.x * ui.ctx().pixels_per_point(),
                            );
                        }
                        if let (Some(ffmpeg), true) =
                            (opts.preview, opts.want_thumb && thumb.hovered())
                        {
                            previewing = true;
                            let ctx = ui.ctx().clone();
                            match previews.hover(
                                &ctx,
                                &clip.path,
                                clip.modified,
                                ffmpeg,
                                size.x * ctx.pixels_per_point(),
                            ) {
                                crate::preview::Frame::Ready(tex, progress) => {
                                    egui::Image::new((tex.id(), size))
                                        .uv(uv)
                                        .corner_radius(CornerRadius::same(10))
                                        .paint_at(ui, r);
                                    let bar = egui::Rect::from_min_max(
                                        pos2(r.left() + 8.0, r.bottom() - 7.0),
                                        pos2(r.right() - 8.0, r.bottom() - 4.0),
                                    );
                                    ui.painter().rect_filled(
                                        bar,
                                        CornerRadius::same(2),
                                        Color32::from_black_alpha(120),
                                    );
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_max(
                                            bar.min,
                                            pos2(bar.left() + bar.width() * progress, bar.bottom()),
                                        ),
                                        CornerRadius::same(2),
                                        pal().accent,
                                    );
                                }
                                crate::preview::Frame::Loading => {
                                    let tm = ui.input(|i| i.time) as f32;
                                    for k in 0..3 {
                                        let a = ((tm * 3.0 - k as f32 * 0.6).sin() * 0.5 + 0.5)
                                            * 0.8
                                            + 0.2;
                                        ui.painter().circle_filled(
                                            pos2(
                                                r.center().x + (k as f32 - 1.0) * 10.0,
                                                r.bottom() - 14.0,
                                            ),
                                            3.0,
                                            pal().accent.gamma_multiply(a),
                                        );
                                    }
                                }
                                crate::preview::Frame::Failed(_) => {}
                            }
                        } else if thumb.hovered() && tex.is_some() {
                            // A soft play badge so it's clear the picture opens the clip.
                            ui.painter().circle_filled(
                                r.center(),
                                20.0,
                                Color32::from_black_alpha(140),
                            );
                            paint_icon(ui.painter(), Icon::Play, r.center(), 18.0, Color32::WHITE);
                        }
                        if thumb.clicked() {
                            action = Some(ClipAction::Play);
                        }

                        // ---- star, always visible on starred clips, on hover otherwise
                        let star_rect = egui::Rect::from_center_size(
                            pos2(r.left() + 20.0, r.top() + 20.0),
                            vec2(30.0, 30.0),
                        );
                        let star = ui.interact(star_rect, thumb.id.with("star"), Sense::click());
                        // Extra animation: starring pops the icon and sends out a ring.
                        let pop_id = egui::Id::unique(("clip-star-pop", &clip.path));
                        let now = ui.input(|i| i.time);
                        let pop = ui
                            .ctx()
                            .data(|d| d.get_temp::<f64>(pop_id))
                            .map(|t0| ((now - t0) / 0.45) as f32)
                            .filter(|a| motion && *a < 1.0);
                        if let Some(a) = pop {
                            let e = 1.0 - (1.0 - a).powi(3);
                            ui.painter().circle_stroke(
                                star_rect.center(),
                                12.0 + 18.0 * e,
                                Stroke::new(2.0 * (1.0 - a), pal().warn.gamma_multiply(1.0 - a)),
                            );
                            ui.ctx().request_repaint();
                        }
                        if favorite || thumb.hovered() || star.hovered() || pop.is_some() {
                            ui.painter().circle_filled(
                                star_rect.center(),
                                14.0,
                                Color32::from_black_alpha(if star.hovered() { 170 } else { 120 }),
                            );
                            paint_icon(
                                ui.painter(),
                                if favorite {
                                    Icon::StarFilled
                                } else {
                                    Icon::Star
                                },
                                star_rect.center(),
                                16.0 * pop
                                    .map(|a| 1.0 + 0.5 * (a * std::f32::consts::PI).sin())
                                    .unwrap_or(1.0),
                                if favorite { pal().warn } else { Color32::WHITE },
                            );
                        }
                        let star = star.on_hover_text(t(if favorite {
                            "Unstar"
                        } else {
                            "Star (never auto-deleted)"
                        }));
                        if star.clicked() {
                            action = Some(ClipAction::Favorite);
                            ui.ctx().data_mut(|d| d.insert_temp(pop_id, now));
                        }

                        // ---- name and details, each on their own full-width line
                        ui.add(egui::Label::new(bold(&clip.name).size(14.0)).truncate())
                            .on_hover_text(&clip.name);
                        ui.add(egui::Label::new(muted(meta)).truncate());

                        // ---- actions: the common ones as icons, the rest in a menu
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if icon_button(ui, Icon::Play, "Play", pal().accent).clicked() {
                                action = Some(ClipAction::Play);
                            }
                            if icon_button(
                                ui,
                                Icon::Copy,
                                "Copy the clip (paste into Discord, WhatsApp…)",
                                pal().muted,
                            )
                            .clicked()
                            {
                                action = Some(ClipAction::Share);
                            }
                            if icon_button(ui, Icon::Folder, "Show in folder", pal().muted)
                                .clicked()
                            {
                                action = Some(ClipAction::Reveal);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let tip = if opts.permanent {
                                    "Delete permanently"
                                } else {
                                    "Move to Recycle Bin"
                                };
                                if icon_button(ui, Icon::Trash, tip, pal().danger).clicked() {
                                    action = Some(ClipAction::Delete);
                                }
                                let more = icon_button(ui, Icon::More, "More", pal().muted);
                                egui::Popup::menu(&more).show(|ui| {
                                    ui.set_min_width(210.0);
                                    if ui.button(t("Rename")).clicked() {
                                        action = Some(ClipAction::Rename);
                                    }
                                    let trim =
                                        ui.add_enabled(opts.can_trim, egui::Button::new(t("Trim")));
                                    let trim = if opts.can_trim {
                                        trim
                                    } else {
                                        trim.on_disabled_hover_text(t(
                                            "Needs ffmpeg next to Rewind.exe or on PATH",
                                        ))
                                    };
                                    if trim.clicked() {
                                        action = Some(ClipAction::Trim);
                                    }
                                    let small = ui.add_enabled(
                                        opts.can_trim && !sharing,
                                        egui::Button::new(if sharing {
                                            t("Making a smaller copy…")
                                        } else {
                                            t("Copy for Discord (under 10 MB)")
                                        }),
                                    );
                                    if small.clicked() {
                                        action = Some(ClipAction::ShareSmall);
                                    }
                                    if ui
                                        .button(t(if favorite { "Unstar" } else { "Star" }))
                                        .clicked()
                                    {
                                        action = Some(ClipAction::Favorite);
                                    }
                                });
                            });
                        });
                    })
                    .response
            },
        )
        .inner;
    if reveal < 1.0 {
        ui.set_opacity(prev_opacity);
    }
    let hover = ui.ctx().animate_bool_with_time(
        resp.id.with("card-hover"),
        ui.rect_contains_pointer(resp.rect),
        anim_time(0.18),
    );
    // Extra animation: a clip saved moments ago glows, so it's easy to spot.
    if extra_motion() {
        let age = std::time::SystemTime::now()
            .duration_since(clip.modified)
            .map(|d| d.as_secs_f32())
            .unwrap_or(f32::MAX);
        if age < 4.0 {
            let fade = 1.0 - age / 4.0;
            let pulse = 0.55 + 0.45 * (age * 5.0).cos();
            ui.painter().rect_stroke(
                resp.rect.expand(1.0),
                CornerRadius::same(15),
                Stroke::new(2.0, pal().accent.gamma_multiply(fade * pulse)),
                egui::StrokeKind::Outside,
            );
            ui.ctx().request_repaint();
        }
    }
    if hover > 0.0 {
        ui.painter().rect_stroke(
            resp.rect,
            CornerRadius::same(14),
            Stroke::new(1.0, pal().stroke.lerp_to_gamma(pal().accent, 0.55 * hover)),
            egui::StrokeKind::Inside,
        );
    }
    (action, previewing)
}

// ---------------------------------------------------------------- settings page

const CAPTURE_OPTS: [(CaptureMode, &str); 2] = [
    (CaptureMode::Desktop, "Whole screen"),
    (CaptureMode::GameOnly, "Games only"),
];
const DISPLAY_OPTS: [(DisplayMethod, &str); 2] =
    [(DisplayMethod::Dxgi, "DXGI"), (DisplayMethod::Wgc, "WGC")];
const CONTAINER_OPTS: [(Container, &str); 2] = [(Container::Mp4, "MP4"), (Container::Mkv, "MKV")];

/// A dropdown of WASAPI endpoints with an explicit "System default" entry.
fn device_picker(ui: &mut egui::Ui, salt: &str, current: &mut String, devices: &[AudioDevice]) {
    let label = crate::audio::label_for(devices, current);
    egui::ComboBox::from_id_salt(salt)
        .width(250.0)
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(current, String::new(), t("System default"));
            for d in devices {
                ui.selectable_value(current, d.id.clone(), d.display());
            }
        });
}

/// A combo box with every celebration style that a downloaded pack has unlocked. Picking
/// one plays it immediately so the choice can be judged before Apply.
fn effect_picker(app: &mut RewindApp, ui: &mut egui::Ui) {
    let available = app.packs.available_effects();
    if available.is_empty() {
        ui.label(muted("Download effects in Add-ons first."));
        return;
    }
    let before = app.draft.effects;
    egui::ComboBox::from_id_salt("effect_style")
        .width(170.0)
        .selected_text(t(before.label()))
        .show_ui(ui, |ui| {
            for e in std::iter::once(EffectStyle::None).chain(available) {
                ui.selectable_value(&mut app.draft.effects, e, t(e.label()));
            }
        });
    if app.draft.effects != before {
        let at = ui.available_rect_before_wrap().right_center();
        effects::queue(app.draft.effects, at);
    }
}

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
    let current = if which == 0 {
        app.draft.save_hotkey.clone()
    } else {
        app.draft.toggle_hotkey.clone()
    };
    let label = if capturing {
        "Press the new keys…".to_string()
    } else {
        current.to_string()
    };
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
        if let Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = ev
        {
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
    page_header(
        ui,
        "Settings",
        "Changes apply when you press Apply. Video changes restart the buffer for a moment.",
    );

    section_title(ui, "PERFORMANCE");
    ui.columns(3, |c| {
        for (i, p) in Preset::ALL.iter().enumerate() {
            // A little nod to the friend whose laptop this preset was tuned on, Turkish UI only.
            let note = (*p == Preset::LowEnd && super::i18n::lang() == Lang::Turkish)
                .then_some("Demirhan'ın PC'si");
            if preset_card(&mut c[i], p.label(), p.blurb(), note) {
                app.draft.apply_preset(*p);
            }
        }
    });
    card(ui, |ui| {
        setting_row(
            ui,
            "Smooth animations",
            "Fades and slides in the interface. Turn off on slow PCs.",
            false,
            |ui| {
                toggle(ui, &mut app.draft.animations);
            },
        );
        setting_row(
            ui,
            "Extra animations",
            "Thumbnails zoom on hover, starring pops, a new clip glows for a moment. Needs smooth animations.",
            false,
            |ui| {
                ui.add_enabled_ui(app.draft.animations, |ui| {
                    toggle(ui, &mut app.draft.extra_motion);
                });
            },
        );
        setting_row(
            ui,
            "Smooth scrolling",
            "Glides the page between wheel notches instead of jumping.",
            false,
            |ui| {
                toggle(ui, &mut app.draft.smooth_scroll);
            },
        );
        setting_row(
            ui,
            "Low-power interface",
            "No pulsing, slower refresh, cheaper video scaling.",
            false,
            |ui| {
                toggle(ui, &mut app.draft.low_power_ui);
            },
        );
        let effects_ready = app.packs.effects_installed();
        setting_row(
            ui,
            "Toggle effects",
            if effects_ready {
                app.draft.effects.blurb()
            } else {
                "Download the toggle effects from Add-ons to turn these on."
            },
            false,
            |ui| {
                if effects_ready {
                    effect_picker(app, ui);
                    let at = ui.available_rect_before_wrap().right_center();
                    if text_button(ui, Some(Icon::Play), "Try it", true).clicked() {
                        effects::queue(app.draft.effects, at);
                    }
                } else if text_button(ui, None, "Open Add-ons", true).clicked() {
                    app.set_page(Page::Addons);
                }
            },
        );
        setting_row(
            ui,
            "Clip thumbnails",
            "Uses Windows' own thumbnail cache; MP4 works best.",
            true,
            |ui| {
                toggle(ui, &mut app.draft.thumbnails);
            },
        );
    });

    section_title(ui, "VIDEO");
    card(ui, |ui| {
        let d = &mut app.draft;
        setting_row(
            ui,
            "Capture",
            "Whole screen follows you into fullscreen games.",
            false,
            |ui| {
                segmented(ui, &mut d.capture_mode, &CAPTURE_OPTS);
            },
        );
        setting_row(
            ui,
            "Screen capture method",
            d.display_method.blurb(),
            false,
            |ui| {
                segmented(ui, &mut d.display_method, &DISPLAY_OPTS);
            },
        );
        setting_row(ui, "Monitor", "", false, |ui| {
            let current = app
                .monitors
                .get(d.monitor_index)
                .map(|m| m.label.clone())
                .unwrap_or_else(|| t("Primary").into());
            egui::ComboBox::from_id_salt("monitor")
                .width(280.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (i, m) in app.monitors.iter().enumerate() {
                        ui.selectable_value(&mut d.monitor_index, i, &m.label);
                    }
                });
        });
        setting_row(ui, "Resolution", "Never upscales.", false, |ui| {
            // A stack array: no heap allocation per frame.
            let items: [(Resolution, &str); Resolution::ALL.len()] = std::array::from_fn(|i| {
                (
                    Resolution::ALL[i],
                    if Resolution::ALL[i] == Resolution::Native {
                        "Native"
                    } else {
                        Resolution::ALL[i].label()
                    },
                )
            });
            segmented(ui, &mut d.resolution, &items);
        });
        setting_row(ui, "Frame rate", "", false, |ui| {
            let items: [(u32, &str); FPS_CHOICES.len()] =
                std::array::from_fn(|i| (FPS_CHOICES[i], FPS_LABELS[i]));
            segmented(ui, &mut d.fps, &items);
        });
        setting_row(
            ui,
            "Codec",
            "HEVC and AV1 give smaller files if your GPU supports them.",
            false,
            |ui| {
                let items: [(Codec, &str); Codec::ALL.len()] =
                    std::array::from_fn(|i| (Codec::ALL[i], Codec::ALL[i].short_label()));
                segmented(ui, &mut d.codec, &items);
            },
        );
        setting_row(ui, "Encoder quality", "", false, |ui| {
            let items: [(Quality, &str); Quality::ALL.len()] =
                std::array::from_fn(|i| (Quality::ALL[i], Quality::ALL[i].label()));
            segmented(ui, &mut d.quality, &items);
        });
        setting_row(
            ui,
            "Bitrate",
            "Higher looks better and makes bigger files.",
            false,
            |ui| {
                if text_button(ui, None, "Suggest", true).clicked() {
                    let (bw, bh) = app
                        .monitors
                        .get(d.monitor_index)
                        .map(|m| (m.width, m.height))
                        .unwrap_or((1920, 1080));
                    let (_, h) = output_size(bw, bh, d.resolution);
                    d.bitrate_kbps = estimate::suggested_bitrate_kbps(h, d.fps);
                }
                let mut mbps = d.bitrate_kbps as f32 / 1000.0;
                ui.add(
                    egui::Slider::new(&mut mbps, 2.0..=100.0)
                        .suffix(" Mbps")
                        .trailing_fill(true),
                );
                d.bitrate_kbps = (mbps * 1000.0) as u32;
            },
        );
        setting_row(ui, "Show mouse cursor", "", true, |ui| {
            toggle(ui, &mut d.capture_cursor);
        });
    });

    section_title(ui, "AUDIO");
    let outputs = app.output_devices.clone();
    let inputs = app.input_devices.clone();
    card(ui, |ui| {
        setting_row(
            ui,
            "Game & desktop sound",
            "Everything you hear from your speakers.",
            false,
            |ui| {
                toggle(ui, &mut app.draft.audio.desktop_enabled);
                let mut pct = app.draft.audio.desktop_volume * 100.0;
                ui.add_enabled(
                    app.draft.audio.desktop_enabled,
                    egui::Slider::new(&mut pct, 0.0..=200.0)
                        .suffix("%")
                        .trailing_fill(true),
                );
                app.draft.audio.desktop_volume = pct / 100.0;
            },
        );
        setting_row(
            ui,
            "Desktop audio device",
            "The speakers or headphones to record. 'System default' follows Windows.",
            false,
            |ui| {
                device_picker(
                    ui,
                    "desktop_device",
                    &mut app.draft.audio.desktop_device,
                    &outputs,
                );
            },
        );
        setting_row(
            ui,
            "Microphone",
            "Mixed into the same track as the game audio.",
            false,
            |ui| {
                toggle(ui, &mut app.draft.audio.mic_enabled);
                let mut pct = app.draft.audio.mic_volume * 100.0;
                ui.add_enabled(
                    app.draft.audio.mic_enabled,
                    egui::Slider::new(&mut pct, 0.0..=400.0)
                        .suffix("%")
                        .trailing_fill(true),
                );
                app.draft.audio.mic_volume = pct / 100.0;
            },
        );
        setting_row(ui, "Microphone device", "Pick the exact mic you speak into. A missing device is the usual reason your voice is silent.", false, |ui| {
            device_picker(ui, "mic_device", &mut app.draft.audio.mic_device, &inputs);
        });
        setting_row(ui, "Audio quality", "", true, |ui| {
            if text_button(ui, None, "Refresh devices", true).clicked() {
                app.refresh_audio_devices();
            }
            ui.add(
                egui::Slider::new(&mut app.draft.audio.bitrate_kbps, 96..=320)
                    .suffix(" kbps")
                    .trailing_fill(true),
            );
        });
    });

    section_title(ui, "REPLAY BUFFER");
    card(ui, |ui| {
        let need = estimate::buffer_ram_mb(
            app.draft.bitrate_kbps,
            app.draft.audio.bitrate_kbps,
            app.draft.buffer_secs,
        );
        let hint = if need > app.draft.buffer_max_mb {
            tf(
                "Needs ~{n} MB, over your limit, so it will keep less than this.",
                &[("n", &need.to_string())],
            )
        } else {
            tf("Uses about {n} MB of RAM.", &[("n", &need.to_string())])
        };
        setting_row(ui, "Replay length", &hint, false, |ui| {
            ui.add(
                egui::Slider::new(&mut app.draft.buffer_secs, 10..=600)
                    .suffix(" s")
                    .logarithmic(true)
                    .trailing_fill(true),
            );
        });
        setting_row(ui, "Memory limit", "", false, |ui| {
            ui.add(
                egui::Slider::new(&mut app.draft.buffer_max_mb, 256..=8192)
                    .suffix(" MB")
                    .logarithmic(true)
                    .trailing_fill(true),
            );
        });
        setting_row(ui, "File format", "", false, |ui| {
            segmented(ui, &mut app.draft.container, &CONTAINER_OPTS);
        });
        setting_row(
            ui,
            "Ask before deleting a clip",
            if app.draft.delete_permanently {
                "Turn off to delete clips with one click."
            } else {
                "Turn off to send clips straight to the Recycle Bin with one click."
            },
            false,
            |ui| {
                toggle(ui, &mut app.draft.confirm_delete);
            },
        );
        setting_row(
            ui,
            "Delete permanently",
            "Skip the Recycle Bin. Deleted clips can't be recovered.",
            false,
            |ui| {
                toggle(ui, &mut app.draft.delete_permanently);
            },
        );
        setting_row(
            ui,
            "Folder per game",
            "Saves each clip into a folder named after the game you were playing.",
            false,
            |ui| {
                toggle(ui, &mut app.draft.per_game_folders);
            },
        );
        setting_row(
            ui,
            "Storage limit",
            "When the clips folder is bigger than this, the oldest clips are deleted for good. Starred clips are never deleted.",
            true,
            |ui| {
                let off = t("Off").to_string();
                ui.add(
                    egui::Slider::new(&mut app.draft.storage_limit_gb, 0..=500)
                        .custom_formatter(move |v, _| {
                            if v < 0.5 {
                                off.clone()
                            } else {
                                format!("{v:.0} GB")
                            }
                        })
                        .trailing_fill(true),
                );
            },
        );
        setting_row(ui, "Save folder", "", true, |ui| {
            if icon_button(ui, Icon::Folder, "Open folder", pal().muted).clicked() {
                let _ = std::fs::create_dir_all(&app.draft.output_dir);
                let _ = opener::open(&app.draft.output_dir);
            }
            let mut path = app.draft.output_dir.display().to_string();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut path)
                        .desired_width(330.0)
                        .margin(Margin::symmetric(10, 7)),
                )
                .changed()
            {
                app.draft.output_dir = path.into();
            }
        });
    });

    section_title(ui, "NOTIFICATIONS");
    card(ui, |ui| {
        let d = &mut app.draft;
        setting_row(
            ui,
            "Show notifications over games",
            "Hidden from your recordings. Needs borderless or windowed games.",
            false,
            |ui| {
                toggle(ui, &mut d.overlay_enabled);
            },
        );
        setting_row(ui, "Position", "", false, |ui| {
            let items: [(Corner, &str); Corner::ALL.len()] =
                std::array::from_fn(|i| (Corner::ALL[i], Corner::ALL[i].label()));
            segmented(ui, &mut d.overlay_corner, &items);
        });
        setting_row(ui, "Duration", "", false, |ui| {
            ui.add(
                egui::Slider::new(&mut d.overlay_secs, 1..=10)
                    .suffix(" s")
                    .trailing_fill(true),
            );
        });
        let mut preview = false;
        setting_row(
            ui,
            "Play a sound when a clip is saved",
            "Also works in exclusive-fullscreen games, where the overlay can't draw.",
            false,
            |ui| {
                if text_button(ui, None, "Preview", true).clicked() {
                    preview = true;
                }
                toggle(ui, &mut d.sound_on_save);
            },
        );
        let sounds = crate::sounds::list(&super::sounds_dir());
        setting_row(
            ui,
            "Sound",
            "Add your own .mp3 or .wav by dragging it onto Rewind (see Add-ons).",
            true,
            |ui| {
                let current = if d.clip_sound.is_empty() || !sounds.contains(&d.clip_sound) {
                    t("Rewind chime").to_string()
                } else {
                    d.clip_sound.clone()
                };
                egui::ComboBox::from_id_salt("clip-sound")
                    .selected_text(current)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.clip_sound, String::new(), t("Rewind chime"));
                        for name in &sounds {
                            ui.selectable_value(&mut d.clip_sound, name.clone(), name.as_str());
                        }
                    });
            },
        );
        if preview {
            app.preview_overlay();
            if app.draft.sound_on_save {
                crate::sounds::play_clip_saved(&super::sounds_dir(), &app.draft.clip_sound);
            }
        }
    });

    section_title(ui, "HOTKEYS");
    card(ui, |ui| {
        setting_row(
            ui,
            "Save replay",
            "Click, then press the keys you want.",
            false,
            |ui| hotkey_field(app, ui, 0),
        );
        setting_row(ui, "Turn replay on/off", "", true, |ui| {
            hotkey_field(app, ui, 1)
        });
    });

    section_title(ui, "GENERAL");
    card(ui, |ui| {
        let d = &mut app.draft;
        setting_row(
            ui,
            "Language",
            "Interface language. English is the default.",
            false,
            |ui| {
                egui::ComboBox::from_id_salt("language")
                    .width(170.0)
                    .selected_text(t(d.language.label()))
                    .show_ui(ui, |ui| {
                        for l in Lang::ALL {
                            ui.selectable_value(&mut d.language, l, t(l.label()));
                        }
                    });
            },
        );
        setting_row(
            ui,
            "Start Instant Replay when Rewind opens",
            "",
            false,
            |ui| {
                toggle(ui, &mut d.start_buffer_on_launch);
            },
        );
        setting_row(
            ui,
            "Closing the window keeps Rewind in the tray",
            "Recording and hotkeys keep working.",
            false,
            |ui| {
                toggle(ui, &mut d.close_to_tray);
            },
        );
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

fn about_page(app: &mut RewindApp, ui: &mut egui::Ui) {
    page_header(
        ui,
        "About Rewind",
        "A small instant-replay recorder written in Rust.",
    );
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(bold(tf("Version {v}", &[("v", env!("CARGO_PKG_VERSION"))])));
            ui.label(muted("· made by pro82xw"));
        });
        ui.add_space(4.0);
        ui.label(muted("Capture, encoding and the replay buffer are provided by libobs from OBS Studio (GPL-3.0-or-later), driven through the libobs-rs bindings. Rewind is licensed GPL-3.0-or-later and isn't affiliated with NVIDIA or the OBS Project."));
        ui.add_space(4.0);
        ui.label(muted("Clips never leave your PC. Network access is limited to the one-time OBS runtime download, optional add-ons, and the update check."));
        ui.add_space(4.0);
        ui.label(muted("Typeface: Inter, SIL Open Font License 1.1."));
    });

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(bold("Something not working?"));
                ui.label(muted("Copies version, settings, encoder, voice status and recent errors (no clips, no personal files) so you can paste them into a bug report."));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if text_button(ui, Some(Icon::Copy), "Copy diagnostics", true).clicked() {
                    let text = app.diagnostics();
                    app.copy_to_clipboard(text);
                }
            });
        });
    });

    section_title(ui, "UPDATES");
    card(ui, |ui| {
        ui.horizontal(|ui| {
            let busy = matches!(
                app.update_state,
                UpdateState::Checking | UpdateState::Installing(..) | UpdateState::Restarting(_)
            );
            if text_button(
                ui,
                None,
                if busy {
                    "Checking…"
                } else {
                    "Check for updates"
                },
                !busy,
            )
            .clicked()
            {
                app.check_updates();
            }
            let mut install: Option<crate::updates::UpdateInfo> = None;
            let mut copy: Option<String> = None;
            match &app.update_state {
                UpdateState::Idle => {
                    ui.label(muted("Not checked yet."));
                }
                UpdateState::Checking => {
                    ui.label(muted("Asking GitHub…"));
                }
                UpdateState::UpToDate => {
                    ui.label(RichText::new(t("You're on the latest version.")).color(pal().accent));
                }
                UpdateState::Available(info) => {
                    ui.label(
                        RichText::new(tf("Version {v} is available.", &[("v", &info.version)]))
                            .color(pal().warn),
                    );
                    if crate::updates::can_install(info)
                        && accent_pill(ui, "Update now", true).clicked()
                    {
                        install = Some(info.clone());
                    }
                    let url = info.url.clone();
                    if text_button(ui, None, "Open release page", true).clicked() {
                        let _ = opener::open(&url);
                    }
                }
                UpdateState::Installing(info, progress) => {
                    let pct = (progress.fraction() * 100.0).round() as u32;
                    ui.label(
                        RichText::new(tf(
                            "Downloading version {v}… {p}%",
                            &[("v", &info.version), ("p", &pct.to_string())],
                        ))
                        .color(pal().accent),
                    );
                    ui.add(
                        egui::ProgressBar::new(progress.fraction())
                            .desired_width(160.0)
                            .fill(pal().accent),
                    );
                }
                UpdateState::Restarting(v) => {
                    ui.label(
                        RichText::new(tf("Restarting into version {v}…", &[("v", v)]))
                            .color(pal().accent),
                    );
                }
                UpdateState::InstallFailed(info, e) => {
                    let msg = tf("Couldn't update: {e}", &[("e", e)]);
                    ui.label(RichText::new(&msg).color(pal().danger));
                    if icon_button(ui, Icon::Copy, "Copy", pal().danger).clicked() {
                        copy = Some(msg);
                    }
                    if text_button(ui, None, "Try again", true).clicked() {
                        install = Some(info.clone());
                    }
                    let url = info.url.clone();
                    if text_button(ui, None, "Open release page", true).clicked() {
                        let _ = opener::open(&url);
                    }
                }
                UpdateState::Failed(e) => {
                    let msg = tf("Couldn't check: {e}", &[("e", e)]);
                    ui.label(RichText::new(&msg).color(pal().danger));
                    if icon_button(ui, Icon::Copy, "Copy", pal().danger).clicked() {
                        copy = Some(msg);
                    }
                }
            };
            if let Some(text) = copy.take() {
                app.copy_to_clipboard(text);
            }
            if let Some(info) = install {
                app.install_update(info);
            }
        });
        let notes_for = match &app.update_state {
            UpdateState::Available(info) | UpdateState::InstallFailed(info, _) => Some(info),
            _ => None,
        };
        if let Some(info) = notes_for {
            let notes = info.notes.trim();
            if !notes.is_empty() {
                ui.add_space(6.0);
                ui.label(muted("What's new:"));
                let mut shown: String = notes.lines().take(12).collect::<Vec<_>>().join("\n");
                if notes.lines().count() > 12 {
                    shown.push_str("\n…");
                }
                ui.label(RichText::new(shown).size(12.5).color(pal().muted));
            }
        }
        ui.add_space(6.0);
        setting_row(
            ui,
            "Check automatically at launch",
            "One small request to GitHub. No account or personal data is sent.",
            false,
            |ui| {
                toggle(ui, &mut app.draft.check_updates);
            },
        );
        setting_row(
            ui,
            "Install updates automatically",
            "Downloads the new version, checks it and restarts Rewind. Off: just tell me and link to the release page.",
            true,
            |ui| {
                toggle(ui, &mut app.draft.auto_install_updates);
            },
        );
    });
}

// ---------------------------------------------------------------- dialogs

fn trim_window(app: &mut RewindApp, ctx: &egui::Context) {
    let Some(state) = &mut app.trim else { return };
    let mut open = true;
    let mut run = false;
    let mut cancel = false;
    egui::Window::new(t("Trim clip"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(bold(&state.clip.name));
            ui.label(muted(
                "Lossless cut with no re-encoding, so the start snaps to the nearest keyframe.",
            ));
            ui.add_space(4.0);
            ui.radio_value(
                &mut state.mode,
                TrimMode::LastSeconds,
                t("Keep only the last…"),
            );
            ui.add_enabled_ui(state.mode == TrimMode::LastSeconds, |ui| {
                ui.add(
                    egui::DragValue::new(&mut state.last_secs)
                        .range(1.0..=3600.0)
                        .suffix(" s"),
                );
            });
            ui.radio_value(&mut state.mode, TrimMode::Range, t("Keep from … to …"));
            ui.add_enabled_ui(state.mode == TrimMode::Range, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut state.start)
                            .range(0.0..=36000.0)
                            .suffix(" s"),
                    );
                    ui.label(t("to"));
                    ui.add(
                        egui::DragValue::new(&mut state.end)
                            .range(0.0..=36000.0)
                            .suffix(" s"),
                    );
                });
            });
            ui.label(muted("A new file is saved next to the original."));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if state.running {
                    ui.add(egui::Spinner::new());
                    ui.label(t("Trimming…"));
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

fn rename_window(app: &mut RewindApp, ctx: &egui::Context) {
    let Some(state) = app.rename.as_ref() else {
        return;
    };
    let clip = state.clip.clone();
    let mut stem = state.stem.clone();
    let mut error = state.error.clone();
    let mut focus = state.focus;
    let mut open = true;
    let mut run = false;
    let mut cancel = false;

    egui::Window::new(t("Rename clip"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(bold(&clip.name));
            let ext = clip
                .path
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut stem)
                        .desired_width(220.0)
                        .margin(Margin::symmetric(10, 7))
                        .hint_text(t("New name")),
                );
                if focus {
                    resp.request_focus();
                    focus = false;
                }
                if !ext.is_empty() {
                    ui.label(muted(format!(".{ext}")));
                }
            });
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                run = true;
            }
            if let Some(e) = &error {
                ui.label(RichText::new(t(e)).color(pal().danger).size(13.0));
            }
            ui.label(muted("The file keeps its folder and format."));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if accent_pill(ui, "Rename", true).clicked() {
                    run = true;
                }
                if text_button(ui, None, "Cancel", true).clicked() {
                    cancel = true;
                }
            });
        });

    if run {
        match rename_clip(&clip, &stem) {
            Ok(new_path) => {
                app.favorite_moved(&clip.path, Some(new_path.as_path()));
                if app.last_saved.as_ref() == Some(&clip.path) {
                    app.last_saved = Some(new_path);
                }
                app.notify(t("Clip renamed"), false);
                app.refresh_clips(true);
                app.rename = None;
            }
            Err(e) => {
                error = Some(e);
            }
        }
    }

    if cancel || !open {
        app.rename = None;
    } else if let Some(state) = &mut app.rename {
        state.stem = stem;
        state.error = error;
        state.focus = focus;
    }
}

/// Rename a clip in place. The extension is kept, Windows-hostile characters are cleaned,
/// and an existing name gets a ` (2)` suffix instead of overwriting another clip.
fn rename_clip(clip: &Clip, input: &str) -> Result<std::path::PathBuf, String> {
    let ext = clip
        .path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mp4".into());
    let mut stem = input.trim().to_string();
    // Typing the extension yourself shouldn't produce "name.mp4.mp4".
    let suffix = format!(".{ext}");
    if stem.to_lowercase().ends_with(&suffix.to_lowercase()) {
        stem.truncate(stem.len() - suffix.len());
    }
    if stem.trim().is_empty() {
        return Err("Give the clip a name first.".into());
    }
    let stem = sanitize_file_name(stem.trim());
    if stem.is_empty() {
        return Err("That name can't be used. Try letters and numbers.".into());
    }
    let Some(dir) = clip.path.parent() else {
        return Err("The clip's folder is missing.".into());
    };
    let candidate = dir.join(format!("{stem}.{ext}"));
    if candidate == clip.path {
        return Ok(clip.path.clone());
    }
    // Windows treats names that differ only by case as the same file, so a case-only
    // rename should rename in place rather than create "name (2)".
    let same_file = candidate.exists()
        && candidate
            .file_name()
            .map(|n| n.to_string_lossy().eq_ignore_ascii_case(&clip.name))
            .unwrap_or(false);
    let target = if same_file {
        candidate
    } else if candidate.exists() {
        unique_path(dir, &stem, &ext)
    } else {
        candidate
    };
    fs::rename(&clip.path, &target).map_err(|e| format!("Couldn't rename: {e}"))?;
    Ok(target)
}

fn delete_window(app: &mut RewindApp, ctx: &egui::Context) {
    let Some(clip) = app.confirm_delete.clone() else {
        return;
    };
    let permanent = app.cfg.delete_permanently;
    let mut keep_open = true;
    let mut keep_it = false;
    let mut delete = false;
    egui::Window::new(t("Delete clip?"))
        .collapsible(false)
        .resizable(false)
        .open(&mut keep_open)
        .show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(bold(&clip.name));
            if permanent {
                ui.label(
                    RichText::new(t("It will be deleted permanently. This can't be undone."))
                        .color(pal().danger)
                        .size(13.0),
                );
            } else {
                ui.label(muted("It will be moved to the Recycle Bin."));
            }
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
        delete_clip_now(app, &clip);
        app.confirm_delete = None;
    } else if !keep_open || keep_it {
        app.confirm_delete = None;
    }
}

fn toast(app: &mut RewindApp, ctx: &egui::Context) {
    let mut copy: Option<String> = None;
    toast_inner(app, ctx, &mut copy);
    if let Some(text) = copy {
        app.copy_to_clipboard(text);
    }
}

fn toast_inner(app: &RewindApp, ctx: &egui::Context, copy: &mut Option<String>) {
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
                .fill(if t.is_error {
                    pal().danger_bg
                } else {
                    pal().accent_bg
                })
                .stroke(Stroke::new(
                    1.0,
                    if t.is_error {
                        pal().danger
                    } else {
                        pal().accent_dim
                    },
                ))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_max_width(460.0);
                    ui.horizontal(|ui| {
                        let col = if t.is_error {
                            pal().danger
                        } else {
                            pal().accent
                        };
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                        ui.painter()
                            .circle_filled(r.center(), 10.0, col.gamma_multiply(0.18));
                        paint_icon(
                            ui.painter(),
                            if t.is_error {
                                Icon::Warning
                            } else {
                                Icon::Check
                            },
                            r.center(),
                            12.0,
                            col,
                        );
                        ui.label(RichText::new(&t.text).color(col));
                        // Errors can be copied straight from the toast for a bug report.
                        if t.is_error && icon_button(ui, Icon::Copy, "Copy", col).clicked() {
                            *copy = Some(t.text.clone());
                        }
                    });
                });
        });
}

/// "Last heard" line in the voice card.
fn card_row_heard(ui: &mut egui::Ui, heard: &str, why: &str, color: Color32) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(muted("Last heard:"));
        ui.label(bold(heard).size(13.5));
        ui.label(RichText::new(why).color(color).size(13.0));
    });
}

fn preset_card(ui: &mut egui::Ui, title: &str, blurb: &str, note: Option<&str>) -> bool {
    let size = vec2(ui.available_width(), 82.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let anim = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), anim_time(0.16));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(14),
        pal().card.lerp_to_gamma(pal().card_hi, anim),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(14),
        Stroke::new(1.0, pal().stroke.lerp_to_gamma(pal().accent, 0.6 * anim)),
        egui::StrokeKind::Inside,
    );
    let title_rect = ui.painter().text(
        pos2(rect.left() + 16.0, rect.top() + 18.0),
        egui::Align2::LEFT_CENTER,
        t(title),
        egui::FontId::new(15.5, semibold_family()),
        pal().text,
    );
    if let Some(note) = note {
        ui.painter().text(
            pos2(title_rect.right() + 6.0, rect.top() + 18.5),
            egui::Align2::LEFT_CENTER,
            note,
            egui::FontId::proportional(11.0),
            pal().faint,
        );
    }
    let galley = ui.painter().layout(
        t(blurb).to_string(),
        egui::FontId::proportional(12.5),
        pal().muted,
        rect.width() - 32.0,
    );
    ui.painter().galley(
        pos2(rect.left() + 16.0, rect.top() + 36.0),
        galley,
        pal().muted,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn swatch(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 12.0, color);
    ui.painter()
        .circle_stroke(r.center(), 12.0, Stroke::new(1.0, pal().stroke));
}

fn status_text(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(RichText::new(t(text)).color(color).size(13.0));
}

/// Download / progress / remove control shared by every add-on row.
fn pack_controls(
    app: &mut RewindApp,
    ui: &mut egui::Ui,
    key: &str,
    installed: bool,
    can_download: bool,
    on_download: impl FnOnce(&mut RewindApp),
    on_remove: impl FnOnce(&mut RewindApp),
) {
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
    } else if ui
        .add_enabled_ui(can_download, |ui| {
            text_button(ui, Some(Icon::Download), "Download", can_download)
        })
        .inner
        .clicked()
        && can_download
    {
        on_download(app);
    }
}

fn addons_page(app: &mut RewindApp, ui: &mut egui::Ui) {
    page_header(
        ui,
        "Add-ons",
        "Optional extras. Nothing here is installed by default, so Rewind stays small.",
    );

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
                        ui.label(
                            RichText::new(tf("Couldn't load the add-on list: {e}", &[("e", &e)]))
                                .color(pal().warn),
                        );
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
        setting_row(
            ui,
            "Speech engine",
            "About 15 MB. Recognition runs on this PC; no audio is sent anywhere.",
            false,
            |ui| {
                let can = index
                    .as_ref()
                    .and_then(|i| i.voice_runtime.as_ref())
                    .is_some();
                pack_controls(
                    app,
                    ui,
                    crate::packs::KEY_RUNTIME,
                    runtime_ready,
                    can,
                    |a| a.packs.install_runtime(),
                    |a| {
                        a.packs.remove_runtime();
                        a.sync_voice();
                    },
                );
            },
        );
        let lang = app.draft.voice.language.clone();
        let voices = index.as_ref().map(|i| i.voice.clone()).unwrap_or_default();
        setting_row(
            ui,
            "Language",
            "A small language model, about 40 MB each.",
            false,
            |ui| {
                let model_ready = app.packs.model_installed(&lang);
                let pack = voices.iter().find(|v| v.id == lang).cloned();
                let key = format!("voice-{lang}");
                let p2 = pack.clone();
                pack_controls(
                    app,
                    ui,
                    &key,
                    model_ready,
                    pack.is_some(),
                    move |a| {
                        if let Some(p) = p2 {
                            a.packs.install_voice(&p);
                        }
                    },
                    |a| {
                        a.packs.remove_voice(&lang);
                        a.sync_voice();
                    },
                );
                let current = voices
                    .iter()
                    .find(|v| v.id == app.draft.voice.language)
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| app.draft.voice.language.clone());
                let lang_before = app.draft.voice.language.clone();
                egui::ComboBox::from_id_salt("voice-lang")
                    .width(190.0)
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for v in &voices {
                            ui.selectable_value(
                                &mut app.draft.voice.language,
                                v.id.clone(),
                                &v.name,
                            );
                        }
                    });
                // Follow the language's default phrase unless the user typed their own. This is
                // what makes switching to Turkish actually listen for "klip al".
                if app.draft.voice.language != lang_before {
                    let old_default = default_phrase(Lang::from_id(&lang_before));
                    let new_default = default_phrase(Lang::from_id(&app.draft.voice.language));
                    let phrase = app.draft.voice.phrase.trim();
                    if phrase.is_empty() || phrase == old_default {
                        app.draft.voice.phrase = new_default.to_string();
                    }
                }
            },
        );
        let ready = runtime_ready && app.packs.model_installed(&app.draft.voice.language);
        // Optional Whisper check: much more accurate (especially Turkish), heavier, so it only
        // runs for sentences Vosk thinks might be the phrase.
        let whisper_ready = app.packs.whisper_installed();
        let whisper_size = index
            .as_ref()
            .and_then(|i| i.whisper.as_ref())
            .map(|f| human_size(f.size_bytes))
            .unwrap_or_default();
        let whisper_can = index.as_ref().and_then(|i| i.whisper.as_ref()).is_some();
        let whisper_hint = if whisper_ready || whisper_can {
            tf(
                "Optional. Double-checks anything that sounds like the phrase with Whisper: far fewer mistakes, especially in Turkish. Uses more CPU for a moment after you speak. {size}",
                &[("size", &whisper_size)],
            )
        } else {
            t("Not available from this release.").to_string()
        };
        setting_row(ui, "Whisper check", &whisper_hint, false, |ui| {
            if whisper_ready {
                toggle(ui, &mut app.draft.voice.use_whisper);
            }
            pack_controls(
                app,
                ui,
                crate::packs::KEY_WHISPER,
                whisper_ready,
                whisper_can,
                |a| a.packs.install_whisper(),
                |a| {
                    a.packs.remove_whisper();
                    a.invalidate_voice_files();
                    a.sync_voice();
                },
            );
        });
        setting_row(
            ui,
            "Save a clip when I say the phrase",
            if ready {
                ""
            } else {
                "Download the engine and a language first."
            },
            false,
            |ui| {
                ui.add_enabled_ui(ready, |ui| {
                    toggle(ui, &mut app.draft.voice.enabled);
                });
            },
        );
        // Which microphone voice clipping hears. Normally the same one the recording uses.
        let listening_to = app
            .voice_handle()
            .map(|v| v.shared.mic_name())
            .filter(|n| !n.is_empty());
        let mic_hint = match &listening_to {
            Some(name) => tf("Listening to: {mic}", &[("mic", name)]),
            None => t("Usually the same one the recording uses.").to_string(),
        };
        setting_row(ui, "Microphone", &mic_hint, false, |ui| {
            let devices = app.input_devices.clone();
            let label = if app.draft.voice.mic.is_empty() {
                t("Same as recording").to_string()
            } else {
                crate::audio::label_for(&devices, &app.draft.voice.mic)
            };
            egui::ComboBox::from_id_salt("voice_mic")
                .width(250.0)
                .selected_text(label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.draft.voice.mic,
                        String::new(),
                        t("Same as recording"),
                    );
                    for d in &devices {
                        ui.selectable_value(&mut app.draft.voice.mic, d.id.clone(), d.display());
                    }
                });
        });
        setting_row(
            ui,
            "Trigger phrase",
            "Type it the way you say it. Rewind matches by sound, so words the model doesn't know still work. Separate several with commas: \"klip al, klibi al\".",
            false,
            |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.draft.voice.phrase)
                            .desired_width(240.0)
                            .margin(Margin::symmetric(10, 7)),
                    );
                    let default =
                        default_phrase(Lang::from_id(&app.draft.voice.language)).to_string();
                    if app.draft.voice.phrase.trim() != default
                        && text_button(ui, None, "Use default", true).clicked()
                    {
                        app.draft.voice.phrase = default;
                    }
                });
            },
        );
        setting_row(
            ui,
            "Sensitivity",
            "Higher picks up quieter speech but wakes more often.",
            false,
            |ui| {
                ui.add(
                    egui::Slider::new(&mut app.draft.voice.sensitivity, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                );
            },
        );
        setting_row(
            ui,
            "Strictness",
            "Relaxed misses less and allows a word or two around the phrase; Strict saves fewer clips by mistake.",
            false,
            |ui| {
                let items: [(rewind_core::config::Strictness, &str); 3] = std::array::from_fn(|i| {
                    let s = rewind_core::config::Strictness::ALL[i];
                    (s, s.label())
                });
                segmented(ui, &mut app.draft.voice.strictness, &items);
            },
        );
        // The live meter and "last heard" only run while a test is open: 20 seconds, or until
        // it's closed. The rest of the time the page doesn't redraw for the voice at all.
        let testing = app
            .voice_test_until
            .map(|t| std::time::Instant::now() < t)
            .unwrap_or(false);
        if !testing {
            app.voice_test_until = None;
        }
        let can_test = app.voice_handle().is_some();
        let mut toggle_test = false;
        setting_row(ui, "Status", "", !testing, |ui| {
            if can_test
                && text_button(ui, None, if testing { "Stop test" } else { "Test" }, true).clicked()
            {
                toggle_test = true;
            }
            match app
                .voice_handle()
                .map(|v| (v.shared.status(), v.shared.level()))
            {
                Some((VoiceStatus::Listening, level)) => {
                    let silent = app
                        .voice_handle()
                        .map(|v| v.shared.mic_silent())
                        .unwrap_or(false);
                    if testing {
                        progress_bar(ui, (level * 6.0).clamp(0.0, 1.0), 120.0);
                    }
                    if silent {
                        status_text(ui, "No sound from the microphone", pal().warn);
                    } else {
                        status_text(ui, "Listening", pal().accent);
                    }
                }
                Some((VoiceStatus::Starting, _)) => status_text(ui, "Starting…", pal().muted),
                Some((VoiceStatus::Phrase(w), _)) => {
                    let quoted = w
                        .missing
                        .iter()
                        .map(|word| format!("\"{word}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let message = if super::i18n::lang() == Lang::Turkish {
                        format!(
                            "{} modeli şu kelimeleri bilmiyor: {}. \"{}\" dene.",
                            w.lang.label(),
                            quoted,
                            w.suggestion
                        )
                    } else {
                        format!(
                            "The {} model doesn't know {}. Try \"{}\".",
                            w.lang.label(),
                            quoted,
                            w.suggestion
                        )
                    };
                    status_text(ui, &message, pal().danger)
                }
                Some((VoiceStatus::Error(e), _)) => {
                    status_text(ui, &e, pal().danger);
                    if icon_button(ui, Icon::Copy, "Copy", pal().danger).clicked() {
                        ui.ctx().copy_text(e.clone());
                    }
                }
                Some((VoiceStatus::Stopped, _)) | None => status_text(
                    ui,
                    if app.cfg.voice.enabled {
                        "Not running"
                    } else {
                        "Off"
                    },
                    pal().muted,
                ),
            }
        });
        if toggle_test {
            app.voice_test_until = if testing {
                None
            } else {
                // Start clean so an old result isn't mistaken for this test.
                if let Some(v) = app.voice_handle() {
                    v.shared.clear_heard();
                }
                Some(std::time::Instant::now() + std::time::Duration::from_secs(20))
            };
        }
        // Voice test: what the recogniser made of the last thing you said, and why it did
        // or didn't save a clip.
        if testing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
            let left = app
                .voice_test_until
                .map(|t| {
                    t.saturating_duration_since(std::time::Instant::now())
                        .as_secs()
                        + 1
                })
                .unwrap_or(0);
            match app.voice_handle().and_then(|v| v.shared.last_heard()) {
                None => {
                    ui.add_space(4.0);
                    ui.label(muted(tf(
                        "Say \"{phrase}\" to test it… (closes in {s} s)",
                        &[("phrase", &app.cfg.voice.phrase), ("s", &left.to_string())],
                    )));
                }
                Some(_) => {
                    ui.label(muted(tf(
                        "Test closes in {s} s",
                        &[("s", &left.to_string())],
                    )));
                }
            }
        }
        if let Some(h) = testing
            .then(|| app.voice_handle().and_then(|v| v.shared.last_heard()))
            .flatten()
        {
            let (why, color) = match h.verdict {
                Verdict::Match => ("Saved a clip", pal().accent),
                Verdict::NotAlone => ("Ignored: say it on its own", pal().warn),
                Verdict::Unsure => ("Ignored: not sure enough, say it clearer", pal().warn),
                Verdict::Cooldown => ("Ignored: a clip was just saved", pal().muted),
                Verdict::Different | Verdict::Nothing => ("Not the phrase", pal().muted),
            };
            let conf = if h.by_whisper {
                " · Whisper".to_string()
            } else {
                h.conf
                    .map(|c| format!(" · {:.0}%", c * 100.0))
                    .unwrap_or_default()
            };
            card_row_heard(ui, &format!("\"{}\"{conf}", h.text), t(why), color);
        }
    });
    if app
        .voice_handle()
        .map(|v| v.shared.mic_silent())
        .unwrap_or(false)
    {
        ui.label(
            egui::RichText::new(t(
                "This microphone sends only silence: it may be muted, or it's not the one you talk into. Pick another one above.",
            ))
            .color(pal().warn)
            .size(12.5),
        );
    }
    ui.label(muted("Tip: say the phrase on its own, with a short pause before and after; it doesn't count inside a longer sentence. Press Apply after changing the phrase or language."));

    // ---- Custom sounds
    section_title(ui, "CUSTOM SOUNDS");
    card(ui, |ui| {
        let dir = super::sounds_dir();
        let sounds = crate::sounds::list(&dir);
        let limit = app.cfg.limit_custom_sounds;
        let count = if limit {
            tf(
                "{n} of {max} · drag an .mp3 or .wav onto Rewind to add one",
                &[
                    ("n", &sounds.len().to_string()),
                    ("max", &crate::sounds::LIMIT.to_string()),
                ],
            )
        } else {
            tf(
                "{n} sounds · drag an .mp3 or .wav onto Rewind to add one",
                &[("n", &sounds.len().to_string())],
            )
        };
        setting_row(ui, "Clip-saved sounds", &count, false, |ui| {
            if icon_button(ui, Icon::Folder, "Open sounds folder", pal().muted).clicked() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = opener::open(&dir);
            }
        });
        let mut remove: Option<String> = None;
        for name in &sounds {
            let active = app.cfg.clip_sound == *name;
            setting_row(
                ui,
                name,
                if active {
                    "Plays when a clip is saved"
                } else {
                    ""
                },
                false,
                |ui| {
                    if icon_button(ui, Icon::Trash, "Remove", pal().danger).clicked() {
                        remove = Some(name.clone());
                    }
                    if icon_button(ui, Icon::Play, "Play", pal().accent).clicked() {
                        crate::sounds::play_clip_saved(&dir, name);
                    }
                    if !active && text_button(ui, None, "Use", true).clicked() {
                        app.draft.clip_sound = name.clone();
                    }
                },
            );
        }
        if let Some(name) = remove {
            app.remove_sound(&name);
        }
        setting_row(
            ui,
            "Limit to 5 sounds",
            "Turn off to keep as many as you like.",
            true,
            |ui| {
                toggle(ui, &mut app.draft.limit_custom_sounds);
            },
        );
    });

    section_title(ui, "HOVER PREVIEWS");
    card(ui, |ui| {
        let installed = app.packs.preview_ffmpeg().is_some();
        let downloadable = index.as_ref().and_then(|i| i.preview.as_ref()).is_some();
        let size = index
            .as_ref()
            .and_then(|i| i.preview.as_ref())
            .map(|f| human_size(f.size_bytes))
            .unwrap_or_default();
        let hint = if installed {
            "Hover a clip's thumbnail to play its last 20 seconds. Also enables trimming."
                .to_string()
        } else if downloadable {
            tf(
                "Plays the last 20 seconds of a clip when you hover its thumbnail, and enables trimming. Uses ffmpeg ({size}).",
                &[("size", &size)],
            )
        } else {
            "Not available from this release.".to_string()
        };
        setting_row(ui, "Clip previews", &hint, !installed, |ui| {
            pack_controls(
                app,
                ui,
                crate::packs::KEY_PREVIEW,
                installed,
                downloadable,
                |a| a.packs.install_preview(),
                |a| {
                    a.packs.remove_preview();
                    a.refresh_preview_exe();
                },
            );
        });
        if installed {
            setting_row(
                ui,
                "Play previews on hover",
                "Frames are made once per clip, at low priority, and cached.",
                true,
                |ui| {
                    toggle(ui, &mut app.draft.hover_preview);
                },
            );
        }
    });

    section_title(ui, "TOGGLE EFFECTS");
    card(ui, |ui| {
        let installed = app.packs.effects.clone();
        let available: Vec<_> = index
            .as_ref()
            .map(|i| {
                i.effects
                    .iter()
                    .filter(|e| !installed.iter().any(|x| x.id == e.id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if installed.is_empty() && available.is_empty() {
            ui.label(muted("No effect packs are available right now."));
        }
        for (i, p) in installed.iter().enumerate() {
            let hint = if p.author.is_empty() {
                tf(
                    "{n} effects · Installed",
                    &[("n", &p.styles.len().to_string())],
                )
            } else {
                tf(
                    "{n} effects · by {author}",
                    &[("n", &p.styles.len().to_string()), ("author", &p.author)],
                )
            };
            let id = p.id.clone();
            setting_row(
                ui,
                effects_pack_name(&p.name),
                &hint,
                i + 1 == installed.len() && available.is_empty(),
                |ui| {
                    if text_button(ui, None, "Remove", true).clicked() {
                        app.packs.remove_effects(&id);
                        if !app.packs.available_effects().contains(&app.draft.effects) {
                            app.draft.effects = EffectStyle::None;
                        }
                    }
                    status_text(ui, "Installed", pal().accent);
                },
            );
        }
        for (i, p) in available.iter().enumerate() {
            let hint = if p.author.is_empty() {
                "Download to unlock these toggle effects.".to_string()
            } else {
                tf("by {author}", &[("author", &p.author)])
            };
            let key = format!("effects-{}", p.id);
            let p2 = p.clone();
            setting_row(
                ui,
                effects_pack_name(&p.name),
                &hint,
                i + 1 == available.len(),
                |ui| {
                    pack_controls(
                        app,
                        ui,
                        &key,
                        false,
                        true,
                        move |a| a.packs.install_effects(&p2),
                        |_| {},
                    );
                },
            );
        }
    });

    // ---- Themes
    section_title(ui, "THEMES");
    // While a theme is previewed it's on screen but not installed; it goes away when you
    // leave this page or press End preview.
    if let Some((t, _)) = app.packs.preview.clone() {
        egui::Frame::new()
            .fill(pal().accent_bg)
            .stroke(Stroke::new(1.0, pal().accent_dim))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(bold(tf("Previewing {name}", &[("name", &t.name)])));
                        ui.label(muted(
                            "Not installed. It disappears when you leave this page.",
                        ));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if text_button(ui, None, "End preview", true).clicked() {
                            app.end_theme_preview();
                        }
                        if accent_pill(ui, "Install it", true).clicked() {
                            app.keep_theme_preview();
                        }
                    });
                });
            });
        ui.add_space(6.0);
    }
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
                if current == "default" && app.packs.preview.is_none() {
                    status_text(ui, "In use", pal().accent);
                } else if text_button(ui, None, "Use", true).clicked() {
                    // Picking a theme ends any preview, so the choice shows at once.
                    app.end_theme_preview();
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
                    ui.label(muted(if t.author.is_empty() {
                        "Installed".to_string()
                    } else {
                        tf("by {author}", &[("author", &t.author)])
                    }));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if text_button(ui, None, "Remove", true).clicked() {
                        if app.draft.theme == t.id {
                            app.draft.theme = "default".into();
                        }
                        app.packs.remove_theme(&t.id);
                    }
                    if current == t.id && app.packs.preview.is_none() {
                        status_text(ui, "In use", pal().accent);
                    } else if text_button(ui, None, "Use", true).clicked() {
                        app.end_theme_preview();
                        app.draft.theme = t.id.clone();
                    }
                });
            });
        }
        let available: Vec<_> = index
            .as_ref()
            .map(|i| {
                i.theme
                    .iter()
                    .filter(|t| !installed.iter().any(|x| x.id == t.id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for t in available {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                swatch(
                    ui,
                    rewind_core::theme::parse_hex(&t.swatch)
                        .map(|c| Color32::from_rgb(c[0], c[1], c[2]))
                        .unwrap_or(pal().faint),
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(bold(&t.name));
                    ui.label(muted(if t.author.is_empty() {
                        "Optional download".to_string()
                    } else {
                        tf("by {author} · optional download", &[("author", &t.author)])
                    }));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let key = format!("theme-{}", t.id);
                    pack_controls(
                        app,
                        ui,
                        &key,
                        false,
                        true,
                        |a| a.packs.install_theme(&t),
                        |_| {},
                    );
                    // Try it on without installing anything.
                    let previewing = app
                        .packs
                        .preview
                        .as_ref()
                        .map(|(_, p)| p.id == t.id)
                        .unwrap_or(false);
                    let loading = app
                        .packs
                        .jobs
                        .contains_key(&format!("preview-theme-{}", t.id));
                    if previewing {
                        status_text(ui, "Previewing", pal().accent);
                    } else if loading {
                        status_text(ui, "Loading…", pal().muted);
                    } else if !app.packs.jobs.contains_key(&key)
                        && text_button(ui, None, "Preview", true).clicked()
                    {
                        app.packs.preview_theme(&t);
                    }
                });
            });
        }
    });
    ui.label(muted(
        "Make your own: drop a theme .toml file into the themes folder, or start from the example.",
    ));
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

/// The built-in effects pack was called "Celebration effects" in older pack lists.
fn effects_pack_name(name: &str) -> &str {
    if name == "Celebration effects" {
        "Toggle effects"
    } else {
        name
    }
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
            app.notify(
                tf(
                    "Created {name}.toml in the themes folder",
                    &[("name", &name)],
                ),
                false,
            );
        }
        Err(e) => app.notify(
            tf(
                "Couldn't create the example theme: {e}",
                &[("e", &e.to_string())],
            ),
            true,
        ),
    }
}
