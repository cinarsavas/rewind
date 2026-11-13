# Contributing

Thanks for helping! Bug reports on real hardware are the most valuable contribution right now.

## Reporting bugs

Use the **Bug report** issue template. Please include your Windows version, CPU/GPU, and what the **THIS APP** card on the Replay page shows. For antivirus false positives see [docs/ANTIVIRUS.md](docs/ANTIVIRUS.md).

## Development

```powershell
cargo test -p rewind-core           # any OS, fast
cargo clippy -p rewind --release    # Windows
cargo run -p rewind                 # Windows; downloads the OBS runtime on first run
```

- Put anything that doesn't need Windows APIs in `crates/core` and add unit tests; CI runs them on Linux.
- Keep the UI cheap: avoid unconditional `request_repaint`, prefer waking via channels, and respect the **animations** and **low-power** settings (`theme::anim_time`).
- New downloads must go through `packs::allowed_download_url` and a SHA-256.
- Themes: add `themes/<id>.toml`; `cargo test -p rewind-core` validates it.

## Pull requests

Describe what changed and how you tested it. Small, focused PRs are easiest to review. By contributing you agree to license your work under GPL-3.0-or-later.
