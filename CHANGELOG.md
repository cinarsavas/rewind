# Changelog

All notable changes are listed here. The section for a tag becomes that release's notes.

## [0.3.0]

### Added
- **Turkish interface.** Settings → General → Language switches the whole UI between English and Turkish; English stays the default. Missing translations fall back to English.
- **Turkish voice clipping.** The Turkish model now folds `I`/`İ`/`ı` correctly, and switching the voice language follows its own default phrase (`clip that` / `klip al`) unless you typed your own.
- **Four more toggle celebrations** — Comet, Confetti, Bloom and Bolt join the existing Shine, Sparkle, Creeper and Ripple, with a live preview when you pick one.
- **A distinct clip-saved chime** so a saved replay is recognisable over game audio.
- **Separate desktop and microphone devices.** Settings → Audio can record a specific speaker/headphones endpoint and a specific mic, like OBS, with a Refresh button. This fixes silent voice tracks caused by the wrong default device.
- **Update checks.** Rewind asks the GitHub releases API at launch (switchable off) and on demand from About, and links straight to a newer release.
- **Deep theming.** Themes can now set navigation width, content width, page margin, card radius, a background image, a gradient or a solid background, and the dimming level. In-game notifications use the theme palette too.
- **One-click clip deletion.** The "Ask before deleting a clip" toggle can be turned off to send clips straight to the Recycle Bin.

### Changed
- Animations, low-power mode and effects now reliably apply to every switch, and live effects are cleared when animations are turned off.
- The CPU-encoder note is softer and offers the Low-end preset instead of sounding like an error.

### Fixed
- Toggle effects sometimes not playing.
- Voice capture staying silent when the default mic was not the one in use.
- The speech listener now restarts itself after an error instead of staying dead.
- Voice models load from paths with non-ASCII characters.

## [0.2.1]

### Fixed
- **Add-ons now work in local builds too.** The Add-ons page no longer says the build "isn't linked to a GitHub repository"; it falls back to the upstream release feed, and `REWIND_REPO` can still point a build at a fork.
- Voice clipping can be installed and started from any build, so "clip that" works after the speech engine and a language pack are downloaded.

### Changed
- The About page credits **pro82xw** instead of opencode.

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
