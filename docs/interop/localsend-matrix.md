# LocalSend Interop Matrix

NightBridge keeps LocalSend v2 compatibility separate from the native
protocol. This matrix tracks official-app interoperability evidence for the 26.5
candidate.

| Platform | LocalSend version | Receive official app to daemon | Send daemon/CLI to official app | Discovery method | Evidence path | Status |
| --- | --- | --- | --- | --- | --- | --- |
| Android | TBD | Passed with `trusted` policy, pending-peer approval, and no daemon restart | Passed by discovered LAN peer alias from CLI to official app | Android discovered daemon through LAN multicast; daemon discovered Android through active LAN scan and sent without manual IP | `target/interop/android/` | Current 26.5 Android bidirectional pass |
| iOS | 2.1 | Passed with `trusted` policy, pending-peer approval, and no daemon restart | Passed by discovered LAN peer alias from CLI to official app | Daemon discovered iOS through active LAN scan and sent without manual IP | `specs/manual-test/release-p3-inbox/` | Current 26.5 iOS bidirectional pass |
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

- iOS official LocalSend app can send files to the daemon with `trusted`
  receive policy after pending-peer approval and without daemon restart.
- iOS official LocalSend app can receive from the NightBridge CLI through
  manual URL send.
- The iOS receive test sent `docs/release/versioning.md` to
  `https://10.16.20.53:53317` and the user confirmed the file was present on
  the device.
- The iOS send-to-daemon test first rejected the unknown official-app
  fingerprint, recorded it as pending, accepted it through
  `night-bridge peers approve-local-send`, and then received
  `2604.03565.pdf`.
- Android and iOS official LocalSend apps are discoverable by the daemon
  without manual IP entry through active LAN scan of the LocalSend `/info`
  endpoint, and both received files from the NightBridge CLI by peer alias.
- Windows official LocalSend desktop app is discoverable by the daemon without
  manual IP entry through active LAN scan, received a file from the NightBridge
  CLI by peer alias, and sent a file to the daemon after pending-peer approval.
- macOS official LocalSend desktop app is discoverable by the daemon without
  manual IP entry through active LAN scan and received a file from the
  NightBridge CLI by peer alias. It also sent a file to the daemon on the
  standard LocalSend port after pending-peer approval.
- Linux official LocalSend desktop app is discoverable by the daemon without
  manual IP entry through active LAN scan, received a file from the NightBridge
  CLI by peer alias, and sent a file to the daemon after pending-peer approval.

## Linux Evidence

### Daemon/CLI To Official Linux App

- Date: 2026-05-18
- Commit: `2d410d7`
- Sender: NightBridge CLI on macOS
- Receiver: official LocalSend app on Ubuntu
- Receiver alias: `rx0-world`
- Receiver fingerprint:
  `f90a189a6a933ca373d35793ef3c0f7fc95a54f29ca611327ae642f42fa58638`
- Discovery: daemon active LAN scan returned the Linux official app peer;
  transfer used peer alias rather than manual URL
- Discovery evidence:

```text
ALIAS                 ADDRESS                PROTOCOL FINGERPRINT
rx0-world             10.16.20.72:53317      localsend_v2 f90a189a6a933ca373d35793ef3c0f7fc95a54f29ca611327ae642f42fa58638
```

- Command:

```bash
night-bridge send --peer rx0-world target/interop/android/daemon-to-android.txt
```

- CLI result:

```text
transfer: 75fa13f5-088c-4810-bc77-462cf52bc610
```

- File: `target/interop/android/daemon-to-android.txt`, 45 bytes
- SHA-256:
  `12221c33c2d601520e9b973b0dda61892cff0a4f0e5920faeb8017984d355d8c`

### Official Linux App To Daemon

- Date: 2026-05-18
- Commit: `2d410d7`
- Sender: official LocalSend app on Ubuntu
- Sender alias: `rx0-world`
- Receiver: NightBridge daemon on macOS
- Daemon alias: `NightBridge Linux Test`
- Daemon address: `0.0.0.0:53317`
- Required daemon config:

```toml
[localsend]
receive_policy = "trusted"
```

- Observed untrusted fingerprint:

```text
f90a189a6a933ca373d35793ef3c0f7fc95a54f29ca611327ae642f42fa58638
```

- Approval flow:

```bash
night-bridge peers pending-local-send
night-bridge peers approve-local-send f90a189a6a933ca373d35793ef3c0f7fc95a54f29ca611327ae642f42fa58638 --label rx0-world
```

- CLI evidence:

```text
FINGERPRINT                                                        ALIAS                    STATUS   ATTEMPTS LAST SEEN
f90a189a6a933ca373d35793ef3c0f7fc95a54f29ca611327ae642f42fa58638   rx0-world                pending  1        1779157778
approved LocalSend peer:
f90a189a6a933ca373d35793ef3c0f7fc95a54f29ca611327ae642f42fa58638   rx0-world                trusted  1        1779157792
```

