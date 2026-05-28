#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${NBRG_WAN_NETNS_RUN_ID:-$$}"
NS_RV="lsi-rv-${RUN_ID}"
NS_A="lsi-a-${RUN_ID}"
NS_B="lsi-b-${RUN_ID}"
RV_PORT="${NBRG_WAN_RENDEZVOUS_PORT:-53410}"
A_NATIVE_PORT="${NBRG_WAN_A_NATIVE_PORT:-53401}"
B_NATIVE_PORT="${NBRG_WAN_B_NATIVE_PORT:-53402}"
RV_ADDR="198.18.53.1"
A_ADDR="198.18.53.2"
B_ADDR="198.18.54.2"
A_GW="198.18.53.1"
B_GW="198.18.54.1"
STATE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/nbrg-wan-netns.XXXXXX")"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 127
  }
}

cleanup() {
  set +e
  for pid_file in "$STATE_ROOT"/*.pid; do
    [ -e "$pid_file" ] || continue
    kill "$(cat "$pid_file")" 2>/dev/null || true
  done
  ip netns del "$NS_A" 2>/dev/null || true
  ip netns del "$NS_B" 2>/dev/null || true
  ip netns del "$NS_RV" 2>/dev/null || true
  rm -rf "$STATE_ROOT"
}
trap cleanup EXIT

require cargo
require ip
require sha256sum

if [ "$(id -u)" -ne 0 ]; then
  echo "wan netns smoke requires root or CAP_NET_ADMIN privileges" >&2
  exit 77
fi

if ! command -v iptables >/dev/null 2>&1 && ! command -v nft >/dev/null 2>&1; then
  echo "warning: neither iptables nor nft was found; running without NAT rule setup" >&2
fi

cargo build -p lsi-rendezvous -p lsi-daemon -p lsi-cli

RENDEZVOUS_BIN="$ROOT_DIR/target/debug/night-bridge-rendezvous"
DAEMON_BIN="$ROOT_DIR/target/debug/night-bridge-daemon"
CLI_BIN="$ROOT_DIR/target/debug/night-bridge"

ip netns add "$NS_RV"
ip netns add "$NS_A"
ip netns add "$NS_B"

ip link add veth-rv-a type veth peer name veth-a
ip link add veth-rv-b type veth peer name veth-b
ip link set veth-rv-a netns "$NS_RV"
ip link set veth-rv-b netns "$NS_RV"
ip link set veth-a netns "$NS_A"
ip link set veth-b netns "$NS_B"

ip -n "$NS_RV" addr add "${A_GW}/24" dev veth-rv-a
ip -n "$NS_RV" addr add "${B_GW}/24" dev veth-rv-b
ip -n "$NS_A" addr add "${A_ADDR}/24" dev veth-a
ip -n "$NS_B" addr add "${B_ADDR}/24" dev veth-b

ip -n "$NS_RV" link set lo up
ip -n "$NS_RV" link set veth-rv-a up
ip -n "$NS_RV" link set veth-rv-b up
ip -n "$NS_A" link set lo up
ip -n "$NS_A" link set veth-a up
ip -n "$NS_B" link set lo up
ip -n "$NS_B" link set veth-b up

ip -n "$NS_A" route add default via "$A_GW" dev veth-a 2>/dev/null || true
ip -n "$NS_B" route add default via "$B_GW" dev veth-b 2>/dev/null || true

if command -v iptables >/dev/null 2>&1; then
  ip netns exec "$NS_A" iptables -t nat -A POSTROUTING -j MASQUERADE || true
  ip netns exec "$NS_B" iptables -t nat -A POSTROUTING -j MASQUERADE || true
fi

mkdir -p "$STATE_ROOT/a/config" "$STATE_ROOT/a/data" "$STATE_ROOT/b/config" "$STATE_ROOT/b/data"
PAYLOAD="$STATE_ROOT/payload.txt"
INBOX_B="$STATE_ROOT/b/inbox"
printf 'wan netns smoke payload\n' > "$PAYLOAD"
mkdir -p "$INBOX_B"

ip netns exec "$NS_RV" "$RENDEZVOUS_BIN" --bind "${RV_ADDR}:${RV_PORT}" --max-ttl-seconds 300 \
  >"$STATE_ROOT/rendezvous.log" 2>&1 &
echo "$!" > "$STATE_ROOT/rendezvous.pid"

sleep 1

ip netns exec "$NS_A" env \
  XDG_CONFIG_HOME="$STATE_ROOT/a/config" \
  XDG_DATA_HOME="$STATE_ROOT/a/data" \
  "$DAEMON_BIN" --alias "wan-a" --native-port "$A_NATIVE_PORT" --disable-localsend-v2 \
  --rendezvous "quic://${RV_ADDR}:${RV_PORT}" --api-grpc-port 53510 --api-http-port 53511 \
  >"$STATE_ROOT/a-daemon.log" 2>&1 &
echo "$!" > "$STATE_ROOT/a-daemon.pid"

ip netns exec "$NS_B" env \
  XDG_CONFIG_HOME="$STATE_ROOT/b/config" \
  XDG_DATA_HOME="$STATE_ROOT/b/data" \
  "$DAEMON_BIN" --alias "wan-b" --native-port "$B_NATIVE_PORT" --disable-localsend-v2 \
  --rendezvous "quic://${RV_ADDR}:${RV_PORT}" --api-grpc-port 53520 --api-http-port 53521 \
  --inbox "$INBOX_B" \
  >"$STATE_ROOT/b-daemon.log" 2>&1 &
echo "$!" > "$STATE_ROOT/b-daemon.pid"

sleep 3

echo "Smoke scaffold started rendezvous and two daemons."
echo "Manual trust bootstrap is still required before invoking:"
echo "  ip netns exec $NS_A $CLI_BIN send --wan --peer <peer-b-fingerprint> $PAYLOAD"
echo "Expected verification:"
echo "  sha256sum $PAYLOAD $INBOX_B/$(basename "$PAYLOAD")"
echo "Current harness exits after setup because Sprint 5 does not yet automate trust bootstrap."
