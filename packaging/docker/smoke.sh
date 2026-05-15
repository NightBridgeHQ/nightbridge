#!/usr/bin/env bash
set -euo pipefail

image="localsend-improved:sprint4"

docker build -t "${image}" .
docker run --rm "${image}" --help
