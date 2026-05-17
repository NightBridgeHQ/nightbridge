# Sprint 7 Hardening + 1.0 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Use `superpowers:test-driven-development` for code changes, `superpowers:systematic-debugging` for failures, and `superpowers:subagent-driven-development` only when explicitly asked to parallelize.

**Goal:** Stabilize LocalSend Improved for a credible 1.0 candidate with security hardening, repeatable soak/interop verification, release artifacts, operator docs, and clear remaining-risk disclosure.

**Architecture:** Keep the daemon and protocol crates as the security boundary. Harden native transfers by binding trusted peer identity to Ed25519 fingerprints and TLS certificate material before investing in release automation. Treat platform packaging, docs, and release metadata as downstream work that can proceed only after the production trust gate and baseline verification are green.

**Tech Stack:** Rust 1.78, Quinn/rustls, Svelte/Vite WebUI, Tauri 2, GitHub Actions, Docker, existing packaging scripts, Markdown docs, optional mdBook for docs site.

---

## Sprint 7 Deliverable Boundary

**Must ship by end of Sprint 7:**

1. Native production send paths no longer use `TrustAnyServer` or equivalent trust bypass.
2. Native client certificate verification binds the server certificate to the expected trusted peer identity.
3. Daemon/API send paths reject unknown and blocked native peers by default.
4. Negative tests cover unknown peer, blocked peer, mismatched advertised fingerprint, and mismatched certificate identity.
5. A repeatable soak harness exists for 7-day transfer/resume runs, with docs for local execution and evidence capture.
6. LocalSend interop matrix is formalized and at least one automated official-artifact or documented manual matrix path exists.
7. GUI smoke path is executable on machines with `tauri-driver`; missing tooling remains a clear skip, not a false pass.
8. Threat model, operator guide, migration guide, security policy, changelog, and release notes exist.
9. Release workflow produces or documents binaries, checksums, SBOMs, signatures, and platform gaps.
10. Sprint 7 demo evidence and retro exist.

**Explicitly not required for Sprint 7 closure unless credentials/tooling are available:**

- Completed third-party audit report.
- Production Apple Developer notarization.
- Production Windows Authenticode signing.
- Public relay/VIP fallback.
- A hosted docs site if GitHub Pages or DNS is not configured locally.

## Carry-Forward Constraints

- Rust MSRV remains `1.78.0` unless the plan is explicitly amended.
- Do not weaken LocalSend v2 compatibility by applying native trust rules to LocalSend's self-signed compatibility mode.
- Do not put API tokens, signing keys, updater private keys, Apple credentials, Authenticode certificates, or PGP private keys in git.
- Keep docs in English; conversation can remain Spanish.
- Preserve DCO sign-off on every commit and do not add `Co-Authored-By`.
- Generated planning artifacts under `specs/` are ignored by git; use `git add -f` for new plans, demos, and retros.

## Execution Order

1. Baseline verification and Sprint 7 branch setup.
2. Native trust-gate audit and failing tests.
3. Production native TLS verifier and daemon trust enforcement.
4. Soak harness and long-run evidence capture.
5. LocalSend interop matrix and GUI smoke.
6. Release workflow/artifact hardening.
7. Security/operator/user docs.
8. Final 1.0 evidence, changelog, release notes, and retro.

## Safe Parallelization Notes

Safe to parallelize after Task 7.3 lands:

- Documentation tasks can run beside soak/interop harness work.
- Release workflow/SBOM work can run beside GUI smoke harness improvements.
- Changelog/release-notes drafting can run beside final verification.

Keep serial:

- Baseline verification before code changes.
- Native trust-gate tests before implementation.
- Native TLS verifier before daemon/API production send hardening.
- Release artifacts after trust-gate and baseline tests pass.
- Final demo/retro after all code/docs/workflows are committed.

## Task 7.1: Branch And Baseline Verification

**Files:**
- No code changes unless verification exposes an immediate blocker

**Step 1: Start the feature branch**

Run:

```bash
git switch -c feature/sprint-7-hardening-1.0
```

Expected: new branch created from `main`.

**Step 2: Verify clean baseline**

Run:

```bash
git status --short --branch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check --prefix crates/webui
npm run build --prefix crates/webui
```

Expected: all pass. `cargo fmt` may print existing warnings for nightly-only rustfmt options under Rust 1.78; exit code must be 0.

**Step 3: Record baseline if needed**

If any command fails because of environment only, create a short note in the first demo evidence draft later. Do not commit a code workaround before root-causing it.

## Task 7.2: Audit Native Trust Bypass Surface

