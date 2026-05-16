# Rendezvous Privacy Model

LocalSend Improved WAN rendezvous is a control-plane service. It helps peers find each other; it is not a relay and it does not proxy transfers.

## What Rendezvous Sees

A rendezvous server can see:

- peer public keys and derived fingerprints
- peer aliases
- local and server-reflexive IP:port candidates
- registration and lookup timestamps
- source IPs for QUIC connections to the rendezvous server
- notify requests between registered peers

Operators can still log connection metadata. Treat a rendezvous server as metadata-sensitive infrastructure.

## What Rendezvous Does Not See

Rendezvous does not see file names, file bytes, file hashes, transfer contents, or native transfer control messages exchanged after peers connect directly.

File transfer traffic uses the native QUIC path between peers. The rendezvous server only handles registration, lookup, and notify messages.

## No Relay In Base V1

There is no relay in the open-base v1 build. If candidate-pair dialing fails because of symmetric NAT or firewall policy, the transfer fails with an actionable no-direct-path diagnostic instead of falling back to a relay.

This keeps the base rendezvous service simpler and prevents accidental file-byte proxying through an operator-controlled server.

## Privacy Boundary

Self-hosting is the default privacy boundary. Run rendezvous on infrastructure you operate or trust, and configure daemons to use that rendezvous URL explicitly.

There is no default public rendezvous service. A public service would centralize peer metadata and should be treated as a separate product/security decision.

## Operational Guidance

- Use a dedicated UDP port and firewall only that port.
- Keep rendezvous logs scoped and retained only as long as needed.
- Avoid collecting packet captures unless debugging a specific incident.
- Rotate aliases if they reveal more than you intend.
- Prefer short registration TTLs for mobile or roaming peers.
