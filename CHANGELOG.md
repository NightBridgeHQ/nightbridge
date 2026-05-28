# Changelog

All notable changes to NightBridge will be documented in this file.

This project follows the spirit of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

- No unreleased changes yet.

## [26.5.0-alpha] - 2026-05-28

### Added

- Rust workspace for the headless NightBridge stack, including core,
  daemon, CLI, protocol, TUI, WebUI, GUI, and rendezvous crates.
- Headless daemon for server, NAS, Raspberry Pi, and homelab deployment.
- CLI for daemon status, inbox operations, LocalSend-compatible sends, native
  sends, WAN lookup, and send-by-peer flows.
- LocalSend v2 compatibility path for receiving from and sending to official
  LocalSend peers on LAN.
- Native QUIC + TLS 1.3 transfer protocol with Ed25519 identity material,
  resumable transfer foundations, and focused native protocol tests.
- HTTP/gRPC daemon API protected by bearer tokens.
- WebUI and TUI surfaces for daemon status and operations.
- WebUI token bootstrap policy that keeps API requests authenticated instead
  of exposing an unauthenticated token endpoint.
- Hook system with stable event schema, HMAC-signed webhooks, exec hooks, and
  daemon dispatcher integration.
- Prometheus metrics plus `/metrics`, `/healthz`, and `/readyz` endpoints.
- Configurable daemon logging formats: `json`, `pretty`, and `compact`.
- systemd unit, Docker skeleton, DEB/RPM packaging metadata, release workflow
  skeleton, checksum script, and SBOM script.
- Python SDK generation/bootstrap scripts and example usage.
- Self-hosted WAN rendezvous service for native peer registration, lookup, and
  notification.
- WAN candidate discovery, dedupe, priority ordering, and direct candidate-pair
  dialing for native QUIC transfers.
- WAN diagnostics for no relay, failed candidate attempts, and likely symmetric
  NAT or firewall failures.
- Linux network namespace WAN smoke harness gated behind explicit opt-in.
- Tauri 2 desktop GUI that bundles the Svelte WebUI.
- Desktop remote daemon mode and standalone local daemon mode.
- GUI surfacing of daemon WAN direct-path failure diagnostics.
- Local Tauri smoke harness and GUI smoke documentation.
- Operator, user, deployment, security, interop, release artifact, and docs-site
  documentation.
- mdBook wrapper for browsing the repository documentation as a small docs site.
- Security threat model, native trust audit, audit invite packet, and security
  disclosure policy.
- Repeatable native soak harness and LocalSend interop matrix documentation.

### Changed

- Native production send paths now require trusted peer certificate identity
  instead of accepting arbitrary server certificates.
- Daemon native transfer paths reject unknown or blocked trusted peers before
  dialing.
- Direct native URL sends require explicit certificate fingerprint material
  when trust metadata is not available.
- LocalSend v2 self-signed TLS compatibility remains isolated to the LocalSend
  compatibility path and is not reused by native protocol sends.
- Desktop WebUI API calls support a configurable daemon API base for remote
  daemon mode.
- WebUI bundle paths are compatible with Tauri packaging.
- Tauri dependency selections are pinned to stay compatible with the Rust 1.78
  project baseline.
- Release artifact documentation distinguishes required binaries from optional
  unsigned desktop bundles and package formats.

### Security

- Native QUIC clients verify the server certificate fingerprint against the
  expected trusted peer before transfer traffic is allowed.
- WAN candidate dialing uses the same native certificate verification boundary
  as direct native sends.
- Rendezvous remains a control-plane service only; it does not relay file bytes
  or authorize transfer data.
- Trust policy enforcement rejects unknown and blocked native peers by default.
- Daemon API access remains bearer-token protected.
- WebUI token bootstrap avoids adding an unauthenticated token recovery
  endpoint.
- Threat model documents trusted assets, attacker assumptions, security
  boundaries, and known limitations for 26.5.
- Security policy and audit invite packet are ready for external review, but no
  third-party audit has been completed yet.

### Fixed

- LocalSend LAN discovery builds in final preflight by enabling the `socket2`
  feature required for multicast `SO_REUSEPORT` setup on Unix hosts.
- Docker release builds use the pinned Rust 1.93.1 toolchain instead of a
  floating `stable` compiler.

### Known Limitations

- `26.5.0-alpha` is published as a GitHub pre-release with validated Linux and
  macOS server tarballs, direct Debian package asset, SBOM, checksums, and
  installer smoke evidence.
- Desktop packages are unsigned pre-release artifacts until platform signing,
  notarization, and updater infrastructure are configured.
- Windows and Linux desktop packages need platform-native validation.
- The GUI WebDriver smoke path requires `tauri-driver` and an interactive-capable
  host to exercise the real app window.
- WAN transfers have no relay fallback; symmetric NAT, carrier-grade NAT, strict
  firewalls, and UDP-blocking networks can prevent direct transfer.
- Rendezvous state is in-memory and disappears on server restart.
- No default public rendezvous service is configured; operators must self-host.
- Existing trusted peers without native certificate metadata must be refreshed
  or re-paired before native WAN auto-send is allowed.
- RPM, APT/PPA, Homebrew, production desktop signing/notarization, and strict
  GUI WebDriver coverage remain deferred.