**Files:**
- Create: `docs/security/native-trust-audit.md`
- Inspect: `crates/protocol-native-v1/src/client.rs`
- Inspect: `crates/protocol-native-v1/src/hole_punch.rs`
- Inspect: `crates/rendezvous/src/client.rs`
- Inspect: `crates/rendezvous/src/server.rs`
- Inspect: `crates/protocol-localsend-v2/src/client.rs`
- Inspect: `crates/protocol-localsend-v2/src/discovery.rs`
- Inspect: `crates/protocol-localsend-v2/src/server.rs`

**Step 1: Search for trust bypasses**

Run:

```bash
rg -n "TrustAny|dangerous\\(|danger_accept_invalid_certs|with_custom_certificate_verifier" crates
```

Expected: every hit is classified as native production path, LocalSend compatibility path, rendezvous test/internal path, or test-only helper.

**Step 2: Write the audit document**

Create `docs/security/native-trust-audit.md` with these sections:

```markdown
# Native Trust Audit

## Production Native Paths

| File | Current behavior | Required Sprint 7 outcome |
| --- | --- | --- |
| `crates/protocol-native-v1/src/client.rs` | Uses `TrustAnyServer` for native direct sends. | Verify server certificate identity against the expected trusted peer. |
| `crates/protocol-native-v1/src/hole_punch.rs` | Uses `TrustAnyServer` for WAN candidate dialing. | Verify server certificate identity against the expected trusted peer before transfer. |

## Compatibility Exceptions

LocalSend v2 HTTPS compatibility may continue to accept official-app self-signed TLS behavior where explicitly documented. These paths must not be reused by native protocol sends.

## Rendezvous Boundary

Rendezvous is control-plane only. Any test-only trust bypass must stay scoped to rendezvous tests or be replaced with explicit certificate pinning before public deployment.

## Required Negative Tests

- Unknown native peer is rejected.
- Blocked native peer is rejected.
- Advertised native fingerprint mismatch is rejected.
- Server TLS certificate identity mismatch is rejected.
- Trusted auto-accept peer succeeds.
```

**Step 3: Verify audit coverage**

Run:

```bash
rg -n "Production Native Paths|Compatibility Exceptions|Rendezvous Boundary|Required Negative Tests|TrustAnyServer" docs/security/native-trust-audit.md
```

Expected: all sections and at least the native `TrustAnyServer` references are present.

**Step 4: Commit**

```bash
git add -f docs/security/native-trust-audit.md
git commit -s -m "docs(security): audit native trust bypasses"
```

## Task 7.3: Add Native Certificate Identity Test Harness

**Files:**
- Modify: `crates/protocol-native-v1/src/tls.rs`
- Modify: `crates/protocol-native-v1/src/client.rs`
- Modify: `crates/protocol-native-v1/src/hole_punch.rs`
- Test: `crates/protocol-native-v1/src/client.rs`
- Test: `crates/protocol-native-v1/src/hole_punch.rs`

**Step 1: Add a certificate fingerprint assertion helper test**

In `crates/protocol-native-v1/src/tls.rs`, add or extend tests to prove certificate DER fingerprints are stable and can be compared by callers:

```rust
#[test]
fn certificate_fingerprint_changes_for_distinct_identities() {
    let first = NativeTlsIdentity::generate("first").unwrap();
    let second = NativeTlsIdentity::generate("second").unwrap();

    assert_ne!(first.fingerprint_sha256_hex(), second.fingerprint_sha256_hex());
}
```

**Step 2: Add failing client verifier tests**

In `crates/protocol-native-v1/src/client.rs`, add tests for the new verifier shape before implementing it:

```rust
#[test]
fn native_server_verifier_accepts_expected_certificate_fingerprint() {
    let identity = crate::tls::NativeTlsIdentity::generate("trusted").unwrap();
    let verifier = NativeServerVerifier::new(identity.fingerprint_sha256_hex());

    assert!(verifier.verify_certificate_fingerprint(&identity.cert_der).is_ok());
}

#[test]
fn native_server_verifier_rejects_mismatched_certificate_fingerprint() {
    let expected = crate::tls::NativeTlsIdentity::generate("trusted").unwrap();
    let actual = crate::tls::NativeTlsIdentity::generate("attacker").unwrap();
    let verifier = NativeServerVerifier::new(expected.fingerprint_sha256_hex());

    let error = verifier.verify_certificate_fingerprint(&actual.cert_der).unwrap_err();
    assert!(error.to_string().contains("native peer certificate fingerprint mismatch"));
}
```

Expected initially: FAIL because `NativeServerVerifier` does not exist.

**Step 3: Add failing WAN dialer signature tests**

In `crates/protocol-native-v1/src/hole_punch.rs`, add a compile-level test or unit test that calls the new expected fingerprint API:

