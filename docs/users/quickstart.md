# Quickstart

NightBridge is for headless and desktop-adjacent workflows where a NAS,
server, Raspberry Pi, or workstation should participate in the LocalSend
ecosystem without keeping the official desktop app open.

## Receive From The Official LocalSend App

1. Build or install the daemon.

   ```bash
   cargo build --release -p lsi-daemon
   ```

2. Configure the LocalSend receive policy.

   ```toml
   # ~/.config/night-bridge/config.toml or /etc/night-bridge/config.toml
   [localsend]
   receive_policy = "trusted"
   ```

   Optional static allowlist files are still supported:

   ```text
   # /etc/night-bridge/localsend-trusted.txt
   <official-app-fingerprint>
   ```

3. Start the daemon with an inbox path and LocalSend port.

   ```bash
   target/release/night-bridge-daemon \
     --alias "NAS" \
     --inbox "$HOME/LocalSendInbox" \
     --localsend-port 53317
   ```

4. Open the official LocalSend app on your phone or desktop.
5. Select the daemon by alias and send a file. The first attempt from an
   unknown device is rejected and recorded as pending.
6. Approve or deny the pending fingerprint.

   ```bash
   night-bridge peers pending-local-send
   night-bridge peers approve-local-send <fingerprint> --label "My phone"
   ```

7. Ask the sender to retry, then check the daemon inbox.

The daemon defaults to `--localsend-receive-policy prompt`, which rejects
incoming LocalSend uploads until an approval workflow is available. Use
`trusted` for unattended receive from devices approved in the daemon trust
database, listed in `trusted_fingerprints`, or listed in
`trusted_fingerprints_file`. Approved fingerprints and fingerprint files are read
for each new upload session, so adding a device does not require restarting the
daemon. Use `auto` only on a trusted test LAN; it accepts any
LocalSend-compatible sender that can reach the daemon port.

The LocalSend v2 compatibility path keeps official app behavior, including
self-signed HTTPS compatibility expectations, but receive authorization remains
controlled by the daemon policy above.

## Send From CLI To A LocalSend Peer

Use an explicit LocalSend peer URL when discovery is not wired into your flow:

```bash
night-bridge send --direct --url http://192.168.1.20:53317 ./photo.jpg
```

For daemon-mediated sends:

```bash
night-bridge send --url http://192.168.1.20:53317 ./photo.jpg
```

Native direct sends are separate from LocalSend compatibility and require
native trust metadata or an explicit certificate fingerprint for direct CLI
testing.

## Native Mesh LAN Preview

Use Native Mesh LAN when two NightBridge daemons on the same trusted LAN should
send directly to each other without LocalSend apps or manual URLs.

On both machines, start the daemon with native networking enabled. Then discover
NightBridge peers:

```bash
night-bridge peers list-native
```

Approve a discovered peer once:

```bash
night-bridge peers approve-native <alias-or-fingerprint> --label "Home NAS"
```

Send over native QUIC by alias or fingerprint:

```bash
night-bridge send --native --peer "Home NAS" ./archive.tar
```

Firewalls must allow mDNS and the native QUIC port on the trusted LAN:

- UDP `5353` for mDNS discovery.
- UDP `53400` for native QUIC transfers, unless started with another
  `--native-port`.

The alpha mesh path requires a discovered peer with native certificate metadata
and a trusted `auto_accept` record in the daemon trust database. Unknown,
blocked, or prompt-policy native senders are rejected.

## WebUI And TUI Status

Use the WebUI or TUI for status and operations once the daemon API token is
available.

The TUI defaults to the 26.5 LocalSend receiver view. It shows daemon status,
pending official LocalSend senders, active transfers, and inbox entries. Use
`j`/`k` to select a pending LocalSend sender, `a` to approve it, `d` to deny
it, and `q` to quit.

Native protocol details stay hidden by default. Use `night-bridge-tui
--advanced` only when validating native/QUIC behavior.

Typical checks:

```bash
night-bridge daemon status
night-bridge peers list-lan
night-bridge peers list-native
night-bridge peers list-trusted
```

The daemon API is protected by a bearer token. Treat the token like a password.

## Desktop Choice

Use remote daemon mode when the daemon runs on a NAS or server and the desktop
app is only an operator console.

Use standalone mode when the desktop app should start and manage a local daemon
on the same machine.

Before the first CalVer release, desktop packages are pre-release. Unsigned
package warnings are expected until signing and notarization are configured.

## WAN

WAN uses a self-hosted rendezvous service for native direct-path discovery. It
does not relay file bytes. If peers are behind restrictive NATs, WAN sends can
fail even when rendezvous is working.
