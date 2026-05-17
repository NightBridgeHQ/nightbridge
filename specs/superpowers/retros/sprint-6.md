# Sprint 6 Retro: GUI Tauri

Date: 2026-05-16
Branch: `feature/sprint-6-gui-tauri`

## What Shipped

- `lsi-gui` is now a real Tauri 2 desktop app instead of a stub crate.
- The desktop app bundles the existing Svelte WebUI from `crates/webui/dist`.
- WebUI API calls support a configurable daemon API base for desktop remote
  mode.
- Desktop settings persist GUI mode, remote daemon endpoint, and API token
  without putting secrets in URLs.
- First-launch mode selection supports remote daemon and standalone local
  daemon modes.
- Standalone mode can manage a local daemon process with a private config,
  state, and data root.
- WAN direct-path transfer failures are visible in the GUI with the actionable
  daemon error message.
- Local Tauri packaging produces a macOS DMG.
- Desktop packaging, signing, updater boundaries, demo evidence, and README
  status are documented.

## Standalone Evidence

Standalone mode is implemented in `crates/gui/src/daemon.rs`.

Evidence:

- `cargo test -p lsi-gui` passed with daemon manager tests covering stopped
  status, loopback API args, and private state root setup.
- `cargo clippy --workspace --all-targets -- -D warnings` passed after the
  standalone code was added.
- `cargo test --workspace` passed with GUI tests included.

Remaining standalone risk:

- Interactive app-window startup of the embedded daemon was not exercised with
  WebDriver because `tauri-driver` is not installed on this machine.

## Remote Evidence

Remote daemon mode is implemented through the shared WebUI API base and desktop
settings bridge.

Evidence:

- `cargo test -p lsi-gui` passed settings persistence and endpoint validation.
- `npm run check --prefix crates/webui` passed.
- `npm run build --prefix crates/webui` passed.
- Remote mode rejects endpoints that are not valid `http://` or `https://`
  URLs.

Remaining remote risk:

- Manual connection against a long-running external daemon should be repeated
  before a public desktop release.

## Packaging Evidence

Local package command:

```bash
cd crates/gui
../webui/node_modules/.bin/tauri build
```

Result:

```text
PASS
target/release/bundle/dmg/LocalSend Improved_0.1.0_aarch64.dmg
```

Packaging fixes made during Sprint 6:

- Vite now emits relative bundle asset paths with `base: "./"`.
- `native-tls` and `security-framework` are pinned to Rust 1.78-compatible
  versions for Tauri release builds that enable `tauri/native-tls`.
- Tauri `beforeBuildCommand` uses the path that matches Tauri's `crates/`
  working directory in this workspace.

Signing and updater status:

- macOS `.app` and `.dmg` are local unsigned artifacts.
- Apple notarization was not attempted because credentials are not configured.
- Windows MSI/AuthentiCode was not validated on this macOS host.
- Linux AppImage was not validated on this macOS host.
- No updater endpoint is configured; this avoids a fake production update
  channel before signed releases exist.

## Test Evidence

Final verification recorded for Sprint 6:

- `cargo fmt --all -- --check`: PASS, with known rustfmt warnings for
  nightly-only import options under Rust 1.78.
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS.
- `cargo test --workspace`: PASS.
- `cargo check -p lsi-gui`: PASS.
- `cargo test -p lsi-gui`: PASS, 7 tests.
- `npm run check --prefix crates/webui`: PASS.
- `npm run build --prefix crates/webui`: PASS.
- `cargo test --test gui_smoke -- --nocapture`: PASS, default gated skip.
- `LSI_RUN_GUI_SMOKE=1 cargo test --test gui_smoke -- --nocapture`: PASS,
  skipped because `tauri-driver` is not installed.

Commit hygiene:

- Every Sprint 6 commit on `main..HEAD` has `Signed-off-by`.
- No `Co-Authored-By` trailers were added.

## Remaining Risks

- Desktop release signing and notarization are not configured.
- Windows and Linux desktop packaging need platform-native verification.
- Full GUI WebDriver smoke awaits `tauri-driver` and a real interactive run.
- Standalone mode currently launches a local daemon process directly; service
  installation and background lifecycle management remain out of Sprint 6
  scope.
- WAN still has no relay fallback in the base protocol, so GUI diagnostics must
  continue to state direct-path failures clearly.

## Sprint 7

- Decide whether unsigned desktop packages are acceptable for pre-release
  distribution or whether signing blocks the next milestone.
- Run GUI smoke on macOS with `tauri-driver`, then repeat on Windows and Linux.
- Add release workflow jobs for desktop bundles only after local signing and
  packaging are stable.
- Re-run LocalSend interop and native resume soak tests on `main`.
- Prepare release notes that call out WAN diagnostics, packaging status, and
  unsigned desktop limitations.

