# WebUI Token Bootstrap Decision

## Decision

For Sprint 4, the embedded WebUI stays loadable without credentials when served on loopback. All `/api/*` requests remain bearer-authenticated.

The WebUI must use manual token paste: the user copies the daemon API token from `api.token` into the browser UI. The token is still managed by the local API token vault and is not exposed through a browser bootstrap endpoint.

## Rejected Bootstrap Paths

- Do not pass the API token in URL query parameters, fragments, redirects, or links. Query token bootstrap leaks through browser history, logs, referrers, screenshots, and copied URLs.
- Do not add an unauthenticated token endpoint. A public loopback WebUI is acceptable only while the API remains protected.
- Do not weaken `/api/*` bearer checks for WebUI convenience.

## Future Secure Options

A future bootstrap flow needs a separate design before implementation. Acceptable directions are:

- Print a one-time local pairing code to daemon stderr, then require the browser to exchange that code for a short-lived WebUI session.
- Open the browser through a CLI-mediated session that proves local user intent before issuing a scoped WebUI credential.

Both options must stay local-only, expire quickly, and avoid placing bearer tokens in URLs.
