# Publishing Rewind on GitHub

You need a GitHub account and (optionally) the [GitHub CLI](https://cli.github.com/).

## 1. Create the repository

```powershell
cd rewind
git init -b main
git add .
git commit -m "Initial commit"
gh repo create rewind --public --source . --push
```

Or create an empty repo on github.com and `git remote add origin ... ; git push -u origin main`.

## 2. Point the docs at your repo

Replace the `OWNER` placeholder (README badges and links, `docs/ANTIVIRUS.md`) with your GitHub username:

```powershell
Get-ChildItem -Recurse -Include *.md,*.yml | ForEach-Object {
  (Get-Content $_ -Raw) -replace 'OWNER/rewind', 'YOUR-USERNAME/rewind' | Set-Content $_ -NoNewline
}
git commit -am "Set repository owner"; git push
```

You do **not** need to edit any code: release builds read the repository name from `GITHUB_REPOSITORY`, so the app's Add-ons page automatically downloads from *your* releases. A local `cargo run` falls back to the upstream release feed; set `REWIND_REPO=YOUR-USERNAME/rewind` to point a local build at your own releases.

## 3. Turn on the checks

In the repo's **Settings, Actions, General**, allow workflows to have **read and write permissions** (the release workflow needs to create releases and attestations). CI runs on every push.

## 4. Cut a release

1. Update `CHANGELOG.md`: add a `## [x.y.z]` section (it becomes the release notes) and bump `version` in `Cargo.toml`.
2. Tag and push:

```powershell
git tag v0.2.1
git push origin v0.2.1
```

The **Release** workflow then builds `Rewind.exe`, bundles the OBS runtime, produces the installer and portable zip, builds the add-on packs, computes checksums, attests provenance and publishes the GitHub release. Expect about 15 to 25 minutes.

Run it from the **Actions** tab (*Run workflow*) first to get a test build without publishing.

## 5. Make the installer yourself (optional)

```powershell
cargo build --release -p rewind
New-Item -ItemType Directory -Force dist\stage | Out-Null
Copy-Item target\release\rewind.exe dist\stage\Rewind.exe
Start-Process dist\stage\Rewind.exe -ArgumentList "--install-runtime" -Wait   # downloads the OBS runtime
Copy-Item LICENSE, README.md, THIRD_PARTY_NOTICES.md dist\stage\
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" /DMyAppVersion=0.2.1 installer\rewind.iss
```

## 6. Reduce antivirus warnings

Read [ANTIVIRUS.md](ANTIVIRUS.md). Apply for free signing at SignPath Foundation, and submit false-positive reports to Microsoft for each release.

## Maintaining voice-pack languages

Edit `LANGUAGES` in `scripts/build_packs.py` to add Vosk models (see <https://alphacephei.com/vosk/models>). They are re-hosted as release assets with checksums on the next release.
