#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
OUT_DIR=${OUT_DIR:-/tmp/wfb-mac-wf-tun-$RUN_ID}

RADIO_SERVICE_BIN=${RADIO_SERVICE_BIN:-$REPO_ROOT/target/debug/wfb-radio-service}
RADIO_CONFIG=${RADIO_CONFIG:-$REPO_ROOT/configs/radio-run-robust-short-range.toml}
WFB_TX_BIN=${WFB_TX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_tx}
WFB_RX_BIN=${WFB_RX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_rx}
TUN_SCRIPT=${TUN_SCRIPT:-$REPO_ROOT/scripts/wfb-mac-wf-tun.py}
PYTHON=${PYTHON:-python3}

WFB_KEY=${WFB_KEY:-}
LINK_ID=${LINK_ID:-0x000001}
WFB_CLI_LINK_ID=${WFB_CLI_LINK_ID:-$(printf '%d' "$((LINK_ID))")}
CHANNEL=${CHANNEL:-36}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
MCS=${MCS:-1}
FEC_K=${FEC_K:-1}
FEC_N=${FEC_N:-2}

# WFB-NG stock GS tunnel direction: RX 0x20 from drone, TX 0xa0 to drone.
TUN_RX_RADIO_PORT=${TUN_RX_RADIO_PORT:-0x20}
TUN_TX_RADIO_PORT=${TUN_TX_RADIO_PORT:-0xa0}
TUN_RX_RADIO_PORT_DEC=$(printf '%d' "$((TUN_RX_RADIO_PORT))")
TUN_TX_RADIO_PORT_DEC=$(printf '%d' "$((TUN_TX_RADIO_PORT))")

RADIO_BIND_HOST=${RADIO_BIND_HOST:-127.0.0.1}
RADIO_BIND_PORT=${RADIO_BIND_PORT:-5611}
RADIO_BIND=${RADIO_BIND:-$RADIO_BIND_HOST:$RADIO_BIND_PORT}
AGG_PORT=${AGG_PORT:-5801}
TUN_TX_PORT=${TUN_TX_PORT:-56020}
TUN_RX_PORT=${TUN_RX_PORT:-56021}
LOCAL_IP=${LOCAL_IP:-10.5.0.1}
PEER_IP=${PEER_IP:-10.5.0.2}
PREFIX_LEN=${PREFIX_LEN:-24}
RADIO_READY_WAIT_SECONDS=${RADIO_READY_WAIT_SECONDS:-90}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}

mkdir -p "$OUT_DIR"

require_file() {
  local path=$1
  local label=$2
  if [[ ! -e "$path" ]]; then
    echo "Missing $label: $path" >&2
    exit 1
  fi
}

require_file "$RADIO_SERVICE_BIN" "wfb-radio-service binary"
require_file "$RADIO_CONFIG" "radio config"
require_file "$WFB_TX_BIN" "wfb_tx binary"
require_file "$WFB_RX_BIN" "wfb_rx binary"
require_file "$TUN_SCRIPT" "Mac wf_tun script"
if [[ -z "$WFB_KEY" || ! -r "$WFB_KEY" ]]; then
  echo "Set WFB_KEY to the GS-side WFB-NG keypair file, normally gs.key, readable on this Mac." >&2
  exit 1
fi

pids=()
cleanup() {
  local status=$?
  for pid in "${pids[@]:-}"; do
    kill "$pid" >/dev/null 2>&1 || true
  done
  wait >/dev/null 2>&1 || true
  echo "wf_tun recovery artifacts: $OUT_DIR" >&2
  exit "$status"
}
trap cleanup EXIT INT TERM

echo "Starting radio service..." >&2
service_cmd=(
  "$RADIO_SERVICE_BIN"
  --config "$RADIO_CONFIG"
  --bind "$RADIO_BIND"
  --channel "$CHANNEL"
  --bandwidth "$BANDWIDTH_MHZ"
  --duration-ms 0
  --max-datagrams 0
  --wfb-link-id "$LINK_ID"
  --wfb-radio-port "$TUN_RX_RADIO_PORT_DEC"
  --rx-aggregator "127.0.0.1:$AGG_PORT"
  --tx-calibration-profile "$TX_CALIBRATION_PROFILE"
  --ready-file "$OUT_DIR/radio-ready.json"
  --health-file "$OUT_DIR/radio-health.json"
  --report "$OUT_DIR/radio-service-report.json"
  --i-understand-this-transmits
  --i-understand-this-writes-registers
)
if [[ -n "${RADIO_SERVICE_EXTRA_ARGS:-}" ]]; then
  # shellcheck disable=SC2206
  extra_args=($RADIO_SERVICE_EXTRA_ARGS)
  service_cmd+=("${extra_args[@]}")
fi
"${service_cmd[@]}" >"$OUT_DIR/radio-service.log" 2>&1 &
pids+=("$!")

deadline=$((SECONDS + RADIO_READY_WAIT_SECONDS))
while [[ ! -s "$OUT_DIR/radio-ready.json" && "$SECONDS" -lt "$deadline" ]]; do
  if ! kill -0 "${pids[0]}" >/dev/null 2>&1; then
    echo "radio service exited before ready; tail follows" >&2
    tail -80 "$OUT_DIR/radio-service.log" >&2 || true
    exit 1
  fi
  sleep 1
done
if [[ ! -s "$OUT_DIR/radio-ready.json" ]]; then
  echo "radio service did not become ready within ${RADIO_READY_WAIT_SECONDS}s" >&2
  tail -80 "$OUT_DIR/radio-service.log" >&2 || true
  exit 1
fi

echo "Starting WFB-NG RX aggregator for tunnel port $TUN_RX_RADIO_PORT_DEC..." >&2
"$WFB_RX_BIN" \
  -a "$AGG_PORT" \
  -K "$WFB_KEY" \
  -i "$WFB_CLI_LINK_ID" \
  -p "$TUN_RX_RADIO_PORT_DEC" \
  -c 127.0.0.1 \
  -u "$TUN_RX_PORT" \
  >"$OUT_DIR/wfb-rx.log" 2>&1 &
pids+=("$!")

echo "Starting WFB-NG TX distributor for tunnel port $TUN_TX_RADIO_PORT_DEC..." >&2
"$WFB_TX_BIN" \
  -d \
  -K "$WFB_KEY" \
  -i "$WFB_CLI_LINK_ID" \
  -p "$TUN_TX_RADIO_PORT_DEC" \
  -B "$BANDWIDTH_MHZ" \
  -M "$MCS" \
  -k "$FEC_K" \
  -n "$FEC_N" \
  -u "$TUN_TX_PORT" \
  "$RADIO_BIND" \
  >"$OUT_DIR/wfb-tx.log" 2>&1 &
pids+=("$!")

echo "Starting macOS utun bridge. It needs sudo because macOS gates utun creation/configuration." >&2
echo "Try SSH after it starts: ssh pi@$PEER_IP" >&2
sudo -n "$PYTHON" "$TUN_SCRIPT" \
  --local-ip "$LOCAL_IP" \
  --peer-ip "$PEER_IP" \
  --prefix-len "$PREFIX_LEN" \
  --tx-peer "127.0.0.1:$TUN_TX_PORT" \
  --rx-bind "127.0.0.1:$TUN_RX_PORT" \
  2>"$OUT_DIR/wf-tun.log"
