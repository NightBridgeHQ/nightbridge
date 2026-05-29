#!/bin/sh
# POSIX sh installer (runs under dash/ash/bash); safe for `curl ... | sh`.
set -eu

repo="${NIGHTBRIDGE_REPO:-NightBridgeHQ/nightbridge}"
version="${NIGHTBRIDGE_VERSION:-latest}"
install_dir="${NIGHTBRIDGE_INSTALL_DIR:-/usr/local/bin}"
github_base="${GITHUB_API_URL:-https://api.github.com}"
modify_path=1

usage() {
  cat <<'EOF'
Usage: install.sh [OPTIONS]

Install NightBridge binaries from GitHub Releases.

Options:
  --repo OWNER/REPO      GitHub repository. Defaults to NIGHTBRIDGE_REPO.
  --version VERSION      Release version or tag. Defaults to latest.
  --install-dir DIR      Install directory. Defaults to /usr/local/bin.
  --no-modify-path       Do not add the install directory to your shell PATH.
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
  if ! command -v "$1" >/dev/null 2>&1; then
    err "missing required command: $1"
    exit 1
  fi
}

while [ $# -gt 0 ]; do
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
    --no-modify-path)
      modify_path=0
      shift
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
  curl -fsSL "$1" -o "$2"
}

release_download_base() {
  printf 'https://github.com/%s/releases/download/%s\n' "$repo" "$version"
}

resolve_latest_version() {
  response="$(curl -fsSL "${github_base}/repos/${repo}/releases/latest")"
  tag="$(printf '%s\n' "$response" | sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
  if [ -z "$tag" ]; then
    err "could not resolve latest release tag for ${repo}"
    exit 1
  fi
  printf '%s\n' "$tag"
}

install_file() {
  if [ -w "$install_dir" ]; then
    install -m 0755 "$1" "$2"
  else
    sudo install -m 0755 "$1" "$2"
  fi
}

ensure_on_path() {
  # Nothing to do when the install dir is already reachable (e.g. the default
  # /usr/local/bin). Match the whole path component, not a substring.
  case ":${PATH}:" in
    *":${install_dir}:"*) return 0 ;;
  esac

  shell_name="$(basename "${SHELL:-sh}")"

  if [ "$modify_path" -ne 1 ]; then
    err "${install_dir} is not on your PATH"
    printf 'Add it with:\n  export PATH="%s:$PATH"\n' "$install_dir" >&2
    return 0
  fi

  marker="# Added by NightBridge install.sh"
  case "$shell_name" in
    fish)
      target="${HOME}/.config/fish/config.fish"
      line="fish_add_path \"${install_dir}\""
      ;;
    zsh)
      target="${ZDOTDIR:-$HOME}/.zshrc"
      line="export PATH=\"${install_dir}:\$PATH\""
      ;;
    bash)
      target="${HOME}/.bashrc"
      line="export PATH=\"${install_dir}:\$PATH\""
      ;;
    *)
      target="${HOME}/.profile"
      line="export PATH=\"${install_dir}:\$PATH\""
      ;;
  esac

  mkdir -p "$(dirname "$target")"
  if [ -f "$target" ] && grep -qF "$marker" "$target"; then
    printf 'PATH entry for %s already present in %s\n' "$install_dir" "$target"
  else
    printf '\n%s\n%s\n' "$marker" "$line" >> "$target"
    printf 'Added %s to PATH in %s\n' "$install_dir" "$target"
  fi
  printf 'Open a new shell, or run now:\n  export PATH="%s:$PATH"\n' "$install_dir"
}

main() {
  need_cmd curl
  need_cmd shasum
  need_cmd tar
  need_cmd install
  need_cmd grep

  if ! printf '%s' "$repo" | grep -qE '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$'; then
    err "invalid GitHub repo: $repo"
    exit 1
  fi

  if [ "$version" = "latest" ]; then
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
  if [ -z "$checksum_line" ]; then
    err "SHA256SUMS does not contain ${asset}"
    exit 1
  fi

  printf '%s\n' "$checksum_line" > "${tmp}/SHA256SUMS.selected"
  (cd "$tmp" && shasum -a 256 -c SHA256SUMS.selected)

  mkdir -p "${tmp}/unpack"
  tar -xzf "$tarball" -C "${tmp}/unpack"

  if [ ! -d "$install_dir" ]; then
    mkdir -p "$install_dir" 2>/dev/null || sudo mkdir -p "$install_dir"
  fi

  for bin in night-bridge nbrg night-bridge-daemon night-bridge-tui; do
    if [ -f "${tmp}/unpack/${bin}" ]; then
      install_file "${tmp}/unpack/${bin}" "${install_dir}/${bin}"
      printf 'installed %s\n' "${install_dir}/${bin}"
    fi
  done

  ensure_on_path

  printf 'NightBridge installed. Try: night-bridge --help\n'
}

main "$@"
