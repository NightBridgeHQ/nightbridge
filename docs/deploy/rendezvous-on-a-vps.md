# Deploying Rendezvous On A VPS

NightBridge WAN rendezvous is a self-hosted QUIC control plane. It lets daemons register and look up native QUIC candidates, but it does not relay file bytes.

There is no default public rendezvous service. Run your own rendezvous server for the peers you operate or trust.

## Build

Build the rendezvous binary from the repository root:

```bash
cargo build --release -p lsi-rendezvous
```

The binary is written to:

```text
target/release/night-bridge-rendezvous
```

## Run

Example direct run:

```bash
target/release/night-bridge-rendezvous --bind 0.0.0.0:53410 --max-ttl-seconds 300
```

Use a UDP port. The current Sprint 5 default examples use `53410/udp`.

## systemd

Example unit:

```ini
[Unit]
Description=NightBridge Rendezvous
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=localsend-rendezvous
Group=localsend-rendezvous
ExecStart=/usr/local/bin/night-bridge-rendezvous --bind 0.0.0.0:53410 --max-ttl-seconds 300
Restart=on-failure
RestartSec=3
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true

[Install]
WantedBy=multi-user.target
```

Install and start:

```bash
sudo install -m 0755 target/release/night-bridge-rendezvous /usr/local/bin/night-bridge-rendezvous
sudo useradd --system --home /nonexistent --shell /usr/sbin/nologin localsend-rendezvous
sudo install -m 0644 localsend-rendezvous.service /etc/systemd/system/localsend-rendezvous.service
sudo systemctl daemon-reload
sudo systemctl enable --now localsend-rendezvous.service
```

## Firewall

Open the rendezvous UDP port:

```bash
sudo ufw allow 53410/udp
```

Or with firewalld:

```bash
sudo firewall-cmd --add-port=53410/udp --permanent
sudo firewall-cmd --reload
```

## Resources

Target resource envelope for the open-base v1 rendezvous service:

- under 50 MB RAM for 1000 registered peers
- low CPU when idle
- no persistent database in Sprint 5; registrations live in memory and expire by TTL

## Logs

With systemd:

```bash
journalctl -u localsend-rendezvous.service -f
```

The server logs registration, lookup, notify, pruning, and connection errors. Avoid running public multi-tenant rendezvous unless you are prepared to operate and protect the resulting metadata.

## Health Check

There is no HTTP health endpoint in Sprint 5. Use process and UDP socket checks:

```bash
systemctl status localsend-rendezvous.service
ss -lunp | grep ':53410'
```

For functional health, run a client registration/lookup smoke from a trusted host or use the Linux netns harness:

```bash
NBRG_RUN_NETNS_TESTS=1 cargo test --test wan_netns -- --nocapture
```
