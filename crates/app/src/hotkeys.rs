//! Global hotkeys (work while a game has focus) via the `global-hotkey` crate.

use std::str::FromStr;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use rewind_core::hotkey::Hotkey;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    SaveReplay,
    ToggleBuffer,
}

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    registered: Vec<(HotKey, u32, Action)>,
    rx: Receiver<GlobalHotKeyEvent>,
    /// Ids of the save hotkey, handled right inside the key handler.
    save_ids: Arc<Mutex<Vec<u32>>>,
}

fn to_code(key: &str) -> Option<Code> {
    let name = if key.len() == 1 {
        let c = key.chars().next()?;
        if c.is_ascii_digit() {
            format!("Digit{c}")
        } else {
            format!("Key{c}")
        }
    } else {
        match key {
            "Up" => "ArrowUp".to_string(),
            "Down" => "ArrowDown".to_string(),
            "Left" => "ArrowLeft".to_string(),
            "Right" => "ArrowRight".to_string(),
            other => other.to_string(),
        }
    };
    Code::from_str(&name).ok()
}

fn to_hotkey(h: &Hotkey) -> Result<HotKey, String> {
    let mut mods = Modifiers::empty();
    if h.ctrl {
        mods |= Modifiers::CONTROL;
    }
    if h.shift {
        mods |= Modifiers::SHIFT;
    }
    if h.alt {
        mods |= Modifiers::ALT;
    }
    if h.win {
        mods |= Modifiers::META;
    }
    let code = to_code(&h.key).ok_or_else(|| format!("key '{}' isn't supported", h.key))?;
    Ok(HotKey::new(
        if mods.is_empty() { None } else { Some(mods) },
        code,
    ))
}

impl Hotkeys {
    /// The save hotkey saves (and plays the sound) straight from the key handler, without
    /// waiting for the app's window to wake up. Other keys wake the UI loop through `ctx`,
    /// so they work while the window is hidden.
    pub fn new(ctx: egui::Context, quick: Arc<crate::quick::QuickSave>) -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new().map_err(|e| format!("hotkey service: {e}"))?;
        let (tx, rx) = channel();
        let save_ids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
        let ids = save_ids.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            let is_save = ids.lock().map(|ids| ids.contains(&e.id)).unwrap_or(false);
            if is_save {
                if e.state != HotKeyState::Pressed {
                    return;
                }
                if quick.trigger() || quick.is_saving() {
                    ctx.request_repaint();
                    return;
                }
                // Nothing to save (Instant Replay is off): let the app say so.
            }
            let _ = tx.send(e);
            ctx.request_repaint();
        }));
        Ok(Self {
            manager,
            registered: Vec::new(),
            rx,
            save_ids,
        })
    }

    /// Replace all registered hotkeys. Returns a message per hotkey that couldn't be
    /// registered (usually because another app already owns that combination).
    pub fn set(&mut self, bindings: &[(Action, &Hotkey)]) -> Vec<String> {
        for (hk, _, _) in self.registered.drain(..) {
            let _ = self.manager.unregister(hk);
        }
        let mut problems = Vec::new();
        for (action, h) in bindings {
            match to_hotkey(h) {
                Ok(hk) => match self.manager.register(hk) {
                    Ok(()) => self.registered.push((hk, hk.id(), *action)),
                    Err(e) => problems.push(format!(
                        "{h} couldn't be registered ({e}). Another app may be using it."
                    )),
                },
                Err(e) => problems.push(e),
            }
        }
        if let Ok(mut ids) = self.save_ids.lock() {
            *ids = self
                .registered
                .iter()
                .filter(|(_, _, a)| *a == Action::SaveReplay)
                .map(|(_, id, _)| *id)
                .collect();
        }
        problems
    }

    /// Drain pending key presses (not releases) and map them to actions.
    pub fn poll(&self) -> Vec<Action> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            if ev.state == HotKeyState::Pressed {
                if let Some((_, _, a)) = self.registered.iter().find(|(_, id, _)| *id == ev.id) {
                    out.push(*a);
                }
            }
        }
        out
    }
}
