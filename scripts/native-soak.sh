#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${NBRG_SOAK_BYTES:=134217728}"
: "${NBRG_SOAK_RECONNECTS:=10}"
: "${NBRG_SOAK_SEED:=0}"
: "${NBRG_SOAK_LOG:=target/soak/native-soak.log}"

mkdir -p "$(dirname "$NBRG_SOAK_LOG")"

echo "NBRG_SOAK_BYTES=$NBRG_SOAK_BYTES"
echo "NBRG_SOAK_RECONNECTS=$NBRG_SOAK_RECONNECTS"
echo "NBRG_SOAK_SEED=$NBRG_SOAK_SEED"
echo "NBRG_SOAK_LOG=$NBRG_SOAK_LOG"

NBRG_SOAK=1 \
NBRG_SOAK_BYTES="$NBRG_SOAK_BYTES" \
NBRG_SOAK_RECONNECTS="$NBRG_SOAK_RECONNECTS" \
NBRG_SOAK_SEED="$NBRG_SOAK_SEED" \
cargo test -p lsi-protocol-native-v1 --test soak_resume -- --ignored --nocapture \
  2>&1 | tee "$NBRG_SOAK_LOG"