```rust
#[test]
fn dialer_requires_expected_certificate_fingerprint() {
    let expected = "abcd".to_string();
    let config = NativeDialTrust::new(expected.clone());

    assert_eq!(config.expected_certificate_fingerprint(), expected);
}
```

Expected initially: FAIL because `NativeDialTrust` does not exist.

**Step 4: Run failing tests**

Run:

```bash
cargo test -p lsi-protocol-native-v1 native_server_verifier --no-default-features
cargo test -p lsi-protocol-native-v1 dialer_requires_expected_certificate_fingerprint --no-default-features
```

Expected: FAIL at compile/test level for missing verifier types.

**Step 5: Commit tests only if the project allows red commits**

This repo has so far committed passing increments. Do not commit failing tests alone. Keep them in the worktree for Task 7.4.

## Task 7.4: Replace Native `TrustAnyServer` With Pinned Certificate Verification

**Files:**
- Modify: `crates/protocol-native-v1/src/client.rs`
- Modify: `crates/protocol-native-v1/src/hole_punch.rs`
- Modify: `crates/protocol-native-v1/src/tls.rs`
- Test: `crates/protocol-native-v1/src/client.rs`
- Test: `crates/protocol-native-v1/src/hole_punch.rs`

**Step 1: Implement verifier**

Replace `TrustAnyServer` in native production paths with a verifier that compares the server certificate DER SHA-256 fingerprint to an expected value:

```rust
#[derive(Debug, Clone)]
pub struct NativeServerVerifier {
    expected_certificate_fingerprint: String,
}

impl NativeServerVerifier {
    pub fn new(expected_certificate_fingerprint: impl Into<String>) -> Self {
        Self {
            expected_certificate_fingerprint: expected_certificate_fingerprint.into(),
        }
    }

    pub fn verify_certificate_fingerprint(&self, cert_der: &[u8]) -> std::result::Result<(), rustls::Error> {
        let actual = crate::tls::certificate_fingerprint_sha256_hex(cert_der);
        if actual == self.expected_certificate_fingerprint {
            Ok(())
        } else {
            Err(rustls::Error::General(format!(
                "native peer certificate fingerprint mismatch: expected {}, got {}",
                self.expected_certificate_fingerprint, actual
            )))
        }
    }
}
```

Implement `ServerCertVerifier` for this type and call `verify_certificate_fingerprint(end_entity.as_ref())` in `verify_server_cert`.

**Step 2: Change native client APIs to require trusted certificate identity**

Update native send functions to accept the expected certificate fingerprint:

```rust
pub async fn send_files_to_url(
    url: &str,
    paths: Vec<PathBuf>,
    keypair: Keypair,
    expected_certificate_fingerprint: String,
) -> Result<()>
```

For WAN candidate sends, thread the same expected certificate fingerprint into `dial_candidates`.

**Step 3: Update hole punch dialer**

Add a trust config:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeDialTrust {
    expected_certificate_fingerprint: String,
}

impl NativeDialTrust {
    pub fn new(expected_certificate_fingerprint: impl Into<String>) -> Self;
    pub fn expected_certificate_fingerprint(&self) -> String;
}
```

Change:

```rust
pub async fn dial_candidates(
    local_candidates: Vec<NativeCandidate>,
    remote_candidates: Vec<NativeCandidate>,
    trust: NativeDialTrust,
) -> Result<DialedCandidateConnection>
```

**Step 4: Update call sites**

Update call sites in:

- `crates/protocol-native-v1/src/client.rs`
- `crates/daemon/src/api/transfers.rs`
- `crates/cli/src/cmd/send.rs` if direct native URL still exists

If direct `quic://` sends cannot know the certificate fingerprint yet, make production direct URL sends require an explicit `--native-cert-fingerprint` flag or reject with a clear error until trust metadata is available. Do not silently fall back to trust-any.

**Step 5: Remove native `TrustAnyServer`**

Run:

```bash
rg -n "TrustAnyServer" crates/protocol-native-v1/src/client.rs crates/protocol-native-v1/src/hole_punch.rs
```

Expected: no matches.

**Step 6: Verify**

Run:

```bash
cargo test -p lsi-protocol-native-v1 native_server_verifier
cargo test -p lsi-protocol-native-v1 hole_punch
cargo test -p lsi-daemon wan
cargo test -p lsi-cli cli_smoke
```

Expected: PASS.

**Step 7: Commit**

```bash
git add crates/protocol-native-v1/src/client.rs crates/protocol-native-v1/src/hole_punch.rs crates/protocol-native-v1/src/tls.rs crates/daemon/src/api/transfers.rs crates/cli/src/cmd/send.rs crates/cli/tests/cli_smoke.rs
git commit -s -m "fix(native): verify trusted peer certificates"
```

