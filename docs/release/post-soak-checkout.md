# Post-Soak Release Checkout

Use this checklist after the current 7-day native soak completes and before
tagging NightBridge `26.5.0-alpha`. It is intentionally limited to release closure
work. NightBridge `26.5.0` is in pre-release closure: do not add alpha features
during this pass. Limit changes to bug fixes, release-blocker fixes, security
fixes, documentation corrections, packaging/release evidence improvements, and
small enhancements that improve the existing pre-release experience without
adding a new product surface.

Current local release candidate:

- Branch: `main`
- Artifact source commit: `05af10d`
- Status: long native soak, post-refresh delta soak, final preflight, final
  artifact generation, Docker smoke, DEB build, and systemd smoke passed
- Gate: publish GitHub release assets and smoke the installer against the
  published release before broad distribution

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

## 2. Confirm Delta Soak For Security Refresh

The current long soak started before the dependency security refresh. Keep that
evidence, but do not treat it as the only final soak evidence for the refreshed
binary. A shorter delta soak covered the updated TLS, HTTP, URL, and time
dependency stack.

Delta soak evidence on `camelia`:

```bash
ssh camelia 'sudo -n -u serveradmin sh -lc '"'"'
  cd /home/serveradmin/nightbridge-release/e10ade9
  tail -40 target/soak/evidence/delta-soak-session.log
  find target/soak/evidence -maxdepth 1 -name "delta-native-soak-*.log" | wc -l
  tail -20 target/soak/evidence/delta-native-soak-928.log
'"'"''
```

Recorded in `docs/release/26.5-notes.md`:

- start: `2026-05-27T17:35:58Z`
- finish: `2026-05-28T05:36:16Z`
- release commit: `e10ade9`
- completed runs: `928`
- result: PASS
- evidence path: `/home/serveradmin/nightbridge-release/e10ade9/target/soak/evidence/`

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

Final preflight evidence:

```bash
Host: erebor
Start: 2026-05-28T05:46:59Z
Finish: 2026-05-28T05:47:08Z
Result: PASS
Evidence log: target/release-evidence/preflight-26.5.log
```

Recorded in `docs/release/26.5-notes.md`.

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

Recorded final artifact evidence:

- Dist path: `/private/tmp/nightbridge-dist-05af10d-macos-arm64`
- Source commit: `05af10d`
- Result: PASS; `shasum -a 256 -c SHA256SUMS` verified every listed asset
- Assets:
  - `nightbridge-26.5.0-alpha-macos-arm64.tar.gz`
  - `nightbridge-26.5.0-alpha-linux-amd64.tar.gz`
  - `nightbridge-26.5.0-alpha-linux-arm64.tar.gz`
  - `night-bridge-daemon_26.5.0-1_amd64.deb`
  - `SHA256SUMS`
  - `sbom.cdx.json`

Linux `amd64` was built on `zelda`. Linux `arm64` was built in Colima
`aarch64` with `CARGO_BUILD_JOBS=1` and copied into the final local asset set
before rerunning `packaging/release/checksums.sh`.

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

Recorded final Docker smoke evidence:

- Host: `link` (`10.16.20.130`)
- Image tag: `night-bridge:26.5.0`
- Image ID:
  `sha256:1a53e8b3e133342aa1d2f8186366e33716026f5ef9b35728582ec04924c25239`
- Image size: `43690136`
- Result: PASS
- Evidence log:
  `/home/serveradmin/nightbridge-release/05af10d/target/release-evidence/docker/smoke-26.5.0.log`

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

Recorded final DEB and systemd smoke evidence:

- Host: `zelda` (`10.16.20.129`)
- Package:
  `/home/serveradmin/nightbridge-release/05af10d/target/debian/night-bridge-daemon_26.5.0-1_amd64.deb`
- Package size: `4508660`
- Service status: `active`
- `/healthz`: PASS on retry attempt 2 after restart
- Evidence logs:
  - `/home/serveradmin/nightbridge-release/05af10d/target/release-evidence/systemd-deb/build-26.5.0.log`
  - `/home/serveradmin/nightbridge-release/05af10d/target/release-evidence/systemd-deb/install-systemd-26.5.0.log`

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
git tag -a 26.5.0-alpha -m "NightBridge 26.5.0 alpha"
```

Do not push the tag until the release notes contain final evidence paths and
the local tag points at the intended release commit.

## 9. Publish GitHub Release Assets

Created GitHub pre-release:

- URL: `https://github.com/NightBridgeHQ/nightbridge/releases/tag/26.5.0-alpha`
- Result: PASS
- Uploaded assets:

- `nightbridge-26.5.0-alpha-linux-amd64.tar.gz`
- `nightbridge-26.5.0-alpha-linux-arm64.tar.gz`
- `nightbridge-26.5.0-alpha-macos-arm64.tar.gz`
- `night-bridge-daemon_26.5.0-1_amd64.deb`
- `SHA256SUMS`
- `sbom.cdx.json`
- `install.sh`

Verified the curl installer against the published release:

```bash
tmp_home="$(mktemp -d)"
curl -fsSL https://raw.githubusercontent.com/NightBridgeHQ/nightbridge/main/install.sh | \
  sh -s -- --version 26.5.0-alpha --install-dir "${tmp_home}/bin"
"${tmp_home}/bin/night-bridge" --help
rm -rf "${tmp_home}"
```

Record the release URL, uploaded asset list, installer command, and installer
smoke result in `docs/release/26.5-notes.md`.

Installer smoke result:

- Start: `2026-05-28T07:02:13Z`
- Finish: `2026-05-28T07:02:15Z`
- Result: PASS
- Evidence: `nightbridge-26.5.0-alpha-macos-arm64.tar.gz: OK`,
  four binaries installed to a temporary directory, and
  `night-bridge --help` ran successfully.
