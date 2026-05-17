#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "building WebUI"
npm run build --prefix crates/webui

echo "checking GUI crate"
cargo check -p lsi-gui

if ! command -v tauri-driver >/dev/null 2>&1; then
  echo "missing: tauri-driver"
  echo "install: cargo install tauri-driver --locked"
  exit 0
fi

echo "tauri-driver is available"
echo "interactive WebDriver launch is intentionally deferred until the desktop flow has standalone mode wired"
