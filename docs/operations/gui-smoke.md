# GUI Smoke Testing

The GUI smoke path verifies that the Svelte WebUI builds, the `lsi-gui` Tauri
crate type-checks, and WebDriver tooling is available when a strict GUI run is
requested.

## Install Tooling

Install `tauri-driver` when you want the strict WebDriver check:

```bash
cargo install tauri-driver --locked
```

Linux runners may also need platform WebKit, GTK, and AppIndicator development
packages before a full Tauri launch can run.

## Local Run

Default gated run:

```bash
cargo test --test gui_smoke -- --nocapture
```

Run the smoke path:

```bash
LSI_RUN_GUI_SMOKE=1 cargo test --test gui_smoke -- --nocapture
```

If `tauri-driver` is missing, this exits successfully in non-strict mode and
prints the install hint.

## Strict CI Run

Strict mode fails when `tauri-driver` is missing or does not expose WebDriver
status:

```bash
LSI_RUN_GUI_SMOKE=1 LSI_GUI_SMOKE_STRICT=1 cargo test --test gui_smoke -- --nocapture
```

The script accepts:

- `LSI_GUI_SMOKE_STRICT=1` to require `tauri-driver`
- `LSI_GUI_SMOKE_WEBDRIVER_PORT=4444` to choose the local WebDriver port
- `LSI_GUI_SMOKE_LOG=target/gui-smoke/tauri-driver.log` to choose the driver log

## Known Prerequisites

- `npm ci --prefix crates/webui` before WebUI builds on a fresh checkout.
- Rust toolchain with the workspace MSRV-compatible dependencies resolved.
- `curl` for the bounded WebDriver `/status` check.
- `tauri-driver` for strict smoke verification.
- Platform desktop libraries for a future full app launch check.