## Task 7.5: Enforce Daemon Native Trust Policies

**Files:**
- Modify: `crates/daemon/src/api/transfers.rs`
- Modify: `crates/core/src/trust/store.rs` if certificate metadata is needed
- Modify: `crates/core/src/trust/schema.sql` if certificate metadata is needed
- Test: `crates/daemon/src/api/transfers.rs`
- Test: `crates/core/src/trust/store.rs`

**Step 1: Add failing daemon tests**

In `crates/daemon/src/api/transfers.rs`, add tests for:

- WAN/native trusted peer missing from `trust.db` returns `not_found` or permission-denied before dialing.
- `PeerPolicy::Block` returns permission-denied.
- `PeerPolicy::Prompt` does not auto-send.
- `PeerPolicy::AutoAccept` is the only policy that can auto-send.

Expected assertions:

```rust
assert!(status.message().contains("trusted peer not found"));
assert_eq!(status.code(), tonic::Code::PermissionDenied);
```

**Step 2: Add trust metadata if needed**

If the native certificate fingerprint cannot be derived from stored peer public key, extend `Peer` and `trust.db` to store the pinned native certificate fingerprint or enough identity material to verify it.

Keep migration simple for Sprint 7:

- Add nullable column for new metadata.
- Existing peers without metadata must not auto-send over native protocol.
- Error must say how to re-pair or refresh trust.

**Step 3: Implement policy gate**

Before `send_files_to_wan_peer` dials candidates:

1. Load peer from `TrustStore`.
2. Reject missing peer.
3. Reject `Block`.
4. Reject `Prompt` for unattended send.
5. Require certificate identity metadata.
6. Pass expected certificate fingerprint to native client/dialer.

**Step 4: Verify**

Run:

```bash
cargo test -p lsi-core trust
cargo test -p lsi-daemon grpc_send_wan
cargo test -p lsi-daemon wan
cargo test -p lsi-protocol-native-v1
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/core/src/trust crates/daemon/src/api/transfers.rs crates/protocol-native-v1
git commit -s -m "fix(daemon): enforce native trust policies"
```

## Task 7.6: Add Security Threat Model

**Files:**
- Create: `docs/security/threat-model.md`
- Modify: `README.md`

**Step 1: Create threat model**

Create `docs/security/threat-model.md` with:

- assets: files, identity keys, trust store, API token, webhook secrets, updater keys
- actors: LAN attacker, WAN rendezvous observer, malicious peer, compromised CI, local user
- trust boundaries: LocalSend compatibility, native QUIC, daemon API, GUI bridge, hooks, packaging
- mitigations: bearer token API, Ed25519 identities, trust policies, direct-path WAN diagnostics, no relay in base v1
- known limitations: unsigned desktop packages, LocalSend self-signed compatibility, no third-party audit yet
- disclosure contact placeholder

**Step 2: Link it**

Add a short security section to `README.md` linking:

- `docs/security/threat-model.md`
- `docs/security/rendezvous-privacy.md`
- `docs/security/webui-token-bootstrap.md`

**Step 3: Verify**

Run:

```bash
rg -n "Assets|Actors|Trust Boundaries|Mitigations|Known Limitations|threat-model" docs/security/threat-model.md README.md
git diff --check
```

Expected: PASS.

**Step 4: Commit**

```bash
git add -f docs/security/threat-model.md README.md
git commit -s -m "docs(security): add threat model"
```

## Task 7.7: Build Repeatable Soak Harness

**Files:**
- Modify: `crates/protocol-native-v1/tests/soak_resume.rs`
- Create: `scripts/native-soak.sh`
- Create: `docs/operations/soak-testing.md`

**Step 1: Improve ignored soak test controls**

Extend `crates/protocol-native-v1/tests/soak_resume.rs` to honor:

- `LSI_SOAK_BYTES`
- `LSI_SOAK_RECONNECTS`
- `LSI_SOAK_SEED`

Keep existing defaults fast enough for local manual runs.

**Step 2: Add script**

Create `scripts/native-soak.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${LSI_SOAK_BYTES:=134217728}"
: "${LSI_SOAK_RECONNECTS:=10}"
: "${LSI_SOAK_LOG:=target/soak/native-soak.log}"

mkdir -p "$(dirname "$LSI_SOAK_LOG")"

echo "LSI_SOAK_BYTES=$LSI_SOAK_BYTES"
echo "LSI_SOAK_RECONNECTS=$LSI_SOAK_RECONNECTS"
echo "LSI_SOAK_LOG=$LSI_SOAK_LOG"

LSI_SOAK=1 \
LSI_SOAK_BYTES="$LSI_SOAK_BYTES" \
LSI_SOAK_RECONNECTS="$LSI_SOAK_RECONNECTS" \
cargo test -p lsi-protocol-native-v1 --test soak_resume -- --ignored --nocapture \
  2>&1 | tee "$LSI_SOAK_LOG"
```

