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
| Docker image | `night-bridge:<version>` | Docker build job | Final `night-bridge:26.5.0` smoke passed on Ubuntu host |
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
- Host: a representative Ubuntu Docker host
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
- Host: a representative Ubuntu systemd host
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

Final 26.5 artifact generation:

- Date: 2026-05-28
- Artifact source commit: `05af10d`
- Directory: `/private/tmp/nightbridge-dist-05af10d-macos-arm64`
- Assets:
  - `nightbridge-26.5.0-alpha-macos-arm64.tar.gz`
  - `nightbridge-26.5.0-alpha-linux-amd64.tar.gz`
  - `nightbridge-26.5.0-alpha-linux-arm64.tar.gz`
  - `night-bridge-daemon_26.5.0-1_amd64.deb`
  - `SHA256SUMS`
  - `sbom.cdx.json`
- SHA-256:
  - `0d70428b7c403c32a56a7f0763ec33d98d17517719705589fefe320cb9f9a977  night-bridge-daemon_26.5.0-1_amd64.deb`
  - `8b7791ba284333055f987a7cabe3880ddf1b6e59f0bedb2d3886d5bcb6d0d5a8  nightbridge-26.5.0-alpha-linux-amd64.tar.gz`
  - `85cc9f6bddffb7ef35a37bc7a8767989af2b57f91db135727640f8a37b166148  nightbridge-26.5.0-alpha-linux-arm64.tar.gz`
  - `6c4909938353fd548d13c1feab6db07a500b6ba32dca2728d453d2f820b84860  nightbridge-26.5.0-alpha-macos-arm64.tar.gz`
  - `13bd095e125e2519a3e0db159adc2671ff601b324da60601a08806a4be6a41d1  sbom.cdx.json`
- Result: PASS; `shasum -a 256 -c SHA256SUMS` verified every listed asset
- Notes: `sbom.cdx.json` is the fallback minimal CycloneDX output from
  `packaging/release/sbom.sh`.

Final Docker smoke:

- Date: 2026-05-28
- Host: a representative Ubuntu Docker host
- Commit: `05af10d`
- Docker server: `29.1.3`
- Image: `night-bridge:26.5.0`
- Image ID:
  `sha256:1a53e8b3e133342aa1d2f8186366e33716026f5ef9b35728582ec04924c25239`
- Image size: `43690136`
- Command: `NIGHTBRIDGE_DOCKER_IMAGE=night-bridge:26.5.0 bash packaging/docker/smoke.sh`
- Result: PASS
- Evidence log:
  `~/nightbridge-release/<commit>/target/release-evidence/docker/smoke-26.5.0.log`

Final Debian package and systemd smoke:

- Date: 2026-05-28
- Host: a representative Ubuntu systemd host
- Commit: `05af10d`
- Package:
  `~/nightbridge-release/<commit>/target/debian/night-bridge-daemon_26.5.0-1_amd64.deb`
- Package size: `4508660`
- Package was copied into the final local release asset directory and covered
  by `SHA256SUMS`.
- Installed with `dpkg -i`, verified with `systemd-analyze verify`, restarted
  with `systemctl restart night-bridge.service`, reported `active`, and
  `/healthz` on `127.0.0.1:53501` passed on retry attempt 2.
- Service was stopped after validation.
- Evidence logs:
  - `~/nightbridge-release/<commit>/target/release-evidence/systemd-deb/build-26.5.0.log`
  - `~/nightbridge-release/<commit>/target/release-evidence/systemd-deb/install-systemd-26.5.0.log`

Final GitHub release and installer smoke:

- Date: 2026-05-28
- Tag: `26.5.0-alpha`
- URL: `https://github.com/NightBridgeHQ/nightbridge/releases/tag/26.5.0-alpha`
- Release type: pre-release
- Uploaded assets:
  - `nightbridge-26.5.0-alpha-linux-amd64.tar.gz`
  - `nightbridge-26.5.0-alpha-linux-arm64.tar.gz`
  - `nightbridge-26.5.0-alpha-macos-arm64.tar.gz`
  - `night-bridge-daemon_26.5.0-1_amd64.deb`
  - `SHA256SUMS`
  - `sbom.cdx.json`
  - `install.sh`
- Installer smoke: PASS on 2026-05-28T07:02:15Z using the public
  `raw.githubusercontent.com/NightBridgeHQ/nightbridge/main/install.sh`
  command with `--version 26.5.0-alpha`.
