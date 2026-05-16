# LocalSend Improved

> Headless-first file transfer for homelab and server use cases.
> Bidirectional compatibility with LocalSend v2 plus a native QUIC+TLS1.3+Ed25519 protocol.

**Status:** pre-alpha. The repository now includes the headless daemon, CLI, LocalSend v2 receive/send paths, native QUIC transfer foundations, and Sprint 5 WAN rendezvous work for self-hosted candidate registration, lookup, and direct-path diagnostics. See [the v1 roadmap](docs/superpowers/plans/2026-05-10-localsend-improved-v1.md) and [Sprint 5 plan](docs/superpowers/plans/2026-05-15-sprint-5-wan-rendezvous.md).

## Why?

LocalSend is useful on phones and desktops. It is a poor fit for a NAS, a
Raspberry Pi, or a homelab server: there is no headless mode, no first-class
CLI, no daemon, and no API. This project fills that gap while staying
compatible with the LocalSend ecosystem, so users can send files between
LocalSend on their phone and this daemon on their server without extra setup.

## WAN Rendezvous

Sprint 5 adds a self-hosted rendezvous service for WAN discovery. Rendezvous is
control-plane only: it registers peer candidates and helps peers attempt direct
native QUIC connections. It does not relay file bytes, and there is no default
public rendezvous service.

- Deployment guide: [docs/deploy/rendezvous-on-a-vps.md](docs/deploy/rendezvous-on-a-vps.md)
- Privacy model: [docs/security/rendezvous-privacy.md](docs/security/rendezvous-privacy.md)

## License

- **Base** (this repo): AGPL-3.0-only.
- **VIP crates** (separate repo, when shipped): BSL 1.1.
- All contributions require **DCO sign-off** (`git commit -s`). No CLA is
  required, and by deliberate design, **no CLA exists**, so the base cannot be
  relicensed away from AGPL-3.0 by maintainers alone.

See [the design spec](docs/superpowers/specs/2026-05-10-localsend-improved-design.md) for the full architecture.
