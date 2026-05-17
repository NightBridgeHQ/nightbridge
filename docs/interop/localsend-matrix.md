# LocalSend Interop Matrix

NightBridge keeps LocalSend v2 compatibility separate from the native
protocol. This matrix tracks official-app interoperability evidence for the 26.5
candidate.

| Platform | LocalSend version | Receive official app to daemon | Send daemon/CLI to official app | Discovery method | Evidence path | Status |
| --- | --- | --- | --- | --- | --- | --- |
| Android | TBD | Manual | Manual | LAN multicast / manual URL | `target/interop/android/` | Pending official-device run |
| iOS | TBD | Passed before receive-policy hardening; retest with `trusted` pending | Passed manual accept from daemon/CLI to official app | Manual URL `https://10.16.20.53:53317`; LAN multicast discovery did not find peer | `target/interop/ios/` | Partial after receive-policy hardening |
| Desktop macOS | TBD | Manual | Manual | LAN multicast / manual URL | `target/interop/macos/` | Pending official-app run |
| Desktop Windows | TBD | Manual | Manual | LAN multicast / manual URL | `target/interop/windows/` | Pending official-app run |
| Desktop Linux | TBD | Manual | Manual | LAN multicast / manual URL | `target/interop/linux/` | Pending official-app run |

## Automated Baseline

`scripts/localsend-interop-smoke.sh` runs the deterministic local receive test:

```bash
cargo test -p lsi-protocol-localsend-v2 --test interop_receive -- --nocapture
```

That test proves the LocalSend v2 client/server implementation can complete an
upload against a local receiver. It is not a substitute for the official app
matrix above.

## Current Manual Findings

- iOS official LocalSend app could send files to the daemon before receive
  policy hardening. This path must be retested with
  `--localsend-receive-policy trusted` and an allowlisted iOS fingerprint.
- iOS official LocalSend app can receive from the NightBridge CLI through
  manual URL send. LAN discovery did not find the peer in this run.
- The iOS receive test sent `docs/release/versioning.md` to
  `https://10.16.20.53:53317` and the user confirmed the file was present on
  the device.
- Android and official desktop app evidence still need platform-specific manual
  runs.

## iOS Evidence

### Daemon/CLI To Official iOS App

- Date: 2026-05-17
- Commit: `c510bd2`
- Sender: NightBridge CLI on macOS
- Receiver: official LocalSend app on iOS
- Discovery: manual URL after LAN discovery returned no peers
- Command:

```bash
cargo run -p lsi-cli --bin night-bridge -- send --direct --url https://10.16.20.53:53317 docs/release/versioning.md
```

- CLI result: `sent 1 file(s)`
- User confirmation: iOS device received `versioning.md`

### Official iOS App To Daemon

- Status: retest required after receive-policy hardening.
- Required daemon config:

```toml
[localsend]
receive_policy = "trusted"
```

- Approval flow:

```bash
night-bridge peers pending-local-send
night-bridge peers approve-local-send <ios-local-send-fingerprint> --label "iOS LocalSend"
```

- Optional static allowlist or command-line override:

```bash
night-bridge-daemon \
  --localsend-receive-policy trusted \
  --trusted-localsend-fingerprint <ios-local-send-fingerprint>
```

- Expected: unknown iOS fingerprints are rejected before upload session
  creation and recorded as pending; approved or allowlisted iOS fingerprint can
  send without daemon restart.

## Manual Evidence

For each platform, preserve:

- official LocalSend app version and download source
- daemon commit SHA and OS
- sender and receiver screenshots or logs
- transferred file name, size, and checksum
- discovery path used, including whether manual URL fallback was needed

If `NBRG_OFFICIAL_LOCALSEND_ARTIFACT` later points to a verified
headless-capable official artifact, the smoke wrapper may grow an automated
official-app path. Until then, official-app status stays manual.
