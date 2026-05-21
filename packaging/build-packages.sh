#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: packaging/build-packages.sh [--check-tools] [--deb-only] [--rpm-only]

Build daemon packages for the current checkout.
EOF
}

check_tools() {
  local missing=0
  local target="${1:-all}"

  if ! command -v protoc >/dev/null 2>&1; then
    echo "missing: protoc"
    echo "install: sudo apt-get install protobuf-compiler"
    missing=1
  fi

  if [[ "${target}" != "rpm" ]] && ! cargo deb --version >/dev/null 2>&1; then
    echo "missing: cargo-deb"
    echo "install: cargo install cargo-deb --version 3.6.2 --locked"
    missing=1
  fi

  if [[ "${target}" != "deb" ]] && ! cargo generate-rpm --version >/dev/null 2>&1; then
    echo "missing: cargo-generate-rpm"
    echo "install: cargo install cargo-generate-rpm --locked"
    missing=1
  fi

  if [[ "${missing}" -eq 0 ]]; then
    echo "package tools available"
  fi

  return "${missing}"
}

target="all"
check_only=0

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --check-tools)
      check_only=1
      ;;
    --deb-only)
      target="deb"
      ;;
    --rpm-only)
      target="rpm"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [[ "${check_only}" -eq 1 ]]; then
  check_tools "${target}" || true
  exit 0
fi

check_tools "${target}"
cargo build --release -p lsi-daemon

if [[ "${target}" != "rpm" ]]; then
  cargo deb -p lsi-daemon
fi

if [[ "${target}" != "deb" ]]; then
  cargo generate-rpm -p lsi-daemon
fi
