# Native Mesh LAN Alpha Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a demoable Native Mesh LAN preview for alpha: discover trusted NightBridge daemons and send files over native QUIC by alias.

**Architecture:** Extend existing native mDNS advertisements to include the daemon TLS certificate fingerprint, expose native LAN discovery through the daemon peer API, and reuse the existing trust store as the approval boundary. Native sends by peer resolve a fresh LAN advertisement, verify the peer is trusted with pinned cert metadata, then call the existing native QUIC client.

**Tech Stack:** Rust 1.78, tonic/prost, mdns-sd, quinn/rustls, rusqlite trust store, existing CLI and daemon APIs.

---

### Task 1: Native Discovery Metadata

**Files:**
- Modify: `crates/protocol-native-v1/src/dto.rs`
- Modify: `crates/protocol-native-v1/src/discovery.rs`
- Modify: `crates/daemon/src/main.rs`

- [ ] Add optional `native_certificate_fingerprint` to native peer advertisements.
- [ ] Add discovery output that includes resolved source address.
- [ ] Advertise the daemon's current native TLS certificate fingerprint.
- [ ] Test TXT roundtrip and resolved peer parsing.

### Task 2: Native Peer API

**Files:**
- Modify: `crates/proto/proto/lsi/peers/v1/peers.proto`
- Modify: `crates/daemon/src/api/peers.rs`
- Modify: `crates/cli/src/daemon_client.rs`
- Modify: `crates/cli/src/cmd/peers.rs`

- [ ] Add `ListNativeLan` and `TrustNativeLan` RPCs.
- [ ] Print `night-bridge peers list-native`.
- [ ] Implement `night-bridge peers approve-native <peer> --label <label>`.
- [ ] Test empty and fixture discovery paths.

### Task 3: Native Send By Peer

**Files:**
- Modify: `crates/cli/src/cmd/send.rs`
- Modify: `crates/daemon/src/api/transfers.rs`

- [ ] Allow `night-bridge send --native --peer <alias|fingerprint> <file>`.
- [ ] Resolve the native peer from fresh discovery.
- [ ] Require trusted `auto_accept` peer with pinned certificate fingerprint.
- [ ] Send through `NativeTransferClient::send_files_to_url`.

### Task 4: Native Receive Trust Gate

**Files:**
- Modify: `crates/daemon/src/main.rs`

- [ ] Pass trust DB path into the native accept loop.
- [ ] Reject unknown, blocked, prompt-policy, or missing-trust native senders.
- [ ] Preserve current loopback test by trusting the test peer first.

### Task 5: Docs And Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/release/26.5-notes.md`
- Modify: `docs/users/quickstart.md`

- [ ] Document Native Mesh LAN Preview.
- [ ] Run focused Rust tests for native discovery, daemon peers, daemon transfers, CLI smoke.
- [ ] Run `cargo fmt --check`, `cargo build -p lsi-daemon -p lsi-cli`, and `git diff --check`.
