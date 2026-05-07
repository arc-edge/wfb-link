#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
OUT_DIR=${OUT_DIR:-/tmp/wfb-mac-wf-tun-$RUN_ID}
RUN_SUMMARY_FILE=${RUN_SUMMARY_FILE:-$OUT_DIR/summary.json}
TUN_SUMMARY_FILE=${TUN_SUMMARY_FILE:-$OUT_DIR/wf-tun-summary.json}
TUN_PROBE_STATUS_FILE=${TUN_PROBE_STATUS_FILE:-$OUT_DIR/tun-probe-status.json}

RADIO_SERVICE_BIN=${RADIO_SERVICE_BIN:-$REPO_ROOT/target/debug/wfb-radio-service}
RADIO_CONFIG=${RADIO_CONFIG:-$REPO_ROOT/configs/radio-run-robust-short-range.toml}
WFB_TX_BIN=${WFB_TX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_tx}
WFB_RX_BIN=${WFB_RX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_rx}
TUN_SCRIPT=${TUN_SCRIPT:-$REPO_ROOT/scripts/wfb-mac-wf-tun.py}
PYTHON=${PYTHON:-python3}

WFB_KEY=${WFB_KEY:-}
LINK_ID=${LINK_ID:-0x000000}
WFB_CLI_LINK_ID=${WFB_CLI_LINK_ID:-$(printf '%d' "$((LINK_ID))")}
CHANNEL=${CHANNEL:-161}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
MCS=${MCS:-1}
FEC_K=${FEC_K:-2}
FEC_N=${FEC_N:-4}
AIRTIME_MODE=${AIRTIME_MODE:-tdd}
AIRTIME_TDD_FIRST_WINDOW=${AIRTIME_TDD_FIRST_WINDOW:-rx}
AIRTIME_TDD_RX_WINDOW_MS=${AIRTIME_TDD_RX_WINDOW_MS:-7000}
AIRTIME_TDD_TX_WINDOW_MS=${AIRTIME_TDD_TX_WINDOW_MS:-20000}
AIRTIME_TDD_GUARD_MS=${AIRTIME_TDD_GUARD_MS:-500}
AIRTIME_TDD_START_DELAY_MS=${AIRTIME_TDD_START_DELAY_MS:-0}

# Arc tunnel direction: RX stream 3 from drone, TX stream 4 to drone.
TUN_RX_RADIO_PORT=${TUN_RX_RADIO_PORT:-3}
TUN_TX_RADIO_PORT=${TUN_TX_RADIO_PORT:-4}
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
TUN_MTU=${TUN_MTU:-1400}
RADIO_MTU=${RADIO_MTU:-1445}
TUN_AGG_TIMEOUT_MS=${TUN_AGG_TIMEOUT_MS:-5}
RADIO_READY_WAIT_SECONDS=${RADIO_READY_WAIT_SECONDS:-90}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}
TUN_SETTLE_SECONDS=${TUN_SETTLE_SECONDS:-3}
TUN_PROBE_COMMAND=${TUN_PROBE_COMMAND:-}

mkdir -p "$OUT_DIR"

