#!/usr/bin/env bash
set -euo pipefail

image="${NIGHTBRIDGE_DOCKER_IMAGE:-night-bridge:sprint4}"

docker build -t "${image}" .
docker run --rm "${image}" --help
