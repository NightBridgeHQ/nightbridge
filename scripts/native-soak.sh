#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${LSI_SOAK_BYTES:=134217728}"
: "${LSI_SOAK_RECONNECTS:=10}"
: "${LSI_SOAK_SEED:=0}"
: "${LSI_SOAK_LOG:=target/soak/native-soak.log}"

mkdir -p "$(dirname "$LSI_SOAK_LOG")"

echo "LSI_SOAK_BYTES=$LSI_SOAK_BYTES"
echo "LSI_SOAK_RECONNECTS=$LSI_SOAK_RECONNECTS"
echo "LSI_SOAK_SEED=$LSI_SOAK_SEED"
echo "LSI_SOAK_LOG=$LSI_SOAK_LOG"

LSI_SOAK=1 \
LSI_SOAK_BYTES="$LSI_SOAK_BYTES" \
LSI_SOAK_RECONNECTS="$LSI_SOAK_RECONNECTS" \
LSI_SOAK_SEED="$LSI_SOAK_SEED" \
cargo test -p lsi-protocol-native-v1 --test soak_resume -- --ignored --nocapture \
  2>&1 | tee "$LSI_SOAK_LOG"
