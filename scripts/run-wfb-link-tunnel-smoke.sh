#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
OUT_DIR=${OUT_DIR:-/tmp/wfb-link-tunnel-smoke-$RUN_ID}
RADIO_CONFIG=${RADIO_CONFIG:-$REPO_ROOT/configs/radio-run-video-control-tdd.toml}
WFB_KEY=${WFB_KEY:-}
SSH_KEY=${SSH_KEY:-}
SSH_USER=${SSH_USER:-pi}
PEER_IP=${PEER_IP:-10.5.0.2}
CHANNEL=${CHANNEL:-161}
LINK_ID=${LINK_ID:-0x000000}
RADIO_BIND=${RADIO_BIND:-127.0.0.1:5611}
AGGREGATOR=${AGGREGATOR:-127.0.0.1:5801}
TUN_RX_RADIO_PORT=${TUN_RX_RADIO_PORT:-3}
TUN_TX_RADIO_PORT=${TUN_TX_RADIO_PORT:-4}
TUN_USE_SUDO=${TUN_USE_SUDO:-1}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
MCS=${MCS:-0}
FEC_K=${FEC_K:-2}
FEC_N=${FEC_N:-4}
TX_MIN_INTERVAL_US=${TX_MIN_INTERVAL_US:-700}
AIRTIME_MODE=${AIRTIME_MODE:-tdd}
AIRTIME_TDD_FIRST_WINDOW=${AIRTIME_TDD_FIRST_WINDOW:-rx}
AIRTIME_TDD_RX_WINDOW_MS=${AIRTIME_TDD_RX_WINDOW_MS:-2200}
AIRTIME_TDD_TX_WINDOW_MS=${AIRTIME_TDD_TX_WINDOW_MS:-3600}
AIRTIME_TDD_GUARD_MS=${AIRTIME_TDD_GUARD_MS:-200}
AIRTIME_TDD_START_DELAY_MS=${AIRTIME_TDD_START_DELAY_MS:-0}
WFB_TX_BIN=${WFB_TX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_tx}
WFB_RX_BIN=${WFB_RX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_rx}
TUN_BIN=${TUN_BIN:-$REPO_ROOT/target/debug/wfb-tun-macos}
WFB_LINK_READY_TIMEOUT_S=${WFB_LINK_READY_TIMEOUT_S:-90}
SSH_DD_BLOCK_SIZE=${SSH_DD_BLOCK_SIZE:-1024}
SSH_DD_COUNT=${SSH_DD_COUNT:-256}
SSH_DD_MIN_BYTES=${SSH_DD_MIN_BYTES:-$((SSH_DD_BLOCK_SIZE * SSH_DD_COUNT))}

die() {
  printf '[wfb-link-tunnel-smoke] error: %s\n' "$*" >&2
  exit 1
}

require_file() {
  local path=$1 label=$2
  [[ -e "$path" ]] || die "missing $label: $path"
}

quote() {
  printf '%q' "$1"
}

require_file "$RADIO_CONFIG" "radio config"
require_file "$WFB_TX_BIN" "wfb_tx binary"
require_file "$WFB_RX_BIN" "wfb_rx binary"
require_file "$TUN_BIN" "wfb-tun-macos binary"
[[ -n "$WFB_KEY" && -r "$WFB_KEY" ]] || die "set WFB_KEY to a readable GS-side WFB-NG key"
if [[ -n "$SSH_KEY" ]]; then
  [[ -r "$SSH_KEY" ]] || die "SSH_KEY is not readable: $SSH_KEY"
fi
if [[ "$TUN_USE_SUDO" == "1" || "$TUN_USE_SUDO" == "true" || "$TUN_USE_SUDO" == "yes" || "$TUN_USE_SUDO" == "on" ]]; then
  command -v sudo >/dev/null 2>&1 || die "sudo is required when TUN_USE_SUDO=$TUN_USE_SUDO"
  sudo -n "$TUN_BIN" --help >/dev/null 2>&1 || die "passwordless sudo is required for $TUN_BIN; set TUN_USE_SUDO=0 only on hosts allowed to open utun without sudo"
