# Sprint 6 GUI Tauri Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. If the user explicitly asks for parallel work, use `superpowers:subagent-driven-development` and split only along the safe parallelization notes below.

**Goal:** Ship a desktop GUI that reuses the Sprint 3 WebUI, runs in Tauri, supports remote daemon mode, and has a credible standalone path for local desktop use.

**Architecture:** Keep the daemon as the source of truth for transfers, peers, inbox, events, and WAN diagnostics. The desktop app wraps the existing Svelte WebUI in Tauri and adds a small Rust-side bridge for desktop-only configuration, token storage, remote endpoint selection, and optional embedded daemon lifecycle. Remote mode talks to a daemon through the same HTTP/SSE API the WebUI already uses; standalone mode starts a local daemon process and points the WebUI at it.

**Tech Stack:** Rust 1.78, Tauri 2.x after MSRV/tooling preflight, Svelte/Vite WebUI, existing daemon HTTP API and SSE events, `tauri-build`, `tauri`, `tauri-plugin-shell` only if needed, platform packaging with Tauri bundle, optional `tauri-driver`/WebDriver smoke tests where local tooling supports it.

---

## Sprint 6 Deliverable Boundary

**Must ship by end of Sprint 6:**

1. `lsi-gui` builds as a real Tauri desktop app instead of a stub crate.
2. The app bundles the existing `crates/webui/dist` frontend.
3. First-launch mode selection supports at least `Remote daemon` and `Standalone local daemon`.
4. Remote mode stores endpoint and token locally, then loads status, peers, inbox, transfers, and events from the selected daemon.
5. Standalone mode can start a local `night-bridge-daemon` process with a local config/state root and connect to its API.
6. GUI displays WAN/direct-path failure diagnostics from daemon/API state without hiding the actionable error string.
7. Tauri packaging metadata exists for macOS, Windows, and Linux.
8. Local packaging smoke is documented with exact pass/fail reasons for this machine.
9. Sprint 6 demo evidence and retro exist.

**Explicitly not required for Sprint 6 closure:**

- Apple Developer notarization with real production certificates.
- Windows Authenticode signing with real production certificates.
- Fully automatic background service install on Windows/macOS.
- Auto-update against a live public release channel.
- New transfer protocol behavior.
- Rewriting the WebUI design system from scratch.
- Relay/VIP connectivity work.

## Carry-Forward Constraints

- Rust MSRV remains `1.78.0`; Tauri and every new dependency must be checked before adoption.
- The existing WebUI uses daemon HTTP/SSE endpoints. Prefer preserving that contract over adding a second UI-specific API surface.
- The daemon API is bearer-token protected. Do not put tokens into URL query params or command-line logs.
- Sprint 5 WAN has no relay fallback. GUI must surface direct-path failure details honestly.
- Packaging/signing tasks should produce local scaffolding and documented environment gaps if certificates or platform tools are unavailable.

## Execution Order

1. Tauri/MSRV/tooling preflight.
2. GUI crate shape and bundle config.
3. WebUI runtime API-base abstraction.
4. Desktop app settings and first-launch mode selector.
5. Remote daemon mode.
6. Standalone embedded daemon mode.
7. WAN diagnostics UI.
8. Packaging/signing/updater scaffolding.
9. Tauri smoke tests, demo evidence, and retro.

## Safe Parallelization Notes

Safe to parallelize after Task 6.2 lands:

- WebUI API-base abstraction and Rust-side settings models can be separate workers.
- Packaging docs can run beside GUI feature work.
- Tauri test scaffolding can run beside standalone daemon wiring once the app launches.

Keep serial:

- Tauri dependency preflight before committing Tauri crates/plugins.
- WebUI API-base abstraction before remote endpoint selection.
- Remote mode before standalone mode.
- WAN diagnostics display after daemon/API error shape is confirmed.
- Final packaging/demo/retro after core GUI flows pass.

## Task 6.1: Tauri Tooling And MSRV Preflight

**Files:**
- Modify: `specs/superpowers/plans/2026-05-16-sprint-6-gui-tauri.md` only if scope changes after preflight
- No code changes unless dependency versions are proven compatible

**Step 1: Record current toolchain**

Run:

```bash
rustc --version
cargo --version
node --version
npm --version
```

Expected: Rust is compatible with workspace MSRV expectations and Node/npm are available for WebUI builds.

**Step 2: Check current workspace before GUI changes**

