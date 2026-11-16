//! Optional Whisper double-check for voice clipping (add-on).
//!
//! Vosk listens all the time because it is cheap. When it hears something that might be the
//! trigger phrase, the short recording of that sentence is handed to Whisper, which is far
//! more accurate (especially in Turkish) but heavier. So Whisper only ever runs for a second
//! or two right after you said something phrase-like, never continuously.
//!
//! Whisper runs as the add-on's own `whisper-server.exe`, started once with the model loaded
//! and reached over 127.0.0.1 only, at below-normal priority so games come first.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const SERVER_EXE: &str = "whisper-server.exe";

/// The model file inside the add-on folder (the first `.bin`).
pub fn model_in(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().map(|e| e == "bin").unwrap_or(false))
}

/// True when the add-on folder has the server and a model.
pub fn is_installed(dir: &Path) -> bool {
    dir.join(SERVER_EXE).is_file() && model_in(dir).is_some()
}

pub struct Whisper {
    child: Child,
    addr: SocketAddr,
    ready: bool,
}

impl Drop for Whisper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Whisper {
    /// Start the server for `lang` ("tr", "en"). Returns quickly; the model loads in the
    /// background and the first [`Self::transcribe`] waits for it.
    pub fn start(dir: &Path, lang: &str) -> Result<Whisper, String> {
        let model = model_in(dir).ok_or("the Whisper model is missing")?;
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("no free port: {e}"))?;
            l.local_addr().map_err(|e| e.to_string())?.port()
        };
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(1, 4);
        let mut cmd = Command::new(dir.join(SERVER_EXE));
        cmd.current_dir(dir)
            .arg("-m")
            .arg(&model)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["-l", lang, "-t", &threads.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS
            cmd.creation_flags(0x0800_0000 | 0x0000_4000);
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("couldn't start Whisper: {e}"))?;
        Ok(Whisper {
            child,
            addr: SocketAddr::from(([127, 0, 0, 1], port)),
            ready: false,
        })
    }

    /// Transcribe 16 kHz mono audio. Waits up to 20 s for the server the first time.
    pub fn transcribe(&mut self, samples: &[i16]) -> Result<String, String> {
        if let Ok(Some(status)) = self.child.try_wait() {
            return Err(format!("Whisper stopped ({status})"));
        }
        let wav = wav_bytes(samples, 16_000);
        let deadline = Instant::now()
            + if self.ready {
                Duration::from_secs(2)
            } else {
                Duration::from_secs(20)
            };
        let mut stream = loop {
            match TcpStream::connect_timeout(&self.addr, Duration::from_millis(500)) {
                Ok(s) => break s,
                Err(e) if Instant::now() >= deadline => {
                    return Err(format!("Whisper isn't answering: {e}"))
                }
                Err(_) => std::thread::sleep(Duration::from_millis(250)),
            }
        };
        self.ready = true;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

        let boundary = "----rewindwhisperboundary";
        let mut body: Vec<u8> = Vec::with_capacity(wav.len() + 512);
        for (name, value) in [("response_format", "json"), ("temperature", "0.0")] {
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
                )
                .as_bytes(),
            );
        }
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(&wav);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let head = format!(
            "POST /inference HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|_| stream.write_all(&body))
            .map_err(|e| format!("sending to Whisper: {e}"))?;
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(|e| format!("reading from Whisper: {e}"))?;
        parse_response(&response)
    }
}

/// Pull `text` out of the server's JSON reply.
fn parse_response(raw: &[u8]) -> Result<String, String> {
    let text = String::from_utf8_lossy(raw);
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return Err("Whisper sent an empty reply".into());
    };
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return Err(format!(
            "Whisper error: {}",
            head.lines().next().unwrap_or("unknown")
        ));
    }
    let v: serde_json::Value =
        serde_json::from_str(body.trim()).map_err(|e| format!("Whisper reply: {e}"))?;
    Ok(v.get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string())
}

/// A minimal 16-bit PCM mono WAV file.
fn wav_bytes(samples: &[i16], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_and_reply() {
        let w = wav_bytes(&[0, 1, -1], 16_000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(w.len(), 44 + 6);
        let reply = b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n{\"text\":\" klip al\"}";
        assert_eq!(parse_response(reply).unwrap(), "klip al");
        assert!(parse_response(b"HTTP/1.1 500 Oops\r\n\r\n{}").is_err());
    }
}
