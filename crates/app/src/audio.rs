//! Enumerate Windows audio endpoints so the user can pick the exact microphone and
//! playback device OBS should record, just like the audio settings in OBS Studio.
//!
//! The list is only built when the Settings page asks for it; it is cached by the UI so
//! the UI thread doesn't touch COM every frame.

use wasapi::{initialize_mta, DeviceEnumerator, Direction, Role};

/// One selectable endpoint. `id` is the WASAPI endpoint id OBS wants for `device_id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: String,
    pub label: String,
    pub is_default: bool,
}

impl AudioDevice {
    /// Label for a dropdown entry; the Windows default is marked so "default" stays obvious.
    pub fn display(&self) -> String {
        if self.is_default {
            format!("{} (default)", self.label)
        } else {
            self.label.clone()
        }
    }
}

/// List active endpoints in `direction`. `Direction::Render` gives the devices you hear
/// (desktop audio), `Direction::Capture` gives microphones. Never panics: on any WASAPI
/// error it returns an empty list and the UI falls back to the system default.
pub fn list(direction: Direction) -> Vec<AudioDevice> {
    // The UI thread is normally already in a COM apartment; a changed-mode result is fine.
    let _ = initialize_mta();
    let Ok(enumerator) = DeviceEnumerator::new() else {
        return Vec::new();
    };
    // For microphones, "default" means the default *communications* device: that's the one
    // OBS records when the setting is "System default", and the one voice clipping hears.
    let default_id = match direction {
        Direction::Capture => enumerator
            .get_default_device_for_role(direction, Role::Communications)
            .or_else(|_| enumerator.get_default_device(direction)),
        Direction::Render => enumerator.get_default_device(direction),
    }
    .ok()
    .and_then(|d| d.get_id().ok());
    let Ok(collection) = enumerator.get_device_collection(direction) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for device in &collection {
        let Ok(device) = device else { continue };
        let Ok(id) = device.get_id() else { continue };
        let label = device
            .get_friendlyname()
            .unwrap_or_else(|_| "Audio device".into());
        let is_default = default_id.as_deref() == Some(id.as_str());
        out.push(AudioDevice {
            id,
            label,
            is_default,
        });
    }
    out
}

/// A fresh list of playback devices (desktop audio).
pub fn output_devices() -> Vec<AudioDevice> {
    list(Direction::Render)
}

/// A fresh list of recording devices (microphones).
pub fn input_devices() -> Vec<AudioDevice> {
    list(Direction::Capture)
}

/// Resolve a stored endpoint id to a friendly label, for the dropdown's closed state.
pub fn label_for(devices: &[AudioDevice], id: &str) -> String {
    if id.is_empty() {
        return "System default".to_string();
    }
    devices
        .iter()
        .find(|d| d.id == id)
        .map(AudioDevice::display)
        .unwrap_or_else(|| "Missing device".to_string())
}