fi

mkdir -p "$OUT_DIR"
ssh_args=(
  -o BatchMode=yes
  -o ConnectTimeout=30
  -o ServerAliveInterval=5
  -o ServerAliveCountMax=2
  -o StrictHostKeyChecking=no
  -o UserKnownHostsFile="$OUT_DIR/known_hosts"
)
if [[ -n "$SSH_KEY" ]]; then
  ssh_args=(-i "$SSH_KEY" -o IdentitiesOnly=yes "${ssh_args[@]}")
fi
PROBE_SCRIPT="$OUT_DIR/ssh-download-probe.sh"
{
  printf '#!/usr/bin/env bash\n'
  printf 'set -euo pipefail\n'
  printf 'bytes=$(ssh'
  for arg in "${ssh_args[@]}"; do
    printf ' %q' "$arg"
  done
  printf ' %q %q | wc -c)\n' \
    "$SSH_USER@$PEER_IP" \
    "dd if=/dev/zero bs=$SSH_DD_BLOCK_SIZE count=$SSH_DD_COUNT 2>/dev/null"
  printf 'echo "$bytes" >&2\n'
  printf 'test "$bytes" -ge %q\n' "$SSH_DD_MIN_BYTES"
} >"$PROBE_SCRIPT"
chmod +x "$PROBE_SCRIPT"
PROBE_COMMAND=$(printf 'bash %s' "$(quote "$PROBE_SCRIPT")")

printf '[wfb-link-tunnel-smoke] artifacts: %s\n' "$OUT_DIR" >&2
WFB_KEY="$WFB_KEY" \
WFB_TX_BIN="$WFB_TX_BIN" \
WFB_RX_BIN="$WFB_RX_BIN" \
TUN_BIN="$TUN_BIN" \
OUT_DIR="$OUT_DIR" \
CHANNEL="$CHANNEL" \
LINK_ID="$LINK_ID" \
RADIO_BIND="$RADIO_BIND" \
AGGREGATOR="$AGGREGATOR" \
TUN_RX_RADIO_PORT="$TUN_RX_RADIO_PORT" \
TUN_TX_RADIO_PORT="$TUN_TX_RADIO_PORT" \
TUN_USE_SUDO="$TUN_USE_SUDO" \
BANDWIDTH_MHZ="$BANDWIDTH_MHZ" \
MCS="$MCS" \
FEC_K="$FEC_K" \
FEC_N="$FEC_N" \
TX_MIN_INTERVAL_US="$TX_MIN_INTERVAL_US" \
AIRTIME_MODE="$AIRTIME_MODE" \
AIRTIME_TDD_FIRST_WINDOW="$AIRTIME_TDD_FIRST_WINDOW" \
AIRTIME_TDD_RX_WINDOW_MS="$AIRTIME_TDD_RX_WINDOW_MS" \
AIRTIME_TDD_TX_WINDOW_MS="$AIRTIME_TDD_TX_WINDOW_MS" \
AIRTIME_TDD_GUARD_MS="$AIRTIME_TDD_GUARD_MS" \
AIRTIME_TDD_START_DELAY_MS="$AIRTIME_TDD_START_DELAY_MS" \
PEER_IP="$PEER_IP" \
WFB_LINK_READY_TIMEOUT_S="$WFB_LINK_READY_TIMEOUT_S" \
WFB_LINK_PROBE_COMMAND="$PROBE_COMMAND" \
cargo run -p wfb-link --example macos-tunnel-link -- "$RADIO_CONFIG" \
  >"$OUT_DIR/wfb-link-tunnel.stdout.log" \
  2>"$OUT_DIR/wfb-link-tunnel.stderr.log"

printf '[wfb-link-tunnel-smoke] complete: %s\n' "$OUT_DIR" >&2