**Step 3: Document 7-day run**

Create `docs/operations/soak-testing.md` with:

- quick local run
- 7-day run recipe using shell loop or `timeout`
- optional `heaptrack` or `valgrind massif` commands
- evidence file paths to preserve outside git
- pass/fail criteria

**Step 4: Verify**

Run:

```bash
bash -n scripts/native-soak.sh
cargo test -p lsi-protocol-native-v1 --test soak_resume patterned_bytes_have_stable_hash
LSI_SOAK_BYTES=1048576 LSI_SOAK_RECONNECTS=2 bash scripts/native-soak.sh
rg -n "7-day|heaptrack|valgrind|LSI_SOAK_BYTES|pass/fail" docs/operations/soak-testing.md
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/protocol-native-v1/tests/soak_resume.rs scripts/native-soak.sh
git add -f docs/operations/soak-testing.md
git commit -s -m "test(native): add repeatable soak harness"
```

## Task 7.8: Formalize LocalSend Interop Matrix

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `docs/interop/localsend-matrix.md`
- Create: `scripts/localsend-interop-smoke.sh`
- Test: `crates/protocol-localsend-v2/tests/interop_receive.rs`

**Step 1: Document matrix**

Create `docs/interop/localsend-matrix.md` with columns:

- platform
- LocalSend version
- receive official app to daemon
- send daemon/CLI to official app
- discovery method
- evidence path
- status

Include rows for Android, iOS, Desktop macOS, Desktop Windows, Desktop Linux.

**Step 2: Add script wrapper**

Create `scripts/localsend-interop-smoke.sh` that runs deterministic local interop first:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test -p lsi-protocol-localsend-v2 --test interop_receive -- --nocapture

cat <<'MSG'
Official LocalSend app matrix is manual unless LSI_OFFICIAL_LOCALSEND_ARTIFACT
points to a verified headless-capable artifact for this platform.
MSG
```

If an official artifact path is provided later, the script can launch it, but do not fake that behavior in this task.

**Step 3: Update CI placeholder**

In `.github/workflows/ci.yml`, change the manual interop job to call:

```yaml
- run: bash scripts/localsend-interop-smoke.sh
```

**Step 4: Verify**

Run:

```bash
bash -n scripts/localsend-interop-smoke.sh
bash scripts/localsend-interop-smoke.sh
rg -n "Android|iOS|Desktop macOS|Desktop Windows|Desktop Linux|status" docs/interop/localsend-matrix.md
```

Expected: PASS.

**Step 5: Commit**

```bash
git add .github/workflows/ci.yml scripts/localsend-interop-smoke.sh
git add -f docs/interop/localsend-matrix.md
git commit -s -m "test(interop): formalize localsend matrix"
```

## Task 7.9: Make GUI Smoke Actually Exercise Tauri When Tooling Exists

**Files:**
- Modify: `scripts/gui-smoke.sh`
- Modify: `tests/gui_smoke.rs`
- Modify: `.github/workflows/ci.yml`
- Create: `docs/operations/gui-smoke.md`

**Step 1: Extend script**

Update `scripts/gui-smoke.sh` to:

1. Build WebUI.
2. Check `lsi-gui`.
3. Check `tauri-driver`.
4. If `LSI_GUI_SMOKE_STRICT=1`, fail when `tauri-driver` is missing.
5. If present, start `tauri-driver`, run a bounded Tauri/WebDriver launch check, then clean up.

Keep the first implementation minimal if WebDriver selectors are not stable:

```bash
if [[ "${LSI_GUI_SMOKE_STRICT:-0}" == "1" ]] && ! command -v tauri-driver >/dev/null 2>&1; then
  echo "missing: tauri-driver"
  exit 1
