# Release Artifacts

Sprint 7 defines the expected 26.5 release artifact set. Some artifacts require
platform credentials or local tools and may remain documented gaps until those
are available.

## Artifact Matrix

| Artifact | Expected path or name | Produced by | Status |
| --- | --- | --- | --- |
| Linux CLI binary | `night-bridge` | release workflow Rust build | Required |
| Linux daemon binary | `night-bridge-daemon` | release workflow Rust build | Required |
| Linux TUI binary | `night-bridge-tui` | release workflow Rust build | Required |
| macOS CLI binary | `night-bridge` | release workflow Rust build | Required |
| macOS daemon binary | `night-bridge-daemon` | release workflow Rust build | Required |
| macOS TUI binary | `night-bridge-tui` | release workflow Rust build | Required |
| Windows CLI binary | `night-bridge.exe` | release workflow Rust build | Required |
| Windows daemon binary | `night-bridge-daemon.exe` | release workflow Rust build | Required |
| Windows TUI binary | `night-bridge-tui.exe` | release workflow Rust build | Required |
| GitHub release tarballs | `nightbridge-<version>-<os>-<arch>.tar.gz` | release workflow or local final `dist/` | Required for curl installer |
| Tauri desktop bundles | platform-specific app bundle, installer, or archive | Tauri build job | Deferred for production 26.5; unsigned builds are pre-release only |
| Docker image | `night-bridge:<version>` | Docker build job | Validated on Ubuntu host; final tag still required |
| Debian package | `.deb` | `packaging/build-packages.sh --deb-only` | Required as a GitHub release asset; no APT/PPA repo |
| RPM package | `.rpm` | `packaging/build-packages.sh` | Deferred for 26.5 |
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

Signing is deferred for 26.5 desktop artifacts:

- local `workflow_dispatch` runs do not sign by default
- macOS notarization requires Apple Developer Program membership, a Developer
  ID Application certificate, Apple account credentials, and Team ID metadata
- Windows signing requires an Authenticode certificate from a trusted CA or a
  managed service such as Azure Artifact Signing
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

## Local Host Evidence

Docker smoke:

- Date: 2026-05-17
- Host: `link` (`10.16.20.130`)
- Commit: `1d18b86`
- Docker server: `29.1.3`
- Command: `bash packaging/docker/smoke.sh`
- Result: image `night-bridge:sprint4` built and
  `night-bridge-daemon --help` ran successfully
- Image ID:
  `sha256:6aaae986812a532abd441ae43ddf1d2308e3266dbaf3f4f1f295331686d70ca1`
- Image size: `44077739`
- Evidence log:
  `~/nightbridge-release/1d18b86/target/release-evidence/docker/smoke.log`

Debian package and systemd smoke:

- Date: 2026-05-18
- Host: `zelda` (`10.16.20.129`)
- Commit: `1d18b86`
- Tooling: `rustc 1.78.0`, `cargo 1.78.0`, `cargo-deb 3.6.2`,
  `libprotoc 3.21.12`
- Note: this evidence predates the dependency security refresh that raised
  the workspace MSRV to Rust 1.85; regenerate package evidence before release.
- Package:
  `target/debian/night-bridge-daemon_26.5.0-1_amd64.deb`
- Package size: `4523220`
- Package contents verified with `dpkg-deb -I` and `dpkg-deb -c`
- Installed with `dpkg -i`, verified with `systemd-analyze verify`, started
  through `systemctl start night-bridge.service`, reported `active`, and
  `/healthz` on `127.0.0.1:53501` returned success
- Service was stopped after validation
- Evidence logs:
  `~/nightbridge-release/1d18b86/target/release-evidence/systemd-deb/deb-build-retry.log`
  and
  `~/nightbridge-release/1d18b86/target/release-evidence/systemd-deb/install-systemd.log`
- Notes: final 26.5 public distribution includes the `.deb` as a GitHub release
  asset. APT and PPA repository distribution are deferred.

Release script smoke:

- Date: 2026-05-17
- Host: local macOS workstation
- Command:
  `packaging/release/sbom.sh /private/tmp/nightbridge-dist-smoke` and
  `packaging/release/checksums.sh /private/tmp/nightbridge-dist-smoke`
- Result: fallback CycloneDX SBOM and `SHA256SUMS` were generated for a
  temporary artifact directory
- Notes: final release must rerun these scripts against the clean final
  `dist/` directory on the release commit

Current release-script rehearsal:

- Date: 2026-05-21
- Host: local macOS workstation
- Commit: `9f99bbd`
- Directory: `/private/tmp/nightbridge-dist-rehearsal-9f99bbd-macos-arm64`
- Scope: non-final macOS artifact rehearsal only; final release artifacts must
  still be regenerated from the final release commit after soak
- Artifacts:
  `night-bridge`, `nbrg`, `night-bridge-daemon`, and `night-bridge-tui`
- Command:
  `packaging/release/sbom.sh /private/tmp/nightbridge-dist-rehearsal-9f99bbd-macos-arm64`
  and
  `packaging/release/checksums.sh /private/tmp/nightbridge-dist-rehearsal-9f99bbd-macos-arm64`
- Result: PASS; fallback CycloneDX SBOM and clean relative-path `SHA256SUMS`
  were generated, and `shasum -a 256 -c SHA256SUMS` verified every file
