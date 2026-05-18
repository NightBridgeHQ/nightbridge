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
- official LocalSend app interop is completed or explicitly scoped down; iOS
  has current bidirectional official-app evidence, while Android and desktop
  official-app coverage still need platform runs or explicit scoping
- release artifacts are reproducible from a clean final `dist/` directory
- checksums and SBOM are generated for the final artifacts
- Docker, systemd, and DEB validation evidence is recorded for representative
  Ubuntu hosts; final release artifacts still need to be regenerated from the
  release commit
- RPM packaging is deferred for 26.5
- desktop signing, notarization, updater, Windows packaging, and Linux desktop
  packaging decisions are explicit; production desktop artifacts are deferred
  and unsigned desktop artifacts must be labeled pre-release
- GUI WebDriver strict mode is scoped out for 26.5 unless `tauri-driver` and an
  interactive host are available before tagging
- security disclosure contact is real: `diego.resendez@zero-oneit.com`
- third-party audit expectations are documented: no completed audit for 26.5,
  public open-source review is welcome, and a paid audit remains future work
