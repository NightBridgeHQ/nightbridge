#!/usr/bin/env bash
set -euo pipefail

image="night-bridge:sprint4"

docker build -t "${image}" .
docker run --rm "${image}" --help
