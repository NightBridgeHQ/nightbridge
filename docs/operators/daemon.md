# Daemon Operations

The daemon is the primary headless service. It receives LocalSend v2 uploads,
serves the API/WebUI, advertises native peers, and owns local runtime state.

## Build And Install

Build locally:

```bash
cargo build --release -p lsi-daemon
```

Install the binary and systemd unit:

```bash
sudo install -D -m 0755 target/release/localsend-improved-daemon \
  /usr/bin/localsend-improved-daemon
sudo install -D -m 0644 packaging/systemd/localsend-improved.service \
  /etc/systemd/system/localsend-improved.service
sudo install -d -m 0755 /etc/localsend-improved
sudo install -d -m 0750 /var/lib/localsend-improved
sudo systemctl daemon-reload
sudo systemctl enable --now localsend-improved.service
```

Package builds use:

```bash
bash packaging/build-packages.sh
```

## Paths

- Config path: `/etc/localsend-improved/config.toml`
- State path: `/var/lib/localsend-improved`
- API token path: `/var/lib/localsend-improved/api.token`
- Trust store: `/var/lib/localsend-improved/trust.db`
- Default inbox: `/var/lib/localsend-improved/inbox`

For local development, the same paths are derived from the configured state
root and inbox arguments.

## Ports

- LocalSend v2 receive API: configured LocalSend port, commonly `53317/tcp`
- Native QUIC listener: configured native port, commonly `53318/udp`
- Daemon HTTP/gRPC API: configured API bind address
- Rendezvous client: outbound QUIC to the configured rendezvous URL

Bind API ports to localhost unless an external reverse proxy or firewall policy
protects them.

## API Token

The daemon API requires a bearer token. The token is stored in the API token
path and must be treated as a local secret.

Example header:

```text
Authorization: Bearer <token>
```

If the token file is missing, start the daemon once with a writable state path
or provide the token explicitly through the supported CLI/API configuration.

## Health, Readiness, And Metrics

Operator endpoints:

- `/healthz`: process health
- `/readyz`: readiness
- `/metrics`: Prometheus metrics

Protect these endpoints if the API bind address is reachable beyond localhost.

## Trust Store Operations

Trusted peers are stored in `trust.db`.

Policies:

- `auto_accept`: allow unattended trusted-peer operations.
- `prompt`: require an operator/user decision before transfer.
- `block`: refuse the peer.

Native WAN sends require `auto_accept` and pinned native certificate metadata.
Existing peers without native certificate metadata must be re-paired or
refreshed before native WAN auto-send is allowed.

Useful commands:

```bash
localsend-improved peers list-trusted
localsend-improved peers list-lan
localsend-improved send --wan --peer <fingerprint> <file>
```
