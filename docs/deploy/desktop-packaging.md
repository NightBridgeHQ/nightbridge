# Desktop Packaging, Signing, And Updates

Sprint 6 ships local Tauri packaging scaffolding for the desktop GUI. Packages
are pre-release until signing, notarization, and updater infrastructure are
configured with production credentials.

## Local Build

Build the WebUI and macOS bundle from this workspace:

```bash
npm run build --prefix crates/webui
cd crates/gui
../webui/node_modules/.bin/tauri build
```

On this machine, the Tauri build produced:

```text
target/release/bundle/dmg/NightBridge_26.5.0_aarch64.dmg
```

The Tauri CLI runs `beforeBuildCommand` from the `crates/` directory, so the
configured command is:

```json
"beforeBuildCommand": "npm run build --prefix webui"
```

## macOS Signing And Notarization

Production macOS distribution still needs Apple Developer signing and
notarization. Required values must be provided by CI or the local shell and must
not be committed:

- `APPLE_ID`
- `APPLE_PASSWORD` or app-specific password
- `APPLE_TEAM_ID`
- signing identity or certificate import material
- keychain password and temporary keychain path, when CI imports certificates

Until those values are configured, local `.app` and `.dmg` builds are unsigned
developer artifacts.

## Windows Authenticode

Windows packages require an Authenticode certificate and private key material.
Required values must be injected as CI secrets or local environment variables:

- certificate path or base64-encoded certificate secret
- certificate password secret
- timestamp server URL
- signing identity metadata

Do not commit `.pfx`, `.p12`, private keys, passwords, or timestamping tokens.

## Linux AppImage

Linux AppImage packaging should be run on Linux with the Tauri system
dependencies installed. At minimum, CI or the packaging host needs WebKitGTK,
GTK, AppImage tooling, and the distro packages required by Tauri 2 for Linux
bundles.

This macOS machine did not validate Linux AppImage output.

## Updater Boundary

No production updater endpoint is configured in Sprint 6. This is deliberate:
there is no signed public release channel yet, and a placeholder endpoint could
make pre-release builds look update-capable when they are not.

When updater support is added, store the updater private key only as a CI secret
or local secret. Public update metadata should be generated as part of a signed
release job, not by regular development builds.

## Local Gaps Recorded

- macOS `.dmg` packaging passes locally for `aarch64-apple-darwin`.
- macOS notarization was not attempted because Apple Developer credentials are
  not configured in this workspace.
- Windows MSI packaging was not attempted on this macOS host.
- Windows Authenticode signing was not attempted because no signing certificate
  is configured.
- Linux AppImage packaging was not attempted on this macOS host.
- Updater signing and update metadata are not configured for Sprint 6.
