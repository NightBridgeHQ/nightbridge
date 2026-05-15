# Sprint 4 Retro

## What Shipped

- mDNS hostname normalization for `.local.` advertisements.
- WebUI token bootstrap policy documentation.
- `config.toml` loader with hook, metrics, and logging validation.
- Stable hook event schema and daemon event adapter.
- Signed webhook hook sink with HMAC SHA-256.
- Exec hook sink with timeout and `LSI_EVENT_*` environment variables.
- Hook dispatcher runtime wired into daemon startup and shutdown.
- Prometheus recorder plus `/metrics`, `/healthz`, and `/readyz`.
- Configurable daemon log format: `json`, `pretty`, and `compact`.
- systemd unit skeleton with hardening and explicit writable state paths.
- Docker image skeleton and smoke script.
- DEB/RPM packaging skeleton and tool-check script.
- Release workflow skeleton and checksum script.
- Sprint 4 operational demo evidence.
- Python SDK example file required by SDK tests.

## Verification Evidence

Passed:

```bash
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
npm run check --prefix crates/webui
npm run build --prefix crates/webui
scripts/setup-python-sdk.sh
PYTHON_BIN=sdks/python/.venv/bin/python scripts/generate-sdks.sh python
sdks/python/.venv/bin/python -m pytest sdks/python/tests
```

Also passed during demo evidence refresh:

```bash
cargo test -p lsi-daemon hooks
cargo test -p lsi-daemon metrics
cargo test -p lsi-daemon healthz
cargo test -p lsi-daemon readyz
bash -n packaging/release/checksums.sh packaging/build-packages.sh packaging/docker/smoke.sh
```

Commit hygiene:

```text
git log --format=%B main..HEAD | rg -n "Co-Authored-By|Signed-off-by"
```

Result: every sprint commit has `Signed-off-by`; no `Co-Authored-By` trailers were found.

## Packaging Evidence

`packaging/build-packages.sh --check-tools` reports missing local package tools with install hints:

```text
missing: cargo-deb
install: cargo install cargo-deb --locked
missing: cargo-generate-rpm
install: cargo install cargo-generate-rpm --locked
```

`packaging/docker/smoke.sh` could not run fully because Docker CLI exists but the daemon was not running:

```text
ERROR: failed to connect to the docker API at unix:///var/run/docker.sock; check if the path is correct and if the daemon is running: dial unix /var/run/docker.sock: connect: no such file or directory
```

`systemd-analyze verify packaging/systemd/localsend-improved.service` was not run because `systemd-analyze` is not installed in this environment.

## Demo Evidence

Detailed demo notes are in:

```text
docs/demos/sprint-4-hooks-observability-packaging.md
```

The demo evidence records hook tests, health/metrics tests, packaging tool checks, Docker environment failure, and release-build proof.

## Sprint 5 Adjustments

- Install packaging tools in CI before relying on DEB/RPM artifact output.
- Run Docker smoke in an environment with Docker daemon access.
- Validate the systemd unit on a Linux host with `systemd-analyze`.
- Decide whether metrics should remain on the existing HTTP API port or move to the configured `metrics.host` and `metrics.port`.
- Add an end-to-end hook demo that triggers a real transfer and captures webhook, exec, and metrics output in one run.

## Remaining Risks

- Hook delivery is best-effort; failures are logged but not persisted for replay.
- Webhook retries are bounded and in-memory only.
- Exec hook command parsing is intentionally conservative and does not support quoted arguments yet.
- `/metrics`, `/healthz`, and `/readyz` are unauthenticated by design; deployment must bind them appropriately.
- Packaging metadata is a skeleton until validated on real Debian/RPM hosts.
