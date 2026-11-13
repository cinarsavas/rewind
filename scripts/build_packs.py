#!/usr/bin/env python3
"""Build the optional add-on packs and the packs.toml index for a release.

Run by the release workflow, it needs internet access and nothing but the Python standard library.

  python scripts/build_packs.py --repo OWNER/rewind --tag v1.0.0 --out dist/packs

Produces, in --out:
  voice-runtime.zip    libvosk.dll + its sibling DLLs (flat, from the official Vosk release)
  voice-<id>.zip       one Vosk language model per language (flat)
  theme-<id>.toml      every file in /themes
  packs.toml           index with a SHA-256 for every file, read by the app (Add-ons page)
"""
import argparse
import hashlib
import io
import pathlib
import shutil
import sys
import tempfile
import urllib.request
import zipfile

VOSK_VERSION = "0.3.45"
VOSK_RUNTIME_URL = f"https://github.com/alphacep/vosk-api/releases/download/v{VOSK_VERSION}/vosk-win64-{VOSK_VERSION}.zip"
RUNTIME_FILES = ["libvosk.dll", "libgcc_s_seh-1.dll", "libstdc++-6.dll", "libwinpthread-1.dll"]

# id -> (display name, model zip name on alphacephei.com). Add more languages here.
LANGUAGES = {
    "en-us": ("English (US)", "vosk-model-small-en-us-0.15"),
    "tr": ("Türkçe", "vosk-model-small-tr-0.3"),
}
MODEL_URL = "https://alphacephei.com/vosk/models/{name}.zip"


def fetch(url: str) -> bytes:
    print(f"  downloading {url}", flush=True)
    req = urllib.request.Request(url, headers={"User-Agent": "rewind-pack-builder"})
    with urllib.request.urlopen(req, timeout=300) as r:
        return r.read()


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def flat_zip(src_dir: pathlib.Path, dest: pathlib.Path) -> None:
    """Zip the *contents* of src_dir (not the folder itself) with forward-slash names."""
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for p in sorted(src_dir.rglob("*")):
            if p.is_file():
                z.write(p, p.relative_to(src_dir).as_posix())


def toml_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True, help="owner/name of the GitHub repo")
    ap.add_argument("--tag", required=True, help="release tag the assets will be uploaded to")
    ap.add_argument("--out", default="dist/packs")
    ap.add_argument("--themes", default="themes")
    args = ap.parse_args()

    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    base = f"https://github.com/{args.repo}/releases/download/{args.tag}"
    lines = ["version = 1", ""]

    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)

        print("Voice runtime")
        z = zipfile.ZipFile(io.BytesIO(fetch(VOSK_RUNTIME_URL)))
        rt = tmp / "runtime"
        rt.mkdir()
        names = {pathlib.PurePosixPath(n).name: n for n in z.namelist()}
        for f in RUNTIME_FILES:
            if f not in names:
                print(f"error: {f} not found in the Vosk release", file=sys.stderr)
                return 1
            (rt / f).write_bytes(z.read(names[f]))
        flat_zip(rt, out / "voice-runtime.zip")
        p = out / "voice-runtime.zip"
        lines += ["[voice_runtime]", f"url = {toml_str(f'{base}/{p.name}')}", f"sha256 = {toml_str(sha256(p))}", f"size_bytes = {p.stat().st_size}", ""]

        for lang, (label, model) in LANGUAGES.items():
            print(f"Voice model {lang}")
            mz = zipfile.ZipFile(io.BytesIO(fetch(MODEL_URL.format(name=model))))
            mdir = tmp / f"model-{lang}"
            mz.extractall(mdir)
            roots = [d for d in mdir.iterdir() if d.is_dir()]
            root = roots[0] if len(roots) == 1 and not (mdir / "am").exists() else mdir
            p = out / f"voice-{lang}.zip"
            flat_zip(root, p)
            lines += ["[[voice]]", f"id = {toml_str(lang)}", f"name = {toml_str(label)}",
                      f"url = {toml_str(f'{base}/{p.name}')}", f"sha256 = {toml_str(sha256(p))}", f"size_bytes = {p.stat().st_size}", ""]

    for t in sorted(pathlib.Path(args.themes).glob("*.toml")):
        text = t.read_text(encoding="utf-8")
        name = next((l.split("=", 1)[1].strip().strip('"') for l in text.splitlines() if l.startswith("name")), t.stem)
        author = next((l.split("=", 1)[1].strip().strip('"') for l in text.splitlines() if l.startswith("author")), "")
        swatch = next((l.split("=", 1)[1].strip().strip('"') for l in text.splitlines() if l.startswith("accent")), "")
        dest = out / f"theme-{t.stem}.toml"
        shutil.copyfile(t, dest)
        lines += ["[[theme]]", f"id = {toml_str(t.stem)}", f"name = {toml_str(name)}", f"author = {toml_str(author)}", f"swatch = {toml_str(swatch)}",
                  f"url = {toml_str(f'{base}/{dest.name}')}", f"sha256 = {toml_str(sha256(dest))}", f"size_bytes = {dest.stat().st_size}", ""]

    (out / "packs.toml").write_text("\n".join(lines), encoding="utf-8")
    print(f"Wrote {len(list(out.iterdir()))} files to {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
