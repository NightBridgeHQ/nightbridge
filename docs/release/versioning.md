# Versioning

NightBridge uses CalVer in `YY.M.PATCH` format.

For example, a release prepared in May 2026 starts at `26.5.0`. Patch releases
for that release train increment the final component: `26.5.1`, `26.5.2`, and
so on.

## Version Bump Rule

Do not bump versions for ordinary commits, feature work, merges, or pushes.
Version fields stay unchanged until a release is being prepared.

Version changes are allowed only during release preparation, when the release
owner is ready to:

- pick the CalVer release number
- update Cargo, WebUI, Tauri, Python SDK, and TypeScript SDK manifests
- update changelog and release notes
- run release verification
- tag the release if all release blockers are cleared

## Current Release Blockers

NightBridge `26.5.0` must not be tagged until:

- final `scripts/preflight-26.5.sh` evidence passes on the release commit and
  is attached to the release notes
- real 7-day native soak evidence passes
- official LocalSend app interop is completed or explicitly scoped down,
  especially daemon/CLI-to-official-app acceptance
- official LocalSend app send-to-daemon is retested with trusted LocalSend
  receive policy and a persistent fingerprint allowlist file
- release artifacts are reproducible from a clean final `dist/` directory
- checksums and SBOM are generated for the final artifacts
- Docker and systemd validation run on representative target hosts or are
  explicitly deferred
- DEB/RPM package status is decided after `cargo-deb` and
  `cargo-generate-rpm` are available, or those package formats are deferred
- desktop signing, notarization, updater, Windows packaging, and Linux desktop
  packaging decisions are explicit
- GUI WebDriver smoke runs on an interactive-capable host with `tauri-driver`,
  or that check is explicitly scoped out
- security disclosure contact is real
- third-party audit expectations are documented
