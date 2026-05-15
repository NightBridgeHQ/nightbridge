#!/usr/bin/env bash
set -Eeuo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
venv_dir="${PYTHON_SDK_VENV:-$repo_root/sdks/python/.venv}"
python_bin="${PYTHON_BIN:-python3}"

command -v "$python_bin" >/dev/null 2>&1 || {
  printf 'error: missing Python command: %s\n' "$python_bin" >&2
  exit 1
}

"$python_bin" -m venv "$venv_dir"
"$venv_dir/bin/python" -m pip install --upgrade pip
"$venv_dir/bin/python" -m pip install -e "$repo_root/sdks/python[dev]"

printf 'Python SDK environment ready: %s\n' "$venv_dir"
