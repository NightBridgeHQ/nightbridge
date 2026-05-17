# Sprint 4 Hooks, Observability, and Packaging Demo

## Scope

Sprint 4 added:

- validated `config.toml` loading
- stable hook event JSON
- signed webhook delivery
- exec hook delivery
- hook dispatcher runtime
- Prometheus recorder and `/metrics`
- `/healthz` and `/readyz`
- structured logging controls
- systemd, Docker, DEB/RPM, and release workflow skeletons

## Hook Evidence

Command:

```bash
cargo test -p lsi-daemon hooks
```

Result:

```text
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 44 filtered out
```

Covered evidence:

- webhook POST sends parseable JSON
- `X-NBRG-Signature: sha256=<hmac>` is verified in test
- `X-NBRG-Event` carries the stable event type
- webhook retries 5xx responses
- exec hook exports `NBRG_EVENT_ID`, `NBRG_EVENT_TYPE`, and `NBRG_EVENT_JSON`
- dispatcher fans out an `InboxChanged` daemon event to a configured sink

## Health And Metrics Evidence

Commands:

```bash
cargo test -p lsi-daemon healthz
cargo test -p lsi-daemon readyz
cargo test -p lsi-daemon metrics
```

Results:

```text
healthz: test result: ok. 1 passed; 0 failed
readyz:  test result: ok. 1 passed; 0 failed
metrics: test result: ok. 4 passed; 0 failed
```

Covered evidence:

- `GET /healthz` returns `200 OK` without bearer token
- `GET /readyz` returns `200 OK` without bearer token
- `GET /metrics` returns Prometheus text without bearer token when enabled
- `GET /metrics` returns `404` when metrics are disabled
- `/api/v1/status` remains bearer-token protected

## Packaging Evidence

Command:

```bash
bash packaging/build-packages.sh --check-tools
```

Result on this machine:

```text
missing: cargo-deb
install: cargo install cargo-deb --locked
missing: cargo-generate-rpm
install: cargo install cargo-generate-rpm --locked
```

The check mode exits successfully and reports exact install hints. Full DEB/RPM
builds require installing those tools.

Command:

```bash
bash packaging/docker/smoke.sh
```

Result on this machine:

```text
ERROR: failed to connect to the docker API at unix:///var/run/docker.sock; check if the path is correct and if the daemon is running: dial unix /var/run/docker.sock: connect: no such file or directory
```

The Docker CLI exists, but the Docker daemon was not running in this local
environment. The equivalent release build passed:

```bash
cargo build --release -p lsi-daemon -p lsi-cli -p lsi-tui
```

Result:

```text
Finished `release` profile [optimized] target(s) in 53.64s
```

## Script Syntax Evidence

Command:

```bash
bash -n packaging/release/checksums.sh packaging/build-packages.sh packaging/docker/smoke.sh
```

Result: passed.

## Manual Demo Path

For a full manual LAN demo:

1. Install `cargo-deb`, `cargo-generate-rpm`, and start Docker.
2. Start a webhook receiver that records headers and request body.
3. Create a config with webhook, exec hook, and metrics enabled.
4. Start `night-bridge-daemon` with that config.
5. Send a file through LocalSend v2 or native transfer.
6. Capture webhook JSON, signature verification, exec env output, `/metrics`,
   `/healthz`, `/readyz`, and packaging smoke output.