fi
```

**Step 2: Update test**

Update `tests/gui_smoke.rs` so:

- default remains a skip unless `LSI_RUN_GUI_SMOKE=1`
- strict mode failure is test-visible when `LSI_GUI_SMOKE_STRICT=1`

**Step 3: Document**

Create `docs/operations/gui-smoke.md` with:

- install command for `tauri-driver`
- local run command
- strict CI command
- known platform prerequisites

**Step 4: Verify**

Run:

```bash
bash -n scripts/gui-smoke.sh
cargo test --test gui_smoke -- --nocapture
LSI_RUN_GUI_SMOKE=1 cargo test --test gui_smoke -- --nocapture
rg -n "tauri-driver|LSI_RUN_GUI_SMOKE|LSI_GUI_SMOKE_STRICT" docs/operations/gui-smoke.md scripts/gui-smoke.sh tests/gui_smoke.rs
```

Expected: PASS, with non-strict skip allowed if `tauri-driver` is absent.

**Step 5: Commit**

```bash
git add scripts/gui-smoke.sh tests/gui_smoke.rs .github/workflows/ci.yml
git add -f docs/operations/gui-smoke.md
git commit -s -m "test(gui): strengthen tauri smoke harness"
```

## Task 7.10: Harden Release Workflow Artifacts

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/ci.yml`
- Create: `docs/release/artifacts.md`
- Inspect: `packaging/release/checksums.sh`
- Inspect: `packaging/build-packages.sh`

**Step 1: Define artifact matrix**

Document expected artifacts in `docs/release/artifacts.md`:

- Linux CLI/daemon/TUI binaries
- macOS CLI/daemon/TUI binaries
- Windows CLI/daemon/TUI binaries
- Tauri desktop bundles per platform
- Docker image
- `.deb` and `.rpm`
- checksums
- SBOM
- signatures

**Step 2: Add SBOM job**

Add a release workflow job that installs a compatible SBOM tool only if available under Rust 1.78 or uses a documented fallback. Prefer a script wrapper if tool installation can fail:

```yaml
sbom:
  name: SBOM generation
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: mkdir -p dist
    - run: bash packaging/release/sbom.sh dist
```

**Step 3: Add script if needed**

Create `packaging/release/sbom.sh` only if no existing SBOM script exists. It may initially emit a minimal CycloneDX placeholder only if clearly labeled as incomplete. Prefer real tool invocation if available.

**Step 4: Gate signing**

Document and wire signing as opt-in:

- no signing by default on local workflow_dispatch
- macOS notarization only with Apple secrets
- Windows signing only with certificate secrets
- checksums always generated

**Step 5: Verify**

Run:

```bash
rg -n "SBOM|checksum|sign|notar|Authenticode|artifact" .github/workflows/release.yml docs/release/artifacts.md packaging/release
bash -n packaging/release/checksums.sh
test ! -f packaging/release/sbom.sh || bash -n packaging/release/sbom.sh
git diff --check
```

Expected: PASS.

**Step 6: Commit**

```bash
git add .github/workflows/release.yml .github/workflows/ci.yml packaging/release
git add -f docs/release/artifacts.md
git commit -s -m "ci(release): document release artifact pipeline"
```

## Task 7.11: Add Operator Guide

**Files:**
- Create directory: `docs/operators/`
- Create: `docs/operators/index.md`
- Create: `docs/operators/daemon.md`
- Create: `docs/operators/rendezvous.md`
- Create: `docs/operators/desktop.md`
- Create: `docs/operators/troubleshooting.md`
- Modify: `README.md`

**Step 1: Write guide index**

`docs/operators/index.md` should route operators to:

- daemon setup
- rendezvous setup
- desktop GUI setup
- security model
- troubleshooting

**Step 2: Write daemon guide**

Include:

- install/build command
- config path
- state path
- API token path
- LocalSend port
- native QUIC port
- metrics/health readiness endpoints
- trust store operations

**Step 3: Write rendezvous guide**

Summarize and link `docs/deploy/rendezvous-on-a-vps.md`.

**Step 4: Write desktop guide**

Include:

- remote daemon mode
- standalone mode
- token setup
- packaging pre-release status
- signing limitations

**Step 5: Write troubleshooting**

Include:

- API token missing
- no LAN peers
- WAN no direct path
- symmetric NAT/no relay
- GUI cannot start standalone daemon
- unsigned desktop package warnings

**Step 6: Link from README**

Add a short `Operations` section linking `docs/operators/index.md`.

**Step 7: Verify**

Run:

```bash
rg -n "Daemon|Rendezvous|Desktop|Troubleshooting|Operations|API token|WAN" docs/operators README.md
git diff --check
```

Expected: PASS.

**Step 8: Commit**

```bash
git add -f docs/operators README.md
git commit -s -m "docs(ops): add operator guide"
```

## Task 7.12: Add User Migration Guide

**Files:**
- Create: `docs/users/migration-from-localsend.md`
- Create: `docs/users/quickstart.md`
- Modify: `README.md`

**Step 1: Write quickstart**

Include:

- daemon receive from official LocalSend app
- CLI send to LocalSend peer
- WebUI/TUI status
- desktop remote vs standalone choice

**Step 2: Write migration guide**

Include:

- what stays compatible
- what changes for NAS/headless users
- how trust differs between LocalSend compatibility and native protocol
- WAN rendezvous expectations
- limitations before 1.0