Run:

```bash
cargo check --workspace
npm run check --prefix crates/webui
npm run build --prefix crates/webui
```

Expected: PASS before adding Tauri.

**Step 3: Inspect Tauri compatibility before selecting versions**

Use official Tauri docs or local package metadata to confirm a Tauri 2.x combination that works with Rust `1.78.0`.

Candidate crates to evaluate:

```toml
tauri = { version = "=2.0.0", features = [] }
tauri-build = "=2.0.0"
```

Candidate npm packages to evaluate:

```json
"@tauri-apps/cli": "2.0.0",
"@tauri-apps/api": "2.0.0"
```

Expected: exact versions selected or a documented blocker if current Tauri requires a newer Rust toolchain.

Sprint 6 preflight result:

- `tauri = 2.0.0` and `tauri-build = 2.0.0` compile on Rust `1.78.0`.
- Cargo's default resolution pulls newer compatible-range Tauri family crates and transitive crates with higher MSRV.
- Keep the initial `Cargo.lock` resolution on the Tauri 2.0 line:
  - `tauri-codegen = 2.0.0`
  - `tauri-macros = 2.0.0`
  - `tauri-runtime = 2.0.0`
  - `tauri-runtime-wry = 2.0.0`
  - `tauri-utils = 2.0.0`
  - `plist = 1.7.0`
  - `serde_with = 3.15.0`
  - `serde_with_macros = 3.15.0`

**Step 4: Commit only if the plan is updated**

If scope or version constraints change:

```bash
git add specs/superpowers/plans/2026-05-16-sprint-6-gui-tauri.md
git commit -s -m "docs(gui): refine sprint 6 tauri constraints"
```

Otherwise do not commit.

## Task 6.2: Scaffold Real Tauri GUI Crate

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/gui/Cargo.toml`
- Modify: `crates/gui/src/lib.rs`
- Create: `crates/gui/src/main.rs`
- Create: `crates/gui/build.rs`
- Create: `crates/gui/tauri.conf.json`
- Create: `crates/gui/capabilities/default.json`
- Create: `crates/gui/icons/.gitkeep`

**Step 1: Add GUI smoke test before implementation**

In `crates/gui/src/lib.rs`, replace the stub with a minimal config helper and test:

```rust
pub fn app_name() -> &'static str {
    "NightBridge"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_stable() {
        assert_eq!(app_name(), "NightBridge");
    }
}
```

**Step 2: Run test**

Run:

```bash
cargo test -p lsi-gui app_name_is_stable
```

Expected: PASS.

**Step 3: Add Tauri dependencies**

After Task 6.1 selects exact compatible versions, add Tauri dependencies to `crates/gui/Cargo.toml`.

Also define a binary target:

```toml
[[bin]]
name = "night-bridge-gui"
path = "src/main.rs"
```

**Step 4: Add Tauri config**

Create `crates/gui/tauri.conf.json` with:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "NightBridge",
  "version": "0.1.0",
  "identifier": "com.nightbridge.app",
  "build": {
    "beforeBuildCommand": "npm run build --prefix ../webui",
    "frontendDist": "../webui/dist",
    "devUrl": "http://localhost:5173"
  },
  "app": {
    "windows": [
      {
        "title": "NightBridge",
        "width": 1180,
        "height": 760,
        "minWidth": 920,
        "minHeight": 620
      }
    ]
  },
  "bundle": {
    "active": true,
    "targets": ["dmg", "appimage", "msi"],
    "icon": []
  }
}
```

Adjust the schema/config keys to match the selected Tauri version if needed.

**Step 5: Add minimal main**

Create `crates/gui/src/main.rs`:

```rust
fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run NightBridge GUI");
}
```

**Step 6: Verify**

Run:

```bash
cargo check -p lsi-gui
cargo test -p lsi-gui
npm run build --prefix crates/webui
```

Expected: PASS.

**Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/gui crates/webui/package-lock.json crates/webui/package.json
git commit -s -m "feat(gui): scaffold tauri desktop app"
```

## Task 6.3: Add Desktop Runtime API Base In WebUI

**Files:**
- Modify: `crates/webui/src/api.ts`
- Modify: `crates/webui/src/state.ts`
- Test: `crates/webui/src/api.ts`

**Step 1: Add API base helper**

In `crates/webui/src/api.ts`, add:

```ts
let apiBase = "";

export function setApiBase(base: string): void {
  apiBase = base.replace(/\/+$/, "");
}

