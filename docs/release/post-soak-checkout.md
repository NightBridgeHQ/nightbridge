# Post-Soak Release Checkout

This checklist records the completed post-soak closure for NightBridge
`26.5.0-alpha`. It is intentionally limited to release closure work. For
follow-up commits on this train, do not add alpha features; limit changes to
bug fixes, release-blocker fixes, security fixes, documentation corrections,
packaging/release evidence improvements, and small enhancements that improve
the existing pre-release experience without adding a new product surface.

Current local release candidate:

- Branch: `main`
- Artifact source commit: `05af10d`
- Status: long native soak, post-refresh delta soak, final preflight, final
  artifact generation, Docker smoke, DEB build, and systemd smoke passed
- Gate: GitHub release assets were published and the installer passed smoke
  against the published release

## 1. Confirm Soak Completion

Read the soak session on a representative soak host:

```bash
ssh soak-host 'sudo -n -u nightbridge sh -lc '"'"'
  cd ~/nightbridge-release/<commit>
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

Delta soak evidence on a representative soak host:

```bash
ssh soak-host 'sudo -n -u nightbridge sh -lc '"'"'
  cd ~/nightbridge-release/<commit>
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
- evidence path: `~/nightbridge-release/<commit>/target/soak/evidence/`

## 3. Freeze The Release Commit

Confirm the local release branch and record the commit:

```bash
git status --short --branch
git rev-parse --short HEAD
git log -1 --oneline
```

Only continue if the worktree contains the intended release docs and release
script changes. This gate passed before the alpha tag was published.

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

Linux `amd64` was built on a representative Ubuntu package host. Linux `arm64`
was built in Colima `aarch64` with `CARGO_BUILD_JOBS=1` and copied into the
final local asset set before rerunning `packaging/release/checksums.sh`.

## 6. Run Final Docker Smoke

On a representative Docker host, build and smoke the final release tag from the
release commit:

```bash
release_commit="$(git rev-parse --short HEAD)"
ssh docker-host "mkdir -p ~/nightbridge-release/${release_commit}"
rsync -a --delete --exclude target . docker-host:~/nightbridge-release/${release_commit}/
ssh docker-host "cd ~/nightbridge-release/${release_commit} && \
  mkdir -p target/release-evidence/docker && \
  NIGHTBRIDGE_DOCKER_IMAGE=night-bridge:26.5.0 \
  bash packaging/docker/smoke.sh 2>&1 | tee target/release-evidence/docker/smoke-26.5.0.log && \
  docker image inspect night-bridge:26.5.0 --format '{{.Id}} {{.Size}}'"
```

Recorded final Docker smoke evidence:

- Host: a representative Ubuntu Docker host
- Image tag: `night-bridge:26.5.0`
- Image ID:
  `sha256:1a53e8b3e133342aa1d2f8186366e33716026f5ef9b35728582ec04924c25239`
- Image size: `43690136`
- Result: PASS
- Evidence log:
  `~/nightbridge-release/<commit>/target/release-evidence/docker/smoke-26.5.0.log`

## 7. Run Final DEB And systemd Smoke

On a representative systemd host, build the final Debian package and run the
systemd smoke:

```bash
release_commit="$(git rev-parse --short HEAD)"
ssh systemd-host "mkdir -p ~/nightbridge-release/${release_commit}"
rsync -a --delete --exclude target . systemd-host:~/nightbridge-release/${release_commit}/
ssh systemd-host "cd ~/nightbridge-release/${release_commit} && \
  . ~/.cargo/env && \
  mkdir -p target/release-evidence/systemd-deb && \
  bash packaging/build-packages.sh --deb-only 2>&1 | tee target/release-evidence/systemd-deb/build-26.5.0.log"
```

Then install and validate the generated package on the representative systemd
host:

```bash
release_commit="$(git rev-parse --short HEAD)"
ssh systemd-host "cd ~/nightbridge-release/${release_commit} && \
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

- Host: a representative Ubuntu systemd host
- Package:
  `~/nightbridge-release/<commit>/target/debian/night-bridge-daemon_26.5.0-1_amd64.deb`
- Package size: `4508660`
- Service status: `active`
- `/healthz`: PASS on retry attempt 2 after restart
- Evidence logs:
  - `~/nightbridge-release/<commit>/target/release-evidence/systemd-deb/build-26.5.0.log`
  - `~/nightbridge-release/<commit>/target/release-evidence/systemd-deb/install-systemd-26.5.0.log`

## 8. Update Release Notes And Tag

Final tag readiness checks:

```bash
rg -n "TBD" docs/release/26.5-notes.md docs/release/artifacts.md
git diff -- docs/release packaging install.sh
git status --short --branch
```

The alpha tag was created after all blockers in `docs/release/versioning.md`
were closed or explicitly scoped out:

```bash
git tag -a 26.5.0-alpha -m "NightBridge 26.5.0 alpha"
```

The tag was pushed after the release notes contained final evidence paths and
the local tag pointed at the intended release commit.

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

The release URL, uploaded asset list, installer command, and installer smoke
result are recorded in `docs/release/26.5-notes.md`.

Installer smoke result:

- Start: `2026-05-28T07:02:13Z`
- Finish: `2026-05-28T07:02:15Z`
- Result: PASS
- Evidence: `nightbridge-26.5.0-alpha-macos-arm64.tar.gz: OK`,
  four binaries installed to a temporary directory, and
  `night-bridge --help` ran successfully.
