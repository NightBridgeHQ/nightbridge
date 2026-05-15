# Debian package skeleton

The daemon package is built with `cargo-deb` from the `lsi-daemon` manifest.

Expected output:

```bash
cargo build --release -p lsi-daemon
cargo deb -p lsi-daemon
```

The package installs:

- `/usr/bin/localsend-improved-daemon`
- `/lib/systemd/system/localsend-improved.service`
- `/etc/localsend-improved/config.toml`

Use `packaging/build-packages.sh --check-tools` to verify local tool
availability before attempting a package build.