export function resolveApiPath(path: string): string {
  if (!path.startsWith("/")) {
    throw new Error("API path must start with /");
  }
  return `${apiBase}${path}`;
}
```

Update all `fetch(path, ...)` calls to use `resolveApiPath(path)`.

**Step 2: Add lightweight TypeScript checks through exported helper**

Add a small exported helper if needed so `tsc` can catch wrong path usage. Avoid adding a browser test framework in this task.

**Step 3: Update state to accept endpoint selection later**

In `crates/webui/src/state.ts`, add endpoint to app state:

```ts
apiBase: string;
```

Initialize it from `localStorage` key `nbrg.apiBase`, defaulting to `""`.

**Step 4: Verify**

Run:

```bash
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo test -p lsi-webui
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/webui/src/api.ts crates/webui/src/state.ts
git commit -s -m "feat(webui): support configurable daemon API base"
```

## Task 6.4: Add GUI Settings Model And Persistence Commands

**Files:**
- Modify: `crates/gui/Cargo.toml`
- Create: `crates/gui/src/settings.rs`
- Modify: `crates/gui/src/lib.rs`
- Modify: `crates/gui/src/main.rs`
- Test: `crates/gui/src/settings.rs`

**Step 1: Add failing settings tests**

Create tests for defaults and validation:

```rust
#[test]
fn default_settings_start_in_remote_mode_without_secret_token() {
    let settings = GuiSettings::default();

    assert_eq!(settings.mode, GuiMode::Remote);
    assert!(settings.remote_endpoint.is_none());
    assert!(settings.api_token.is_none());
}

#[test]
fn remote_endpoint_must_be_http_or_https() {
    assert!(GuiSettings::validate_endpoint("http://127.0.0.1:53317").is_ok());
    assert!(GuiSettings::validate_endpoint("https://nas.example.test").is_ok());
    assert!(GuiSettings::validate_endpoint("file:///tmp/socket").is_err());
}
```

**Step 2: Implement settings types**

Create:

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuiMode {
    Remote,
    Standalone,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GuiSettings {
    pub mode: GuiMode,
    pub remote_endpoint: Option<String>,
    pub api_token: Option<String>,
}
```

Implement file load/save under the app config directory using existing `directories` if available.

**Step 3: Add Tauri commands**

Expose:

```rust
#[tauri::command]
fn gui_load_settings() -> Result<GuiSettings, String>

#[tauri::command]
fn gui_save_settings(settings: GuiSettings) -> Result<(), String>
```

Do not log `api_token`.

**Step 4: Register commands in `main.rs`**

Use `tauri::generate_handler![gui_load_settings, gui_save_settings]`.

**Step 5: Verify**

Run:

```bash
cargo test -p lsi-gui settings
cargo check -p lsi-gui
```

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/gui/Cargo.toml crates/gui/src
git commit -s -m "feat(gui): persist desktop connection settings"
```

## Task 6.5: Build First-Launch Mode Selector

**Files:**
- Modify: `crates/webui/package.json`
- Modify: `crates/webui/src/App.svelte`
- Modify: `crates/webui/src/state.ts`
- Modify: `crates/webui/src/style.css`

**Step 1: Add Tauri invoke wrapper with browser fallback**

Add `@tauri-apps/api` after Task 6.1 selects the exact version.

In state code, detect Tauri safely:

```ts
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
```

Create functions:

```ts
export async function loadDesktopSettings(): Promise<void>
export async function saveDesktopSettings(mode: "remote" | "standalone", endpoint: string, token: string): Promise<void>
```

If not running under Tauri, use existing localStorage behavior.

**Step 2: Add first-launch UI**

In `App.svelte`, show a settings panel when no endpoint/token is configured in desktop mode:

- segmented control: Remote daemon / Standalone local daemon
- endpoint input for remote mode
- token input for remote mode
- save button
- connect button

Keep the existing dashboard as the first screen after connection.

**Step 3: Apply API base on save**

Remote mode should set API base to the selected endpoint. Browser-served daemon WebUI keeps API base as `""`.

**Step 4: Verify**

Run:

```bash
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo check -p lsi-gui
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/webui/package.json crates/webui/package-lock.json crates/webui/src
git commit -s -m "feat(webui): add desktop mode selector"
```

## Task 6.6: Remote Daemon Mode Smoke Path

**Files:**
- Modify: `crates/webui/src/state.ts`
- Modify: `crates/webui/src/App.svelte`
- Modify: `crates/webui/src/style.css`
- Test: existing WebUI build checks

**Step 1: Start a local daemon for smoke testing**

Use a temporary config/state root and start:

```bash
cargo run -p lsi-daemon -- --api-bind 127.0.0.1:53317
```

Record the token path or configured token needed for API access.

**Step 2: Verify WebUI can point at remote endpoint**

Run the WebUI dev server:

```bash
npm run build --prefix crates/webui
```

If interactive dev tooling is used:

```bash
npm run dev --prefix crates/webui
```

Expected: entering `http://127.0.0.1:53317` and the token loads daemon status.

