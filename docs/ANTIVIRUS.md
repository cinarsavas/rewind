# Antivirus and SmartScreen warnings

Rewind is a new, **unsigned** app, and that combination is the most common reason for warnings. This page explains what is going on and what you can do. It is not a promise that no scanner will ever complain.

## Why it can get flagged

- **No code-signing certificate yet.** Windows SmartScreen and many scanners treat unsigned programs with little download history as suspicious. Signed programs build "reputation"; new ones start at zero.
- **Rust binaries trigger machine-learning heuristics** more often than you'd expect, simply because they're statically linked and less common than C++/.NET.
- **Rewind legitimately does things malware also does**, and heuristics can't tell the difference:
  - registers global hotkeys (so you can save a clip while a game has focus),
  - draws an always-on-top, click-through window (the notification toast) that is excluded from screen capture,
  - captures the screen and audio,
  - can add a "start with Windows" entry (only if you tick it),
  - downloads optional add-ons (only if you click Download).

## What we do to reduce it

- The installer and portable zip **bundle the OBS runtime**, so Rewind doesn't download executable code on first launch.
- The exe carries a proper icon, version information and manifest.
- Add-on downloads are restricted to HTTPS on GitHub hosts and **checked against a SHA-256** before use; archives are extracted with path-traversal protection.
- Releases are built by a public GitHub workflow, with **build-provenance attestations** and a `SHA256SUMS.txt`.

## Verify your download

```powershell
# 1. Compare the hash with SHA256SUMS.txt from the same release
Get-FileHash .\Rewind-Setup-0.2.0.exe -Algorithm SHA256

# 2. (optional) Verify it was built by this repository's workflow, needs the GitHub CLI
gh attestation verify .\Rewind-Setup-0.2.0.exe --repo cinarsavas/rewind
```

You can also upload the file to <https://www.virustotal.com>. A few detections out of ~70 engines on a new unsigned Rust app is typical for false positives; check which engines flag it and what they call it ("Generic", "ML", "Heur" labels are heuristic guesses).

## If Windows Defender blocks it

1. Windows Security, Virus & threat protection, Protection history: select the item and choose **Allow on device**.
2. Please also **submit it as a false positive** so it's fixed for everyone: <https://www.microsoft.com/en-us/wdsi/filesubmission> (choose "I believe this file is not malware"). Other vendors have similar forms; VirusTotal links to them.

## SmartScreen ("Windows protected your PC")

Click **More info**, then **Run anyway**. This appears for any file without enough reputation and fades as more people run the same signed build.

## For maintainers: getting properly signed

The durable fix is code signing:

- **[SignPath Foundation](https://signpath.org/)** offers free certificates for open-source projects and integrates with GitHub Actions.
- **Azure Trusted Signing** and commercial certificates also work.

After signing, sign the installer *and* `Rewind.exe` in the release workflow. Reputation builds with downloads over time.
