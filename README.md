# NightBridge

> A LocalSend-compatible headless server for homelabs, NAS boxes, and always-on machines.

**Status:** pre-alpha. The first release is focused on the original problem:
run a LocalSend-compatible receiver without a desktop app open. The repository
now includes the headless daemon, CLI, LocalSend v2 receive/send paths, TUI,
Docker/systemd/DEB packaging work, Native Mesh LAN preview, WAN
rendezvous work, and Tauri desktop GUI scaffolding. Desktop packages are
pre-release until signing, notarization, and updater infrastructure are
configured. See the [26.5 release notes draft](docs/release/26.5-notes.md),
[release artifact matrix](docs/release/artifacts.md), and
[operator guide](docs/operators/index.md).

## Why?

LocalSend is useful on phones and desktops. It is a poor fit for a NAS, a
Raspberry Pi, or a homelab server: there is no headless mode, no first-class
CLI, no daemon, and no API. This project fills that gap while staying
compatible with the LocalSend ecosystem, so users can send files between
LocalSend on their phone and this daemon on their server without extra setup.

## First Release Focus

NightBridge 26.5 is aimed at operators who want a LocalSend endpoint on a
headless machine:

- run `night-bridge-daemon` as an always-on receiver
- receive from official LocalSend apps into a configured inbox
- approve unknown LocalSend peers before accepting files
- operate the daemon through `night-bridge` / `nbrg`, HTTP/gRPC APIs, or TUI
- deploy with systemd, Docker, or release binaries

Native Mesh LAN is the alpha differentiator: trusted NightBridge daemons on the
same network can discover each other and send files over native QUIC by peer
alias. WAN rendezvous, WebUI, and desktop GUI work remain advanced paths; the
core promise is LocalSend-to-headless-server plus a preview of stricter
server-to-server mesh transfers.

## Native Mesh LAN Preview

Use LocalSend compatibility for phones and desktops. Use Native Mesh LAN for
trusted NightBridge servers on the same LAN:

```bash
night-bridge peers list-native
night-bridge peers approve-native "Home NAS" --label "Home NAS"
night-bridge send --native --peer "Home NAS" ./backup.tar
```

The alpha mesh path uses native QUIC with pinned TLS certificate metadata and
the daemon trust database. It is a LAN preview, not a WAN relay or multi-hop
router.

## User Docs

- Quickstart: [docs/users/quickstart.md](docs/users/quickstart.md)
- Migration from LocalSend: [docs/users/migration-from-localsend.md](docs/users/migration-from-localsend.md)

## WAN Rendezvous

Sprint 5 adds a self-hosted rendezvous service for WAN discovery. Rendezvous is
control-plane only: it registers peer candidates and helps peers attempt direct
native QUIC connections. It does not relay file bytes, and there is no default
public rendezvous service.

- Deployment guide: [docs/deploy/rendezvous-on-a-vps.md](docs/deploy/rendezvous-on-a-vps.md)
- Privacy model: [docs/security/rendezvous-privacy.md](docs/security/rendezvous-privacy.md)

## Desktop GUI

Sprint 6 adds a Tauri desktop app in `crates/gui` that bundles the existing
Svelte WebUI from `crates/webui`. It supports remote daemon mode for connecting
to an existing daemon endpoint and standalone local daemon mode for starting a
local daemon process from the desktop app.

Useful GUI checks:

```bash
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo check -p lsi-gui
```

Local Tauri package build:

```bash
cd crates/gui
../webui/node_modules/.bin/tauri build
```

Packaging and signing notes live in
[docs/deploy/desktop-packaging.md](docs/deploy/desktop-packaging.md).

## Operations

Operator setup, service management, and troubleshooting live in the
[operator guide](docs/operators/index.md).

## Release

- Changelog: [CHANGELOG.md](CHANGELOG.md)
- 26.5 release notes draft: [docs/release/26.5-notes.md](docs/release/26.5-notes.md)
- Release artifact matrix: [docs/release/artifacts.md](docs/release/artifacts.md)
- Versioning policy: [docs/release/versioning.md](docs/release/versioning.md)

## Security

Security design and operator-facing risk notes:

- Security policy: [SECURITY.md](SECURITY.md)
- Threat model: [docs/security/threat-model.md](docs/security/threat-model.md)
- Audit invite packet: [docs/security/audit-invite.md](docs/security/audit-invite.md)
- Rendezvous privacy model: [docs/security/rendezvous-privacy.md](docs/security/rendezvous-privacy.md)
- WebUI token bootstrap: [docs/security/webui-token-bootstrap.md](docs/security/webui-token-bootstrap.md)

## License

- **Base** (this repo): AGPL-3.0-only.
- **VIP crates** (separate repo, when shipped): BSL 1.1.
- All contributions require **DCO sign-off** (`git commit -s`). No CLA is
  required, and by deliberate design, **no CLA exists**, so the base cannot be
  relicensed away from AGPL-3.0 by maintainers alone.

See the [threat model](docs/security/threat-model.md) and [operator guide](docs/operators/index.md) for the current architecture and deployment boundaries.
