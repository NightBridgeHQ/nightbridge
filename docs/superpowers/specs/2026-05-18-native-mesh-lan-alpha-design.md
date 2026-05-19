# Native Mesh LAN Alpha Design

## Goal

Ship `26.5.0-alpha.1` as LocalSend-compatible headless receive plus a Native Mesh LAN preview for trusted NightBridge servers on the same network.

## Scope

- Discover NightBridge daemons on the LAN through native mDNS.
- Show native peers with alias, address, Ed25519 fingerprint, QUIC port, extensions, and native TLS certificate fingerprint.
- Approve a discovered native peer into the trust database with `auto_accept` policy and pinned certificate fingerprint.
- Send files over native QUIC by peer alias or fingerprint without a manual `quic://` URL.
- Reject incoming native transfers unless the sender is trusted and allowed by policy.

## Non-Goals

- No WAN relay.
- No production desktop signing.
- No public promise of retry/resume robustness in alpha copy, even though native transfer internals already contain chunk and resume foundations.
- No multi-hop routing; "mesh" means trusted server-to-server LAN peers, not relay topology.

## UX

```bash
night-bridge peers list-native
night-bridge peers approve-native nas --label "Home NAS"
night-bridge send --native --peer "Home NAS" ./file.iso
```

The release framing is:

> Use LocalSend compatibility for phones and desktop apps; use Native Mesh LAN for stricter, efficient transfers between trusted NightBridge servers.

## Acceptance

- Two daemons on the same LAN discover each other.
- Admin approves the peer once.
- `send --native --peer` transfers a file without manual URL or certificate flags.
- Unknown native senders are rejected by the daemon.
- Docs and release notes call this a preview.
