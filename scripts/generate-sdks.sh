#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/generate-sdks.sh [python|typescript|go|all]

Generates SDK protocol bindings from crates/proto/proto.

Required tools by target:
  python      python3, grpcio-tools, betterproto[compiler]
  typescript  protoc, protoc-gen-es, protoc-gen-connect-es
  go          protoc, protoc-gen-go, protoc-gen-connect-go
EOF
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

info() {
  printf '==> %s\n' "$*"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

require_python_module() {
  local module="$1"

  python3 -c "import ${module}" >/dev/null 2>&1 || die "missing required Python module: ${module}"
}

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
proto_root="$repo_root/crates/proto/proto"

target="${1:-all}"

case "$target" in
  python|typescript|go|all|-h|--help) ;;
  *) die "unknown target '$target'";;
esac

if [[ "$target" == "-h" || "$target" == "--help" ]]; then
  usage
  exit 0
fi

[[ -d "$proto_root" ]] || die "proto root not found: $proto_root"

proto_files=(
  "$proto_root/lsi/common/v1/common.proto"
  "$proto_root/lsi/daemon/v1/daemon.proto"
  "$proto_root/lsi/peers/v1/peers.proto"
  "$proto_root/lsi/transfers/v1/transfers.proto"
  "$proto_root/lsi/inbox/v1/inbox.proto"
  "$proto_root/lsi/events/v1/events.proto"
)

for proto_file in "${proto_files[@]}"; do
  [[ -f "$proto_file" ]] || die "proto file not found: $proto_file"
done

generate_python() {
  local out_dir="$repo_root/sdks/python/src/localsend_improved/gen"

  require_command python3
  require_python_module grpc_tools.protoc
  require_python_module betterproto
  mkdir -p "$out_dir"

  info "generating Python betterproto bindings"
  python3 -m grpc_tools.protoc \
    -I "$proto_root" \
    --python_betterproto_out="$out_dir" \
    "${proto_files[@]}"
}

generate_typescript() {
  local out_dir="$repo_root/sdks/typescript/gen"

  require_command protoc
  require_command protoc-gen-es
  require_command protoc-gen-connect-es
  mkdir -p "$out_dir"

  info "generating TypeScript protobuf/connect bindings"
  protoc \
    -I "$proto_root" \
    --es_out="$out_dir" \
    --es_opt=target=ts \
    --connect-es_out="$out_dir" \
    --connect-es_opt=target=ts \
    "${proto_files[@]}"
}

generate_go() {
  local out_dir="$repo_root/sdks/go/gen"
  local module="github.com/chrnx/localsend-improved/sdks/go/gen"

  require_command protoc
  require_command protoc-gen-go
  require_command protoc-gen-connect-go
  mkdir -p "$out_dir"

  info "generating Go protobuf/connect bindings"
  protoc \
    -I "$proto_root" \
    --go_out="$out_dir" \
    --go_opt=paths=source_relative \
    --go_opt=Mlsi/common/v1/common.proto="$module/lsi/common/v1" \
    --go_opt=Mlsi/daemon/v1/daemon.proto="$module/lsi/daemon/v1" \
    --go_opt=Mlsi/peers/v1/peers.proto="$module/lsi/peers/v1" \
    --go_opt=Mlsi/transfers/v1/transfers.proto="$module/lsi/transfers/v1" \
    --go_opt=Mlsi/inbox/v1/inbox.proto="$module/lsi/inbox/v1" \
    --go_opt=Mlsi/events/v1/events.proto="$module/lsi/events/v1" \
    --connect-go_out="$out_dir" \
    --connect-go_opt=paths=source_relative \
    --connect-go_opt=Mlsi/common/v1/common.proto="$module/lsi/common/v1" \
    --connect-go_opt=Mlsi/daemon/v1/daemon.proto="$module/lsi/daemon/v1" \
    --connect-go_opt=Mlsi/peers/v1/peers.proto="$module/lsi/peers/v1" \
    --connect-go_opt=Mlsi/transfers/v1/transfers.proto="$module/lsi/transfers/v1" \
    --connect-go_opt=Mlsi/inbox/v1/inbox.proto="$module/lsi/inbox/v1" \
    --connect-go_opt=Mlsi/events/v1/events.proto="$module/lsi/events/v1" \
    "${proto_files[@]}"
}

case "$target" in
  python)
    generate_python
    ;;
  typescript)
    generate_typescript
    ;;
  go)
    generate_go
    ;;
  all)
    generate_python
    generate_typescript
    generate_go
    ;;
esac
