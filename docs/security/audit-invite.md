# Security Audit Invite

This packet summarizes the intended security audit request for the NightBridge
26.5 candidate.

## Project Overview

NightBridge adds a headless daemon, CLI, API, WebUI/TUI, native QUIC
protocol, WAN rendezvous, and desktop GUI scaffolding around LocalSend v2
compatibility. The target users are homelab, NAS, server, and workstation
operators who need file transfer without keeping the official desktop app open.

## Audit Scope

- Native protocol TLS 1.3 transport and Ed25519 identity flow.
- Native trust gate, including certificate fingerprint pinning and daemon
  trusted-peer policy enforcement.
- Daemon HTTP/gRPC API authentication and token bootstrap.
- LocalSend v2 compatibility boundary and self-signed TLS exception isolation.
- WAN rendezvous privacy and direct-path candidate handling.
- Hook/webhook execution boundary and secret exposure risk.
- Release, SBOM, checksum, signing, notarization, and update pipeline design.

## Out Of Scope

- VIP relay, public relay, or commercial fallback services not present in the
  base repository.
- Third-party LocalSend app internals.
- Hosted infrastructure outside the self-hosted examples.
- Mobile app implementation, because this repo does not ship one.

## Preferred Auditors To Contact

No third-party auditor has been selected for 26.5. Because NightBridge is open
source, the first practical path is to publish the audit packet, invite public
review, and then seek a focused paid review for the protocol and release
pipeline when budget is available.

Suggested profiles:

- Rust networking and QUIC/TLS auditor.
- Desktop release/signing pipeline auditor.
- API authentication and local-secret handling reviewer.

## Budget Range

Budget is not allocated for the 26.5 release candidate. The expected paid audit
shape is a small fixed-scope review first, then focused follow-up if native
trust or release signing findings require deeper work.

## Artifacts To Send

- Repository commit SHA and release branch.
- [Threat model](threat-model.md)
- [Native trust audit](native-trust-audit.md)
- [Rendezvous privacy model](rendezvous-privacy.md)
- [Release artifact pipeline](../release/artifacts.md)
- Test evidence for native trust, soak, interop, GUI smoke, and release scripts.
- Known limitations and pre-release signing status.

## Known Risks

- No completed third-party audit yet.
- Public open-source review is welcome, but it is not equivalent to a completed
  independent audit.
- Desktop package signing and notarization are not production-ready.
- Rendezvous server certificate trust model still needs operator hardening
  before public deployment.
- LocalSend v2 compatibility intentionally preserves official-app self-signed
  TLS behavior inside the compatibility protocol.
- Existing trusted peers without native certificate metadata cannot auto-send
  over native WAN until refreshed or re-paired.
