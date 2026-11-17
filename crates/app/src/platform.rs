//! Small Windows integrations: single instance, start-with-Windows, system sound.

use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
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
        let rc = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            null(),
            0,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        );
        if rc != 0 {
            return Err(format!("couldn't open the Run registry key (error {rc})"));
        }
        let result = if enabled {
            let cmd = wide(&format!("\"{}\" --tray", exe.display()));
            let rc = RegSetValueExW(
                key,
                wide(RUN_VALUE).as_ptr(),
                0,
                REG_SZ,
                cmd.as_ptr() as *const u8,
                (cmd.len() * 2) as u32,
            );
            if rc == 0 {
                Ok(())
            } else {
                Err(format!("couldn't write the Run entry (error {rc})"))
            }
        } else {
            let rc = RegDeleteValueW(key, wide(RUN_VALUE).as_ptr());
            // ERROR_FILE_NOT_FOUND (2) just means it wasn't set.
            if rc == 0 || rc == 2 {
                Ok(())
            } else {
                Err(format!("couldn't remove the Run entry (error {rc})"))
            }
        };
        RegCloseKey(key);
        result
    }
}

/// The distinctive two-note chime that tells you a clip was saved. It is embedded in the
/// executable as a tiny WAV, so it works offline and sounds the same on every PC.
/// Played asynchronously and from memory, so it never blocks the UI thread.
pub fn clip_saved_sound() {
    static WAV: &[u8] = include_bytes!("../assets/clip-sound.wav");
    unsafe {
        let ok = PlaySoundW(
            WAV.as_ptr() as *const u16,
            null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
        if ok == 0 {
            // No audio endpoint (or the WAV was rejected): fall back to the system ding.
            MessageBeep(MB_OK);
        }
    }
}

/// A short Windows error sound, for a save that failed after its "saved" sound played.
pub fn error_sound() {
    unsafe {
        MessageBeep(MB_ICONHAND);
    }
}

/// Name of the game or app in front right now, for per-game clip folders. `None` for the
/// desktop, Explorer, Rewind itself and other shell windows (those clips stay in the root).
pub fn foreground_app_name() -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(process);
        if !ok {
            return None;
        }
        let exe = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
        let stem = exe
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        const SHELL: [&str; 8] = [
            "explorer",
            "applicationframehost",
            "shellexperiencehost",
            "searchhost",
            "startmenuexperiencehost",
            "searchapp",
            "lockapp",
            "textinputhost",
        ];
        if SHELL.contains(&stem.to_lowercase().as_str()) {
            return None;
        }

        // 1. The exe's own description ("Counter-Strike 2", "VALORANT"), unless it's a
        //    generic runtime such as Java (Minecraft), where the window title is better.
        let description = file_description(&exe).unwrap_or_default();
        let generic = [
            "java", "platform", "host", "runtime", "launcher", "electron",
        ];
        let lower = description.to_lowercase();
        if !description.trim().is_empty() && !generic.iter().any(|g| lower.contains(g)) {
            return Some(description.trim().to_string());
        }
        // 2. The window title, without the "- details" part and trailing version numbers.
        let mut title = vec![0u16; 256];
        let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        if n > 0 {
            let title = String::from_utf16_lossy(&title[..n as usize]);
            let first = [" - ", " | ", " — ", " – "]
                .iter()
                .fold(title.clone(), |t, sep| {
                    t.split(sep).next().unwrap_or("").to_string()
                })
                .trim()
                .to_string();
            let cleaned: Vec<&str> = first
                .split_whitespace()
                .filter(|w| {
                    !w.chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
                })
                .collect();
            let cleaned = cleaned.join(" ");
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
        // 3. The exe name, minus Unreal-style suffixes.
        let mut name = stem;
        for suffix in [
            "-Win64-Shipping",
            "-Win32-Shipping",
            "-WinGDK-Shipping",
            "_x64",
            "64",
        ] {
            if let Some(s) = name.strip_suffix(suffix) {
                name = s.to_string();
            }
        }
        (!name.is_empty()).then_some(name)
    }
}

/// The "FileDescription" from an exe's version resource.
fn file_description(exe: &Path) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    unsafe {
        let path = wide(&exe.to_string_lossy());
        let size = GetFileVersionInfoSizeW(path.as_ptr(), null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut ptr: *mut core::ffi::c_void = null_mut();
        let mut len = 0u32;
        let translation = wide("\\VarFileInfo\\Translation");
        let (lang, codepage) = if VerQueryValueW(
            data.as_ptr().cast(),
            translation.as_ptr(),
            &mut ptr,
            &mut len,
        ) != 0
            && len >= 4
        {
            let words = ptr as *const u16;
            (*words, *words.add(1))
        } else {
            (0x0409, 0x04b0)
        };
        let key = wide(&format!(
            "\\StringFileInfo\\{lang:04x}{codepage:04x}\\FileDescription"
        ));
        if VerQueryValueW(data.as_ptr().cast(), key.as_ptr(), &mut ptr, &mut len) == 0 || len == 0 {
            return None;
        }
        let slice = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let text = String::from_utf16_lossy(slice);
        Some(text.trim_end_matches('\0').trim().to_string()).filter(|s| !s.is_empty())
    }
}

/// Put files on the clipboard the way Explorer's Copy does, so they can be pasted into
/// Discord, WhatsApp, a chat window or a folder.
pub fn copy_files_to_clipboard(paths: &[std::path::PathBuf]) -> Result<(), String> {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    const CF_HDROP: u32 = 15;
    // DROPFILES header: pFiles (offset of the list), pt (x, y), fNC, fWide.
    const HEADER: usize = 20;
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.to_string_lossy().encode_utf16());
        list.push(0);
    }
    list.push(0);
    let bytes = HEADER + list.len() * 2;
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if mem.is_null() {
            return Err("out of memory".into());
        }
        let dst = GlobalLock(mem) as *mut u8;
        if dst.is_null() {
            GlobalFree(mem);
            return Err("couldn't lock clipboard memory".into());
        }
        let header: [u32; 5] = [HEADER as u32, 0, 0, 0, 1];
        std::ptr::copy_nonoverlapping(header.as_ptr() as *const u8, dst, HEADER);
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, dst.add(HEADER), list.len() * 2);
        GlobalUnlock(mem);
        if OpenClipboard(null_mut()) == 0 {
            GlobalFree(mem);
            return Err("the clipboard is busy, try again".into());
        }
        EmptyClipboard();
        let set = SetClipboardData(CF_HDROP, mem);
        CloseClipboard();
        if set.is_null() {
            GlobalFree(mem);
            return Err("couldn't put the file on the clipboard".into());
        }
        Ok(())
    }
}
