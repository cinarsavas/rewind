# Security policy

## Reporting a vulnerability

Please **don't open a public issue** for security problems. Use GitHub's *Report a vulnerability* button (Security tab, Advisories) so it can be fixed before details are public.

## Scope and design notes

- Rewind has no telemetry and no accounts. The only network use is downloading optional add-ons and, in source builds, the OBS runtime.
- Add-on downloads: HTTPS only, restricted to GitHub hosts, verified against a SHA-256 from the release index; archives are extracted with path-traversal and size limits.
- The optional voice add-on loads `libvosk.dll` from the add-ons folder in your user profile.
- Releases are built by public workflows and carry build-provenance attestations (see [docs/ANTIVIRUS.md](docs/ANTIVIRUS.md)).
