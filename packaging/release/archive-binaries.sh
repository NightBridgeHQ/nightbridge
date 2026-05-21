#!/usr/bin/env bash
set -Eeuo pipefail

version="${1:?usage: packaging/release/archive-binaries.sh VERSION [DIST_DIR] [PLATFORM]}"
dist="${2:-dist}"
platform="${3:-}"

detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os="linux" ;;
    Darwin) os="macos" ;;
    *)
      echo "unsupported OS for release archive: $os" >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="amd64" ;;
    arm64|aarch64) arch="arm64" ;;
    *)
      echo "unsupported architecture for release archive: $arch" >&2
      exit 1
      ;;
  esac

  printf '%s-%s\n' "$os" "$arch"
}

if [[ -z "$platform" ]]; then
  platform="$(detect_platform)"
fi

mkdir -p "$dist"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

bins=(night-bridge nbrg night-bridge-daemon night-bridge-tui)
for bin in "${bins[@]}"; do
  if [[ -f "target/release/${bin}" ]]; then
    install -m 0755 "target/release/${bin}" "${tmp}/${bin}"
  fi
done

if [[ ! -f "${tmp}/night-bridge" || ! -f "${tmp}/night-bridge-daemon" ]]; then
  echo "missing required release binaries in target/release" >&2
  exit 1
fi

asset="nightbridge-${version}-${platform}.tar.gz"
tar -C "$tmp" -czf "${dist}/${asset}" .
echo "wrote ${dist}/${asset}"
