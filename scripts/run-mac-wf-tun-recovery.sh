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
TUN_BIN=${TUN_BIN:-$REPO_ROOT/target/debug/wfb-tun-macos}
TUN_IMPL=${TUN_IMPL:-rust}
TUN_SCRIPT=${TUN_SCRIPT:-$REPO_ROOT/scripts/development/wfb-mac-wf-tun.py}
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
TX_MIN_INTERVAL_US=${TX_MIN_INTERVAL_US:-0}

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

DATA_LOAD_MODE=${DATA_LOAD_MODE:-none} # none, m2l, l2m, or duplex
DATA_LOAD_EXPECTED_PAYLOADS=${DATA_LOAD_EXPECTED_PAYLOADS:-100}
DATA_LOAD_MIN_M2L_UNIQUE=${DATA_LOAD_MIN_M2L_UNIQUE:-$DATA_LOAD_EXPECTED_PAYLOADS}
DATA_LOAD_MIN_L2M_UNIQUE=${DATA_LOAD_MIN_L2M_UNIQUE:-$DATA_LOAD_EXPECTED_PAYLOADS}
DATA_LOAD_PAYLOAD_LEN=${DATA_LOAD_PAYLOAD_LEN:-512}
DATA_LOAD_INTERVAL_SEC=${DATA_LOAD_INTERVAL_SEC:-0.020}
DATA_LOAD_WARMUP_PAYLOADS=${DATA_LOAD_WARMUP_PAYLOADS:-20}
DATA_LOAD_TAIL_PAYLOADS=${DATA_LOAD_TAIL_PAYLOADS:-8}
DATA_LOAD_PRE_PROBE_SECONDS=${DATA_LOAD_PRE_PROBE_SECONDS:-0}
DATA_LOAD_COUNTER_SECONDS=${DATA_LOAD_COUNTER_SECONDS:-20}
DATA_LOAD_MCS=${DATA_LOAD_MCS:-1}
DATA_LOAD_FEC_K=${DATA_LOAD_FEC_K:-2}
DATA_LOAD_FEC_N=${DATA_LOAD_FEC_N:-4}
DATA_M2L_RADIO_PORT=${DATA_M2L_RADIO_PORT:-6}
DATA_L2M_RADIO_PORT=${DATA_L2M_RADIO_PORT:-7}
DATA_M2L_SOURCE_PORT=${DATA_M2L_SOURCE_PORT:-56120}
DATA_M2L_COUNTER_PORT=${DATA_M2L_COUNTER_PORT:-5920}
DATA_L2M_SOURCE_PORT=${DATA_L2M_SOURCE_PORT:-56121}
DATA_L2M_AGG_PORT=${DATA_L2M_AGG_PORT:-5821}
DATA_L2M_COUNTER_PORT=${DATA_L2M_COUNTER_PORT:-5921}
DATA_M2L_MARKER=${DATA_M2L_MARKER:-TUNM2LD1}
DATA_L2M_MARKER=${DATA_L2M_MARKER:-TUNL2MD1}
DATA_M2L_WARMUP_MARKER=${DATA_M2L_WARMUP_MARKER:-TUNM2LWP}
DATA_L2M_WARMUP_MARKER=${DATA_L2M_WARMUP_MARKER:-TUNL2LWP}
DATA_LOAD_LINUX_HOST=${DATA_LOAD_LINUX_HOST:-pi@drone-2f389.local}
DATA_LOAD_LINUX_WFB_KEY=${DATA_LOAD_LINUX_WFB_KEY:-/var/lib/arc/wfb/drone.key}
DATA_LOAD_IFACE=${DATA_LOAD_IFACE:-wfb0}
DATA_LOAD_REMOTE_PREFIX=${DATA_LOAD_REMOTE_PREFIX:-/tmp/wfb-mac-wf-tun-load-$RUN_ID}
DATA_LOAD_REQUIRE_PASS=${DATA_LOAD_REQUIRE_PASS:-1}

mkdir -p "$OUT_DIR"

data_load_m2l_enabled() {
  [[ "$DATA_LOAD_MODE" == "m2l" || "$DATA_LOAD_MODE" == "duplex" ]]
}

