# Sprint 7 Retro: Hardening + 1.0

Date: 2026-05-17
Branch: `feature/sprint-7-hardening-1.0`
Head commit at evidence time: `6b90923`

## What Shipped

- Native production client paths now verify trusted peer certificate
  fingerprints instead of accepting arbitrary server certificates.
- Daemon native send policy rejects unknown, blocked, prompt-only, and missing
  native certificate metadata cases before dialing.
- Direct native URL sends require explicit certificate fingerprint material.
- Native trust audit documents production data-plane fixes and remaining
  rendezvous control-plane trust boundaries.
- Security threat model, security policy, and external audit invite packet.
- Repeatable native soak harness and operator soak-testing docs.
- LocalSend interop matrix and automated local interop smoke.
- Strict-capable GUI smoke harness that can run real Tauri checks when
  `tauri-driver` and explicit opt-in are available.
- Release artifact documentation with checksum, SBOM, package, signing, and
  updater boundaries.
- Operator guide, user quickstart, migration guide, release notes draft,
  changelog, preflight docs, and mdBook docs-site skeleton.
- Generated sprint planning artifacts moved from tracked product docs to
  ignored `specs/`, while canonical user/operator/security/release docs remain
  under tracked `docs/`.
- Sprint 7 demo evidence recorded in
  `specs/superpowers/demos/sprint-7-hardening-1.0.md`.

## Trust Gate

Final native production trust check:

```bash
rg -n "TrustAnyServer" \
  crates/protocol-native-v1/src/client.rs \
  crates/protocol-native-v1/src/hole_punch.rs
```

Result: no matches.

Relevant passing tests from preflight:

- native certificate verifier accepts expected certificate fingerprint
- native certificate verifier rejects mismatched certificate fingerprint
- WAN candidate dialer requires expected certificate fingerprint
- daemon native WAN rejects blocked peers
- daemon native WAN rejects prompt-only peers
- daemon native WAN requires native certificate metadata for auto-accept
- CLI direct native sends require explicit certificate fingerprint material

Rendezvous still has documented `TrustAnyServer` usage for control-plane
rendezvous behavior and tests. That is outside the native file-transfer
data-plane fix and remains a deployment trust-model risk before a public
production release.

## Soak

Short ignored soak evidence passed:

```bash
cargo test -p lsi-protocol-native-v1 --test soak_resume -- --ignored --nocapture
```

Result: `repeated_interruptions_resume_to_matching_file_hash` passed.

The repeatable harness exists at `scripts/native-soak.sh`, and operator guidance
lives in `docs/operations/soak-testing.md`.

No real 7-day soak was run in this sprint session. This blocks a 1.0 release
tag.

## Interop

Automated local LocalSend v2 compatibility smoke passed through preflight:

```bash
bash scripts/localsend-interop-smoke.sh
```

Result: `local_client_uploads_file_to_local_receiver` passed.

Official LocalSend app verification remains manual and incomplete. The matrix
is documented in `docs/interop/localsend-matrix.md`.

## GUI Smoke

GUI smoke gating passed:

```bash
cargo test --test gui_smoke -- --nocapture
```

Result: `gui_smoke_is_gated` passed and skipped the real GUI run because
`LSI_RUN_GUI_SMOKE=1` was not set.

The harness is strict-capable when `tauri-driver` exists, but real interactive
WebDriver GUI smoke was not run on this host.

## Release Artifacts

Preflight passed:

```bash
bash scripts/preflight-1.0.sh
```

The preflight covered:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `npm run check --prefix crates/webui`
- `npm run build --prefix crates/webui`
- focused native soak hash test
- GUI smoke gating
- LocalSend interop smoke

Release build passed:

```bash
cargo build --workspace --release
```

Local release binaries were produced for:

- `localsend-improved`
- `localsend-improved-daemon`
- `localsend-improved-gui`
- `lsi-rendezvous`
- `lsi-tui`

WebUI production build passed:

```bash
npm run build --prefix crates/webui
```

Package tool check reported missing local tools:

```text
missing: cargo-deb
install: cargo install cargo-deb --locked
missing: cargo-generate-rpm
install: cargo install cargo-generate-rpm --locked
```

DEB/RPM generation, Docker validation, systemd validation, final checksums, and
signed desktop packages were not completed.

## Docs Evidence

Tracked docs now cover:

- user quickstart and migration
- operator guide
- WAN rendezvous deployment and privacy
- desktop packaging
- GUI smoke
- native soak
- LocalSend interop matrix
- release artifacts
- release preflight
- 1.0 release notes draft
- changelog
- security policy
- threat model
- native trust audit
- audit invite packet

`mdbook build docs` passed after the docs-site skeleton was added.

## Go/No-Go

Sprint 7 status: **GO to close Sprint 7**.

1.0 release tag status: **NO-GO**.

The sprint deliverables are implemented and verified, but the release should not
be tagged until the explicit 1.0 blockers below are resolved or formally scoped
out in release notes.

## Commit Hygiene

Checked:

```bash
git log --format='%h %s%n%B' main..HEAD
git rev-list --count main..HEAD
git log --format='%B' main..HEAD | rg -c '^Signed-off-by:'
git log --format='%B' main..HEAD | rg -n 'Co-Authored-By'
```

Result:

- 16 Sprint 7 commits
- 16 `Signed-off-by` trailers
- no `Co-Authored-By` trailers

## Remaining Risks

- Real 7-day native soak has not passed.
- Official LocalSend app interop matrix is not green yet.
- Release artifacts are not proven reproducible from a clean final `dist/`
  directory.
- `cargo-deb` and `cargo-generate-rpm` are missing locally, so package builds
  were not run.
- Docker daemon validation was not run.
- systemd validation was not run on a Linux host.
- Desktop signing, notarization, updater, Windows packaging, and Linux
  packaging decisions remain open.
- Real GUI WebDriver smoke requires `tauri-driver` and an interactive-capable
  host.
- Rendezvous control-plane certificate trust model still needs a production
  operator decision before public deployment.
- Security disclosure contact and external audit expectations need final
  release-owner confirmation.
