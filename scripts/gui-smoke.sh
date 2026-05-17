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
  if [[ "${LSI_GUI_SMOKE_STRICT:-0}" == "1" ]]; then
    exit 1
  fi
  exit 0
fi

port="${LSI_GUI_SMOKE_WEBDRIVER_PORT:-4444}"
log="${LSI_GUI_SMOKE_LOG:-target/gui-smoke/tauri-driver.log}"
mkdir -p "$(dirname "$log")"

echo "tauri-driver is available"
echo "starting tauri-driver on 127.0.0.1:$port"
tauri-driver --port "$port" >"$log" 2>&1 &
driver_pid=$!
cleanup() {
  kill "$driver_pid" >/dev/null 2>&1 || true
  wait "$driver_pid" >/dev/null 2>&1 || true
}
trap cleanup EXIT

for _ in 1 2 3 4 5; do
  if curl -fsS "http://127.0.0.1:$port/status" >/dev/null 2>&1; then
    echo "tauri-driver status endpoint is available"
    exit 0
  fi
  sleep 1
done

echo "tauri-driver did not respond on /status"
echo "log: $log"
exit 1
