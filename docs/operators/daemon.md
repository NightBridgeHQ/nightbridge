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
sudo install -D -m 0755 target/release/night-bridge-daemon \
  /usr/bin/night-bridge-daemon
sudo install -D -m 0644 packaging/systemd/night-bridge.service \
  /etc/systemd/system/night-bridge.service
sudo install -d -m 0755 /etc/night-bridge
sudo install -d -m 0750 /var/lib/night-bridge
sudo systemctl daemon-reload
sudo systemctl enable --now night-bridge.service
```

Package builds use:

```bash
bash packaging/build-packages.sh
```

## Paths

- Config path: `/etc/night-bridge/config.toml`
- State path: `/var/lib/night-bridge`
- API token path: `/var/lib/night-bridge/api.token`
- Trust store: `/var/lib/night-bridge/trust.db`
- Default inbox: `/var/lib/night-bridge/inbox`

For local development, the same paths are derived from the configured state
root and inbox arguments.

## Ports

- LocalSend v2 receive API: configured LocalSend port, commonly `53317/tcp`
- Native QUIC listener: configured native port, commonly `53318/udp`
- Daemon HTTP/gRPC API: configured API bind address
- Rendezvous client: outbound QUIC to the configured rendezvous URL

Bind API ports to localhost unless an external reverse proxy or firewall policy
protects them.

## LocalSend Receive Policy

Incoming LocalSend-compatible uploads are not auto-accepted by default.

Preferred persistent config:

```toml
[localsend]
receive_policy = "trusted"
trusted_fingerprints = ["lab-phone-fingerprint"]
trusted_fingerprints_file = "/etc/night-bridge/localsend-trusted.txt"
```

The trusted fingerprint file is newline-delimited and supports comments:

```text
# /etc/night-bridge/localsend-trusted.txt
ios-fingerprint
android-fingerprint
```

The daemon reads `trusted_fingerprints_file` when each upload session is
prepared, so adding or removing fingerprints in that file does not require a
daemon restart. Changing `receive_policy` still requires config reload/restart
until live config reload exists.

In `trusted` mode, rejected unknown upload attempts are logged and persisted as
pending LocalSend peers in the daemon trust database. The upload is still
rejected before a session is created. An admin can approve or deny later:

```bash
night-bridge peers pending-local-send
night-bridge peers approve-local-send <fingerprint> --label "iOS Test Device"
night-bridge peers deny-local-send <fingerprint>
```

After approval, ask the sender to retry the upload. The daemon reads approved
LocalSend fingerprints from the trust database for each upload session, so this
does not require a daemon restart. HTTP API equivalents are:

- `GET /api/v1/localsend/pending-peers`
- `POST /api/v1/localsend/pending-peers/{fingerprint}/approve`
- `POST /api/v1/localsend/pending-peers/{fingerprint}/deny`

Policies:

- `prompt`: default. Reject incoming uploads until an operator approval flow is
  available.
- `trusted`: accept only peers whose LocalSend fingerprint is listed with
  `trusted_fingerprints`, `trusted_fingerprints_file`, approved in the daemon
  trust database, or supplied as a one-off `--trusted-localsend-fingerprint`
  override.
- `auto`: accept any LocalSend-compatible sender that can reach the daemon
  port. Use only on trusted test LANs.

Command-line overrides are available for one-off testing:

```bash
night-bridge-daemon \
  --localsend-receive-policy trusted \
  --trusted-localsend-fingerprint <official-app-fingerprint>
```

Example explicit compatibility testing mode:

```bash
night-bridge-daemon --localsend-receive-policy auto
```

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
night-bridge peers list-trusted
night-bridge peers list-lan
night-bridge send --wan --peer <fingerprint> <file>
```