- Result: after approval and without daemon restart, Linux uploaded
  `2f1d7f44-931f-41f8-aace-23864b516f77.txt`; the inbox contains a completed
  file of 8 bytes.
- SHA-256:
  `719bef523a60dfa1ac8f5de20e78e8e17aab80dc8d29991edc7fbc1e38bcd9b4`

## macOS Evidence

### Daemon/CLI To Official macOS App

- Date: 2026-05-18
- Commit: `2d410d7`
- Sender: NightBridge CLI on macOS
- Receiver: official LocalSend app on macOS
- Receiver alias: `zoit-mbp`
- Receiver fingerprint:
  `574efc1166764526a0a4f69bfe07e2750075eec73c226df8453a39a596a55be2`
- Discovery: daemon active LAN scan returned the macOS official app peer;
  transfer used peer alias rather than manual URL
- Discovery evidence:

```text
ALIAS                 ADDRESS                PROTOCOL FINGERPRINT
zoit-mbp              10.16.20.49:53317      localsend_v2 574efc1166764526a0a4f69bfe07e2750075eec73c226df8453a39a596a55be2
```

- Command:

```bash
night-bridge send --peer zoit-mbp target/interop/android/daemon-to-android.txt
```

- CLI result:

```text
transfer: 0942f3b5-ac38-4379-aa44-03ced3d2e0d8
```

- File: `target/interop/android/daemon-to-android.txt`, 45 bytes
- SHA-256:
  `12221c33c2d601520e9b973b0dda61892cff0a4f0e5920faeb8017984d355d8c`

### Official macOS App To Daemon

- Date: 2026-05-18
- Commit: `2d410d7`
- Sender: official LocalSend app on macOS
- Sender alias: `zoit-mbp`
- Receiver: NightBridge daemon on macOS
- Daemon alias: `NightBridge macOS Standard`
- Daemon address: `0.0.0.0:53317`
- Required daemon config:

```toml
[localsend]
receive_policy = "trusted"
```

- Observed untrusted fingerprint:

```text
574efc1166764526a0a4f69bfe07e2750075eec73c226df8453a39a596a55be2
```

- Approval flow:

```bash
night-bridge peers pending-local-send
night-bridge peers approve-local-send 574efc1166764526a0a4f69bfe07e2750075eec73c226df8453a39a596a55be2 --label zoit-mbp
```

- CLI evidence:

```text
FINGERPRINT                                                        ALIAS                    STATUS   ATTEMPTS LAST SEEN
574efc1166764526a0a4f69bfe07e2750075eec73c226df8453a39a596a55be2   zoit-mbp                 pending  1        1779151274
approved LocalSend peer:
574efc1166764526a0a4f69bfe07e2750075eec73c226df8453a39a596a55be2   zoit-mbp                 trusted  1        1779151289
```

- Result: after approval and without daemon restart, macOS uploaded
  `291c1ae1-ff48-4843-86af-fdf7d10e5772.txt`; the inbox contains a completed
  file of 8 bytes.
- SHA-256:
  `719bef523a60dfa1ac8f5de20e78e8e17aab80dc8d29991edc7fbc1e38bcd9b4`

## Windows Evidence

### Daemon/CLI To Official Windows App

- Date: 2026-05-18
- Commit: `2d410d7`
- Sender: NightBridge CLI on macOS
- Receiver: official LocalSend app on Windows
- Receiver alias: `mamalonav3`
- Receiver fingerprint:
  `fe172ccac71a9c39f068f49804bdc4b411af1b1a9e7818ca2ac2c54fc7c80374`
- Discovery: daemon active LAN scan returned the Windows official app peer;
  transfer used peer alias rather than manual URL
- Discovery evidence:

```text
ALIAS                 ADDRESS                PROTOCOL FINGERPRINT
mamalonav3            10.16.20.138:53317     localsend_v2 fe172ccac71a9c39f068f49804bdc4b411af1b1a9e7818ca2ac2c54fc7c80374
```

- Command:

```bash
night-bridge send --peer mamalonav3 target/interop/android/daemon-to-android.txt
```

- CLI result:

```text
transfer: cff6b4cf-a21e-4d4c-8c5f-800a471f4540
```

- File: `target/interop/android/daemon-to-android.txt`, 45 bytes
- SHA-256:
  `12221c33c2d601520e9b973b0dda61892cff0a4f0e5920faeb8017984d355d8c`

### Official Windows App To Daemon

- Date: 2026-05-18
- Commit: `2d410d7`
- Sender: official LocalSend app on Windows
- Sender alias: `mamalonav3`
- Receiver: NightBridge daemon on macOS
- Daemon alias: `NightBridge Windows Test`
- Required daemon config:

```toml
[localsend]
receive_policy = "trusted"
```

- Observed untrusted fingerprint:

```text
fe172ccac71a9c39f068f49804bdc4b411af1b1a9e7818ca2ac2c54fc7c80374
```

- Approval flow:

