# RPM package skeleton

The daemon package is built with `cargo-generate-rpm` from the `lsi-daemon`
manifest.

Expected output:

```bash
cargo build --release -p lsi-daemon
cargo generate-rpm -p lsi-daemon
```

The package installs:

- `/usr/bin/night-bridge-daemon`
- `/usr/lib/systemd/system/night-bridge.service`
- `/etc/night-bridge/config.toml`

Use `packaging/build-packages.sh --check-tools` to verify local tool
availability before attempting a package build.