**Step 3: Link from README**

Add a `User Docs` section with both files.

**Step 4: Verify**

Run:

```bash
rg -n "LocalSend|quickstart|migration|headless|NAS|remote|standalone|WAN" docs/users README.md
git diff --check
```

Expected: PASS.

**Step 5: Commit**

```bash
git add -f docs/users README.md
git commit -s -m "docs(users): add quickstart and migration guide"
```

## Task 7.13: Add Security Policy And Audit Invite Packet

**Files:**
- Create: `SECURITY.md`
- Create: `docs/security/audit-invite.md`
- Modify: `README.md`

**Step 1: Create `SECURITY.md`**

Include:

- supported versions: pre-1.0 main branch only until first release
- reporting email placeholder
- PGP key placeholder or "pending"
- expected response timeline
- no public disclosure before coordination

Use placeholders only where real contact data is not available. Mark them clearly as `TODO before public release`.

**Step 2: Create audit invite packet**

Create `docs/security/audit-invite.md` with:

- project overview
- scope: native protocol, trust gate, daemon API auth, update/signing pipeline
- out of scope: VIP relay not in base repo
- preferred auditors to contact
- budget range from roadmap
- artifacts to send
- current known risks

**Step 3: Link from README**

Add `SECURITY.md` under security/docs links.

**Step 4: Verify**

Run:

```bash
rg -n "Supported Versions|Reporting|PGP|Audit Scope|Known Risks|TODO before public release" SECURITY.md docs/security/audit-invite.md README.md
git diff --check
```

Expected: PASS.

**Step 5: Commit**

```bash
git add SECURITY.md README.md
git add -f docs/security/audit-invite.md
git commit -s -m "docs(security): add disclosure and audit packet"
```

## Task 7.14: Add Docs Site Skeleton

**Files:**
- Create: `docs/book.toml`
- Create: `docs/src/SUMMARY.md`
- Create: `docs/src/index.md`
- Modify: `.github/workflows/ci.yml`

**Step 1: Create mdBook skeleton**

Use mdBook only as a docs-site wrapper over existing docs. Do not move all docs in this task.

`docs/book.toml`:

```toml
[book]
title = "LocalSend Improved"
language = "en"
src = "src"

[build]
build-dir = "book"
```

`docs/src/SUMMARY.md` should link to copied or relative docs pages. If mdBook cannot include files outside `src`, create short routing pages in `docs/src/` linking to canonical repo docs.

**Step 2: Add manual CI check**

Add a workflow_dispatch-only docs job or a non-blocking check:

```yaml
docs-site:
  name: Docs site
  runs-on: ubuntu-latest
  if: github.event_name == 'workflow_dispatch'
```

Install mdBook and build docs.

**Step 3: Verify locally if mdBook exists**

Run:

```bash
test ! -f docs/book.toml || rg -n "LocalSend Improved|SUMMARY|Operators|Security" docs/book.toml docs/src
git diff --check
```

If `mdbook` is installed:

```bash
mdbook build docs
```

Expected: PASS or documented local tooling gap.

**Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git add -f docs/book.toml docs/src
git commit -s -m "docs(site): scaffold documentation site"
```

## Task 7.15: Add CHANGELOG And Release Notes Draft

**Files:**
- Create: `CHANGELOG.md`
- Create: `docs/release/1.0-notes.md`
- Modify: `README.md`

**Step 1: Write changelog**

Create `CHANGELOG.md` using Keep a Changelog style:

```markdown
# Changelog

## [Unreleased]

## [1.0.0] - TBD

### Added

### Changed

### Security