**Step 3: Add clearer remote errors**

Display:

- unreachable daemon
- bad token
- unsupported endpoint scheme
- event stream disconnect

**Step 4: Verify**

Run:

```bash
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo test -p lsi-daemon api::http
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/webui/src
git commit -s -m "feat(gui): connect to remote daemon endpoint"
```

## Task 6.7: Standalone Daemon Process Manager

**Files:**
- Modify: `crates/gui/Cargo.toml`
- Create: `crates/gui/src/daemon.rs`
- Modify: `crates/gui/src/lib.rs`
- Modify: `crates/gui/src/main.rs`
- Test: `crates/gui/src/daemon.rs`

**Step 1: Add process command builder tests**

Test without spawning:

```rust
#[test]
fn daemon_args_use_loopback_api_and_private_state_root() {
    let spec = DaemonSpec::new_for_test("/tmp/lsi-gui-test");

    assert!(spec.args.contains(&"--api-bind".to_string()));
    assert!(spec.args.iter().any(|arg| arg == "127.0.0.1:0"));
}
```

**Step 2: Implement daemon spec**

Create a `DaemonSpec` that defines:

- executable path
- config/state directory
- loopback API bind
- inbox directory
- alias

Do not pass bearer token on the command line if a file-based token can be used.

**Step 3: Implement manager**

Create `EmbeddedDaemonManager` with:

- `start()`
- `stop()`
- `status()`
- child process cleanup on drop

Keep it behind a `tauri::State<Mutex<...>>`.

**Step 4: Expose Tauri commands**

Add:

```rust
#[tauri::command]
fn gui_start_embedded_daemon(...)

#[tauri::command]
fn gui_stop_embedded_daemon(...)
```

**Step 5: Verify**

Run:

