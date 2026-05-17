# Desktop Operations

The desktop GUI is a Tauri app that bundles the WebUI and can operate against a
daemon.

## Remote Daemon Mode

Use remote daemon mode when a server already runs `night-bridge-daemon`.

Operators need:

- daemon API endpoint
- API token
- network path from the desktop to the daemon API
- firewall policy allowing only trusted clients

Remote mode keeps file-transfer state on the daemon host.

## Standalone Mode

Standalone mode starts a local daemon process from the desktop app. Use it for
single-machine desktop workflows where a separate system service is not wanted.

Operators need:

- writable local state path
- local inbox path
- LocalSend and native ports that are not already in use
- permission for the app to start the daemon binary

If standalone startup fails, verify the daemon binary path, port availability,
state directory permissions, and API token bootstrap.

## Token Setup

The GUI needs a daemon API token in both remote and standalone modes. Treat it
as a secret. Do not store production daemon tokens in screenshots, bug reports,
or public logs.

## Packaging Status

Desktop packaging is pre-release until platform signing, notarization, updater
metadata, and release channel policy are configured.

Build checks:

```bash
npm run build --prefix crates/webui
cargo check -p lsi-gui
```

Local Tauri build:

```bash
cd crates/gui
../webui/node_modules/.bin/tauri build
```

## Signing Limitations

- macOS packages are not production-notarized without Apple Developer secrets.
- Windows packages are not Authenticode-signed without certificate secrets.
- Linux packages may still show distro-specific trust warnings until repository
  signing is configured.
- Unsigned builds should be treated as pre-release test artifacts.
