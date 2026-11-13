//! System tray icon with a context menu. Events are forwarded through a channel and wake the
//! UI loop, so the tray keeps working while the main window is hidden.

use std::sync::mpsc::{channel, Receiver};

use eframe::egui;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    SaveReplay,
    ToggleBuffer,
    OpenFolder,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    toggle: CheckMenuItem,
    rx: Receiver<TrayAction>,
    last_on: bool,
}

/// A 32×32 mint disc with a dark "record" dot, drawn in code so no image asset is needed.
fn make_icon() -> Option<Icon> {
    const N: u32 = 32;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5 - 16.0, y as f32 + 0.5 - 16.0);
            let d = (fx * fx + fy * fy).sqrt();
            let outer = (15.0 - d + 0.5).clamp(0.0, 1.0);
            let dot = (6.5 - d + 0.5).clamp(0.0, 1.0);
            let i = ((y * N + x) * 4) as usize;
            let mix = |a: f32, b: f32| (a * (1.0 - dot) + b * dot) as u8;
            rgba[i] = mix(0x4a as f32, 0x0d as f32);
            rgba[i + 1] = mix(0xd6 as f32, 0x0f as f32);
            rgba[i + 2] = mix(0xa7 as f32, 0x14 as f32);
            rgba[i + 3] = (outer * 255.0) as u8;
        }
    }
    Icon::from_rgba(rgba, N, N).ok()
}

impl Tray {
    pub fn new(ctx: egui::Context, buffer_on: bool) -> Result<Tray, String> {
        let open = MenuItem::new("Open Rewind", true, None);
        let save = MenuItem::new("Save replay now", true, None);
        let toggle = CheckMenuItem::new("Instant Replay on", true, buffer_on, None);
        let folder = MenuItem::new("Open clips folder", true, None);
        let quit = MenuItem::new("Quit Rewind", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &save, &toggle, &folder, &PredefinedMenuItem::separator(), &quit])
            .map_err(|e| format!("tray menu: {e}"))?;

        let (tx, rx) = channel();
        let (open_id, save_id, toggle_id, folder_id, quit_id) =
            (open.id().clone(), save.id().clone(), toggle.id().clone(), folder.id().clone(), quit.id().clone());
        {
            let (tx, ctx) = (tx.clone(), ctx.clone());
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                let action = if e.id == open_id {
                    TrayAction::Show
                } else if e.id == save_id {
                    TrayAction::SaveReplay
                } else if e.id == toggle_id {
                    TrayAction::ToggleBuffer
                } else if e.id == folder_id {
                    TrayAction::OpenFolder
                } else if e.id == quit_id {
                    TrayAction::Quit
                } else {
                    return;
                };
                let _ = tx.send(action);
                ctx.request_repaint();
            }));
        }
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            let show = matches!(
                e,
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
                    | TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
            );
            if show {
                let _ = tx.send(TrayAction::Show);
                ctx.request_repaint();
            }
        }));

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(tooltip(buffer_on));
        if let Some(icon) = make_icon() {
            builder = builder.with_icon(icon);
        }
        let icon = builder.build().map_err(|e| format!("tray icon: {e}"))?;
        Ok(Tray { icon, toggle, rx, last_on: buffer_on })
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        self.rx.try_iter().collect()
    }

    /// Keep the check mark and tooltip in sync with the real buffer state.
    pub fn sync(&mut self, buffer_on: bool) {
        if buffer_on != self.last_on {
            self.last_on = buffer_on;
            self.toggle.set_checked(buffer_on);
            let _ = self.icon.set_tooltip(Some(tooltip(buffer_on)));
        } else if self.toggle.is_checked() != buffer_on {
            // The click flipped the native check mark; put it back to the truth.
            self.toggle.set_checked(buffer_on);
        }
    }
}

fn tooltip(on: bool) -> &'static str {
    if on { "Rewind — Instant Replay is on" } else { "Rewind — Instant Replay is off" }
}
