# Rendezvous Operations

Rendezvous is the WAN control plane for native direct-path discovery. It stores
short-lived peer registrations in memory and helps peers exchange candidates.
It does not relay file bytes.

## Deployment

Use the full deployment guide:

- [Deploying Rendezvous On A VPS](../deploy/rendezvous-on-a-vps.md)

Build:

```bash
cargo build --release -p lsi-rendezvous
```

Example run:

```bash
target/release/lsi-rendezvous --bind 0.0.0.0:53410 --max-ttl-seconds 300
```

Open the configured UDP port on the VPS firewall. The current examples use
`53410/udp`.

## Operating Notes

- Run a rendezvous server only for peers you operate or trust.
- Treat registration metadata as sensitive; it can reveal peer availability and
  candidate addresses.
- Keep TTLs short enough to avoid stale candidates.
- There is no default public rendezvous service.
- There is no relay in the open-base v1 build.

## Health

Sprint 7 still uses process and UDP socket checks:

```bash
systemctl status localsend-rendezvous.service
ss -lunp | grep ':53410'
```

Functional smoke:

```bash
LSI_RUN_NETNS_TESTS=1 cargo test --test wan_netns -- --nocapture
```
