# LocalSend Interop Matrix

NightBridge keeps LocalSend v2 compatibility separate from the native
protocol. This matrix tracks official-app interoperability evidence for the 26.5
candidate.

| Platform | LocalSend version | Receive official app to daemon | Send daemon/CLI to official app | Discovery method | Evidence path | Status |
| --- | --- | --- | --- | --- | --- | --- |
| Android | TBD | Manual | Manual | LAN multicast / manual URL | `target/interop/android/` | Pending official-device run |
| iOS | TBD | Passed with `trusted` policy, pending-peer approval, and no daemon restart | Passed manual accept from daemon/CLI to official app before receive-policy hardening; current retest pending listener availability | Manual URL `https://10.16.20.53:53317`; LAN multicast discovery did not find peer | `specs/manual-test/release-p3-inbox/` | Partial current 26.5 evidence |
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

- iOS official LocalSend app can send files to the daemon with `trusted`
  receive policy after pending-peer approval and without daemon restart.
- iOS official LocalSend app can receive from the NightBridge CLI through
  manual URL send before receive-policy hardening. Current retest is blocked by
  the iOS app not listening on the previously verified manual URL.
- The iOS receive test sent `docs/release/versioning.md` to
  `https://10.16.20.53:53317` and the user confirmed the file was present on
  the device.
- The iOS send-to-daemon test first rejected the unknown official-app
  fingerprint, recorded it as pending, accepted it through
  `night-bridge peers approve-local-send`, and then received
  `2604.03565.pdf`.
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

Current 26.5 retest:

- Date: 2026-05-17
- Commit: `5da324d`
- Command:

```bash
cargo run -p lsi-cli --bin night-bridge -- send --direct --url https://10.16.20.53:53317 docs/release/versioning.md
```

- Result: blocked before upload because the iOS app was not listening on
  `10.16.20.53:53317`; `curl -k https://10.16.20.53:53317/api/localsend/v2/info`
  returned connection refused from the same host.

### Official iOS App To Daemon

- Date: 2026-05-17
- Commit: `5da324d`
- Sender: official LocalSend app on iOS
- Receiver: NightBridge daemon on macOS
- Daemon address: `10.16.20.123:53317`
- Daemon alias: `NightBridge Release P3`
- Required daemon config:

```toml
[localsend]
receive_policy = "trusted"
```

- Observed untrusted fingerprint:

```text
639969ae9ce2cd7c05a9c084423da4739eebd454a112b335b0cf8f2ed0e73046
```

- Approval flow:

```bash
night-bridge peers pending-local-send
night-bridge peers approve-local-send 639969ae9ce2cd7c05a9c084423da4739eebd454a112b335b0cf8f2ed0e73046 --label "Diego Personal iPhone"
```

- CLI evidence:

```text
FINGERPRINT                                                        ALIAS                    STATUS   ATTEMPTS LAST SEEN
639969ae9ce2cd7c05a9c084423da4739eebd454a112b335b0cf8f2ed0e73046   Diego Personal iPhone    pending  1        1779059489
approved LocalSend peer:
639969ae9ce2cd7c05a9c084423da4739eebd454a112b335b0cf8f2ed0e73046   Diego Personal iPhone    trusted  3        1779059549
```

- Result: after approval and without daemon restart, iOS uploaded
  `2604.03565.pdf`; the inbox contains completed files of 446409 bytes.
- Notes: stale zero-byte `.part` files remained from canceled pre-approval or
  interrupted attempts and are not counted as successful uploads.

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
