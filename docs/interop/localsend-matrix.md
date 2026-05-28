# LocalSend Interop Matrix

NightBridge keeps LocalSend v2 compatibility separate from the native
protocol. This matrix tracks official-app interoperability evidence for the 26.5
candidate without publishing operator LAN details, device names, or peer
fingerprints.

| Platform | LocalSend version | Receive official app to daemon | Send daemon/CLI to official app | Discovery method | Evidence path | Status |
| --- | --- | --- | --- | --- | --- | --- |
| Android | TBD | Passed with `trusted` policy, pending-peer approval, and no daemon restart | Passed by discovered LAN peer alias from CLI to official app | Android discovered daemon through LAN multicast; daemon discovered Android through active LAN scan and sent without manual IP | `target/interop/android/` | Current 26.5 Android bidirectional pass |
| iOS | 2.1 | Passed with `trusted` policy, pending-peer approval, and no daemon restart | Passed by discovered LAN peer alias from CLI to official app | Daemon discovered iOS through active LAN scan and sent without manual IP | `target/interop/ios/` | Current 26.5 iOS bidirectional pass |
| Desktop macOS | TBD | Passed with `trusted` policy, pending-peer approval, and no daemon restart | Passed by discovered LAN peer alias from CLI to official app | Daemon discovered macOS through active LAN scan and sent without manual IP | `target/interop/macos/` | Current 26.5 macOS bidirectional pass |
| Desktop Windows | TBD | Passed with `trusted` policy, pending-peer approval, and no daemon restart | Passed by discovered LAN peer alias from CLI to official app | Daemon discovered Windows through active LAN scan and sent without manual IP | `target/interop/windows/` | Current 26.5 Windows bidirectional pass |
| Desktop Linux | TBD | Passed with `trusted` policy, pending-peer approval, and no daemon restart | Passed by discovered LAN peer alias from CLI to official app | Daemon discovered Linux through active LAN scan and sent without manual IP | `target/interop/linux/` | Current 26.5 Linux bidirectional pass |

## Automated Baseline

`scripts/localsend-interop-smoke.sh` runs the deterministic local receive test:

```bash
cargo test -p lsi-protocol-localsend-v2 --test interop_receive -- --nocapture
```

That test proves the LocalSend v2 client/server implementation can complete an
upload against a local receiver. It is not a substitute for the official app
matrix above.

## Current Manual Findings

- Official iOS, Android, macOS, Windows, and Linux LocalSend apps can send files
  to the daemon with `trusted` receive policy after pending-peer approval and
  without daemon restart.
- Official iOS, Android, macOS, Windows, and Linux LocalSend apps can receive
  files from the NightBridge CLI through discovered LAN peers.
- Active LAN scan of the LocalSend `/info` endpoint discovered each official
  app without publishing or requiring manual transfer IPs in the public docs.
- Manual evidence includes screenshots or logs, transferred file names, sizes,
  checksums, daemon commit SHA, platform, and discovery path. Raw device names,
  peer fingerprints, private LAN addresses, usernames, and hostnames are kept in
  local release evidence only.

## Public Evidence Summary

### Linux

- Date: 2026-05-18
- Commit: `2d410d7`
- Result: bidirectional pass.
- Daemon-to-app: sent a 45-byte text fixture through a discovered peer alias.
- App-to-daemon: uploaded an 8-byte text fixture after pending-peer approval.
- Receive policy: `trusted`.
- Notes: no daemon restart was required after approval.

### macOS

- Date: 2026-05-18
- Commit: `2d410d7`
- Result: bidirectional pass.
- Daemon-to-app: sent a 45-byte text fixture through a discovered peer alias.
- App-to-daemon: uploaded an 8-byte text fixture after pending-peer approval.
- Receive policy: `trusted`.
- Notes: no daemon restart was required after approval.

### Windows

- Date: 2026-05-18
- Commit: `2d410d7`
- Result: bidirectional pass.
- Daemon-to-app: sent a 45-byte text fixture through a discovered peer alias.
- App-to-daemon: uploaded a 12-byte text fixture after pending-peer approval.
- Receive policy: `trusted`.
- Notes: no daemon restart was required after approval.

### Android

- Date: 2026-05-18
- Commit: `2d410d7`
- Result: bidirectional pass.
- App-to-daemon: uploaded a 10-byte text fixture after pending-peer approval.
- Daemon-to-app: sent a 45-byte text fixture through a discovered peer alias.
- Receive policy: `trusted`.
- Notes: Android discovered the daemon through LAN multicast; the daemon
  discovered Android through active LAN scan.

### iOS

- Date: 2026-05-17 and 2026-05-18 retest
- Commits: `c510bd2`, `5da324d`, and `2d410d7`
- Result: bidirectional pass.
- Daemon-to-app: sent a documentation fixture through manual URL first, then
  through discovered peer alias during the 26.5 retest.
- App-to-daemon: uploaded a PDF fixture after pending-peer approval.
- Receive policy: `trusted`.
- Notes: no daemon restart was required after approval.

## Evidence Handling

For each platform, preserve privately:

- official LocalSend app version and download source
- daemon commit SHA and OS
- sender and receiver screenshots or logs
- transferred file name, size, and checksum
- discovery path used, including whether manual URL fallback was needed

Do not publish raw operator LAN addresses, device aliases, peer fingerprints,
usernames, hostnames, or local filesystem paths in public docs.

If `NBRG_OFFICIAL_LOCALSEND_ARTIFACT` later points to a verified
headless-capable official artifact, the smoke wrapper may grow an automated
official-app path. Until then, official-app status stays manual.
