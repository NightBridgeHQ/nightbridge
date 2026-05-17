# Migration From LocalSend

NightBridge does not replace the official LocalSend app for phones and
general desktop users. It adds a headless daemon, CLI, API, WebUI/TUI, native
protocol work, and WAN rendezvous support for server-style deployments.

## What Stays Compatible

- Official LocalSend apps can send files to the daemon through the LocalSend v2
  receive API.
- LocalSend v2 metadata, file preparation, and upload behavior remain the
  compatibility target.
- Self-signed HTTPS compatibility is preserved inside the LocalSend protocol
  path.
- Manual URL sends to a LocalSend peer remain available.

## What Changes For NAS And Headless Users

Instead of keeping a desktop app open, you can run a daemon as a service:

- files land in a configured inbox
- CLI commands can inspect status and send files
- API/WebUI/TUI surfaces can be used for operations
- systemd packaging can keep the daemon running after reboot

This is the main migration path for a NAS, Raspberry Pi, homelab server, or
always-on workstation.

## Trust Model Differences

LocalSend compatibility follows the official app's LAN behavior. It is designed
for interoperability and intentionally keeps LocalSend v2 self-signed TLS
compatibility scoped to that protocol.

The native protocol is stricter. Native trusted peers use Ed25519 identity,
trust policies, and pinned native certificate metadata. Native WAN auto-send
requires a trusted peer in `auto_accept` policy with native certificate
metadata. Unknown, blocked, prompt-only, or mismatched native peers fail closed.

## WAN Rendezvous Expectations

WAN rendezvous is a control plane:

- peers register short-lived candidates
- peers look each other up
- file bytes still travel over direct native QUIC paths
- the open-base v1 build does not include a relay

If NAT or firewall policy prevents a direct path, use LAN, VPN, port forwarding,
or a future relay-capable deployment.

## Limitations Before 1.0

- Desktop packages are pre-release until signing and notarization are complete.
- No third-party audit has been completed.
- Existing trusted peers without native certificate metadata must be re-paired
  or refreshed before native WAN auto-send works.
- Official LocalSend app matrix evidence is still manual.
- Public relay/VIP fallback is not part of the open-base v1 build.
