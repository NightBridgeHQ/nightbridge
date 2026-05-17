# Security Threat Model

This document describes the security boundaries for the NightBridge 26.5
candidate. It is scoped to the open-base repository and does not cover future
VIP relay services.

## Assets

- User files in transit and in the daemon inbox.
- Ed25519 identity keys used for native peer identity.
- `trust.db`, including peer policies and native certificate pins.
- Daemon API bearer token and token file permissions.
- Hook environment, webhook secrets, and outbound hook payloads.
- Release, updater, signing, and packaging keys.
- CI credentials and generated release artifacts.

## Actors

- LAN attacker able to spoof discovery, observe traffic, or connect to open
  daemon ports.
- WAN rendezvous observer able to see peer registration metadata and candidate
  lookups.
- Malicious or compromised peer with a valid-looking LocalSend or native
  endpoint.
- Compromised CI runner, build dependency, or release artifact publisher.
- Local user on the daemon host with access to config, token, inbox, or trust
  store files.

## Trust Boundaries

- LocalSend v2 compatibility accepts official-app self-signed HTTPS behavior
  only inside the LocalSend compatibility protocol path.
- LocalSend v2 receive authorization is controlled by
  `--localsend-receive-policy`. The default `prompt` mode rejects incoming
  uploads; `trusted` requires LocalSend fingerprints approved in the daemon
  trust database or allowlisted through config/file; `auto` is explicit
  trusted-LAN compatibility mode.
- Native QUIC uses TLS 1.3 and Ed25519 peer identity, with outbound sends pinned
  to trusted peer certificate fingerprints before transfer data is sent.
- Daemon API access is protected by a bearer token and should be bound only to
  trusted interfaces unless the operator adds an external access control layer.
- GUI bridge code must treat daemon endpoints and tokens as local secrets.
- Hook execution crosses from transfer events into operator-provided commands or
  webhooks and must not expose secrets by default.
- Packaging and release workflows cross from source code into binaries,
  checksums, SBOMs, signatures, and updater metadata.

## Mitigations

- Bearer-token authentication protects HTTP and gRPC daemon APIs.
- Incoming LocalSend-compatible uploads are rejected by default unless the
  operator chooses trusted fingerprint approval through the daemon API/CLI,
  config/file allowlisting, or explicit auto-accept.
- Native peers advertise Ed25519 public keys and stable fingerprints.
- Trust policies distinguish `auto_accept`, `prompt`, and `block`.
- Native outbound WAN sends require trusted peer records and pinned certificate
  metadata.
- Native certificate verification rejects mismatched server certificate
  fingerprints.
- Direct-path WAN diagnostics explain failed candidate dialing without silently
  falling back to a relay.
- The open-base v1 build has no relay, so rendezvous cannot carry file bytes.
- LocalSend compatibility exceptions are documented and kept separate from the
  native protocol.

## Known Limitations

- Desktop packages are unsigned until platform signing and notarization
  credentials are configured.
- LocalSend v2 compatibility intentionally permits official-app self-signed TLS
  behavior inside that protocol path.
- Rendezvous server certificates still need an operator trust model before
  public deployment.
- There is no completed third-party security audit.
- Existing trusted peers without native certificate metadata cannot auto-send
  over native WAN until they are re-paired or refreshed.
- Local daemon compromise can expose inbox contents, tokens, hook secrets, and
  trust metadata.

## Disclosure

Until a dedicated security contact is published, report security issues through
the private maintainer channel for this repository. Do not publish exploit
details before the maintainer has confirmed receipt and a remediation window.
