#!/usr/bin/env bash
set -euo pipefail

check_tools() {
  local missing=0

  if ! command -v protoc >/dev/null 2>&1; then
    echo "missing: protoc"
    echo "install: sudo apt-get install protobuf-compiler"
    missing=1
  fi

  if ! cargo deb --version >/dev/null 2>&1; then
    echo "missing: cargo-deb"
    echo "install: cargo install cargo-deb --version 3.6.2 --locked"
    missing=1
  fi

  if ! cargo generate-rpm --version >/dev/null 2>&1; then
    echo "missing: cargo-generate-rpm"
    echo "install: cargo install cargo-generate-rpm --locked"
    missing=1
  fi

  if [[ "${missing}" -eq 0 ]]; then
    echo "package tools available"
  fi

  return "${missing}"
}

if [[ "${1:-}" == "--check-tools" ]]; then
  check_tools || true
  exit 0
fi

check_tools
cargo build --release -p lsi-daemon
cargo deb -p lsi-daemon
cargo generate-rpm -p lsi-daemon
