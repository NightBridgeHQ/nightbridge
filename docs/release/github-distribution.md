# GitHub Distribution

NightBridge 26.5 uses GitHub Releases as the only public distribution channel.
APT repositories, PPAs, Homebrew taps, and platform app stores are deferred.

## Release Assets

Attach these files to the GitHub release tagged `26.5.0-alpha`:

- `nightbridge-26.5.0-alpha-linux-amd64.tar.gz`
- `nightbridge-26.5.0-alpha-linux-arm64.tar.gz`
- `nightbridge-26.5.0-alpha-macos-arm64.tar.gz`
- `night-bridge-daemon_26.5.0-1_amd64.deb`
- `SHA256SUMS`
- `sbom.cdx.json`

Each tarball contains the validated binaries available for that platform:

- `night-bridge`
- `nbrg`
- `night-bridge-daemon`
- `night-bridge-tui`

Build a tarball from a validated checkout with:

```bash
cargo build --release -p lsi-cli -p lsi-daemon -p lsi-tui
packaging/release/archive-binaries.sh 26.5.0 dist
packaging/release/checksums.sh dist
```

Windows binaries may be attached manually as a zip when validated, but the
curl installer is scoped to Linux and macOS for 26.5.

The Debian package is published as a direct GitHub release download. APT and
PPA repository setup are explicitly deferred.

Install the Debian package manually with:

```bash
curl -fLO https://github.com/NightBridgeHQ/nightbridge/releases/download/26.5.0-alpha/night-bridge-daemon_26.5.0-1_amd64.deb
sudo apt install ./night-bridge-daemon_26.5.0-1_amd64.deb
```

## Installer

The installer downloads a tarball from GitHub Releases, verifies it against
`SHA256SUMS`, and installs binaries into `/usr/local/bin` by default.

```bash
curl -fsSL https://raw.githubusercontent.com/NightBridgeHQ/nightbridge/main/install.sh | sh
```

For a pinned release:

```bash
curl -fsSL https://raw.githubusercontent.com/NightBridgeHQ/nightbridge/main/install.sh | sh -s -- --version 26.5.0-alpha
```

For a different repository or install directory:

```bash
curl -fsSL https://raw.githubusercontent.com/NightBridgeHQ/nightbridge/main/install.sh | \
  sh -s -- --repo NightBridgeHQ/nightbridge --version 26.5.0-alpha --install-dir "$HOME/.local/bin"
```

## Trust Boundary

The installer is intentionally small:

- it detects OS and architecture
- it downloads one release tarball and `SHA256SUMS`
- it verifies the tarball checksum before extraction
- it installs only known NightBridge binary names
- it does not configure systemd, shell profiles, or auto-updates

Users who do not want `curl | sh` can download the same tarball and verify it
manually:

```bash
curl -fLO https://github.com/NightBridgeHQ/nightbridge/releases/download/26.5.0-alpha/nightbridge-26.5.0-alpha-linux-amd64.tar.gz
curl -fLO https://github.com/NightBridgeHQ/nightbridge/releases/download/26.5.0-alpha/SHA256SUMS
grep '  nightbridge-26.5.0-alpha-linux-amd64.tar.gz$' SHA256SUMS | shasum -a 256 -c -
tar -xzf nightbridge-26.5.0-alpha-linux-amd64.tar.gz
```
