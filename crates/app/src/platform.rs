//! Small Windows integrations: single instance, start-with-Windows, system sound.

use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Returns false if another Rewind is already running (the mutex lives for the whole process).
pub fn acquire_single_instance() -> bool {
    unsafe {
        let name = wide("Local\\RewindInstantReplaySingleInstance");
        let handle = CreateMutexW(null(), 0, name.as_ptr());
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        // The handle is intentionally leaked: the OS releases it when the process exits.
        !handle.is_null() && !already
    }
}

/// Bring the already-running instance's window to the front (it may be hidden in the tray).
pub fn focus_existing_window() {
    unsafe {
        let hwnd = FindWindowW(null(), wide("Rewind").as_ptr());
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "Rewind";

/// Add or remove the per-user "run at login" entry. Starts hidden in the tray.
pub fn set_autostart(enabled: bool, exe: &Path) -> Result<(), String> {
    unsafe {
        let mut key: HKEY = null_mut();
        let rc = RegCreateKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, null(), 0, KEY_SET_VALUE, null(), &mut key, null_mut());
        if rc != 0 {
            return Err(format!("couldn't open the Run registry key (error {rc})"));
        }
        let result = if enabled {
            let cmd = wide(&format!("\"{}\" --tray", exe.display()));
            let rc = RegSetValueExW(key, wide(RUN_VALUE).as_ptr(), 0, REG_SZ, cmd.as_ptr() as *const u8, (cmd.len() * 2) as u32);
            if rc == 0 { Ok(()) } else { Err(format!("couldn't write the Run entry (error {rc})")) }
        } else {
            let rc = RegDeleteValueW(key, wide(RUN_VALUE).as_ptr());
            // ERROR_FILE_NOT_FOUND (2) just means it wasn't set.
            if rc == 0 || rc == 2 { Ok(()) } else { Err(format!("couldn't remove the Run entry (error {rc})")) }
        };
        RegCloseKey(key);
        result
    }
}

/// The default Windows "ding". Works in exclusive-fullscreen games where the overlay can't draw.
pub fn beep() {
    unsafe {
        MessageBeep(MB_OK);
    }
}
