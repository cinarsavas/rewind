# Changelog

All notable changes are listed here. The section for a tag becomes that release's notes.

## [0.3.2]

### Added
- **Updates install themselves.** When Rewind finds a newer release (at launch, or when you press **Check for updates**), it downloads it, checks it against the release's SHA-256 list, closes, installs it and starts again on the new version. Installed copies run the new setup silently; portable copies replace their own folder. If you'd rather do it yourself, turn off About → **Install updates automatically** and Rewind will only tell you and link to the release page. **Update now** and **Open release page** are both on the About page.
- A short "Rewind updated to version …" note after an update.

### Fixed
- **An update that "didn't apply".** The v0.3.1 release was built from the 0.3.0 code, so it still called itself 0.3.0 and kept offering the same update. Releases now refuse to build when the tag doesn't match the version in `Cargo.toml`.

## [0.3.1]

### Fixed
- **Turkish voice clipping now installs.** The Turkish Vosk model uses the older flat layout (`final.mdl`, `HCLr.fst`, …), which the installer rejected with "the download doesn't contain the expected files". Both model layouts are accepted now, so Add-ons → Voice clipping can install Türkçe and the toggle becomes available.
- **A phrase the model doesn't know is explained instead of silently failing.** The status now names the missing words and suggests the language's default phrase, with a one-click **Use default** button. For example, an English phrase left over after switching to Turkish shows `The Türkçe model doesn't know "clip", "that". Try "klip al".`
- Turkish grammar keeps its own letters (`ı`, `ş`, `ç`, `ğ`, `ö`, `ü`) instead of folding them to ASCII, so custom Turkish phrases match what the model actually knows.
- **A config file with one bad value no longer resets every setting.** Rewind keeps every value that still reads correctly, uses the default only for the broken one, and saves a copy of the original as `config.toml.bad`. `effects = "off"` is accepted as well as `"none"`.

### Added
- **Easy clip renaming.** Every clip row has a pencil button: type a new name, press Enter, and the file is renamed in place with its extension and folder kept. Windows-hostile characters are cleaned and an existing name gets a ` (2)` suffix instead of overwriting another clip.
- **Delete clips permanently.** Settings → Replay buffer → **Delete permanently** skips the Recycle Bin, so deleted clips free their disk space right away. It is off by default; while it is on, the delete button and the confirmation say so in red.
- **Downloadable celebration effects.** Toggle effects are now an Add-ons pack instead of being built into the default experience. Install **Celebration effects** from Add-ons to unlock Shine, Sparkle, Creeper, Ripple, Confetti, Comet, Bloom and Bolt; until then the picker stays Off.

### Changed
- **The Apply bar shows on every page.** Changing a setting on Add-ons, Clips or Replay now reveals the same Apply/Revert bar as Settings, so you no longer have to navigate back first.
- Effects are off by default, including in the Balanced and High-quality presets.

### Performance
- **Long clip libraries stay light.** The Clips page only draws the rows on screen and only those load thumbnails. Before, a folder with more than 96 clips kept throwing its thumbnail cache away and reloading it in a loop; the cache now drops only the least recently seen thumbnail.
- The replay-buffer state is cached instead of being asked from OBS's worker thread several times per frame.
- Voice clipping no longer checks its files on disk every frame, and reuses its audio buffer instead of allocating one per chunk.
- The in-game notification stops redrawing while it sits still, waking only for its fade-in and fade-out.
- Saving a clip polls the clips folder without an extra file-system call per file, and the clip library lists its folder once instead of twice.

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
