#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check --prefix crates/webui
npm run build --prefix crates/webui
cargo test -p lsi-protocol-native-v1 --test soak_resume patterned_bytes_have_stable_hash
cargo test --test gui_smoke -- --nocapture
bash scripts/localsend-interop-smoke.sh
