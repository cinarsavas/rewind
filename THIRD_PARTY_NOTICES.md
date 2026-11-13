# Third-party notices

Rewind is licensed GPL-3.0-or-later. The combined work is distributed under GPL-3.0 because it links libobs-rs (GPL-3.0) and libobs (GPL-2.0-or-later). It uses the following components.

| Component | Use | License |
|---|---|---|
| [libobs / OBS Studio](https://github.com/obsproject/obs-studio) | Screen/audio capture, encoding, replay buffer. Unmodified binaries are bundled in the installer and portable zip. | GPL-2.0-or-later |
| [libobs-rs](https://github.com/libobs-rs/libobs-rs) | Rust bindings and runtime bootstrapper for libobs | GPL-3.0 |
| [egui / eframe](https://github.com/emilk/egui) | User interface | MIT OR Apache-2.0 |
| [Inter](https://rsms.me/inter/) | Interface typeface (embedded) | SIL Open Font License 1.1 (`crates/app/assets/Inter-LICENSE.txt`) |
| [Vosk](https://github.com/alphacep/vosk-api) | Optional offline speech recognition (downloaded on demand) | Apache-2.0 |
| Vosk small language models | Optional (downloaded on demand) | Apache-2.0 |
| [tray-icon](https://github.com/tauri-apps/tray-icon), [global-hotkey](https://github.com/tauri-apps/global-hotkey) | Tray and hotkeys | MIT OR Apache-2.0 |
| [windows-sys](https://github.com/microsoft/windows-rs), [windows](https://github.com/microsoft/windows-rs) | Windows API bindings | MIT OR Apache-2.0 |
| [wasapi](https://github.com/HEnquist/wasapi-rs), [libloading](https://github.com/nagisa/rust_libloading), [sha2](https://github.com/RustCrypto/hashes), [zip](https://github.com/zip-rs/zip2), [serde](https://serde.rs), [toml](https://github.com/toml-rs/toml), [directories](https://github.com/dirs-dev/directories-rs), [trash](https://github.com/Byron/trash-rs), [opener](https://github.com/Seeker14491/opener) | Assorted | MIT and/or Apache-2.0 |

Run `cargo tree` or use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) for the complete, current dependency license list. Source for the bundled OBS Studio binaries: <https://github.com/obsproject/obs-studio>.

Rewind is not affiliated with, or endorsed by, NVIDIA, Microsoft or the OBS Project.
