# Debian package skeleton

The daemon package is built with `cargo-deb` from the `lsi-daemon` manifest.

Expected output:

```bash
cargo build --release -p lsi-daemon
cargo deb -p lsi-daemon
```

The package installs:

- `/usr/bin/night-bridge-daemon`
- `/lib/systemd/system/night-bridge.service`
- `/etc/night-bridge/config.toml`

Use `packaging/build-packages.sh --check-tools` to verify local tool
availability before attempting a package build.

NightBridge 26.5 pins Rust 1.78, so install a compatible `cargo-deb` release:

```bash
cargo install cargo-deb --version 3.6.2 --locked
```
