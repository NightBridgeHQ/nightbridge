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

## Alpha Release Evidence

NightBridge `26.5.0-alpha` was tagged and published after:

- final `scripts/preflight-26.5.sh` evidence passes on the release commit and
  is attached to the release notes
- real 7-day native soak evidence passes
- delta soak evidence passes for the post-soak dependency security refresh
- official LocalSend app interop evidence remains attached to the release
  notes; current 26.5 manual bidirectional evidence covers Android, iOS,
  Windows, macOS, and Linux, while future automated official-app coverage
  remains optional unless a headless-capable artifact is available
- release artifacts are reproducible from a clean final `dist/` directory
- checksums and SBOM are generated for the final artifacts
- Docker validation evidence was recorded for a representative Ubuntu host
- Debian package and systemd validation evidence is recorded for a
  representative Ubuntu host; the `.deb` is distributed as a GitHub release
  asset, not through an APT/PPA repository
- RPM packaging is deferred for 26.5
- APT/PPA distribution and Homebrew distribution are deferred for 26.5
- desktop signing, notarization, updater, Windows packaging, and Linux desktop
  packaging decisions are explicit; production desktop artifacts are deferred
  and unsigned desktop artifacts must be labeled pre-release
- GUI WebDriver strict mode is scoped out for 26.5 unless `tauri-driver` and an
  interactive host are available for a later validation pass
- security disclosure contact is real: `diego.resendez@zero-oneit.com`
- third-party audit expectations are documented: no completed audit for 26.5,
  public open-source review is welcome, and a paid audit remains future work

## Published Alpha Scope

NightBridge `26.5.0-alpha` is published. Feature scope is closed for the
`26.5.0-alpha` train.

Allowed follow-up changes:

- bug fixes
- release-blocker fixes
- security fixes
- documentation corrections
- packaging, installer, and release evidence improvements
- small enhancements that reduce release risk or improve the existing
  pre-release experience without adding a new product surface

Not allowed on this alpha train:

- new alpha features
- new public protocol surfaces
- new distribution channels
- new platform packaging promises
- broad refactors unrelated to a release blocker
