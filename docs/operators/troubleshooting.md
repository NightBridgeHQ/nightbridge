# Troubleshooting

## API Token Missing

Symptoms:

- CLI says `api token not found`.
- WebUI or GUI cannot authenticate to the daemon.

Checks:

```bash
ls -l /var/lib/localsend-improved/api.token
systemctl status localsend-improved.service
```

Fix the state path permissions or restart the daemon with a writable state
directory. Treat the token as a secret.

## No LAN Peers

Symptoms:

- `localsend-improved peers list-lan` returns no peers.
- Official LocalSend devices do not see the daemon.

Checks:

- Same LAN/VLAN and multicast allowed.
- LocalSend port is open.
- Local firewall allows inbound LocalSend v2 traffic.
- The daemon is advertising with the expected alias and port.

## WAN No Direct Path

Symptoms:

- WAN send fails with `no direct WAN path`.
- Diagnostics mention attempted candidate pairs.

Checks:

- Rendezvous URL is configured.
- Both peers can register with rendezvous.
- Firewalls allow the native QUIC UDP port.
- Candidate addresses are reachable from the other peer.

## Symmetric NAT Or No Relay

The open-base v1 build has no relay. If both peers are behind restrictive or
symmetric NATs, direct WAN candidate dialing can fail even when rendezvous is
working. Use LAN, VPN, port forwarding, or a future relay-capable deployment.

## GUI Cannot Start Standalone Daemon

Checks:

- daemon binary exists and is executable
- selected inbox and state paths are writable
- configured LocalSend and native ports are free
- API token bootstrap can write to the state path
- desktop app has required local permissions

Try remote daemon mode if standalone process management is blocked by platform
policy.

## Unsigned Desktop Package Warnings

Pre-release packages may trigger macOS Gatekeeper, Windows SmartScreen, or Linux
repository trust warnings. Production release builds require signing,
notarization, and package repository metadata before those warnings can be
treated as defects.
