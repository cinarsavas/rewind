//! Downloading optional add-ons. Uses the Windows HTTP stack (WinHTTP), so it honours the
//! system proxy and certificate store and adds no TLS library to the exe. Every download is
//! restricted to GitHub hosts and verified against a SHA-256 before it is used.

use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use rewind_core::packs::{allowed_download_url, safe_entry_path, valid_sha256};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Networking::WinHttp::*;

/// Shared progress, readable from the UI thread while a worker downloads.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        let t = self.total.load(Ordering::Relaxed);
        if t == 0 { 0.0 } else { (self.done.load(Ordering::Relaxed) as f64 / t as f64).clamp(0.0, 1.0) as f32 }
    }
}

const MAX_DOWNLOAD: u64 = 400 * 1024 * 1024;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn split_url(url: &str) -> Result<(String, String), String> {
    let rest = url.strip_prefix("https://").ok_or("only https downloads are allowed")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    Ok((host.to_string(), path.to_string()))
}

/// Stream `url` into `sink`, reporting progress. Returns the SHA-256 (lower-case hex).
fn stream(url: &str, sink: &mut dyn Write, progress: Option<&Progress>, max: u64) -> Result<String, String> {
    if !allowed_download_url(url) {
        return Err(format!("refusing to download from {url}"));
    }
    let (host, path) = split_url(url)?;
    unsafe {
        let session = Handle(WinHttpOpen(wide("Rewind").as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0));
        if session.0.is_null() {
            return Err("couldn't start the network stack".into());
        }
        WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 30_000);
        let connect = Handle(WinHttpConnect(session.0, wide(&host).as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0));
        if connect.0.is_null() {
            return Err(format!("couldn't connect to {host}"));
        }
        let request = Handle(WinHttpOpenRequest(connect.0, wide("GET").as_ptr(), wide(&path).as_ptr(), null(), null(), null(), WINHTTP_FLAG_SECURE));
        if request.0.is_null() {
            return Err("couldn't create the request".into());
        }
        if WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) == 0 || WinHttpReceiveResponse(request.0, null_mut()) == 0 {
            return Err("no response from the server (are you online?)".into());
        }
        let mut status: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, null(), &mut status as *mut u32 as *mut c_void, &mut len, null_mut());
        if status != 200 {
            return Err(format!("server answered HTTP {status}"));
        }
        let mut clen: u32 = 0;
        let mut l2 = std::mem::size_of::<u32>() as u32;
        if WinHttpQueryHeaders(request.0, WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER, null(), &mut clen as *mut u32 as *mut c_void, &mut l2, null_mut()) != 0 {
            if clen as u64 > max {
                return Err("file is larger than allowed".into());
            }
            if let Some(p) = progress {
                p.total.store(clen as u64, Ordering::Relaxed);
            }
        }
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            if progress.map(|p| p.cancel.load(Ordering::Relaxed)).unwrap_or(false) {
                return Err("cancelled".into());
            }
            let mut read: u32 = 0;
            if WinHttpReadData(request.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut read) == 0 {
                return Err("the connection dropped".into());
            }
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > max {
                return Err("file is larger than allowed".into());
            }
            hasher.update(&buf[..read as usize]);
            sink.write_all(&buf[..read as usize]).map_err(|e| format!("writing file: {e}"))?;
            if let Some(p) = progress {
                p.done.store(total, Ordering::Relaxed);
            }
        }
        Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }
}

/// Download to `dest`, verifying the SHA-256. The file only appears at `dest` if it matched.
pub fn download_verified(url: &str, dest: &Path, sha256: &str, progress: &Arc<Progress>) -> Result<(), String> {
    if !valid_sha256(sha256) {
        return Err("missing checksum, refusing to download".into());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating folder: {e}"))?;
    }
    let part = dest.with_extension("part");
    let result = (|| {
        let mut f = File::create(&part).map_err(|e| format!("creating file: {e}"))?;
        let got = stream(url, &mut f, Some(progress), MAX_DOWNLOAD)?;
        f.flush().map_err(|e| e.to_string())?;
        if !got.eq_ignore_ascii_case(sha256) {
            return Err("checksum mismatch: the download is corrupted or has been tampered with".to_string());
        }
        fs::rename(&part, dest).map_err(|e| format!("saving file: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// Fetch a small text file (the pack index). No checksum: the index is the root of trust and
/// comes from the project's own GitHub release.
pub fn fetch_text(url: &str) -> Result<String, String> {
    let mut out: Vec<u8> = Vec::new();
    stream(url, &mut out, None, 1024 * 1024)?;
    String::from_utf8(out).map_err(|_| "index isn't valid UTF-8".to_string())
}

/// Extract a zip into `out_dir`, refusing path traversal and absurd sizes.
pub fn unzip(zip_path: &Path, out_dir: &Path) -> Result<(), String> {
    const MAX_FILES: usize = 20_000;
    const MAX_BYTES: u64 = 800 * 1024 * 1024;
    let file = File::open(zip_path).map_err(|e| format!("opening archive: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("reading archive: {e}"))?;
    if archive.len() > MAX_FILES {
        return Err("archive has too many files".into());
    }
    fs::create_dir_all(out_dir).map_err(|e| format!("creating folder: {e}"))?;
    let mut written: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("reading archive: {e}"))?;
        let name = entry.name().map_err(|e| format!("archive has an unreadable file name: {e}"))?.into_owned();
        let Some(rel) = safe_entry_path(&name) else {
            return Err(format!("archive contains an unsafe path: {name}"));
        };
        let target = out_dir.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("creating folder: {e}"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("creating folder: {e}"))?;
        }
        let mut out = File::create(&target).map_err(|e| format!("writing {}: {e}", target.display()))?;
        let mut limited = (&mut entry).take(MAX_BYTES.saturating_sub(written) + 1);
        let n = std::io::copy(&mut limited, &mut out).map_err(|e| format!("extracting: {e}"))?;
        written += n;
        if written > MAX_BYTES {
            return Err("archive expands to more than allowed".into());
        }
    }
    Ok(())
}