### Known Limitations
```

Populate it from Sprints 0-7.

**Step 2: Write release notes draft**

`docs/release/1.0-notes.md` should include:

- what 1.0 is
- install artifacts expected
- LocalSend compatibility
- native protocol
- WAN rendezvous
- GUI status
- security posture
- audit status
- known limitations

**Step 3: Link from README**

Add release docs links.

**Step 4: Verify**

Run:

```bash
rg -n "1.0.0|Added|Security|Known Limitations|audit|WAN|GUI|LocalSend" CHANGELOG.md docs/release/1.0-notes.md README.md
git diff --check
```

Expected: PASS.

**Step 5: Commit**

```bash
git add CHANGELOG.md README.md
git add -f docs/release/1.0-notes.md
git commit -s -m "docs(release): draft 1.0 notes"
```

## Task 7.16: Final 1.0 Verification Script

**Files:**
- Create: `scripts/preflight-1.0.sh`
- Create: `docs/release/preflight.md`

**Step 1: Create script**

Create `scripts/preflight-1.0.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo test -p lsi-protocol-native-v1 --test soak_resume patterned_bytes_have_stable_hash
cargo test --test gui_smoke -- --nocapture
bash scripts/localsend-interop-smoke.sh
```

Do not include 7-day soak in this script; link to separate soak docs.

**Step 2: Document preflight**

Create `docs/release/preflight.md` with:

- required local tools
- command
- expected duration
- what is intentionally excluded
- how to attach evidence to release notes

**Step 3: Verify**

Run:

```bash
bash -n scripts/preflight-1.0.sh
bash scripts/preflight-1.0.sh
rg -n "preflight|7-day|evidence|release" docs/release/preflight.md
```

Expected: PASS.

**Step 4: Commit**

```bash
git add scripts/preflight-1.0.sh
git add -f docs/release/preflight.md
git commit -s -m "test(release): add 1.0 preflight"
```

## Task 7.17: Sprint 7 Demo Evidence

**Files:**
- Create: `specs/superpowers/demos/sprint-7-hardening-1.0.md`

**Step 1: Run final verification**

Run:

```bash
bash scripts/preflight-1.0.sh
cargo test -p lsi-protocol-native-v1 --test soak_resume -- --ignored --nocapture
```

If a real 7-day soak has been run, record the exact external evidence path. If not, mark 1.0 as not releasable yet and record the short soak result only.

**Step 2: Run release artifact checks**

Run:

```bash
cargo build --workspace --release
npm run build --prefix crates/webui
```

If packaging tools are installed:

```bash
bash packaging/build-packages.sh
bash packaging/release/checksums.sh dist
```

If not installed, record exact missing tool names.

**Step 3: Write evidence doc**

Include:

- branch and commit
- trust-gate tests
- soak status
- interop status
- GUI smoke status
- release artifact status
- docs/security status
- remaining release blockers

**Step 4: Verify doc**

Run:

```bash
rg -n "Trust Gate|Soak|Interop|GUI Smoke|Release Artifacts|Remaining Blockers" specs/superpowers/demos/sprint-7-hardening-1.0.md
git diff --check
```

Expected: PASS.

**Step 5: Commit**

```bash
git add -f specs/superpowers/demos/sprint-7-hardening-1.0.md
git commit -s -m "test(demo): record sprint 7 hardening evidence"
```

## Task 7.18: Close Sprint 7 Retro

**Files:**
- Create: `specs/superpowers/retros/sprint-7.md`

**Step 1: Check commit hygiene**

Run:

```bash
git log --format='%h %s%n%B' main..HEAD
```

Expected:

- every commit has `Signed-off-by`
- no commit contains `Co-Authored-By`

**Step 2: Write retro**

Include:

- what shipped
- trust-gate evidence
- soak evidence
- interop evidence
- release artifacts evidence
- docs evidence
- 1.0 go/no-go decision
- remaining post-1.0 or release-blocking risks

**Step 3: Verify retro**

Run:

```bash
rg -n "What Shipped|Trust Gate|Soak|Interop|Release Artifacts|Go/No-Go|Remaining Risks" specs/superpowers/retros/sprint-7.md
git diff --check
```

Expected: PASS.

**Step 4: Commit**

```bash
git add -f specs/superpowers/retros/sprint-7.md
git commit -s -m "docs(retro): close sprint 7"
```

## Sprint 7 Acceptance Criteria

Sprint 7 is complete only when:

1. `cargo fmt --all -- --check` passes.
2. `cargo clippy --workspace --all-targets -- -D warnings` passes.
3. `cargo test --workspace` passes.
4. `npm run check --prefix crates/webui` passes.
5. `npm run build --prefix crates/webui` passes.
6. Native production paths no longer use `TrustAnyServer`.
7. Native trusted sends require explicit trusted identity material.
8. Unknown, blocked, prompt-only, fingerprint-mismatch, and certificate-mismatch native peers have negative tests.
9. Soak harness is repeatable and evidence is recorded.
10. LocalSend interop matrix exists with automated local interop and clear official-app status.
11. GUI smoke is strict-capable when `tauri-driver` exists.
12. Threat model, security policy, operator guide, user docs, migration guide, changelog, release notes, and artifact docs exist.
13. Release artifact workflow includes checksums and SBOM/signing boundaries.
14. Sprint 7 demo evidence and retro exist.

## 1.0 Go/No-Go Rule

Sprint 7 can close with a documented no-go, but 1.0 release cannot be tagged unless:

- the 7-day soak has actually passed,
- the official LocalSend interop matrix is green or explicitly scoped down,
- release artifacts are reproducible,
- security disclosure contact is real,
- signing/notarization decision is explicit,
- all release notes disclose remaining limitations.
