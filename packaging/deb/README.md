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

Use `packaging/build-packages.sh --deb-only --check-tools` to verify local tool
availability before attempting a Debian package build. Use
`packaging/build-packages.sh --deb-only` for the 26.5 Debian/systemd release
smoke because RPM packaging is deferred for this release.

NightBridge 26.5 pins Rust 1.78, so install a compatible `cargo-deb` release:

```bash
cargo install cargo-deb --version 3.6.2 --locked
```

The daemon build also requires `protoc`:

```bash
sudo apt-get install protobuf-compiler
```
