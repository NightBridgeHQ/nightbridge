# 26.5 Preflight

The 26.5 preflight is the short, repeatable release-readiness check for the
current branch. It verifies formatting, linting, workspace tests, WebUI checks,
the targeted native soak hash test, GUI smoke gating, and LocalSend interop
smoke coverage.

## Required Local Tools

- Rust toolchain compatible with this workspace.
- Cargo with network-independent access to already resolved dependencies.
- Node.js and npm for `crates/webui`.
- WebUI dependencies installed with `npm ci --prefix crates/webui` on a fresh
  checkout.
- Platform tooling required by the tests being exercised.

Optional tools may expand coverage:

- `tauri-driver` lets the GUI smoke harness exercise a real Tauri app window.
- Official LocalSend application artifacts let the interop matrix move beyond
  the automated local compatibility smoke.

## Command

Run from the repository root:

```bash
bash scripts/preflight-26.5.sh
```

The script runs:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo test -p lsi-protocol-native-v1 --test soak_resume patterned_bytes_have_stable_hash
cargo test --test gui_smoke -- --nocapture
bash scripts/localsend-interop-smoke.sh
```

## Expected Duration

On a warm developer machine, expect several minutes. A cold checkout may take
longer because Cargo and npm need to populate build caches.

## Intentionally Excluded

The preflight does not include the 7-day native soak. Use the soak guide for
long-running release evidence:

```bash
timeout 7d bash -c 'while true; do bash scripts/native-soak.sh; done'
```

The preflight also excludes platform signing, notarization, updater publishing,
Docker daemon validation, Linux systemd validation, and full official LocalSend
app matrix runs unless those tools and artifacts are explicitly available.

## Evidence

Attach the command output or terminal log to the release notes draft before
tagging 26.5. At minimum, record:

- branch and commit SHA
- start and finish time
- preflight command
- pass or fail result
- skipped optional coverage, including GUI WebDriver, official LocalSend app
  interop, signing, and 7-day soak evidence
- links or paths to any artifact, soak, or release logs
