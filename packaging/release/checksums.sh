#!/usr/bin/env bash
set -euo pipefail

artifact_dir="${1:-dist}"
checksum_file="${artifact_dir}/SHA256SUMS"

if [[ ! -d "${artifact_dir}" ]]; then
  echo "artifact directory not found: ${artifact_dir}" >&2
  exit 1
fi

find "${artifact_dir}" -maxdepth 1 -type f ! -name "SHA256SUMS" -print0 \
  | sort -z \
  | xargs -0 shasum -a 256 > "${checksum_file}"

echo "wrote ${checksum_file}"
