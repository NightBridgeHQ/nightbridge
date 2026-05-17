# Architecture

See the full design at [`specs/superpowers/specs/2026-05-10-night-bridge-design.md`](specs/superpowers/specs/2026-05-10-night-bridge-design.md).

## TL;DR

- **`crates/core`**: protocol-agnostic library. No direct I/O in core protocol
  primitives; filesystem-backed implementations are behind small traits.
- **`crates/daemon`**: the long-running binary. Wires listeners, storage,
  policy, hooks, and the local API together. It is the single source of truth
  at runtime.
- **`crates/cli`**, **`crates/tui`**, **`crates/webui`**, **`crates/gui`**:
  thin clients of the daemon's local API. They do not re-implement protocol
  logic.
- **`crates/protocol-localsend-v2`**: bidirectional compatibility with
  LocalSend v2 HTTP protocol. Receives from and sends to vanilla LocalSend.
- **`crates/protocol-native-v1`**: native QUIC+TLS1.3+Ed25519 protocol with
  persistent identities, resume, and extension negotiation.
- **`crates/rendezvous`**: separate binary; users self-host it for WAN
  discovery between two NATted peers.