```bash
cargo test -p lsi-gui daemon
cargo check -p lsi-gui
```

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/gui/Cargo.toml crates/gui/src
git commit -s -m "feat(gui): manage embedded daemon process"
```

## Task 6.8: Wire Standalone Mode In WebUI

**Files:**
- Modify: `crates/webui/src/state.ts`
- Modify: `crates/webui/src/App.svelte`
- Modify: `crates/webui/src/style.css`

**Step 1: Add standalone startup flow**

When mode is `standalone`, call `gui_start_embedded_daemon`, receive endpoint/token metadata, set API base, set token, and load snapshot.

**Step 2: Add visible standalone state**

Show:

- starting
- running
- failed to start
- stopped

Do not expose token text in the normal UI after save.

**Step 3: Add shutdown command**

Add a settings action to stop the embedded daemon.

**Step 4: Verify**

Run:

```bash
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo check -p lsi-gui
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/webui/src
git commit -s -m "feat(gui): add standalone daemon mode"
```

## Task 6.9: Surface WAN Diagnostics In GUI

**Files:**
- Modify: `crates/daemon/src/api/transfers.rs` only if current API omits failure details
- Modify: `crates/webui/src/api.ts`
- Modify: `crates/webui/src/App.svelte`
- Modify: `crates/webui/src/style.css`
- Test: daemon tests only if API shape changes

**Step 1: Inspect transfer failure shape**

Run:

```bash
rg -n "NoDirectPath|last_error|diagnostic|failure|transfers" crates/daemon crates/protocol-native-v1 crates/webui/src
```

Expected: identify whether WAN diagnostics are already exposed through active transfer/error state.

**Step 2: Add API coverage only if needed**

If diagnostics are not exposed, add a focused daemon test that a WAN direct-path error serializes the actionable message.

Run:

```bash
cargo test -p lsi-daemon wan diagnostics
```

Expected before implementation: FAIL if test is new.

**Step 3: Implement minimal API and UI**

Display the exact diagnostic string from the daemon in Transfers or Dashboard. Include attempted pair count if available.

Do not rewrite the transfer model beyond what the UI needs.

**Step 4: Verify**

Run:

```bash
cargo test -p lsi-daemon wan
cargo test -p lsi-protocol-native-v1 hole_punch
npm run check --prefix crates/webui
npm run build --prefix crates/webui
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/daemon/src/api/transfers.rs crates/webui/src
git commit -s -m "feat(gui): show WAN direct-path diagnostics"
```

## Task 6.10: Package WebUI Assets Into Tauri Bundle

**Files:**
- Modify: `crates/gui/tauri.conf.json`
- Modify: `crates/webui/vite.config.ts` only if base path needs adjustment
- Modify: `crates/webui/src/api.ts` only if asset protocol changes require it

**Step 1: Verify WebUI asset paths**

Run:

```bash
npm run build --prefix crates/webui
rg -n "assets/|/api/v1|http://|https://" crates/webui/dist/index.html crates/webui/dist/assets
```

Expected: static assets resolve from Tauri bundle; API calls stay runtime-configured.

**Step 2: Build GUI app**

Run the selected Tauri build command, for example:

```bash
cargo tauri build --config crates/gui/tauri.conf.json
```

or the equivalent package script chosen in Task 6.1.

Expected: either a local bundle is produced or a documented missing-platform-tool error appears.

**Step 3: Fix only bundle path issues**

If the window launches blank because of asset paths, adjust Vite base conservatively.

**Step 4: Verify**

Run:

```bash
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo check -p lsi-gui
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/gui/tauri.conf.json crates/webui
git commit -s -m "build(gui): bundle webui assets in tauri"
```

## Task 6.11: Add Local Tauri Smoke Test Harness

**Files:**
- Create: `tests/gui_smoke.rs`
- Modify: `Cargo.toml`
- Optional create: `scripts/gui-smoke.sh`

**Step 1: Add gated smoke test**

Create a root integration test that skips unless explicitly enabled:

```rust
#[test]
fn gui_smoke_is_gated() {
    if std::env::var("NBRG_RUN_GUI_SMOKE").as_deref() != Ok("1") {
        eprintln!("skipping GUI smoke: set NBRG_RUN_GUI_SMOKE=1");
        return;
    }

    panic!("GUI smoke harness is enabled but not implemented for this platform yet");
}
```

Then replace the panic with real launcher/WebDriver logic in later steps once tooling is available.

**Step 2: Verify skip behavior**

Run:

```bash
cargo test --test gui_smoke -- --nocapture
```

Expected: PASS with clear skip message.

**Step 3: Add script wrapper**

If useful, create `scripts/gui-smoke.sh` that:

- builds WebUI
- builds GUI
- starts a local daemon
- launches Tauri driver when available
- records missing tooling clearly

**Step 4: Verify**

Run:

```bash
bash -n scripts/gui-smoke.sh
cargo test --test gui_smoke -- --nocapture
```

Expected: PASS or documented skip.

**Step 5: Commit**

```bash
git add Cargo.toml tests/gui_smoke.rs scripts/gui-smoke.sh
git commit -s -m "test(gui): add gated desktop smoke harness"
```

## Task 6.12: Add Packaging, Signing, And Updater Scaffolding

**Files:**
- Modify: `crates/gui/tauri.conf.json`
- Create: `docs/deploy/desktop-packaging.md`
- Optional modify: `.github/workflows/ci.yml`

**Step 1: Document local signing boundary**

Create `docs/deploy/desktop-packaging.md` with:

- macOS `.app`/`.dmg` build command
- notarization variables required but not committed
- Windows signing variables required but not committed
- Linux AppImage prerequisites
- updater signing key handling
- exact local gaps on this machine

**Step 2: Add disabled updater config only if supported by selected Tauri version**

Do not point to a fake production endpoint. Use a disabled or placeholder config that cannot accidentally auto-update.

**Step 3: Add CI job as manual or non-blocking if platform tools are unavailable**

If `.github/workflows/ci.yml` changes, gate packaging with a variable or workflow dispatch.

**Step 4: Verify**

Run:

```bash
rg -n "notar|Authenticode|AppImage|updater|secret|token" docs/deploy/desktop-packaging.md crates/gui/tauri.conf.json
git diff --check
```

Expected: docs and config mention required setup without committing secrets.

**Step 5: Commit**

```bash
git add -f docs/deploy/desktop-packaging.md
git add crates/gui/tauri.conf.json .github/workflows/ci.yml
git commit -s -m "docs(gui): document desktop packaging and signing"
```

## Task 6.13: Refresh README With GUI Status

**Files:**
- Modify: `README.md`

**Step 1: Update status**

Change status to mention Sprint 6 GUI work and clarify that desktop packages are pre-release until signing/updater are fully configured.

**Step 2: Add GUI commands**

Document:

```bash
npm run build --prefix crates/webui
cargo check -p lsi-gui
```

Add the selected Tauri run/build command from Task 6.1.

**Step 3: Verify**

Run:

```bash
rg -n "GUI|Tauri|desktop|Sprint 6|remote daemon|standalone" README.md
```

Expected: the README has discoverable GUI instructions.

**Step 4: Commit**

```bash
git add README.md
git commit -s -m "docs(readme): add sprint 6 GUI status"
```

## Task 6.14: Sprint 6 Demo Evidence

**Files:**
- Create: `specs/superpowers/demos/sprint-6-gui-tauri.md`

**Step 1: Run final GUI-focused commands**

Run:

```bash
cargo check -p lsi-gui
cargo test -p lsi-gui
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo test --test gui_smoke -- --nocapture
```

Expected: PASS or documented skip for GUI smoke when platform tooling is unavailable.

**Step 2: Run workspace checks**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: PASS.

**Step 3: Capture manual app launch evidence**

Record:

- GUI launch command
- whether app window opens
- remote daemon connection result
- standalone daemon startup result
- packaging command result
- missing signing/notarization/tooling gaps

**Step 4: Write evidence doc**

Include:

- commands run
- test results
- launch screenshots path if screenshots are captured outside git
- standalone/remote mode status
- packaging status
- known limitations

**Step 5: Commit**

```bash
git add -f specs/superpowers/demos/sprint-6-gui-tauri.md
git commit -s -m "test(demo): record sprint 6 GUI evidence"
```

## Task 6.15: Close Sprint 6 Retro

**Files:**
- Create: `specs/superpowers/retros/sprint-6.md`

**Step 1: Run final verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check --prefix crates/webui
npm run build --prefix crates/webui
git log --format='%h %s%n%B' main --not origin/main
```

