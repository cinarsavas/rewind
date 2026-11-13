# Architecture

```
crates/
  core/   rewind-core: platform-independent, fully unit-tested logic
  app/    rewind: the Windows app (capture engine, UI, tray, overlay, add-ons)
themes/   optional theme files, validated by tests and published as add-ons
scripts/  build_packs.py: builds the optional add-on packs for a release
installer/ Inno Setup script
```

## rewind-core (runs on any OS)

`config` (settings, presets, validation), `hotkey`, `library` (clip scanning), `trim` (ffmpeg arguments), `estimate`, `toast` (notification placement/easing/anti-aliasing maths), `theme` (palettes and contrast checks), `packs` (add-on index, URL allow-list, zip-slip protection), `voice` (noise gate, phrase matching, Vosk JSON), `perf` (CPU percentage). Keeping this logic free of Windows APIs is what lets CI test it on Linux.

## rewind (Windows)

- **engine**: builds a libobs scene (monitor capture + game-capture layer + audio sources) and a replay-buffer output through libobs-rs. Chooses the scaler and x264 preset by settings and reports whether encoding is on the CPU.
- **ui**: egui. `theme` holds a runtime palette and the animation switch; `widgets` are custom-painted controls; `views` are the pages.
- **overlay**: a native layered Win32 window on its own thread, excluded from capture, sleeping when idle.
- **tray**, **hotkeys**: events wake the UI loop through channels, so a hidden window costs nothing.
- **thumbs**: the Windows shell thumbnail provider on one worker thread.
- **packs / download**: add-on manager, WinHTTP downloader with SHA-256 verification, safe unzip.
- **voice**: loads `libvosk.dll` at run time (nothing linked), captures the mic with WASAPI, gates audio, matches a phrase.
- **perf**: process CPU and memory sampling.

## Release pipeline

`release.yml` builds `Rewind.exe`, runs `Rewind.exe --install-runtime` to fetch and verify the OBS runtime into a staging folder, zips it (portable) and wraps it with Inno Setup (installer). A second job runs `build_packs.py` to build the voice and theme packs and `packs.toml`. The publish job adds checksums and build-provenance attestations and creates the GitHub release.
