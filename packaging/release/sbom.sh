#!/usr/bin/env bash
set -euo pipefail

artifact_dir="${1:-dist}"
sbom_file="${artifact_dir}/sbom.cdx.json"

mkdir -p "${artifact_dir}"

if command -v cyclonedx-rust-cargo >/dev/null 2>&1; then
  cyclonedx-rust-cargo --format json --output-file "${sbom_file}"
elif command -v cyclonedx >/dev/null 2>&1; then
  cyclonedx --format json --output-file "${sbom_file}"
else
  cat >"${sbom_file}" <<'JSON'
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.5",
  "serialNumber": "urn:uuid:00000000-0000-4000-8000-000000000000",
  "version": 1,
  "metadata": {
    "component": {
      "type": "application",
      "name": "localsend-improved"
    },
    "properties": [
      {
        "name": "localsend-improved:sbom-status",
        "value": "fallback-minimal; install cyclonedx-rust-cargo for full dependency SBOM"
      }
    ]
  },
  "components": []
}
JSON
fi

echo "wrote ${sbom_file}"