write_recovery_summary() {
  local status=$1
  STATUS="$status" \
  RUN_ID="$RUN_ID" \
  OUT_DIR="$OUT_DIR" \
  SUMMARY_PATH="$RUN_SUMMARY_FILE" \
  TUN_SUMMARY_FILE="$TUN_SUMMARY_FILE" \
  RADIO_SERVICE_REPORT="$OUT_DIR/radio-service-report.json" \
  RADIO_HEALTH_FILE="$OUT_DIR/radio-health.json" \
  TUN_PROBE_LOG="$OUT_DIR/tun-probe.log" \
  TUN_PROBE_STATUS_FILE="$TUN_PROBE_STATUS_FILE" \
  TUN_PROBE_COMMAND="$TUN_PROBE_COMMAND" \
  CHANNEL="$CHANNEL" \
  BANDWIDTH_MHZ="$BANDWIDTH_MHZ" \
  MCS="$MCS" \
  FEC_K="$FEC_K" \
  FEC_N="$FEC_N" \
  AIRTIME_MODE="$AIRTIME_MODE" \
  AIRTIME_TDD_FIRST_WINDOW="$AIRTIME_TDD_FIRST_WINDOW" \
  AIRTIME_TDD_RX_WINDOW_MS="$AIRTIME_TDD_RX_WINDOW_MS" \
  AIRTIME_TDD_TX_WINDOW_MS="$AIRTIME_TDD_TX_WINDOW_MS" \
  AIRTIME_TDD_GUARD_MS="$AIRTIME_TDD_GUARD_MS" \
  AIRTIME_TDD_START_DELAY_MS="$AIRTIME_TDD_START_DELAY_MS" \
  LOCAL_IP="$LOCAL_IP" \
  PEER_IP="$PEER_IP" \
  TUN_MTU="$TUN_MTU" \
  RADIO_MTU="$RADIO_MTU" \
  TX_CALIBRATION_PROFILE="$TX_CALIBRATION_PROFILE" \
  WFB_KEY_BASENAME="$(basename "$WFB_KEY")" \
  "$PYTHON" - <<'PY' || true
import json
import os
import time
from pathlib import Path


def read_json(path):
    try:
        with open(path, "r", encoding="utf-8") as fh:
            return json.load(fh)
    except FileNotFoundError:
        return None
    except json.JSONDecodeError as exc:
        return {"parse_error": str(exc), "path": path}


def read_tail(path, limit=8000):
    try:
        data = Path(path).read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return None
    return data[-limit:]


def getenv_int(name):
    return int(os.environ[name], 0)


status = int(os.environ["STATUS"])
probe_command = os.environ.get("TUN_PROBE_COMMAND") or None
summary = {
    "schema": "wfb_mac_wf_tun_recovery_summary/v1",
    "result": "pass" if status == 0 else "fail",
    "exit_status": status,
    "generated_at_unix": time.time(),
    "run_id": os.environ["RUN_ID"],
    "out_dir": os.environ["OUT_DIR"],
    "settings": {
        "channel": getenv_int("CHANNEL"),
        "bandwidth_mhz": getenv_int("BANDWIDTH_MHZ"),
        "mcs": getenv_int("MCS"),
        "fec_k": getenv_int("FEC_K"),
        "fec_n": getenv_int("FEC_N"),
        "airtime_mode": os.environ["AIRTIME_MODE"],
        "airtime_tdd_first_window": os.environ["AIRTIME_TDD_FIRST_WINDOW"],
        "airtime_tdd_rx_window_ms": getenv_int("AIRTIME_TDD_RX_WINDOW_MS"),
        "airtime_tdd_tx_window_ms": getenv_int("AIRTIME_TDD_TX_WINDOW_MS"),
        "airtime_tdd_guard_ms": getenv_int("AIRTIME_TDD_GUARD_MS"),
        "airtime_tdd_start_delay_ms": getenv_int("AIRTIME_TDD_START_DELAY_MS"),
        "local_ip": os.environ["LOCAL_IP"],
        "peer_ip": os.environ["PEER_IP"],
        "tun_mtu": getenv_int("TUN_MTU"),
        "radio_mtu": getenv_int("RADIO_MTU"),
        "tx_calibration_profile": os.environ["TX_CALIBRATION_PROFILE"],
        "wfb_key_basename": os.environ.get("WFB_KEY_BASENAME"),
    },
    "probe": {
        "command": probe_command,
        "passed": status == 0 and probe_command is not None,
        "status": read_json(os.environ["TUN_PROBE_STATUS_FILE"]),
        "log_tail": read_tail(os.environ["TUN_PROBE_LOG"]),
    },
    "tunnel": read_json(os.environ["TUN_SUMMARY_FILE"]),
    "radio": read_json(os.environ["RADIO_SERVICE_REPORT"]),
    "radio_health": read_json(os.environ["RADIO_HEALTH_FILE"]),
    "paths": {
        "summary": os.environ["SUMMARY_PATH"],
        "tunnel_summary": os.environ["TUN_SUMMARY_FILE"],
        "radio_service_report": os.environ["RADIO_SERVICE_REPORT"],
        "radio_health": os.environ["RADIO_HEALTH_FILE"],
        "probe_log": os.environ["TUN_PROBE_LOG"],
        "probe_status": os.environ["TUN_PROBE_STATUS_FILE"],
    },
}
path = os.environ["SUMMARY_PATH"]
tmp = f"{path}.tmp"
with open(tmp, "w", encoding="utf-8") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)
    fh.write("\n")
