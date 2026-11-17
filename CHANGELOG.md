# Changelog

All notable changes are listed here. The section for a tag becomes that release's notes.

## [0.3.8]

### Changed
- **Voice clipping matches by sound, not by exact words.** Turkish is written the way it's said, so Rewind now compares the *sounds* of what was heard with your phrase. Any phrase works, even with words the small Turkish model has never seen ("kliple", "klipşunu", "klipliyorum"): Rewind reads the model's own dictionary (about 98 000 Turkish words) and spells the phrase with known words that sound the same, plus the way people actually say it ("klip" → "kilip"). "klip şunu" and "klipşunu" are the same; "klip all" counts as "klip al".
- **Far fewer clips nobody asked for.** For every phrase, Rewind now picks near-miss words from the dictionary automatically ("kalıp", "kulüp", "galip" for "klip"; "keep", "creep", "clap" for "clip"), so speech that only *sounds a bit like* the phrase lands on one of those. On a test set of natural-sounding synthetic speech with game-like noise: Turkish false triggers went from 4 % to none; English from 23 % to 16 % ("keep that" used to save a clip 14 times out of 18; now never). Confidence levels were re-tuned per language from the same tests.
- A phrase split in two by the recogniser ("klip" … "al") is now recognised as one.
- **The clip sound plays the moment you press the hotkey or say the phrase**, not after the file is written. The hotkey, voice and tray now start the save directly instead of first waking the app's window, which could lag in a game. A custom .mp3 stays loaded, so it plays without a decoding delay; if a save then fails you hear the Windows error sound.
- The replay page's recording dot redraws at 20 instead of 30 frames a second (less CPU, looks the same).

### Fixed
- **Voice clipping couldn't hear some microphones that recorded fine.** Two causes:
  - Windows keeps two default microphones (the default and the default *communications* device). For "System default", the recording (OBS) uses the communications one; voice clipping used the other. When they differ, e.g. a headset for calls and a webcam mic as the plain default, voice heard the wrong mic. Both now use the same one, and the mic list marks that one as "(default)".
  - Voice asked Windows to convert the mic to 16 kHz mono, which some drivers (many USB headsets, some Realtek setups) refuse or answer with silence. It now opens the mic in its own format, like the recording does, and converts it itself (with the old way as a fallback).
- Changing the microphone in Settings now restarts voice listening on the new one (it kept the old one until a restart). "System default" follows Windows when the default mic changes, e.g. when you plug in a headset.

### Added
- **Microphone** choice for voice clipping (Add-ons → Voice clipping): "Same as recording" by default, or any microphone. The row shows which mic it's listening to.
- If the microphone sends nothing but silence (muted, or not the one you talk into), the status says so instead of "Listening".

## [0.3.7]

### Changed
- **Hover previews start almost instantly.** Frames are streamed: the preview starts playing as soon as the first frame is ready (about a tenth of a second) instead of after the whole 20 seconds had been processed. It's also smoother (12 frames a second), skims faster, and a clip you skim past is finished in the background so it's instant next time. Background preparation that a hover interrupted is picked up again instead of being dropped.
- **Trimmed clips sit right next to the clip they came from**, newest trim first, instead of jumping to the top of the list.
- **Clips page spacing.** The grid now uses the whole window and picks the number of columns closest to the size you chose, so there's no more single giant card with empty space beside it, and nothing is cut off at the edge.
- "Celebration effects" are now called **Toggle effects**.
- **New: Extra animations** (Settings → Interface, on by default, off in Low-end): thumbnails zoom gently on hover, starring a clip pops, and a clip you just saved glows for a moment. Turns off on its own with Smooth animations or Low-power mode.

### Fixed
- **Updating no longer closes your games or browser.** Games and browsers that Rewind captured keep a Rewind file (the capture hook) loaded; the installer offered to close them, and an update from inside the app closed them without asking. Now only Rewind itself is closed; a file that's still in use is set aside and cleaned up next time.
- Hover previews used an option ffmpeg doesn't accept (`-skip_frame nonref`), which could stop previews from appearing at all.
- Picking another theme while one is being previewed now ends the preview and shows your choice right away.

## [0.3.6]

### Fixed
- **Clips page layout.** Cards were laid out sideways (buttons next to the thumbnail, no name, the last column cut off). Each card now stacks the thumbnail, name, details and buttons top to bottom and fits the window.

## [0.3.5]

