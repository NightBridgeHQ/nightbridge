# LocalSend Interop Matrix

LocalSend Improved keeps LocalSend v2 compatibility separate from the native
protocol. This matrix tracks official-app interoperability evidence for the 1.0
candidate.

| Platform | LocalSend version | Receive official app to daemon | Send daemon/CLI to official app | Discovery method | Evidence path | Status |
| --- | --- | --- | --- | --- | --- | --- |
| Android | TBD | Manual | Manual | LAN multicast / manual URL | `target/interop/android/` | Pending official-device run |
| iOS | TBD | Manual | Manual | LAN multicast / manual URL | `target/interop/ios/` | Pending official-device run |
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

## Manual Evidence

For each platform, preserve:

- official LocalSend app version and download source
- daemon commit SHA and OS
- sender and receiver screenshots or logs
- transferred file name, size, and checksum
- discovery path used, including whether manual URL fallback was needed

If `LSI_OFFICIAL_LOCALSEND_ARTIFACT` later points to a verified
headless-capable official artifact, the smoke wrapper may grow an automated
official-app path. Until then, official-app status stays manual.
