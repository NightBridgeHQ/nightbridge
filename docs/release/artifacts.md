# Release Artifacts

Sprint 7 defines the expected 1.0 release artifact set. Some artifacts require
platform credentials or local tools and may remain documented gaps until those
are available.

## Artifact Matrix

| Artifact | Expected path or name | Produced by | Status |
| --- | --- | --- | --- |
| Linux CLI binary | `localsend-improved` | release workflow Rust build | Required |
| Linux daemon binary | `localsend-improved-daemon` | release workflow Rust build | Required |
| Linux TUI binary | `lsi-tui` | release workflow Rust build | Required |
| macOS CLI binary | `localsend-improved` | release workflow Rust build | Required |
| macOS daemon binary | `localsend-improved-daemon` | release workflow Rust build | Required |
| macOS TUI binary | `lsi-tui` | release workflow Rust build | Required |
| Windows CLI binary | `localsend-improved.exe` | release workflow Rust build | Required |
| Windows daemon binary | `localsend-improved-daemon.exe` | release workflow Rust build | Required |
| Windows TUI binary | `lsi-tui.exe` | release workflow Rust build | Required |
| Tauri desktop bundles | platform-specific app bundle, installer, or archive | Tauri build job | Required before desktop GA |
| Docker image | `localsend-improved:<version>` | Docker build job | Required |
| Debian package | `.deb` | `packaging/build-packages.sh` | Required when `cargo-deb` is available |
| RPM package | `.rpm` | `packaging/build-packages.sh` | Required when `cargo-generate-rpm` is available |
| Checksums | `dist/SHA256SUMS` | `packaging/release/checksums.sh` | Always generated |
| SBOM | `dist/sbom.cdx.json` | `packaging/release/sbom.sh` | Always generated, may be fallback metadata |
| Signatures | detached signatures or platform signatures | signing jobs | Opt-in only |

## Checksums

`packaging/release/checksums.sh dist` writes SHA-256 checksums for files in
`dist/`. Checksums are always generated, even when signing is not configured.

## SBOM

`packaging/release/sbom.sh dist` writes `dist/sbom.cdx.json`. If `cyclonedx`
or `cyclonedx-rust-cargo` is available, the script uses it. Otherwise it emits
a clearly marked minimal CycloneDX document so the release records that a full
tool-generated SBOM is still pending.

## Signing And Notarization

Signing is opt-in:

- local `workflow_dispatch` runs do not sign by default
- macOS notarization requires Apple Developer credentials and signing identity
- Windows Authenticode requires certificate secrets
- detached artifact signatures require a configured signing key

Unsigned artifacts must remain labeled pre-release until platform signing and
notarization are wired for the release channel.

## Evidence To Preserve

- workflow run URL
- commit SHA and tag
- `dist/SHA256SUMS`
- `dist/sbom.cdx.json`
- package build logs
- signing/notarization logs when enabled
- explicit list of missing platform credentials or tools