If there is no remote, use:

```bash
git log --format='%h %s%n%B' --max-count 40
```

Expected:

- fmt PASS
- clippy PASS
- tests PASS
- WebUI check/build PASS
- every local commit has `Signed-off-by`
- no `Co-Authored-By`

**Step 2: Write retro**

Include:

- what shipped
- standalone mode evidence
- remote mode evidence
- packaging/signing evidence
- GUI smoke evidence
- Sprint 7 adjustments
- remaining GUI and release risks

**Step 3: Verify retro references**

Run:

```bash
rg -n "What Shipped|Standalone|Remote|Packaging|Test Evidence|Remaining Risks|Sprint 7" specs/superpowers/retros/sprint-6.md
```

Expected: all sections exist.

**Step 4: Commit**

```bash
git add -f specs/superpowers/retros/sprint-6.md
git commit -s -m "docs(retro): close sprint 6"
```

## Sprint 6 Acceptance Criteria

Sprint 6 is complete only when:

1. `cargo test --workspace` passes.
2. `cargo clippy --workspace --all-targets -- -D warnings` passes or every failure is documented as unrelated pre-existing debt.
3. `npm run check --prefix crates/webui` passes.
4. `npm run build --prefix crates/webui` passes.
5. `lsi-gui` is a real Tauri app and not a stub crate.
6. Remote mode can load daemon status from a configured endpoint.
7. Standalone mode can start or clearly fail to start the local daemon with actionable diagnostics.
8. WAN/direct-path diagnostics are visible in the GUI.
9. Packaging/signing/updater status is documented with exact local gaps.
10. Demo evidence and retro are written.

## Sprint 7 Adjustments To Consider

- Decide whether desktop package signing blocks 1.0 or remains a documented pre-release limitation.
- Run full GUI smoke on macOS, Windows, and Linux before 1.0.
- Add release workflow jobs for desktop bundles only after local packaging is stable.
- Promote WAN metrics and GUI diagnostics into release notes.
- Re-run LocalSend interop matrix and native resume soak tests on `main`.
- Prepare final docs site, SBOM, checksums, signatures, and audit invite.
