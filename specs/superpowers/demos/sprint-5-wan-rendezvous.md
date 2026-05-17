# Sprint 5 WAN Rendezvous Demo Evidence

## Scope

Sprint 5 added:

- signed rendezvous protocol messages over QUIC
- in-memory peer registration, lookup, and notify queues
- daemon WAN rendezvous registration and lookup
- STUN-derived server-reflexive candidate gathering
- direct native QUIC dialing across ordered WAN candidates
- CLI commands for WAN lookup and send-by-peer
- diagnostics for direct-path failures without relay fallback
- a gated Linux network namespace smoke harness
- deployment and privacy docs for self-hosted rendezvous servers

## Rendezvous Startup Evidence

Command:

```bash
cargo run -p lsi-rendezvous -- --bind 127.0.0.1:53410 --max-ttl-seconds 300
```

Result:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
Running `target/debug/lsi-rendezvous --bind '127.0.0.1:53410' --max-ttl-seconds 300`
2026-05-16T03:21:50.150711Z  INFO lsi_rendezvous::server: rendezvous server started addr=127.0.0.1:53410
2026-05-16T03:21:50.150776Z  INFO lsi_rendezvous: rendezvous server ready addr=127.0.0.1:53410
```

The first sandboxed attempt failed with `Operation not permitted` while binding
the local socket, so the command above was rerun with local socket permission.

## Focused Test Evidence

Commands:

```bash
cargo test -p lsi-rendezvous
cargo test -p lsi-protocol-native-v1 candidates
cargo test -p lsi-protocol-native-v1 hole_punch
cargo test -p lsi-daemon wan
cargo test -p lsi-cli wan
```

Results:

```text
lsi-rendezvous: 17 passed; 0 failed
lsi-protocol-native-v1 candidates: 8 passed; 0 failed; 63 filtered out
lsi-protocol-native-v1 hole_punch: 7 passed; 0 failed; 64 filtered out
lsi-daemon wan: 9 passed; 0 failed; 57 filtered out
lsi-cli wan: 4 passed; 0 failed; 14 filtered out
```

Covered evidence:

- rendezvous register, lookup, notify, TTL capping, expiry, and missing-peer behavior
- client/server QUIC rendezvous round trips against a local test server
- local and STUN candidate gathering with dedupe and priority sorting
- direct candidate pair dialing through native QUIC
- explicit diagnostics for no direct WAN path, relay unavailable, and likely symmetric NAT cases
- daemon WAN registration, lookup, notify, retries, shutdown, and missing-peer errors
- CLI `lookup-wan` and `send --peer` command routing through the daemon API

## Workspace Test Evidence

Command:

```bash
cargo test --workspace
```

Result:

```text
test result: ok across workspace crates
```

The workspace run included CLI integration tests, daemon API tests, LocalSend v2
interop receive, native transfer/resume tests, rendezvous protocol/server/client
tests, TUI tests, WebUI embedding tests, and doc tests.

## Linux Netns Evidence

Command:

```bash
cargo test --test wan_netns -- --nocapture
```

Result on this machine:

```text
running 1 test
skipping WAN netns smoke: Linux network namespaces are required
test wan_netns_smoke_is_gated_and_runs_when_requested ... ok
```

The full smoke path is intentionally gated because it needs Linux network
namespaces and root or `CAP_NET_ADMIN`:

```bash
LSI_RUN_NETNS_TESTS=1 cargo test --test wan_netns -- --nocapture
```

or:

```bash
sudo bash scripts/wan-netns-smoke.sh
```

## Rendezvous Privacy Evidence

The rendezvous server does not proxy file bytes in this sprint. It only accepts
signed protocol messages for peer registration, lookup, and notify queues. File
transfer still happens over native QUIC after the daemon resolves a peer's
advertised candidates and opens a direct candidate connection.

Code and test coverage reflect that boundary:

- `lsi-rendezvous` tests exercise protocol metadata, candidate exchange, and notification queues
- `lsi-daemon` WAN tests exercise lookup and API routing, not byte relay
- `lsi-protocol-native-v1` transfer tests write files through direct native listeners
- there is no relay server, relay stream, or upload/download byte path in the rendezvous crate

The privacy docs capture the same guarantee: the rendezvous can observe peer
public keys, aliases, candidate addresses, TTLs, and request timing, but it
does not see filenames, file sizes, file contents, or transfer progress.

## Known Limitations

- No relay fallback exists in the open-base v1 path; direct candidate dialing can fail.
- Symmetric NAT, strict firewalls, carrier-grade NAT, or UDP-blocking networks can still prevent a transfer.
- STUN candidates depend on reachable STUN servers and may be incomplete when a runtime cannot bind the expected native port.
- Trust bootstrap is manual in the Linux netns smoke harness.
- The local machine for this evidence is macOS, so the full Linux netns topology was not executed here.
