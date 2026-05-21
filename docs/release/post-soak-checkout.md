# Post-Soak Release Checkout

Use this checklist after the current 7-day native soak completes and before
tagging NightBridge `26.5.0`. It is intentionally limited to release closure
work; do not add alpha features during this pass.

Current local release candidate:

- Branch: `main`
- Commit: `dd14836` (`fix(security): refresh vulnerable dependencies`)
- Status: security refresh committed locally; no push has been performed
- Gate: finish the existing soak, then run the delta soak below against a
  binary built from `dd14836` or a later docs-only release commit

## 1. Confirm Soak Completion

Read the soak session on `camelia`:

```bash
ssh camelia 'sudo -n -u serveradmin sh -lc '"'"'
  cd /home/serveradmin/nightbridge-release/d46c64a
  tail -40 target/soak/evidence/soak-session.log
  find target/soak/evidence -maxdepth 1 -name "native-soak-*.log" | wc -l
  grep -nEi "fail|error|panic|aborted|SIG|timeout|killed" \
    target/soak/evidence/soak-session.log target/soak/evidence/native-soak-*.log \
    | tail -80 || true
'"'"''
```

Record in `docs/release/26.5-notes.md`:

- final soak end time
- completed run count
- evidence path
- pass/fail result

## 2. Schedule Delta Soak For Security Refresh

The current long soak started before the dependency security refresh. Keep that
evidence, but do not treat it as the only final soak evidence for the refreshed
binary. After the existing soak completes, run a shorter delta soak from the
current release candidate to cover the updated TLS, HTTP, URL, and time
dependency stack.

Suggested delta soak on `camelia`:

```bash
release_commit="$(git rev-parse --short HEAD)"
ssh camelia "mkdir -p ~/nightbridge-release/${release_commit}"
rsync -a --delete --exclude target . camelia:~/nightbridge-release/${release_commit}/
ssh camelia "cd ~/nightbridge-release/${release_commit} && \
  mkdir -p target/soak/evidence && \
  start=\$(date -u +%Y-%m-%dT%H:%M:%SZ) && \
  echo \"delta soak start: \${start} commit=${release_commit}\" | tee target/soak/evidence/delta-soak-session.log && \
  deadline=\$((SECONDS + 12 * 60 * 60)); run=0; \
  while (( SECONDS < deadline )); do \
    run=\$((run + 1)); \
    NBRG_SOAK_BYTES=1073741824 \
    NBRG_SOAK_RECONNECTS=50 \
    NBRG_SOAK_SEED=\$((100000 + run)) \
    NBRG_SOAK_LOG=\"target/soak/evidence/delta-native-soak-\${run}.log\" \
      bash scripts/native-soak.sh || exit 1; \
  done; \
  finish=\$(date -u +%Y-%m-%dT%H:%M:%SZ) && \
  echo \"delta soak finish: \${finish} runs=\${run}\" | tee -a target/soak/evidence/delta-soak-session.log"
```

Record in `docs/release/26.5-notes.md`:

- delta soak start and finish time
- release commit
- completed run count
- evidence path
- pass/fail result

## 3. Freeze The Release Commit

Confirm the local release branch and record the commit:

```bash
git status --short --branch
git rev-parse --short HEAD
git log -1 --oneline
```

Only continue if the worktree contains the intended release docs and release
script changes. Do not tag until all remaining checklist items pass.

## 4. Run Final Preflight

Run the final preflight outside sandboxed environments:

```bash
mkdir -p target/release-evidence
bash scripts/preflight-26.5.sh 2>&1 | tee target/release-evidence/preflight-26.5.log
```

Record in `docs/release/26.5-notes.md`:

- host
- start and finish time
- release commit
- result
- evidence log path

## 5. Build Final Artifacts

Create a clean final artifact directory:

```bash
release_commit="$(git rev-parse --short HEAD)"
dist="/private/tmp/nightbridge-dist-${release_commit}-macos-arm64"
rm -rf "${dist}"
mkdir -p "${dist}" target/release-evidence

cargo build --release -p lsi-cli -p lsi-daemon -p lsi-tui
packaging/release/archive-binaries.sh 26.5.0 "${dist}"
packaging/release/sbom.sh "${dist}"
packaging/release/checksums.sh "${dist}"
(cd "${dist}" && shasum -a 256 -c SHA256SUMS)
```

