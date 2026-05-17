#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test -p lsi-protocol-localsend-v2 --test interop_receive -- --nocapture

cat <<'MSG'
Official LocalSend app matrix is manual unless NBRG_OFFICIAL_LOCALSEND_ARTIFACT
points to a verified headless-capable artifact for this platform.
MSG
