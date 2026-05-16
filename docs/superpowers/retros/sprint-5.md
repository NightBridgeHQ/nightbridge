# Sprint 5 Retro

## What Shipped

- QUIC-based `lsi-rendezvous` service for WAN peer metadata exchange.
- Signed rendezvous register, lookup, and notify request/response messages.
- In-memory rendezvous registry with TTL capping, expiry, replacement, and queued notifications.
- Rendezvous client library with local client/server integration coverage.
- `config.toml` and daemon CLI settings for WAN rendezvous, STUN servers, TTL, and registration interval.
- Native WAN candidate model with local and STUN server-reflexive candidates.
- WAN candidate dedupe, priority sorting, and direct candidate-pair dialing.
- Daemon WAN registration loop with retry and shutdown behavior.
- Daemon WAN lookup flow for trusted peers and notify fallback for missing peers.
- CLI WAN lookup and send-by-peer routing through the daemon API.
- Native send-by-peer path that resolves rendezvous candidates before direct QUIC transfer.
- Clear direct-path diagnostics for no relay, failed candidates, and likely symmetric NAT cases.
- Gated Linux network namespace smoke harness and manual CI job.
- Rendezvous VPS deployment docs and rendezvous privacy docs.
- Sprint 5 demo evidence.

## Demo Evidence

Detailed demo notes are in:

```text
docs/demos/sprint-5-wan-rendezvous.md
```

The demo evidence records:

- `cargo build --workspace`
- local `lsi-rendezvous` startup on `127.0.0.1:53410`
- focused rendezvous, candidate, hole-punch, daemon WAN, and CLI WAN tests
- full `cargo test --workspace`
- macOS netns skip reason and the Linux commands required for the full smoke path
- the architecture boundary proving rendezvous does not proxy file bytes

## Test Evidence

Final verification passed:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`cargo fmt --all -- --check` exited successfully. It still prints existing
stable-toolchain warnings for nightly-only rustfmt options:

```text
can't set `imports_granularity = Crate`, unstable features are only available in nightly channel
can't set `group_imports = StdExternalCrate`, unstable features are only available in nightly channel
```

Focused Sprint 5 tests also passed:

```bash
cargo test -p lsi-rendezvous
cargo test -p lsi-protocol-native-v1 candidates
cargo test -p lsi-protocol-native-v1 hole_punch
cargo test -p lsi-daemon wan
cargo test -p lsi-cli wan
cargo test --test wan_netns -- --nocapture
```

Commit hygiene:

```text
git rev-list --count main..HEAD
git log --format='%B' main..HEAD | rg -c "^Signed-off-by:"
git log --format='%B' main..HEAD | rg -n "Co-Authored-By"
```

Result: 16 sprint commits, 16 `Signed-off-by` trailers, and no
`Co-Authored-By` trailers. `origin/main` is not configured in this local
checkout, so the fallback `main..HEAD` range was used.

## Netns Evidence

The netns harness is present and gated:

```bash
cargo test --test wan_netns -- --nocapture
```

Result on this macOS machine:

```text
skipping WAN netns smoke: Linux network namespaces are required
test wan_netns_smoke_is_gated_and_runs_when_requested ... ok
```

The full topology still needs a Linux host with root or `CAP_NET_ADMIN`:

```bash
LSI_RUN_NETNS_TESTS=1 cargo test --test wan_netns -- --nocapture
sudo bash scripts/wan-netns-smoke.sh
```

## Security And Trust Limitations

- Rendezvous exchanges peer metadata only; it does not relay file bytes.
- The rendezvous service can observe peer public keys, aliases, candidate addresses, TTLs, and timing.
- Trust still gates native file receipt; unknown peers require user trust policy decisions.
- The smoke harness keeps trust bootstrap manual instead of silently trusting peers.
- No public default rendezvous server is configured; deployments should self-host and choose their own retention/logging posture.

## Sprint 6 Adjustments

- Surface WAN diagnostics in GUI and TUI without hiding NAT failure details.
- Add WAN metrics for registrations, lookups, candidate attempts, and direct-path failures.
- Decide whether relay remains out of open-base v1 or becomes a separate VIP-only feature.
- Harden trust bootstrap before any public beta path.
- Run the netns smoke on a Linux host in CI or on a dedicated test VM.
- Consider collecting operational evidence before adding persistent rendezvous storage.

## Remaining Risks

- Symmetric NAT, carrier-grade NAT, strict firewalls, and UDP-blocking networks can still prevent direct transfer.
- There is no relay fallback in Sprint 5.
- STUN-derived candidates depend on reachable STUN servers and runtime bind behavior.
- Rendezvous registry state is in-memory and disappears on server restart.
- The full Linux netns transfer proof was not run on this macOS machine.
- Real cross-network UX still needs GUI/TUI polish and clearer remediation paths.
