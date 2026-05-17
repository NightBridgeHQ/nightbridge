# LocalSend Improved

> Headless-first file transfer for homelab and server use cases.
> Bidirectional compatibility with LocalSend v2 plus a native QUIC+TLS1.3+Ed25519 protocol.

**Status:** pre-alpha. The repository now includes the headless daemon, CLI, LocalSend v2 receive/send paths, native QUIC transfer foundations, Sprint 5 WAN rendezvous work, and Sprint 6 Tauri desktop GUI scaffolding. Desktop packages are pre-release until signing, notarization, and updater infrastructure are configured. See [the v1 roadmap](docs/superpowers/plans/2026-05-10-localsend-improved-v1.md), [Sprint 5 plan](docs/superpowers/plans/2026-05-15-sprint-5-wan-rendezvous.md), and [Sprint 6 GUI plan](docs/superpowers/plans/2026-05-16-sprint-6-gui-tauri.md).

## Why?

LocalSend is useful on phones and desktops. It is a poor fit for a NAS, a
Raspberry Pi, or a homelab server: there is no headless mode, no first-class
CLI, no daemon, and no API. This project fills that gap while staying
compatible with the LocalSend ecosystem, so users can send files between
LocalSend on their phone and this daemon on their server without extra setup.

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

See [the design spec](docs/superpowers/specs/2026-05-10-localsend-improved-design.md) for the full architecture.
