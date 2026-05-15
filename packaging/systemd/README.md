# systemd service

This directory contains the baseline unit for running `localsend-improved-daemon`
as a hardened system service.

Install the daemon binary at `/usr/bin/localsend-improved-daemon`, then install
the unit:

```bash
sudo install -D -m 0644 packaging/systemd/localsend-improved.service \
  /etc/systemd/system/localsend-improved.service
sudo install -d -m 0755 /etc/localsend-improved
sudo install -d -m 0750 /var/lib/localsend-improved
sudo systemctl daemon-reload
sudo systemctl enable --now localsend-improved.service
```

The service reads configuration from `/etc/localsend-improved/config.toml` and
stores runtime state under `/var/lib/localsend-improved`.

Validate the unit syntax when `systemd-analyze` is available:

```bash
systemd-analyze verify packaging/systemd/localsend-improved.service
```
