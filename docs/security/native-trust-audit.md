# Native Trust Audit

Sprint 7 hardens the native transfer protocol without weakening LocalSend v2
compatibility. This audit classifies every current trust bypass hit from:

```bash
rg -n "TrustAny|dangerous\(\)|danger_accept_invalid_certs|with_custom_certificate_verifier" crates
```

## Production Native Paths

| File | Current behavior | Required Sprint 7 outcome |
| --- | --- | --- |
| `crates/protocol-native-v1/src/client.rs` | Uses `TrustAnyServer` for native direct sends. Any certificate presented by the QUIC server is accepted. | Replace `TrustAnyServer` with native server verification that binds the server certificate fingerprint to the expected trusted peer. |
| `crates/protocol-native-v1/src/hole_punch.rs` | Uses `TrustAnyServer` for WAN candidate dialing. Any certificate presented by the selected candidate is accepted before transfer. | Verify the server certificate fingerprint against the expected trusted peer before a candidate can carry transfer traffic. |

These are production data-plane paths and are the primary Sprint 7 hardening
target. They must fail closed when the expected peer identity is missing,
blocked, unknown, or mismatched.

## Compatibility Exceptions

| File | Current behavior | Classification |
| --- | --- | --- |
| `crates/protocol-localsend-v2/src/client.rs` | Builds a `reqwest` client with `danger_accept_invalid_certs(true)` for LocalSend v2 HTTPS uploads. | Compatibility exception for official-app self-signed TLS behavior. Must not be reused by native protocol sends. |
| `crates/protocol-localsend-v2/src/discovery.rs` | Uses `danger_accept_invalid_certs(true)` when posting `/register` to a discovered LocalSend peer. | Compatibility exception for LocalSend v2 LAN discovery/register behavior. Must stay scoped to LocalSend v2. |
| `crates/protocol-localsend-v2/src/server.rs` | Uses `danger_accept_invalid_certs(true)` in tests. | Test-only LocalSend v2 compatibility helper. |

LocalSend v2 HTTPS compatibility may continue to accept official-app
self-signed TLS behavior where explicitly documented. These paths must not be
used by the native protocol.

## Rendezvous Boundary

| File | Current behavior | Required Sprint 7 outcome |
| --- | --- | --- |
| `crates/rendezvous/src/client.rs` | Uses `TrustAnyServer` for rendezvous QUIC client connections. | Treat as a control-plane risk. Before public deployment, replace with explicit certificate pinning or a documented operator trust model. |
| `crates/rendezvous/src/server.rs` | Defines a test-only `TrustAnyServer` helper for rendezvous server tests. | Keep scoped to tests or replace with a pinned test certificate helper. |

Rendezvous is control-plane only and must not authorize native transfer data.
It can help discover candidates, but native transfer trust still has to be
bound at the data-plane certificate boundary.

## Test-Only Helpers

| File | Current behavior | Classification |
| --- | --- | --- |
| `crates/daemon/src/main.rs` | `native_test_client_endpoint()` defines a test-local `TrustAnyServer` verifier for daemon integration tests. | Test-only helper. If production native verification changes require it, convert tests to use an explicit expected certificate fingerprint instead of blanket trust. |

## Required Negative Tests

- Unknown native peer is rejected.
- Blocked native peer is rejected.
- Advertised native fingerprint mismatch is rejected.
- Server TLS certificate identity mismatch is rejected.
- Trusted auto-accept peer succeeds.