```bash
night-bridge peers pending-local-send
night-bridge peers approve-local-send fe172ccac71a9c39f068f49804bdc4b411af1b1a9e7818ca2ac2c54fc7c80374 --label mamalonav3
```

- CLI evidence:

```text
FINGERPRINT                                                        ALIAS                    STATUS   ATTEMPTS LAST SEEN
fe172ccac71a9c39f068f49804bdc4b411af1b1a9e7818ca2ac2c54fc7c80374   mamalonav3               pending  1        1779127924
approved LocalSend peer:
fe172ccac71a9c39f068f49804bdc4b411af1b1a9e7818ca2ac2c54fc7c80374   mamalonav3               trusted  1        1779127935
```

- Result: after approval and without daemon restart, Windows uploaded
  `233aadf9-113d-4200-a38d-010e8d7a7180.txt`; the inbox contains a completed
  file of 12 bytes.
- SHA-256:
  `82a7a037f91c2fe9241ae67219659ea380a3b47013603bb2e6324ba4b1beb7f5`

## Android Evidence

### Official Android App To Daemon

- Date: 2026-05-18
- Commit: `2d410d7`
- Sender version: LocalSend Android `TBD`
- Sender: official LocalSend app on Android
- Sender alias: `Rich Pear`
- Receiver: NightBridge daemon on macOS
- Daemon address: `10.16.20.123:53317`
- Daemon alias: `NightBridge Android Test`
- Discovery: Android discovered the daemon through LAN multicast
- Required daemon config:

```toml
[localsend]
receive_policy = "trusted"
```

- Observed untrusted fingerprint:

```text
fbb9f69507413df141076f3068bf680d7d90cc6453ae6b6541b06006a26bc08c
```

- Approval flow:

```bash
night-bridge peers pending-local-send
night-bridge peers approve-local-send fbb9f69507413df141076f3068bf680d7d90cc6453ae6b6541b06006a26bc08c --label "Android LocalSend"
```

- CLI evidence:

```text
FINGERPRINT                                                        ALIAS                    STATUS   ATTEMPTS LAST SEEN
fbb9f69507413df141076f3068bf680d7d90cc6453ae6b6541b06006a26bc08c   Rich Pear                pending  1        1779123400
approved LocalSend peer:
fbb9f69507413df141076f3068bf680d7d90cc6453ae6b6541b06006a26bc08c   Rich Pear                trusted  1        1779123414
```

- Result: after approval and without daemon restart, Android uploaded
  `5f9a86de-8aaf-410e-9d3f-39739f426344.txt`; the inbox contains a completed
  file of 10 bytes.
- SHA-256:
  `b1abe954d9e79b295a06b36b36663e61b182f4e2e90a818695f1cc4de71fd2c7`

### Daemon/CLI To Official Android App

- Date: 2026-05-18
- Commit: `2d410d7`
- Sender: NightBridge CLI on macOS
- Receiver: official LocalSend app on Android
- Receiver address: `10.16.20.81:53317`
- Discovery: daemon active LAN scan returned the Android peer; transfer used
  peer alias rather than manual URL
- Command:

```bash
night-bridge send --peer "Rich Pear" target/interop/android/daemon-to-android.txt
```

- CLI result:

```text
transfer: 1a204942-ffb9-43e0-a185-51e871ed116d
```

- User confirmation: Android device received the file.
- Cached discovery verification:

```text
ALIAS                 ADDRESS                PROTOCOL FINGERPRINT
Rich Pear             10.16.20.81:53317      localsend_v2 fbb9f69507413df141076f3068bf680d7d90cc6453ae6b6541b06006a26bc08c
```

- File: `target/interop/android/daemon-to-android.txt`, 45 bytes
- SHA-256:
  `12221c33c2d601520e9b973b0dda61892cff0a4f0e5920faeb8017984d355d8c`

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

- Date: 2026-05-18
- Commit: `2d410d7`
- Receiver version: LocalSend iOS `2.1`
- Receiver info: alias `Diego Personal iPhone`, fingerprint
  `639969AE9CE2CD7C05A9C084423DA4739EEBD454A112B335B0CF8F2ED0E73046`
- Discovery: daemon active LAN scan returned Android and iOS official app
  peers; transfer used peer alias rather than manual URL
- Command:

```bash
night-bridge send --peer "Diego Personal iPhone" target/interop/android/daemon-to-android.txt
```

- CLI result: `transfer: 773ed452-de59-4bed-a61b-dcd5a98b2272`
- Cached discovery verification:

```text
ALIAS                 ADDRESS                PROTOCOL FINGERPRINT
Diego Personal iPhone 10.16.20.53:53317      localsend_v2 639969ae9ce2cd7c05a9c084423da4739eebd454a112b335b0cf8f2ed0e73046
Rich Pear             10.16.20.81:53317      localsend_v2 fbb9f69507413df141076f3068bf680d7d90cc6453ae6b6541b06006a26bc08c
```

### Official iOS App To Daemon

- Date: 2026-05-17
- Commit: `5da324d`
- Sender version: LocalSend iOS `2.1`
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