### Added
- **A new Clips page.** Clips are now cards in a grid: a big thumbnail (click to play, hover for the preview), the full-width name and details underneath, and the buttons on their own row, so long names and the buttons never squeeze each other. **Size** (or Ctrl + mouse wheel) changes how big the cards are; the number of columns follows the window. Less common actions (Rename, Trim, Copy for Discord, Star) are in the **⋯** menu.
- **Favourites.** Star a clip from its thumbnail or the ⋯ menu; the star button next to the search box shows only starred clips. Starred clips are never deleted by the storage limit.
- **Quick share (Clips page).** The copy button puts the clip itself on the clipboard, so you can paste it straight into Discord, WhatsApp or a folder. **Copy for Discord (under 10 MB)** makes a 720p copy that fits Discord's free upload limit first (needs ffmpeg, e.g. from the Clip previews add-on).
- **Folder per game (on by default).** Each clip is saved into a folder named after the game or app that was in front when you saved it, e.g. `Videos\Rewind\Counter-Strike 2`. Clips saved from Rewind itself or the desktop stay in the main folder. Settings → Replay buffer.
- **Storage limit (off by default).** Settings → Replay buffer → **Storage limit** deletes the oldest clips for good when the folder grows past the size you pick. Starred clips are never deleted.
- **Whisper check for voice clipping (optional add-on).** Install **Whisper check** under Add-ons → Voice clipping and anything that sounds like your phrase is double-checked by Whisper, which is far more accurate, especially in Turkish. Vosk keeps listening cheaply; Whisper only runs for a moment after a phrase-like sentence, at lower priority. If Whisper isn't available it falls back to the normal check. The voice test shows when Whisper made the call.

### Changed
- **Hover previews are much lighter.** Frames are pulled at the size the card is actually shown at (240 px for normal cards, 400 px only for big ones), at 4 frames a second, and kept compressed in memory: only the frame on screen is decoded. A hovered clip now uses well under 1 MB of memory instead of about 20 MB, so more of them stay ready.

### Fixed
- **The Turkish interface is complete.** Engine notes (for example the CPU-encoding message), error messages from saving, audio devices, downloads, sounds and the clipboard, the tray menu and its tooltip are now translated too, including messages that carry details such as a device error. The tray menu switches language as soon as you press Apply.
- New theme names: Charcoal Glow, Coral Night, Violet Wave, Deep Ocean, Plum Sunset, Aurora, Brushed Metal, Cobalt, Sage and Mono Red. Themes you already installed keep working.
- The voice **Trigger phrase** box was squeezed to a sliver by its long hint. Setting hints now wrap onto a second line instead of crushing the control next to them.

## [0.3.4]

### Added
- **Your own clip-saved sounds.** Drag an .mp3 or .wav onto Rewind to add it, then pick it under Settings → Notifications → **Sound** (the first one you add is picked for you). Add-ons → **Custom sounds** lists them with Play, Use and Remove buttons and opens the sounds folder. Up to 5 sounds are kept unless you turn off **Limit to 5 sounds**. Files over 10 MB are refused; anything that won't play falls back to the Rewind chime.
- **10 new themes** inspired by phone wallpapers and colour schemes. They're optional downloads in Add-ons → Themes.
- **Try a theme before installing it.** Every downloadable theme has a **Preview** button: it shows the theme right away without installing it, and the downloaded file is deleted as soon as it's loaded. Press **Install it** to keep it, or **End preview** (leaving the Add-ons page ends it too).
- **Better Turkish voice clipping.** What the recogniser hears is now cleaned first (a high-pass filter removes rumble and fan noise, and automatic gain lifts quiet or distant mics), many more look-alike Turkish words were added so near-misses don't turn into clips, and you can set **Strictness** (Relaxed / Normal / Strict). You can also give several phrases separated by commas, for example `klip al, klibi al`.
- **Voice test button.** The live mic meter and "Last heard" now appear only while a test is running: press **Test** next to the voice status, and it stays open for 20 seconds or until you press **Stop test**.

### Fixed
- **Hover previews now show up, and fast.** In 0.3.3 a finished preview was never picked up, so hovering just kept loading. Now, while Rewind is in front, the clips you can see are prepared in the background at idle priority, so a hover usually plays at once. A hover that isn't ready jumps the queue and uses every core, and the last 4 previews stay in memory. Nothing is prepared while you're in a game.

## [0.3.3]

### Added
- **Hover previews (add-on).** Install **Clip previews** from Add-ons and hovering a clip's thumbnail plays its last 20 seconds as a quick loop, with a progress line. Frames are made once per clip by ffmpeg at below-normal priority with GPU decoding, cached on disk, and only the hovered clip is kept in memory, so it doesn't slow down the PC or a running game. The add-on also enables **Trim** when ffmpeg isn't installed elsewhere.
- **Bigger thumbnails.** The Clips page has a **Size** slider (or Ctrl + mouse wheel) to make thumbnails up to 2.5× larger. It's remembered without pressing Apply.
- **Voice test.** The voice card shows what the recogniser heard last, how sure it was, and whether it saved a clip or why not ("say it on its own", "say it clearer"…).
- **Copy anything that matters.** Error toasts, the start-up error screen, encoder/capture notes, hotkey warnings, voice errors and update errors all have a copy button, and About → **Copy diagnostics** copies version, settings, encoder, voice status and recent errors for bug reports.

### Fixed
- **Voice clipping no longer saves clips you didn't ask for**, especially in Turkish. The phrase now has to be said on its own: it is matched only on finished sentences (not the recogniser's running guesses), it doesn't count inside a longer sentence, and each word must be recognised with good confidence. Everyday words are added to the recogniser's vocabulary as decoys, so normal talk and game chatter land on them instead of being forced onto "klip al" / "clip that". Clips are saved about half a second after you finish the phrase.

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
