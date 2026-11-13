# Changelog

All notable changes are listed here. The section for a tag becomes that release's notes.

## [0.2.0]

### Added
- **System tray** with Save, Toggle, Open folder and Quit; closing the window keeps recording; optional start-hidden and start-with-Windows.
- **In-game notification toast** (native overlay window, excluded from screen capture) and an optional save sound.
- **Video thumbnails** in the clip list from Windows' own thumbnail cache.
- **Smooth UI animations** (sliding selection, page transitions, animated ring and switches), all switchable off.
- **Eased smooth scrolling** with its own on/off switch, independent of the animations toggle.
- **Toggle celebrations**: Shine, Sparkle, Creeper and Ripple effects when a switch is turned on, with a "Try it" preview and an Off option.
- **DXGI screen capture by default** on Windows 10, which avoids the yellow Windows Graphics Capture recording border; WGC remains selectable.
- **Theme starter kit**: `docs/theme-template.toml` plus an expanded `docs/THEMES.md`, and an Add-ons button that writes a working example theme.
- **Performance presets** (Low-end PC, Balanced, High quality), a low-power interface mode, and a live CPU/RAM/encoder readout.
- **Automatic Low-end preset** on first launch on PCs with four or fewer CPU threads.
- **Optional add-ons**: offline **voice clipping** ("clip that") and downloadable **themes**, fetched on demand and verified by SHA-256.
- Theme support with enforced readability, including light themes.
- **Installer** (Inno Setup) and portable zip with the OBS runtime bundled; build-provenance attestations and checksums.
- Version info, icon and manifest embedded in the exe.

### Changed
- Redesigned interface (Inter typeface, custom-painted controls, key-capture hotkey fields).
- Replay buffer uses a cheaper video scaler (bicubic, bilinear in low-power mode) instead of libobs-rs's Lanczos default.
- CPU fallback encoder preset now follows the quality setting; the app warns when encoding on the CPU.
- The "no hardware encoder" message is now calm and informational instead of an alarming error; changing resolution no longer pops a scary toast.
- HEVC/AV1 hardware failures fall back to H.264 instead of rolling settings back.
- Longer, gentler animation timings and a 30 fps buffer-ring pulse for a softer feel.
- Background threads sleep instead of polling; the UI no longer redraws while hidden or idle.

## [0.1.0]
- First version: replay buffer, global hotkeys, clip list, lossless trim.
