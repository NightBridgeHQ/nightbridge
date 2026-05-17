# Operator Guide

This guide is for people running LocalSend Improved on a server, workstation,
or small homelab fleet.

## Guides

- [Daemon](daemon.md): build, run, configure ports, tokens, health, metrics,
  and trust store operations.
- [Rendezvous](rendezvous.md): operate the WAN control plane for native direct
  path discovery.
- [Desktop](desktop.md): choose remote daemon mode or standalone desktop mode.
- [Troubleshooting](troubleshooting.md): diagnose API token, LAN discovery, WAN,
  and desktop package issues.

## Security Model

Read the security docs before exposing the daemon or rendezvous service outside
a trusted local machine:

- [Threat model](../security/threat-model.md)
- [Native trust audit](../security/native-trust-audit.md)
- [Rendezvous privacy](../security/rendezvous-privacy.md)
- [WebUI token bootstrap](../security/webui-token-bootstrap.md)