data_load_l2m_enabled() {
  [[ "$DATA_LOAD_MODE" == "l2m" || "$DATA_LOAD_MODE" == "duplex" ]]
}

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
  TX_MIN_INTERVAL_US="$TX_MIN_INTERVAL_US" \
  LOCAL_IP="$LOCAL_IP" \
  PEER_IP="$PEER_IP" \
  TUN_MTU="$TUN_MTU" \
  RADIO_MTU="$RADIO_MTU" \
  TX_CALIBRATION_PROFILE="$TX_CALIBRATION_PROFILE" \
  DATA_LOAD_MODE="$DATA_LOAD_MODE" \
  DATA_LOAD_PRE_PROBE_SECONDS="$DATA_LOAD_PRE_PROBE_SECONDS" \
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
probe_status_payload = read_json(os.environ["TUN_PROBE_STATUS_FILE"])
probe_passed = (
    probe_command is not None
    and isinstance(probe_status_payload, dict)
    and probe_status_payload.get("result") == "pass"
)
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
        "tx_min_interval_us": getenv_int("TX_MIN_INTERVAL_US"),
        "local_ip": os.environ["LOCAL_IP"],
        "peer_ip": os.environ["PEER_IP"],
        "tun_mtu": getenv_int("TUN_MTU"),
        "radio_mtu": getenv_int("RADIO_MTU"),
        "tx_calibration_profile": os.environ["TX_CALIBRATION_PROFILE"],
        "data_load_mode": os.environ["DATA_LOAD_MODE"],
        "data_load_pre_probe_seconds": float(os.environ["DATA_LOAD_PRE_PROBE_SECONDS"]),
        "wfb_key_basename": os.environ.get("WFB_KEY_BASENAME"),
    },
    "probe": {
        "command": probe_command,
        "passed": probe_passed,
        "status": probe_status_payload,
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
data_load_path = os.path.join(os.environ["OUT_DIR"], "data-load-summary.json")
summary["data_load"] = read_json(data_load_path)
summary["paths"]["data_load_summary"] = data_load_path
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
case "$TUN_IMPL" in
  rust) require_file "$TUN_BIN" "wfb-tun-macos binary" ;;
  python) require_file "$TUN_SCRIPT" "development Mac wf_tun script" ;;
  *)
    echo "Invalid TUN_IMPL=$TUN_IMPL (expected rust or python)." >&2
    exit 1
    ;;
esac
if [[ -z "$WFB_KEY" || ! -r "$WFB_KEY" ]]; then
  echo "Set WFB_KEY to the GS-side WFB-NG keypair file, normally gs.key, readable on this Mac." >&2
  exit 1
fi
case "$DATA_LOAD_MODE" in
  none|m2l|l2m|duplex) ;;
  *)
    echo "Invalid DATA_LOAD_MODE=$DATA_LOAD_MODE (expected none, m2l, l2m, or duplex)." >&2
    exit 1
    ;;
esac