os.replace(tmp, path)
PY
}

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
  write_recovery_summary "$status"
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
  --airtime-mode "$AIRTIME_MODE"
  --airtime-tdd-first-window "$AIRTIME_TDD_FIRST_WINDOW"
  --airtime-tdd-rx-window-ms "$AIRTIME_TDD_RX_WINDOW_MS"
  --airtime-tdd-tx-window-ms "$AIRTIME_TDD_TX_WINDOW_MS"
  --airtime-tdd-guard-ms "$AIRTIME_TDD_GUARD_MS"
  --airtime-tdd-start-delay-ms "$AIRTIME_TDD_START_DELAY_MS"
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
tun_cmd=(
  sudo -n "$PYTHON" "$TUN_SCRIPT"
  --local-ip "$LOCAL_IP" \
  --peer-ip "$PEER_IP" \
  --prefix-len "$PREFIX_LEN" \
  --tun-mtu "$TUN_MTU" \
  --radio-mtu "$RADIO_MTU" \
  --agg-timeout-ms "$TUN_AGG_TIMEOUT_MS" \
  --tx-peer "127.0.0.1:$TUN_TX_PORT" \
  --rx-bind "127.0.0.1:$TUN_RX_PORT" \
  --summary-file "$TUN_SUMMARY_FILE"
)
if [[ -n "$TUN_PROBE_COMMAND" ]]; then
  "${tun_cmd[@]}" 2>"$OUT_DIR/wf-tun.log" &
  pids+=("$!")
  sleep "$TUN_SETTLE_SECONDS"
  echo "Running tunnel probe: $TUN_PROBE_COMMAND" >&2
  probe_started_at=$("$PYTHON" - <<'PY'
import time
print(time.time())
PY
)
  set +e
  bash -lc "$TUN_PROBE_COMMAND" >"$OUT_DIR/tun-probe.log" 2>&1 < /dev/null
  probe_status=$?
  set -e
  probe_ended_at=$("$PYTHON" - <<'PY'
import time
print(time.time())
PY
)
  PROBE_STATUS="$probe_status" \
  PROBE_STARTED_AT="$probe_started_at" \
  PROBE_ENDED_AT="$probe_ended_at" \
  PROBE_STATUS_FILE="$TUN_PROBE_STATUS_FILE" \
  "$PYTHON" - <<'PY'
import json
import os

started_at = float(os.environ["PROBE_STARTED_AT"])
ended_at = float(os.environ["PROBE_ENDED_AT"])
status = int(os.environ["PROBE_STATUS"])
payload = {
    "exit_status": status,
    "result": "pass" if status == 0 else "fail",
    "started_at_unix": started_at,
    "ended_at_unix": ended_at,
    "duration_s": max(0.0, ended_at - started_at),
}
path = os.environ["PROBE_STATUS_FILE"]
tmp = f"{path}.tmp"
with open(tmp, "w", encoding="utf-8") as fh:
    json.dump(payload, fh, indent=2, sort_keys=True)
    fh.write("\n")
os.replace(tmp, path)
PY
  if (( probe_status != 0 )); then
    echo "Tunnel probe failed with status $probe_status; artifacts: $OUT_DIR" >&2
    exit "$probe_status"
  fi
  echo "Tunnel probe passed; artifacts: $OUT_DIR" >&2
else
  "${tun_cmd[@]}" 2>"$OUT_DIR/wf-tun.log"
fi
