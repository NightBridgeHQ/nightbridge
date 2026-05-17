# Sprint 6 GUI Tauri Demo Evidence

Date: 2026-05-16
Branch: `feature/sprint-6-gui-tauri`

## Scope

Sprint 6 turns `lsi-gui` into a real Tauri desktop app that bundles the
existing Svelte WebUI, supports remote daemon configuration, supports a
standalone local daemon mode, and surfaces WAN direct-path diagnostics from the
daemon event stream.

## GUI-Focused Verification

```text
cargo check -p lsi-gui
PASS

cargo test -p lsi-gui
PASS: 7 passed

npm run check --prefix crates/webui
PASS

npm run build --prefix crates/webui
PASS: dist/index.html and dist/assets generated

cargo test --test gui_smoke -- --nocapture
PASS: gated smoke test skipped by default

LSI_RUN_GUI_SMOKE=1 cargo test --test gui_smoke -- --nocapture
PASS: skipped because tauri-driver is not installed
```

## Workspace Verification

```text
cargo fmt --all -- --check
PASS
Note: rustfmt warns that imports_granularity and group_imports are nightly-only
under the pinned stable Rust 1.78 toolchain.

cargo clippy --workspace --all-targets -- -D warnings
PASS

cargo test --workspace
PASS
```

## Packaging Verification

Command:

```bash
cd crates/gui
../webui/node_modules/.bin/tauri build
```

Result:

```text
PASS
Built application:
target/release/localsend-improved-gui

Built bundle:
target/release/bundle/dmg/LocalSend Improved_0.1.0_aarch64.dmg
```

Two MSRV-related packaging findings were fixed during this task:

- Vite must emit relative asset URLs for Tauri bundle loading, so
  `crates/webui/vite.config.ts` now sets `base: "./"`.
- Tauri release builds enable `tauri/native-tls`, so macOS TLS dependencies are
  pinned to Rust 1.78-compatible versions in `crates/gui/Cargo.toml`.

## Remote Mode Evidence

Remote mode is implemented through the shared WebUI API base setting:

- desktop settings persist `mode`, `remote_endpoint`, and `api_token`
- WebUI fetches daemon status, trusted peers, LAN peers, inbox, transfers, and
  SSE events from the configured API base
- invalid remote endpoints are rejected unless they use `http://` or `https://`

Automated evidence:

- `cargo test -p lsi-gui` covers settings persistence and endpoint validation
- `npm run check --prefix crates/webui` type-checks remote mode state and UI
- `npm run build --prefix crates/webui` verifies production bundle generation

## Standalone Mode Evidence

Standalone mode is implemented by the Rust-side embedded daemon manager:

- starts `localsend-improved-daemon` with a private config/state/data root
- uses loopback-only API ports
- reads the generated daemon API token from the private config root
- exposes start, stop, and status commands to the WebUI

Automated evidence:

- `cargo test -p lsi-gui` covers daemon args, private state root, and stopped
  manager status
- `cargo clippy --workspace --all-targets -- -D warnings` covers the GUI crate
  with the workspace lint gate

## WAN Diagnostics Evidence

The GUI preserves direct-path failure diagnostics from daemon transfer events:

- transfer failure events carry structured error code/message fields
- WebUI exposes the actionable message in dashboard and transfer diagnostics
- Sprint 5 direct-path limitations remain explicit: there is no relay fallback
  in the base protocol

Automated evidence:

- `cargo test --workspace` includes native protocol WAN diagnostic tests
- `npm run check --prefix crates/webui` type-checks the event error shape

## Manual Launch Notes

No screenshot was captured in git. The package build produced a local macOS DMG,
but interactive window launch and WebDriver automation were not run because
`tauri-driver` is not installed in this environment.

Known local packaging gaps are documented in
`docs/deploy/desktop-packaging.md`:

- macOS app and DMG are unsigned local artifacts
- Apple notarization credentials are not configured
- Windows MSI/AuthentiCode was not validated on this macOS host
- Linux AppImage was not validated on this macOS host
- updater signing and release metadata are intentionally not configured