pids=()
data_load_pids=()
cleanup() {
  local status=$?
  for pid in "${pids[@]:-}"; do
    kill "$pid" >/dev/null 2>&1 || true
  done
  for pid in "${data_load_pids[@]:-}"; do
    kill "$pid" >/dev/null 2>&1 || true
  done
  wait >/dev/null 2>&1 || true
  if [[ "$DATA_LOAD_MODE" != "none" ]]; then
    ssh -o BatchMode=yes -o ConnectTimeout=5 "$DATA_LOAD_LINUX_HOST" \
      "DATA_LOAD_REMOTE_PREFIX=$(printf '%q' "$DATA_LOAD_REMOTE_PREFIX") bash -s" <<'REMOTE_CLEANUP' >/dev/null 2>&1 || true
prefix=$DATA_LOAD_REMOTE_PREFIX
if [[ -n "$prefix" && -d "$prefix" ]]; then
  for pidfile in "$prefix"/*.pid; do
    [[ -e "$pidfile" ]] || continue
    kill "$(cat "$pidfile")" >/dev/null 2>&1 || true
  done
fi
REMOTE_CLEANUP
  fi
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
  --tx-min-interval-us "$TX_MIN_INTERVAL_US"
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
if data_load_l2m_enabled; then
  service_cmd+=(
    --rx-forward "${DATA_L2M_RADIO_PORT}=127.0.0.1:${DATA_L2M_AGG_PORT}"
  )
fi
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

write_data_load_helpers() {
  cat > "$OUT_DIR/data-counter.py" <<'PY'
import json
import socket
import sys
import time
from pathlib import Path

host = sys.argv[1]
port = int(sys.argv[2])
marker = sys.argv[3].encode("ascii")
expected = int(sys.argv[4])
duration = float(sys.argv[5])
out = Path(sys.argv[6])

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind((host, port))
sock.settimeout(0.5)
started = time.time()
deadline = started + duration
packets = 0
bytes_total = 0
matched = 0
seqs = set()
last_peer = None

while time.time() < deadline and len(seqs) < expected:
    try:
        data, peer = sock.recvfrom(65535)
    except socket.timeout:
        continue
    packets += 1
    bytes_total += len(data)
    last_peer = f"{peer[0]}:{peer[1]}"
    idx = data.find(marker)
    if idx >= 0 and idx + len(marker) + 4 <= len(data):
        matched += 1
        seqs.add(int.from_bytes(data[idx + len(marker):idx + len(marker) + 4], "big"))

report = {
    "bind": f"{host}:{port}",
    "bytes": bytes_total,
    "duration_s": time.time() - started,
    "expected": expected,
    "last_peer": last_peer,
    "marker": marker.decode("ascii"),
    "matched_datagrams": matched,
    "missing_sequences": [seq for seq in range(expected) if seq not in seqs],
    "packets": packets,
    "unique_sequences": len(seqs),
}
out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY

  cat > "$OUT_DIR/data-source.py" <<'PY'
import json
import socket
import sys
import time
from pathlib import Path

host = sys.argv[1]
port = int(sys.argv[2])
marker = sys.argv[3].encode("ascii")
warmup_marker = sys.argv[4].encode("ascii")
expected = int(sys.argv[5])
warmup = int(sys.argv[6])
tail = int(sys.argv[7])
payload_len = int(sys.argv[8])
interval = float(sys.argv[9])
out = Path(sys.argv[10])

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
target = (host, port)
started = time.time()

def payload(prefix, seq, fill):
    body_len = max(0, payload_len - len(prefix) - 4)
    return prefix + seq.to_bytes(4, "big") + (fill * body_len)

sent = {"warmup": 0, "measured": 0, "tail": 0}
for seq in range(warmup):
    sock.sendto(payload(warmup_marker, seq, b"w"), target)
    sent["warmup"] += 1
    time.sleep(interval)
for seq in range(expected):
    sock.sendto(payload(marker, seq, b"d"), target)
    sent["measured"] += 1
    time.sleep(interval)
for seq in range(tail):
    sock.sendto(payload(warmup_marker, warmup + seq, b"t"), target)
    sent["tail"] += 1
    time.sleep(interval)

out.write_text(json.dumps({
    "duration_s": time.time() - started,
    "interval_s": interval,
    "payload_len": payload_len,
    "sent": sent,
    "target": f"{host}:{port}",
}, indent=2, sort_keys=True) + "\n")
PY
}

start_data_load_receivers() {
  if [[ "$DATA_LOAD_MODE" == "none" ]]; then
    return 0
  fi
  echo "Starting concurrent WFB data load receivers ($DATA_LOAD_MODE)..." >&2
  write_data_load_helpers

  if data_load_l2m_enabled; then
    "$PYTHON" "$OUT_DIR/data-counter.py" \
      127.0.0.1 "$DATA_L2M_COUNTER_PORT" "$DATA_L2M_MARKER" \
      "$DATA_LOAD_EXPECTED_PAYLOADS" "$DATA_LOAD_COUNTER_SECONDS" \
      "$OUT_DIR/data-l2m-counter.json" \
      >"$OUT_DIR/data-l2m-counter.log" 2>&1 &
    data_load_pids+=("$!")

    "$WFB_RX_BIN" \
      -a "$DATA_L2M_AGG_PORT" \
      -K "$WFB_KEY" \
      -i "$WFB_CLI_LINK_ID" \
      -p "$DATA_L2M_RADIO_PORT" \
      -c 127.0.0.1 \
      -u "$DATA_L2M_COUNTER_PORT" \
      >"$OUT_DIR/data-l2m-wfb-rx.log" 2>&1 &
    data_load_pids+=("$!")
  fi

  if data_load_m2l_enabled; then
    "$WFB_TX_BIN" \
      -d \
      -K "$WFB_KEY" \
      -i "$WFB_CLI_LINK_ID" \
      -p "$DATA_M2L_RADIO_PORT" \
      -B "$BANDWIDTH_MHZ" \
      -M "$DATA_LOAD_MCS" \
      -k "$DATA_LOAD_FEC_K" \
      -n "$DATA_LOAD_FEC_N" \
      -u "$DATA_M2L_SOURCE_PORT" \
      "$RADIO_BIND" \
      >"$OUT_DIR/data-m2l-wfb-tx.log" 2>&1 &
    data_load_pids+=("$!")
  fi

  ssh -o BatchMode=yes -o ConnectTimeout=10 "$DATA_LOAD_LINUX_HOST" \
    "DATA_LOAD_REMOTE_PREFIX=$(printf '%q' "$DATA_LOAD_REMOTE_PREFIX") DATA_LOAD_IFACE=$(printf '%q' "$DATA_LOAD_IFACE") DATA_LOAD_LINUX_WFB_KEY=$(printf '%q' "$DATA_LOAD_LINUX_WFB_KEY") WFB_CLI_LINK_ID=$(printf '%q' "$WFB_CLI_LINK_ID") BANDWIDTH_MHZ=$(printf '%q' "$BANDWIDTH_MHZ") DATA_LOAD_MCS=$(printf '%q' "$DATA_LOAD_MCS") DATA_LOAD_FEC_K=$(printf '%q' "$DATA_LOAD_FEC_K") DATA_LOAD_FEC_N=$(printf '%q' "$DATA_LOAD_FEC_N") DATA_M2L_RADIO_PORT=$(printf '%q' "$DATA_M2L_RADIO_PORT") DATA_L2M_RADIO_PORT=$(printf '%q' "$DATA_L2M_RADIO_PORT") DATA_M2L_COUNTER_PORT=$(printf '%q' "$DATA_M2L_COUNTER_PORT") DATA_L2M_SOURCE_PORT=$(printf '%q' "$DATA_L2M_SOURCE_PORT") DATA_M2L_MARKER=$(printf '%q' "$DATA_M2L_MARKER") DATA_LOAD_EXPECTED_PAYLOADS=$(printf '%q' "$DATA_LOAD_EXPECTED_PAYLOADS") DATA_LOAD_COUNTER_SECONDS=$(printf '%q' "$DATA_LOAD_COUNTER_SECONDS") ENABLE_M2L=$(data_load_m2l_enabled && printf 1 || printf 0) ENABLE_L2M=$(data_load_l2m_enabled && printf 1 || printf 0) bash -s" <<'REMOTE_DATA_SETUP'
set -euo pipefail
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH
prefix=$DATA_LOAD_REMOTE_PREFIX
if [[ -d "$prefix" ]]; then
  for pidfile in "$prefix"/*.pid; do
    [[ -e "$pidfile" ]] || continue
    kill "$(cat "$pidfile")" >/dev/null 2>&1 || true
  done
fi
rm -rf "$prefix"
mkdir -p "$prefix"
cat > "$prefix/data-counter.py" <<'PY'
import json
import socket
import sys
import time
from pathlib import Path

host = sys.argv[1]
port = int(sys.argv[2])
marker = sys.argv[3].encode("ascii")
expected = int(sys.argv[4])
duration = float(sys.argv[5])
out = Path(sys.argv[6])
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind((host, port))
sock.settimeout(0.5)
started = time.time()
deadline = started + duration
packets = bytes_total = matched = 0
seqs = set()
last_peer = None
while time.time() < deadline and len(seqs) < expected:
    try:
        data, peer = sock.recvfrom(65535)
    except socket.timeout:
        continue
    packets += 1
    bytes_total += len(data)
    last_peer = f"{peer[0]}:{peer[1]}"
    idx = data.find(marker)
    if idx >= 0 and idx + len(marker) + 4 <= len(data):
        matched += 1
        seqs.add(int.from_bytes(data[idx + len(marker):idx + len(marker) + 4], "big"))
out.write_text(json.dumps({
    "bind": f"{host}:{port}",
    "bytes": bytes_total,
    "duration_s": time.time() - started,
    "expected": expected,
    "last_peer": last_peer,
    "marker": marker.decode("ascii"),
    "matched_datagrams": matched,
    "missing_sequences": [seq for seq in range(expected) if seq not in seqs],
    "packets": packets,
    "unique_sequences": len(seqs),
}, indent=2, sort_keys=True) + "\n")
PY
if [[ "$ENABLE_M2L" == "1" ]]; then
  nohup python3 "$prefix/data-counter.py" 127.0.0.1 "$DATA_M2L_COUNTER_PORT" "$DATA_M2L_MARKER" "$DATA_LOAD_EXPECTED_PAYLOADS" "$DATA_LOAD_COUNTER_SECONDS" "$prefix/data-m2l-counter.json" > "$prefix/data-m2l-counter.log" 2>&1 &
  echo $! > "$prefix/data-m2l-counter.pid"
  nohup sudo -n timeout "$DATA_LOAD_COUNTER_SECONDS" wfb_rx -K "$DATA_LOAD_LINUX_WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$DATA_M2L_RADIO_PORT" -c 127.0.0.1 -u "$DATA_M2L_COUNTER_PORT" "$DATA_LOAD_IFACE" > "$prefix/data-m2l-wfb-rx.log" 2>&1 &
  echo $! > "$prefix/data-m2l-wfb-rx.pid"
fi
if [[ "$ENABLE_L2M" == "1" ]]; then
  nohup sudo -n timeout "$DATA_LOAD_COUNTER_SECONDS" wfb_tx -K "$DATA_LOAD_LINUX_WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$DATA_L2M_RADIO_PORT" -B "$BANDWIDTH_MHZ" -M "$DATA_LOAD_MCS" -k "$DATA_LOAD_FEC_K" -n "$DATA_LOAD_FEC_N" -u "$DATA_L2M_SOURCE_PORT" "$DATA_LOAD_IFACE" > "$prefix/data-l2m-wfb-tx.log" 2>&1 &
  echo $! > "$prefix/data-l2m-wfb-tx.pid"
fi
REMOTE_DATA_SETUP
  sleep 2
}

start_data_load_sources() {
  if [[ "$DATA_LOAD_MODE" == "none" ]]; then
    return 0
  fi
  echo "Starting concurrent WFB data load sources ($DATA_LOAD_MODE)..." >&2
  if data_load_m2l_enabled; then
    "$PYTHON" "$OUT_DIR/data-source.py" \
      127.0.0.1 "$DATA_M2L_SOURCE_PORT" "$DATA_M2L_MARKER" "$DATA_M2L_WARMUP_MARKER" \
      "$DATA_LOAD_EXPECTED_PAYLOADS" "$DATA_LOAD_WARMUP_PAYLOADS" "$DATA_LOAD_TAIL_PAYLOADS" \
      "$DATA_LOAD_PAYLOAD_LEN" "$DATA_LOAD_INTERVAL_SEC" "$OUT_DIR/data-m2l-source.json" \
      >"$OUT_DIR/data-m2l-source.log" 2>&1 &
    data_load_pids+=("$!")
  fi
  if data_load_l2m_enabled; then
    ssh -o BatchMode=yes -o ConnectTimeout=10 "$DATA_LOAD_LINUX_HOST" \
      "DATA_LOAD_REMOTE_PREFIX=$(printf '%q' "$DATA_LOAD_REMOTE_PREFIX") DATA_L2M_SOURCE_PORT=$(printf '%q' "$DATA_L2M_SOURCE_PORT") DATA_L2M_MARKER=$(printf '%q' "$DATA_L2M_MARKER") DATA_L2M_WARMUP_MARKER=$(printf '%q' "$DATA_L2M_WARMUP_MARKER") DATA_LOAD_EXPECTED_PAYLOADS=$(printf '%q' "$DATA_LOAD_EXPECTED_PAYLOADS") DATA_LOAD_WARMUP_PAYLOADS=$(printf '%q' "$DATA_LOAD_WARMUP_PAYLOADS") DATA_LOAD_TAIL_PAYLOADS=$(printf '%q' "$DATA_LOAD_TAIL_PAYLOADS") DATA_LOAD_PAYLOAD_LEN=$(printf '%q' "$DATA_LOAD_PAYLOAD_LEN") DATA_LOAD_INTERVAL_SEC=$(printf '%q' "$DATA_LOAD_INTERVAL_SEC") bash -s" <<'REMOTE_DATA_SOURCE'
set -euo pipefail
prefix=$DATA_LOAD_REMOTE_PREFIX
cat > "$prefix/data-source.py" <<'PY'
import json
import socket
import sys
import time
from pathlib import Path

host = sys.argv[1]
port = int(sys.argv[2])
marker = sys.argv[3].encode("ascii")
warmup_marker = sys.argv[4].encode("ascii")
expected = int(sys.argv[5])
warmup = int(sys.argv[6])
tail = int(sys.argv[7])
payload_len = int(sys.argv[8])
interval = float(sys.argv[9])
out = Path(sys.argv[10])
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
target = (host, port)
started = time.time()
def payload(prefix, seq, fill):
    return prefix + seq.to_bytes(4, "big") + fill * max(0, payload_len - len(prefix) - 4)
sent = {"warmup": 0, "measured": 0, "tail": 0}
for seq in range(warmup):
    sock.sendto(payload(warmup_marker, seq, b"w"), target)
    sent["warmup"] += 1
    time.sleep(interval)
for seq in range(expected):
    sock.sendto(payload(marker, seq, b"d"), target)
    sent["measured"] += 1
    time.sleep(interval)
for seq in range(tail):
    sock.sendto(payload(warmup_marker, warmup + seq, b"t"), target)
    sent["tail"] += 1
    time.sleep(interval)
out.write_text(json.dumps({
    "duration_s": time.time() - started,
    "interval_s": interval,
    "payload_len": payload_len,
    "sent": sent,
    "target": f"{host}:{port}",
}, indent=2, sort_keys=True) + "\n")
PY
nohup python3 "$prefix/data-source.py" 127.0.0.1 "$DATA_L2M_SOURCE_PORT" "$DATA_L2M_MARKER" "$DATA_L2M_WARMUP_MARKER" "$DATA_LOAD_EXPECTED_PAYLOADS" "$DATA_LOAD_WARMUP_PAYLOADS" "$DATA_LOAD_TAIL_PAYLOADS" "$DATA_LOAD_PAYLOAD_LEN" "$DATA_LOAD_INTERVAL_SEC" "$prefix/data-l2m-source.json" > "$prefix/data-l2m-source.log" 2>&1 &
echo $! > "$prefix/data-l2m-source.pid"
REMOTE_DATA_SOURCE
  fi
}

write_data_load_summary() {
  if [[ "$DATA_LOAD_MODE" == "none" ]]; then
    return 0
  fi
  local deadline=$((SECONDS + DATA_LOAD_COUNTER_SECONDS + 5))
  while [[ "$SECONDS" -lt "$deadline" ]]; do
    local done=1
    if data_load_l2m_enabled && [[ ! -s "$OUT_DIR/data-l2m-counter.json" ]]; then
      done=0
    fi
    if data_load_m2l_enabled; then
      ssh -o BatchMode=yes -o ConnectTimeout=5 "$DATA_LOAD_LINUX_HOST" \
        "test -s $(printf '%q' "$DATA_LOAD_REMOTE_PREFIX/data-m2l-counter.json")" >/dev/null 2>&1 || done=0
    fi
    [[ "$done" == "1" ]] && break
    sleep 1
  done

  rm -rf "$OUT_DIR/data-load-peer"
  if [[ "$DATA_LOAD_MODE" != "none" ]]; then
    scp -q -r "$DATA_LOAD_LINUX_HOST:$DATA_LOAD_REMOTE_PREFIX" "$OUT_DIR/data-load-peer" >/dev/null 2>&1 || true
  fi

  OUT_DIR="$OUT_DIR" \
  DATA_LOAD_MODE="$DATA_LOAD_MODE" \
  DATA_LOAD_EXPECTED_PAYLOADS="$DATA_LOAD_EXPECTED_PAYLOADS" \
  DATA_LOAD_MIN_M2L_UNIQUE="$DATA_LOAD_MIN_M2L_UNIQUE" \
  DATA_LOAD_MIN_L2M_UNIQUE="$DATA_LOAD_MIN_L2M_UNIQUE" \
  DATA_LOAD_REQUIRE_PASS="$DATA_LOAD_REQUIRE_PASS" \
  "$PYTHON" - <<'PY'
import json
import os
from pathlib import Path

out = Path(os.environ["OUT_DIR"])
mode = os.environ["DATA_LOAD_MODE"]
expected = int(os.environ["DATA_LOAD_EXPECTED_PAYLOADS"])
min_m2l = int(os.environ["DATA_LOAD_MIN_M2L_UNIQUE"])
min_l2m = int(os.environ["DATA_LOAD_MIN_L2M_UNIQUE"])

def load(path):
    try:
        return json.loads(path.read_text())
    except Exception as exc:
        return {"error": str(exc), "path": str(path)}

def decrypt_failures(path):
    try:
        return sum(1 for line in path.read_text(errors="replace").splitlines() if "Unable to decrypt" in line)
    except Exception:
        return 0

m2l_enabled = mode in {"m2l", "duplex"}
l2m_enabled = mode in {"l2m", "duplex"}
m2l = load(out / "data-load-peer" / "data-m2l-counter.json") if m2l_enabled else {"disabled": True}
l2m = load(out / "data-l2m-counter.json") if l2m_enabled else {"disabled": True}
failures = []
if m2l_enabled and int(m2l.get("unique_sequences") or 0) < min_m2l:
    failures.append(f"m2l_unique_sequences={m2l.get('unique_sequences')}<{min_m2l}")
if l2m_enabled and int(l2m.get("unique_sequences") or 0) < min_l2m:
    failures.append(f"l2m_unique_sequences={l2m.get('unique_sequences')}<{min_l2m}")
m2l_decrypt = decrypt_failures(out / "data-load-peer" / "data-m2l-wfb-rx.log")
l2m_decrypt = decrypt_failures(out / "data-l2m-wfb-rx.log")
if m2l_decrypt:
    failures.append(f"m2l_decrypt_failures={m2l_decrypt}")
if l2m_decrypt:
    failures.append(f"l2m_decrypt_failures={l2m_decrypt}")
summary = {
    "schema": "wfb_mac_wf_tun_data_load/v1",
    "mode": mode,
    "expected_payloads": expected,
    "result": "fail" if failures else "pass",
    "failures": failures,
    "m2l": {
        "enabled": m2l_enabled,
        "counter": m2l,
        "source": load(out / "data-m2l-source.json") if m2l_enabled else {"disabled": True},
        "decrypt_failures": m2l_decrypt,
    },
    "l2m": {
        "enabled": l2m_enabled,
        "counter": l2m,
        "source": load(out / "data-load-peer" / "data-l2m-source.json") if l2m_enabled else {"disabled": True},
        "decrypt_failures": l2m_decrypt,
    },
}
(out / "data-load-summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
if os.environ["DATA_LOAD_REQUIRE_PASS"] == "1" and failures:
    raise SystemExit(1)
PY
}

start_data_load_receivers

echo "Starting macOS utun bridge. It needs sudo because macOS gates utun creation/configuration." >&2
echo "Try SSH after it starts: ssh pi@$PEER_IP" >&2
if [[ "$TUN_IMPL" == "rust" ]]; then
  tun_cmd=(sudo -n "$TUN_BIN")
else
  tun_cmd=(sudo -n "$PYTHON" "$TUN_SCRIPT")
fi
tun_cmd+=(
  --local-ip "$LOCAL_IP"
  --peer-ip "$PEER_IP"
  --prefix-len "$PREFIX_LEN"
  --tun-mtu "$TUN_MTU"
  --radio-mtu "$RADIO_MTU"
  --agg-timeout-ms "$TUN_AGG_TIMEOUT_MS"
  --tx-peer "127.0.0.1:$TUN_TX_PORT"
  --rx-bind "127.0.0.1:$TUN_RX_PORT"
  --summary-file "$TUN_SUMMARY_FILE"
)
if [[ -n "$TUN_PROBE_COMMAND" ]]; then
  "${tun_cmd[@]}" 2>"$OUT_DIR/wf-tun.log" &
  pids+=("$!")
  sleep "$TUN_SETTLE_SECONDS"
  start_data_load_sources
  if [[ "$DATA_LOAD_MODE" != "none" && "$DATA_LOAD_PRE_PROBE_SECONDS" != "0" ]]; then
    echo "Letting concurrent WFB data load warm up for ${DATA_LOAD_PRE_PROBE_SECONDS}s before probe..." >&2
    sleep "$DATA_LOAD_PRE_PROBE_SECONDS"
  fi
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
    write_data_load_summary || true
    echo "Tunnel probe failed with status $probe_status; artifacts: $OUT_DIR" >&2
    exit "$probe_status"
  fi
  write_data_load_summary
  echo "Tunnel probe passed; artifacts: $OUT_DIR" >&2
else
  "${tun_cmd[@]}" 2>"$OUT_DIR/wf-tun.log"
fi
