# Sprint 7 Hardening + 26.5 Demo Evidence

Date: 2026-05-17 16:24:58 CDT
Branch: `feature/sprint-7-hardening-26.5`
Commit: `ddc0946`

## Summary

Sprint 7 hardens the native trust gate, records release-readiness checks, and
adds 26.5 release documentation. The short preflight is green on this branch,
but the project is not ready to tag 26.5 because the real 7-day soak, official
LocalSend app matrix, release artifact reproducibility, and signing decisions
are still outstanding.

The preflight and focused loopback tests were run outside the Codex sandbox
because the sandbox blocked a local listener with `Operation not permitted`.
The same focused test passed outside the sandbox, confirming an environment
restriction rather than a code failure.

## Trust Gate

Native production client paths no longer contain `TrustAnyServer`:

```bash
rg -n "TrustAnyServer" \
  crates/protocol-native-v1/src/client.rs \
  crates/protocol-native-v1/src/hole_punch.rs
```

Result: no matches.

Evidence from the preflight and focused tests:

```bash
bash scripts/preflight-26.5.sh
```

Result: PASS.

Covered trust-gate signals:

- `native_server_verifier_accepts_expected_certificate_fingerprint`: PASS
- `native_server_verifier_rejects_mismatched_certificate_fingerprint`: PASS
- `dialer_requires_expected_certificate_fingerprint`: PASS
- `native_wan_rejects_blocked_peer`: PASS
- `native_wan_rejects_prompt_peer`: PASS
- `native_wan_auto_accept_requires_certificate_metadata`: PASS
- `direct_native_requires_certificate_fingerprint`: PASS

Known remaining trust boundary:

- Rendezvous QUIC client/server still use documented `TrustAnyServer` paths for
  control-plane rendezvous behavior and tests. This is documented in
  `docs/security/native-trust-audit.md` and remains a deployment trust-model
  issue, not a native file-transfer data-plane bypass.

## Soak

Short ignored soak test:

```bash
cargo test -p lsi-protocol-native-v1 --test soak_resume -- --ignored --nocapture
```

Result:

```text
test repeated_interruptions_resume_to_matching_file_hash ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out
```

The repeatable short harness and operator docs exist:

- `scripts/native-soak.sh`
- `docs/operations/soak-testing.md`

No real 7-day soak evidence has been run or attached yet. This is a 26.5
release blocker.

## Interop

Local automated LocalSend v2 compatibility smoke passed through preflight:

```bash
bash scripts/localsend-interop-smoke.sh
```

Result:

```text
test local_client_uploads_file_to_local_receiver ... ok
Official LocalSend app matrix is manual unless NBRG_OFFICIAL_LOCALSEND_ARTIFACT
points to a verified headless-capable artifact for this platform.
```

The official-app matrix is documented in `docs/interop/localsend-matrix.md`.
iOS official app send-to-daemon has passed manual testing. The remaining iOS
gap is the reverse acceptance flow: daemon/CLI sending to the official iOS app
and the user accepting the incoming file in the app. Android and official
desktop app verification remain manual and incomplete for tagging 26.5.

## GUI Smoke

GUI smoke gating passed through preflight:

```bash
cargo test --test gui_smoke -- --nocapture
```

Result:

```text
skipping GUI smoke: set NBRG_RUN_GUI_SMOKE=1 to run
test gui_smoke_is_gated ... ok
```

The strict-capable harness and docs exist:

- `scripts/gui-smoke.sh`
- `docs/operations/gui-smoke.md`

`tauri-driver` was not available, so real interactive WebDriver GUI smoke is
still a release validation gap.

## Release Artifacts

Release build:

```bash
cargo build --workspace --release
```

Result: PASS.

Produced local release binaries:

```text
target/release/night-bridge          8.8M
target/release/night-bridge-daemon    12M
target/release/night-bridge-gui      5.0M
target/release/night-bridge-rendezvous              3.8M
target/release/night-bridge-tui                     2.2M
```

WebUI build:

```bash
npm run build --prefix crates/webui
```

Result: PASS.

Packaging tool check:

```bash
bash packaging/build-packages.sh --check-tools
```

Result:

```text
missing: cargo-deb
install: cargo install cargo-deb --locked
missing: cargo-generate-rpm
install: cargo install cargo-generate-rpm --locked
```

Package generation and checksum generation for `dist/` were not run because the
required package tools and final release artifact directory are not ready.

## Docs And Security

Security docs are present:

- `SECURITY.md`
- `docs/security/threat-model.md`
- `docs/security/native-trust-audit.md`
- `docs/security/audit-invite.md`
- `docs/security/rendezvous-privacy.md`
- `docs/security/webui-token-bootstrap.md`

Release and operations docs are present:

- `CHANGELOG.md`
- `docs/release/26.5-notes.md`
- `docs/release/artifacts.md`
- `docs/release/preflight.md`
- `docs/operations/soak-testing.md`
- `docs/operations/gui-smoke.md`
- `docs/interop/localsend-matrix.md`

Docs site build passed previously with:

```bash
mdbook build docs
```

## Remaining Blockers

Sprint 7 can close with a documented 26.5 no-go. A 26.5 release tag should not be
created until:

- real 7-day soak evidence passes and is attached
- official LocalSend app interop is green or explicitly scoped down, especially
  the daemon/CLI-to-official-app acceptance flow
- release artifact generation is reproducible from a clean release directory
- `cargo-deb` and `cargo-generate-rpm` are available or package formats are
  explicitly deferred
- Docker and systemd validation run on representative target hosts
- desktop signing, notarization, updater, Windows, and Linux packaging
  decisions are explicit
- `tauri-driver` GUI smoke runs on an interactive-capable host or is explicitly
  scoped out of the tag
- security disclosure contact and external audit expectations are finalized