Record the final `dist` path, artifact list, `SHA256SUMS`, and
`sbom.cdx.json` status in `docs/release/26.5-notes.md`.

Repeat the archive step on Linux release hosts for `linux-amd64` and
`linux-arm64`, then copy those tarballs into the final GitHub release asset
set and rerun `packaging/release/checksums.sh`.

## 6. Run Final Docker Smoke

On `link`, build and smoke the final release tag from the release commit:

```bash
release_commit="$(git rev-parse --short HEAD)"
ssh link "mkdir -p ~/nightbridge-release/${release_commit}"
rsync -a --delete --exclude target . link:~/nightbridge-release/${release_commit}/
ssh link "cd ~/nightbridge-release/${release_commit} && \
  mkdir -p target/release-evidence/docker && \
  NIGHTBRIDGE_DOCKER_IMAGE=night-bridge:26.5.0 \
  bash packaging/docker/smoke.sh 2>&1 | tee target/release-evidence/docker/smoke-26.5.0.log && \
  docker image inspect night-bridge:26.5.0 --format '{{.Id}} {{.Size}}'"
```

Record the image tag, image ID, image size, result, and evidence log.

## 7. Run Final DEB And systemd Smoke

On `zelda`, build the final Debian package and run the systemd smoke:

```bash
release_commit="$(git rev-parse --short HEAD)"
ssh zelda "mkdir -p ~/nightbridge-release/${release_commit}"
rsync -a --delete --exclude target . zelda:~/nightbridge-release/${release_commit}/
ssh zelda "cd ~/nightbridge-release/${release_commit} && \
  . ~/.cargo/env && \
  mkdir -p target/release-evidence/systemd-deb && \
  bash packaging/build-packages.sh --deb-only 2>&1 | tee target/release-evidence/systemd-deb/build-26.5.0.log"
```

Then install and validate the generated package on `zelda`:

```bash
release_commit="$(git rev-parse --short HEAD)"
ssh zelda "cd ~/nightbridge-release/${release_commit} && \
  { sudo dpkg -i target/debian/night-bridge-daemon_26.5.0-1_amd64.deb && \
    sudo systemd-analyze verify /lib/systemd/system/night-bridge.service && \
    sudo systemctl start night-bridge.service && \
    systemctl is-active night-bridge.service && \
    curl -fsS http://127.0.0.1:53501/healthz && \
    sudo systemctl stop night-bridge.service; } 2>&1 | \
    tee target/release-evidence/systemd-deb/install-systemd-26.5.0.log"
```

Copy `target/debian/night-bridge-daemon_26.5.0-1_amd64.deb` into the final
GitHub release asset set and rerun `packaging/release/checksums.sh` so the
direct `.deb` download is covered by `SHA256SUMS`.

Record package size, package path, service status, health check result, and
evidence logs.

## 8. Update Release Notes And Tag

Before tagging:

```bash
rg -n "TBD" docs/release/26.5-notes.md docs/release/artifacts.md
git diff -- docs/release packaging install.sh
git status --short --branch
```

Only tag after all blockers in `docs/release/versioning.md` are closed or
explicitly scoped out:

```bash
git tag -a 26.5.0 -m "NightBridge 26.5.0"
```

Do not push the tag until the release notes contain final evidence paths and
the local tag points at the intended release commit.

## 9. Publish GitHub Release Assets

Create or update the GitHub release tagged `26.5.0` and attach:

- `nightbridge-26.5.0-linux-amd64.tar.gz`
- `nightbridge-26.5.0-linux-arm64.tar.gz`
- `nightbridge-26.5.0-macos-arm64.tar.gz`
- `night-bridge-daemon_26.5.0-1_amd64.deb`
- `SHA256SUMS`
- `sbom.cdx.json`
- `install.sh`

Then verify the curl installer against the published release:

```bash
tmp_home="$(mktemp -d)"
curl -fsSL https://raw.githubusercontent.com/NightBridgeHQ/nightbridge/main/install.sh | \
  sh -s -- --version 26.5.0 --install-dir "${tmp_home}/bin"
"${tmp_home}/bin/night-bridge" --help
rm -rf "${tmp_home}"
```

Record the release URL, uploaded asset list, installer command, and installer
smoke result in `docs/release/26.5-notes.md`.
