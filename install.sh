#!/usr/bin/env bash
set -Eeuo pipefail

repo="${NIGHTBRIDGE_REPO:-NightBridgeHQ/nightbridge}"
version="${NIGHTBRIDGE_VERSION:-latest}"
install_dir="${NIGHTBRIDGE_INSTALL_DIR:-/usr/local/bin}"
github_base="${GITHUB_API_URL:-https://api.github.com}"

usage() {
  cat <<'EOF'
Usage: install.sh [OPTIONS]

Install NightBridge binaries from GitHub Releases.

Options:
  --repo OWNER/REPO      GitHub repository. Defaults to NIGHTBRIDGE_REPO.
  --version VERSION      Release version or tag. Defaults to latest.
  --install-dir DIR      Install directory. Defaults to /usr/local/bin.
  -h, --help             Show this help.

Environment:
  NIGHTBRIDGE_REPO
  NIGHTBRIDGE_VERSION
  NIGHTBRIDGE_INSTALL_DIR
EOF
}

err() {
  printf 'error: %s\n' "$*" >&2
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    err "missing required command: $1"
    exit 1
  }
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      repo="${2:?missing value for --repo}"
      shift 2
      ;;
    --version)
      version="${2:?missing value for --version}"
      shift 2
      ;;
    --install-dir)
      install_dir="${2:?missing value for --install-dir}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      err "unknown option: $1"
      usage >&2
      exit 2
      ;;
  esac
done

detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os="linux" ;;
    Darwin) os="macos" ;;
    *)
      err "unsupported OS: $os"
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="amd64" ;;
    arm64|aarch64) arch="arm64" ;;
    *)
      err "unsupported architecture: $arch"
      exit 1
      ;;
  esac

  printf '%s-%s\n' "$os" "$arch"
}

download() {
  local url="$1"
  local output="$2"
  curl -fsSL "$url" -o "$output"
}

release_download_base() {
  printf 'https://github.com/%s/releases/download/%s\n' "$repo" "$version"
}

resolve_latest_version() {
  local response tag
  response="$(curl -fsSL "${github_base}/repos/${repo}/releases/latest")"
  tag="$(printf '%s\n' "$response" | sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
  if [[ -z "$tag" ]]; then
    err "could not resolve latest release tag for ${repo}"
    exit 1
  fi
  printf '%s\n' "$tag"
}

install_file() {
  local src="$1"
  local dst="$2"

  if [[ -w "$install_dir" ]]; then
    install -m 0755 "$src" "$dst"
  else
    sudo install -m 0755 "$src" "$dst"
  fi
}

main() {
  need_cmd curl
  need_cmd shasum
  need_cmd tar
  need_cmd install

  if [[ ! "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
    err "invalid GitHub repo: $repo"
    exit 1
  fi

  local platform asset base tmp tarball checksums checksum_line
  if [[ "$version" == "latest" ]]; then
    version="$(resolve_latest_version)"
  fi

  platform="$(detect_platform)"
  asset="nightbridge-${version#v}-${platform}.tar.gz"
  base="$(release_download_base)"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  tarball="${tmp}/${asset}"
  checksums="${tmp}/SHA256SUMS"

  printf 'Downloading NightBridge %s for %s from %s\n' "$version" "$platform" "$repo"
  download "${base}/${asset}" "$tarball"
  download "${base}/SHA256SUMS" "$checksums"

  checksum_line="$(grep -E "  ${asset}$" "$checksums" || true)"
  if [[ -z "$checksum_line" ]]; then
    err "SHA256SUMS does not contain ${asset}"
    exit 1
  fi

  printf '%s\n' "$checksum_line" > "${tmp}/SHA256SUMS.selected"
  (cd "$tmp" && shasum -a 256 -c SHA256SUMS.selected)

  mkdir -p "${tmp}/unpack"
  tar -xzf "$tarball" -C "${tmp}/unpack"

  if [[ ! -d "$install_dir" ]]; then
    mkdir -p "$install_dir" 2>/dev/null || sudo mkdir -p "$install_dir"
  fi

  local bin
  for bin in night-bridge nbrg night-bridge-daemon night-bridge-tui; do
    if [[ -f "${tmp}/unpack/${bin}" ]]; then
      install_file "${tmp}/unpack/${bin}" "${install_dir}/${bin}"
      printf 'installed %s\n' "${install_dir}/${bin}"
    fi
  done

  printf 'NightBridge installed. Try: %s/night-bridge --help\n' "$install_dir"
}

main "$@"
