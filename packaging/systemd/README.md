# systemd service

This directory contains the baseline unit for running `night-bridge-daemon`
as a hardened system service.

Install the daemon binary at `/usr/bin/night-bridge-daemon`, then install
the unit:

```bash
sudo install -D -m 0644 packaging/systemd/night-bridge.service \
  /etc/systemd/system/night-bridge.service
sudo install -d -m 0755 /etc/night-bridge
sudo install -d -m 0750 /var/lib/night-bridge
sudo systemctl daemon-reload
sudo systemctl enable --now night-bridge.service
```

The service reads configuration from `/etc/night-bridge/config.toml` and
stores runtime state under `/var/lib/night-bridge`.

Validate the unit syntax when `systemd-analyze` is available:

```bash
systemd-analyze verify packaging/systemd/night-bridge.service
```
