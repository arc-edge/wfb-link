#!/usr/bin/env bash
# shellcheck disable=SC2029
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-production-radio-smoke.sh [--mode rx-only|tx-positive|both] [--dry-run] [--skip-deploy] [--local]

Runs repeatable `radio-run` production smokes on the hardware Mac.

Configuration is via environment variables:
  HW_MAC_HOST=rownd@100.104.12.123
  HW_DEPLOY_PATH=projects/arc/wfb-mac-radio-snr-deploy
  LOCAL_HW=1              # run on this checkout instead of SSH deployment
  RADIO_COMMAND=service    # service or diagnostic
  FIRMWARE=/tmp/rtl8812aefw.bin
  RADIO_RUN_CONFIG=configs/radio-run-robust-short-range.toml
  EFUSE_REPORT=/tmp/wfb-remote-macos-efuse-dump.json
  TX_POWER_MODE=current-default     # current-default, efuse-derived, or manual-index
  TX_POWER_INDEX=0x18               # required for manual-index
  TX_POWER_PATH=both                # a, b, or both
  TX_POWER_EFUSE_LOGICAL_MAP=/tmp/wfb-efuse-logical.bin
  TX_POWER_SAFETY_PROFILE=linux-ch36-ht20
  TX_POWER_MAX_INDEX=0x3f
  TX_CALIBRATION_PROFILE=current-default
  VID=0x0bda PID=0x8812 CHANNEL=36 BANDWIDTH_MHZ=20
  DURATION_MS=2500 RX_TIMEOUT_MS=20 TX_BURST_LIMIT=8
  TX_DATAGRAMS=64 TX_BIND=127.0.0.1:5600 TX_INTERVAL_SEC=0.001
  WFB_LINK_ID=0x000001 WFB_RADIO_PORT=0x23 MCS=1 PAYLOAD_LEN=256

The tx-positive mode waits for the ready marker, injects synthetic WFB
distributor-style UDP datagrams into `radio-run`, and fails if the production
report does not show received and submitted TX frames.
EOF
}

log() {
  printf '[prod-smoke] %s\n' "$*" >&2
}

die() {
  printf '[prod-smoke] error: %s\n' "$*" >&2
  exit 1
}

MODE=both
DRY_RUN=0
DEPLOY=1
LOCAL_HW=${LOCAL_HW:-0}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      [[ $# -ge 2 ]] || die "--mode requires rx-only, tx-positive, or both"
      MODE=$2
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --skip-deploy)
      DEPLOY=0
      shift
      ;;
    --local)
      LOCAL_HW=1
      DEPLOY=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

case "$MODE" in
  rx-only|tx-positive|both) ;;
  *) die "invalid mode: $MODE" ;;
esac

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO_ROOT"

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
HW_MAC_HOST=${HW_MAC_HOST:-rownd@100.104.12.123}
HW_DEPLOY_PATH_WAS_SET=${HW_DEPLOY_PATH+x}
HW_DEPLOY_PATH=${HW_DEPLOY_PATH:-projects/arc/wfb-mac-radio-snr-deploy}
FIRMWARE=${FIRMWARE:-/tmp/rtl8812aefw.bin}
RADIO_RUN_CONFIG=${RADIO_RUN_CONFIG:-configs/radio-run-robust-short-range.toml}
RADIO_COMMAND=${RADIO_COMMAND:-service}
EFUSE_REPORT=${EFUSE_REPORT:-/tmp/wfb-remote-macos-efuse-dump.json}
TX_POWER_MODE=${TX_POWER_MODE:-current-default}
TX_POWER_INDEX=${TX_POWER_INDEX:-}
TX_POWER_PATH=${TX_POWER_PATH:-both}
TX_POWER_EFUSE_LOGICAL_MAP=${TX_POWER_EFUSE_LOGICAL_MAP:-}
TX_POWER_SAFETY_PROFILE=${TX_POWER_SAFETY_PROFILE:-linux-ch36-ht20}
TX_POWER_MAX_INDEX=${TX_POWER_MAX_INDEX:-}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}
VID=${VID:-0x0bda}
PID=${PID:-0x8812}
CHANNEL=${CHANNEL:-36}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
DURATION_MS=${DURATION_MS:-2500}
RX_TIMEOUT_MS=${RX_TIMEOUT_MS:-20}
TX_BURST_LIMIT=${TX_BURST_LIMIT:-8}
TX_DATAGRAMS=${TX_DATAGRAMS:-64}
TX_BIND=${TX_BIND:-127.0.0.1:5600}
TX_INTERVAL_SEC=${TX_INTERVAL_SEC:-0.001}
READY_WAIT_SECONDS=${READY_WAIT_SECONDS:-90}
WFB_LINK_ID=${WFB_LINK_ID:-0x000001}
WFB_RADIO_PORT=${WFB_RADIO_PORT:-0x23}
FWMARK=${FWMARK:-0x00000000}
MCS=${MCS:-1}
PAYLOAD_LEN=${PAYLOAD_LEN:-256}
PAYLOAD_MARKER=${PAYLOAD_MARKER:-PRODSMOK}
REMOTE_OUT_DIR=${REMOTE_OUT_DIR:-/tmp/wfb-prod-radio-smoke-$RUN_ID}

case "$RADIO_COMMAND" in
  service|diagnostic) ;;
  diag) RADIO_COMMAND=diagnostic ;;
  *) die "invalid RADIO_COMMAND: $RADIO_COMMAND (expected service or diagnostic)" ;;
esac

LOCAL_RUN=0
case "$HW_MAC_HOST" in
  local|localhost|127.0.0.1) LOCAL_RUN=1 ;;
esac
if [[ "$LOCAL_HW" == "1" ]]; then
  LOCAL_RUN=1
fi
if (( LOCAL_RUN == 1 )); then
  DEPLOY=0
  if [[ -z "${HW_DEPLOY_PATH_WAS_SET:-}" ]]; then
    HW_DEPLOY_PATH=$REPO_ROOT
  fi
fi

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

if (( DRY_RUN == 0 )); then
  if (( LOCAL_RUN == 1 )); then
    require_command cargo
    require_command python3
  else
    require_command ssh
  fi
  if (( DEPLOY == 1 )); then
    require_command rsync
  fi
fi

if (( DEPLOY == 1 )); then
  log "syncing checkout to $HW_MAC_HOST:$HW_DEPLOY_PATH"
  if (( DRY_RUN == 0 )); then
    rsync -az --delete --exclude target --exclude .git "$REPO_ROOT/" "$HW_MAC_HOST:$HW_DEPLOY_PATH/"
  fi
fi

remote_script=''
read -r -d '' remote_script <<'REMOTE' || true
set -euo pipefail

log() {
  printf '[prod-smoke:runner] %s\n' "$*" >&2
}

die() {
  printf '[prod-smoke:runner] error: %s\n' "$*" >&2
  exit 1
}

wait_for_file() {
  local path=$1
  local limit=$2
  local start now
  start=$(date +%s)
  while [[ ! -f "$path" ]]; do
    sleep 0.2
    now=$(date +%s)
    if (( now - start >= limit )); then
      return 1
    fi
  done
}

build_rf_profile_args() {
  TX_POWER_ARGS=()
  WRITE_AUTH_ARGS=()
  local requires_write_auth=0
  local tx_power_mode=$TX_POWER_MODE
  local tx_calibration_profile=$TX_CALIBRATION_PROFILE

  if [[ "$tx_power_mode" == "current_default" ]]; then
    tx_power_mode=current-default
  fi
  if [[ "$tx_calibration_profile" == "current_default" ]]; then
    tx_calibration_profile=current-default
  fi
  TX_CALIBRATION_ARGS=(--tx-calibration-profile "$tx_calibration_profile")

  if [[ "$tx_power_mode" == "current-default" && -n "$TX_POWER_INDEX" ]]; then
    tx_power_mode=manual-index
  fi
  EFFECTIVE_TX_POWER_MODE=$tx_power_mode
  EFFECTIVE_TX_CALIBRATION_PROFILE=$tx_calibration_profile

  if [[ "$tx_power_mode" != "current-default" ]]; then
    requires_write_auth=1
    TX_POWER_ARGS+=(--tx-power-mode "$tx_power_mode")
    if [[ -n "$TX_POWER_PATH" ]]; then
      TX_POWER_ARGS+=(--tx-power-path "$TX_POWER_PATH")
    fi
    case "$tx_power_mode" in
      efuse-derived)
        if [[ -n "$EFUSE_REPORT" && -n "$TX_POWER_EFUSE_LOGICAL_MAP" ]]; then
          die "use only one of EFUSE_REPORT or TX_POWER_EFUSE_LOGICAL_MAP"
        fi
        if [[ -n "$EFUSE_REPORT" ]]; then
          [[ -f "$EFUSE_REPORT" ]] || die "EFUSE report not found: $EFUSE_REPORT"
          TX_POWER_ARGS+=(--tx-power-efuse-report "$EFUSE_REPORT")
        elif [[ -n "$TX_POWER_EFUSE_LOGICAL_MAP" ]]; then
          [[ -f "$TX_POWER_EFUSE_LOGICAL_MAP" ]] || die "EFUSE logical map not found: $TX_POWER_EFUSE_LOGICAL_MAP"
          TX_POWER_ARGS+=(--tx-power-efuse-logical-map "$TX_POWER_EFUSE_LOGICAL_MAP")
        else
          die "TX_POWER_MODE=efuse-derived requires EFUSE_REPORT or TX_POWER_EFUSE_LOGICAL_MAP"
        fi
        if [[ -n "$TX_POWER_SAFETY_PROFILE" ]]; then
          TX_POWER_ARGS+=(--tx-power-safety-profile "$TX_POWER_SAFETY_PROFILE")
        fi
        if [[ -n "$TX_POWER_MAX_INDEX" ]]; then
          TX_POWER_ARGS+=(--tx-power-max-index "$TX_POWER_MAX_INDEX")
        fi
        ;;
      manual-index)
        [[ -n "$TX_POWER_INDEX" ]] || die "TX_POWER_MODE=manual-index requires TX_POWER_INDEX"
        TX_POWER_ARGS+=(--tx-power-index "$TX_POWER_INDEX")
        ;;
      *)
        die "invalid TX_POWER_MODE=$TX_POWER_MODE"
        ;;
    esac
  fi

  case "$tx_calibration_profile" in
    current-default|current_default|rtl8812a-iqk-probe) ;;
    linux-parity-ch36-ht20|rtl8812a-lck|rtl8812a-runtime-iqk)
      requires_write_auth=1
      ;;
    *)
      die "invalid TX_CALIBRATION_PROFILE=$TX_CALIBRATION_PROFILE"
      ;;
  esac

  if (( requires_write_auth == 1 )); then
    WRITE_AUTH_ARGS=(--i-understand-this-writes-registers)
  fi
}

run_radio_smoke() {
  local smoke_mode=$1
  local report="$REMOTE_OUT_DIR/radio-run-$smoke_mode.json"
  local ready="$REMOTE_OUT_DIR/radio-run-$smoke_mode-ready.json"
  local health="$REMOTE_OUT_DIR/radio-run-$smoke_mode-health.json"
  local summary="$REMOTE_OUT_DIR/radio-run-$smoke_mode-summary.json"
  local log_file="$REMOTE_OUT_DIR/radio-run-$smoke_mode.log"
  local max_datagrams=0
  local duration_ms=$DURATION_MS

  rm -f "$report" "$ready" "$health" "$summary" "$log_file"
  if [[ "$smoke_mode" == "tx-positive" ]]; then
    max_datagrams=$TX_DATAGRAMS
    duration_ms=$((DURATION_MS < 3500 ? 3500 : DURATION_MS))
  fi
  build_rf_profile_args

  log "starting $RADIO_COMMAND radio-run $smoke_mode report=$report rf=$EFFECTIVE_TX_POWER_MODE/$TX_POWER_SAFETY_PROFILE/$EFFECTIVE_TX_CALIBRATION_PROFILE"
  set +e
  case "$RADIO_COMMAND" in
    service)
      ./target/debug/wfb-radio-service \
        --json \
        --report "$report" \
        --config "$RADIO_RUN_CONFIG" \
        --vid "$VID" \
        --pid "$PID" \
        --channel "$CHANNEL" \
        --bandwidth "$BANDWIDTH_MHZ" \
        --firmware "$FIRMWARE" \
        --bind "$TX_BIND" \
        --duration-ms "$duration_ms" \
        --rx-timeout-ms "$RX_TIMEOUT_MS" \
        --tx-burst-limit "$TX_BURST_LIMIT" \
        --max-datagrams "$max_datagrams" \
        --ready-file "$ready" \
        --health-file "$health" \
        ${TX_POWER_ARGS[@]+"${TX_POWER_ARGS[@]}"} \
        ${TX_CALIBRATION_ARGS[@]+"${TX_CALIBRATION_ARGS[@]}"} \
        ${WRITE_AUTH_ARGS[@]+"${WRITE_AUTH_ARGS[@]}"} \
        --i-understand-this-transmits \
        >"$log_file" 2>&1 &
      ;;
    diagnostic)
      ./target/debug/wfb-radio-diag --json --report "$report" radio-run \
        --config "$RADIO_RUN_CONFIG" \
        --vid "$VID" \
        --pid "$PID" \
        --channel "$CHANNEL" \
        --bandwidth "$BANDWIDTH_MHZ" \
        --firmware "$FIRMWARE" \
        --bind "$TX_BIND" \
        --duration-ms "$duration_ms" \
        --rx-timeout-ms "$RX_TIMEOUT_MS" \
        --tx-burst-limit "$TX_BURST_LIMIT" \
        --max-datagrams "$max_datagrams" \
        --ready-file "$ready" \
        --health-file "$health" \
        ${TX_POWER_ARGS[@]+"${TX_POWER_ARGS[@]}"} \
        ${TX_CALIBRATION_ARGS[@]+"${TX_CALIBRATION_ARGS[@]}"} \
        ${WRITE_AUTH_ARGS[@]+"${WRITE_AUTH_ARGS[@]}"} \
        --i-understand-this-transmits \
        >"$log_file" 2>&1 &
      ;;
  esac
  local radio_pid=$!
  set -e

  if ! wait_for_file "$ready" "$READY_WAIT_SECONDS"; then
    cat "$log_file" >&2 || true
    kill "$radio_pid" >/dev/null 2>&1 || true
    wait "$radio_pid" >/dev/null 2>&1 || true
    die "radio-run did not write ready marker for $smoke_mode"
  fi

  if [[ "$smoke_mode" == "tx-positive" ]]; then
    log "injecting $TX_DATAGRAMS TX datagrams into $TX_BIND"
    TX_BIND="$TX_BIND" TX_DATAGRAMS="$TX_DATAGRAMS" TX_INTERVAL_SEC="$TX_INTERVAL_SEC" \
      WFB_LINK_ID="$WFB_LINK_ID" WFB_RADIO_PORT="$WFB_RADIO_PORT" FWMARK="$FWMARK" \
      MCS="$MCS" BANDWIDTH_MHZ="$BANDWIDTH_MHZ" PAYLOAD_LEN="$PAYLOAD_LEN" \
      PAYLOAD_MARKER="$PAYLOAD_MARKER" python3 - <<'PY'
import os
import socket
import time

def int_env(name):
    return int(os.environ[name], 0)

bind = os.environ["TX_BIND"]
host, port_text = bind.rsplit(":", 1)
target = (host, int(port_text, 10))
count = int_env("TX_DATAGRAMS")
interval = float(os.environ["TX_INTERVAL_SEC"])
link_id = int_env("WFB_LINK_ID") & 0x00FF_FFFF
radio_port = int_env("WFB_RADIO_PORT") & 0xFF
fwmark = int_env("FWMARK") & 0xFFFF_FFFF
mcs = int_env("MCS") & 0xFF
payload_len = int_env("PAYLOAD_LEN")
marker = os.environ["PAYLOAD_MARKER"].encode("ascii")
bandwidth = int_env("BANDWIDTH_MHZ")
bandwidth_flag = 0x01 if bandwidth == 40 else 0x00
channel = ((link_id << 8) | radio_port).to_bytes(4, "big")
radiotap = bytes([0x00, 0x00, 0x0d, 0x00, 0x00, 0x80, 0x08, 0x00, 0x08, 0x00, 0x37, bandwidth_flag, mcs])

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
for seq in range(count):
    header = bytearray([
        0x08, 0x01, 0x00, 0x00,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0x57, 0x42, 0x00, 0x00, 0x00, 0x00,
        0x57, 0x42, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ])
    header[12:16] = channel
    header[18:22] = channel
    header[22:24] = (seq & 0xFFFF).to_bytes(2, "little")
    payload = bytearray()
    payload.extend(marker)
    payload.extend((seq & 0xFFFF_FFFF).to_bytes(4, "big"))
    while len(payload) < payload_len:
        payload.append((len(payload) + seq) % 251)
    datagram = fwmark.to_bytes(4, "big") + radiotap + bytes(header) + bytes(payload[:payload_len])
    sock.sendto(datagram, target)
    if interval:
        time.sleep(interval)
PY
  fi

  if ! wait "$radio_pid"; then
    cat "$log_file" >&2 || true
    die "radio-run failed for $smoke_mode"
  fi

  SMOKE_MODE="$smoke_mode" RADIO_COMMAND="$RADIO_COMMAND" REPORT="$report" HEALTH="$health" SUMMARY="$summary" EXPECT_TX="$([[ "$smoke_mode" == "tx-positive" ]] && echo 1 || echo 0)" \
    TX_DATAGRAMS="$TX_DATAGRAMS" TX_POWER_MODE="$TX_POWER_MODE" EFFECTIVE_TX_POWER_MODE="$EFFECTIVE_TX_POWER_MODE" TX_POWER_INDEX="$TX_POWER_INDEX" TX_POWER_PATH="$TX_POWER_PATH" EFUSE_REPORT="$EFUSE_REPORT" TX_POWER_EFUSE_LOGICAL_MAP="$TX_POWER_EFUSE_LOGICAL_MAP" \
    TX_POWER_SAFETY_PROFILE="$TX_POWER_SAFETY_PROFILE" TX_POWER_MAX_INDEX="$TX_POWER_MAX_INDEX" TX_CALIBRATION_PROFILE="$TX_CALIBRATION_PROFILE" EFFECTIVE_TX_CALIBRATION_PROFILE="$EFFECTIVE_TX_CALIBRATION_PROFILE" python3 - <<'PY'
import json
import os
import sys

report_path = os.environ["REPORT"]
health_path = os.environ["HEALTH"]
summary_path = os.environ["SUMMARY"]
mode = os.environ["SMOKE_MODE"]
radio_command = os.environ["RADIO_COMMAND"]
expect_tx = os.environ["EXPECT_TX"] == "1"
expected = int(os.environ["TX_DATAGRAMS"])
with open(report_path, "r", encoding="utf-8") as handle:
    report = json.load(handle)
with open(health_path, "r", encoding="utf-8") as handle:
    health = json.load(handle)

result = report.get("result")
tx = report.get("tx", {})
rx = report.get("rx", {})
tx_power_control = report.get("tx_power_control")
tx_calibration_report = report.get("tx_calibration_profile")
rf_profile = {
    "tx_power_mode": os.environ.get("TX_POWER_MODE"),
    "effective_tx_power_mode": os.environ.get("EFFECTIVE_TX_POWER_MODE"),
    "tx_power_index": os.environ.get("TX_POWER_INDEX") or None,
    "tx_power_path": os.environ.get("TX_POWER_PATH"),
    "efuse_report": os.environ.get("EFUSE_REPORT") or None,
    "tx_power_efuse_logical_map": os.environ.get("TX_POWER_EFUSE_LOGICAL_MAP") or None,
    "tx_power_safety_profile": os.environ.get("TX_POWER_SAFETY_PROFILE") or None,
    "tx_power_max_index": os.environ.get("TX_POWER_MAX_INDEX") or None,
    "tx_calibration_profile": os.environ.get("TX_CALIBRATION_PROFILE"),
    "effective_tx_calibration_profile": os.environ.get("EFFECTIVE_TX_CALIBRATION_PROFILE"),
}
tx_power_register_count = 0
if isinstance(tx_power_control, dict):
    tx_power_register_count = int(tx_power_control.get("register_count") or 0)
datagrams = int(tx.get("datagrams_received", 0))
submitted = int(tx.get("submitted_frames", 0))
failed = int(tx.get("failed_submissions", 0))
dropped = int(tx.get("dropped_datagrams", 0))
parsed_frames = int(rx.get("parsed_frames", 0))
rx_outcome_fields = [
    "need_more_data",
    "management_frames",
    "control_frames",
    "data_frames",
    "extension_frames",
]
missing_rx_outcome_fields = [field for field in rx_outcome_fields if field not in rx]
rx_outcome_counts = {}
for field in rx_outcome_fields:
    if field in rx:
        try:
            rx_outcome_counts[field] = int(rx[field])
        except (TypeError, ValueError):
            print(f"rx.{field} is not an integer: {rx[field]!r}", file=sys.stderr)
            sys.exit(4)
frame_type_total = sum(
    rx_outcome_counts.get(field, 0)
    for field in ("management_frames", "control_frames", "data_frames", "extension_frames")
)
summary = {
    "mode": mode,
    "radio_command": radio_command,
    "result": result,
    "stop_reason": report.get("stop_reason"),
    "health_lifecycle": health.get("lifecycle"),
    "health_operator_action": health.get("operator_action"),
    "rf_profile": rf_profile,
    "tx_power_control": tx_power_control,
    "tx_calibration_profile": tx_calibration_report,
    "tx": tx,
    "rx": rx,
}
with open(summary_path, "w", encoding="utf-8") as handle:
    json.dump(summary, handle, indent=2, sort_keys=True)
    handle.write("\n")

print(
    f"{mode}: command={radio_command} result={result} stop={report.get('stop_reason')} "
    f"rf={rf_profile['effective_tx_power_mode']}/{rf_profile['tx_power_safety_profile']}/"
    f"{rf_profile['effective_tx_calibration_profile']} tx_power_regs={tx_power_register_count} "
    f"health={health.get('lifecycle')}/{health.get('operator_action')} "
    f"tx_datagrams={datagrams} submitted={submitted} failed={failed} dropped={dropped} "
    f"rx_buffers={rx.get('buffers_read', 0)} rx_frames={parsed_frames} "
    f"rx_need_more={rx_outcome_counts.get('need_more_data', 'missing')} "
    f"rx_types={rx_outcome_counts.get('management_frames', 'missing')}/"
    f"{rx_outcome_counts.get('control_frames', 'missing')}/"
    f"{rx_outcome_counts.get('data_frames', 'missing')}/"
    f"{rx_outcome_counts.get('extension_frames', 'missing')}"
)

if result != "pass":
    print(json.dumps(report.get("error"), indent=2), file=sys.stderr)
    sys.exit(2)
normalized_tx_power_mode = (rf_profile["effective_tx_power_mode"] or "current-default").replace("_", "-")
normalized_calibration = (rf_profile["effective_tx_calibration_profile"] or "current-default").replace("_", "-")
if normalized_tx_power_mode != "current-default" and tx_power_register_count <= 0:
    print(
        f"expected TX power evidence for TX_POWER_MODE={rf_profile['tx_power_mode']}, "
        f"got register_count={tx_power_register_count}",
        file=sys.stderr,
    )
    sys.exit(4)
if normalized_calibration != "current-default" and not isinstance(tx_calibration_report, dict):
    print(
        f"expected calibration evidence for TX_CALIBRATION_PROFILE={rf_profile['tx_calibration_profile']}",
        file=sys.stderr,
    )
    sys.exit(4)
if health.get("lifecycle") != "exited_pass" or health.get("result") != "pass":
    print(f"unexpected health final state: {json.dumps(health, indent=2)}", file=sys.stderr)
    sys.exit(4)
if missing_rx_outcome_fields:
    print(
        "production RX outcome telemetry is missing field(s): "
        + ", ".join(f"rx.{field}" for field in missing_rx_outcome_fields),
        file=sys.stderr,
    )
    sys.exit(4)
if parsed_frames > 0 and frame_type_total == 0:
    print(
        f"production RX frame type counters are all zero despite parsed_frames={parsed_frames}",
        file=sys.stderr,
    )
    sys.exit(4)
if expect_tx:
    if datagrams < expected or submitted < expected or failed != 0 or dropped != 0:
        print(
            f"expected at least {expected} clean TX datagrams/submissions, "
            f"got datagrams={datagrams} submitted={submitted} failed={failed} dropped={dropped}",
            file=sys.stderr,
        )
        sys.exit(3)
PY
}

cd "$HW_DEPLOY_PATH"
[[ -f "$RADIO_RUN_CONFIG" ]] || die "radio-run config not found: $RADIO_RUN_CONFIG"
mkdir -p "$REMOTE_OUT_DIR"
case "$RADIO_COMMAND" in
  service)
    log "building wfb-radio-service"
    cargo build -p wfb-radio-service
    ;;
  diagnostic)
    log "building wfb-radio-diag"
    cargo build -p wfb-radio-diag
    ;;
esac

case "$MODE" in
  rx-only)
    run_radio_smoke rx-only
    ;;
  tx-positive)
    run_radio_smoke tx-positive
    ;;
  both)
    run_radio_smoke rx-only
    run_radio_smoke tx-positive
    ;;
esac

log "artifacts in $REMOTE_OUT_DIR"
REMOTE

if (( DRY_RUN == 1 )); then
  if (( LOCAL_RUN == 1 )); then
    cat <<EOF
local bash with:
MODE=$(printf '%q' "$MODE") RUN_ID=$(printf '%q' "$RUN_ID") HW_DEPLOY_PATH=$(printf '%q' "$HW_DEPLOY_PATH") REMOTE_OUT_DIR=$(printf '%q' "$REMOTE_OUT_DIR") RADIO_COMMAND=$(printf '%q' "$RADIO_COMMAND") \\
FIRMWARE=$(printf '%q' "$FIRMWARE") RADIO_RUN_CONFIG=$(printf '%q' "$RADIO_RUN_CONFIG") EFUSE_REPORT=$(printf '%q' "$EFUSE_REPORT") TX_POWER_MODE=$(printf '%q' "$TX_POWER_MODE") TX_POWER_INDEX=$(printf '%q' "$TX_POWER_INDEX") TX_POWER_PATH=$(printf '%q' "$TX_POWER_PATH") \\
TX_POWER_EFUSE_LOGICAL_MAP=$(printf '%q' "$TX_POWER_EFUSE_LOGICAL_MAP") TX_POWER_SAFETY_PROFILE=$(printf '%q' "$TX_POWER_SAFETY_PROFILE") TX_POWER_MAX_INDEX=$(printf '%q' "$TX_POWER_MAX_INDEX") TX_CALIBRATION_PROFILE=$(printf '%q' "$TX_CALIBRATION_PROFILE") \\
VID=$(printf '%q' "$VID") PID=$(printf '%q' "$PID") CHANNEL=$(printf '%q' "$CHANNEL") BANDWIDTH_MHZ=$(printf '%q' "$BANDWIDTH_MHZ") \\
DURATION_MS=$(printf '%q' "$DURATION_MS") RX_TIMEOUT_MS=$(printf '%q' "$RX_TIMEOUT_MS") TX_BURST_LIMIT=$(printf '%q' "$TX_BURST_LIMIT") \\
TX_DATAGRAMS=$(printf '%q' "$TX_DATAGRAMS") TX_BIND=$(printf '%q' "$TX_BIND") TX_INTERVAL_SEC=$(printf '%q' "$TX_INTERVAL_SEC") READY_WAIT_SECONDS=$(printf '%q' "$READY_WAIT_SECONDS") \\
WFB_LINK_ID=$(printf '%q' "$WFB_LINK_ID") WFB_RADIO_PORT=$(printf '%q' "$WFB_RADIO_PORT") FWMARK=$(printf '%q' "$FWMARK") MCS=$(printf '%q' "$MCS") PAYLOAD_LEN=$(printf '%q' "$PAYLOAD_LEN") PAYLOAD_MARKER=$(printf '%q' "$PAYLOAD_MARKER") bash -s
$remote_script
EOF
  else
    cat <<EOF
ssh $HW_MAC_HOST with:
MODE=$(printf '%q' "$MODE") RUN_ID=$(printf '%q' "$RUN_ID") HW_DEPLOY_PATH=$(printf '%q' "$HW_DEPLOY_PATH") REMOTE_OUT_DIR=$(printf '%q' "$REMOTE_OUT_DIR") RADIO_COMMAND=$(printf '%q' "$RADIO_COMMAND") \\
FIRMWARE=$(printf '%q' "$FIRMWARE") RADIO_RUN_CONFIG=$(printf '%q' "$RADIO_RUN_CONFIG") EFUSE_REPORT=$(printf '%q' "$EFUSE_REPORT") TX_POWER_MODE=$(printf '%q' "$TX_POWER_MODE") TX_POWER_INDEX=$(printf '%q' "$TX_POWER_INDEX") TX_POWER_PATH=$(printf '%q' "$TX_POWER_PATH") \\
TX_POWER_EFUSE_LOGICAL_MAP=$(printf '%q' "$TX_POWER_EFUSE_LOGICAL_MAP") TX_POWER_SAFETY_PROFILE=$(printf '%q' "$TX_POWER_SAFETY_PROFILE") TX_POWER_MAX_INDEX=$(printf '%q' "$TX_POWER_MAX_INDEX") TX_CALIBRATION_PROFILE=$(printf '%q' "$TX_CALIBRATION_PROFILE") \\
VID=$(printf '%q' "$VID") PID=$(printf '%q' "$PID") CHANNEL=$(printf '%q' "$CHANNEL") BANDWIDTH_MHZ=$(printf '%q' "$BANDWIDTH_MHZ") \\
DURATION_MS=$(printf '%q' "$DURATION_MS") RX_TIMEOUT_MS=$(printf '%q' "$RX_TIMEOUT_MS") TX_BURST_LIMIT=$(printf '%q' "$TX_BURST_LIMIT") \\
TX_DATAGRAMS=$(printf '%q' "$TX_DATAGRAMS") TX_BIND=$(printf '%q' "$TX_BIND") TX_INTERVAL_SEC=$(printf '%q' "$TX_INTERVAL_SEC") READY_WAIT_SECONDS=$(printf '%q' "$READY_WAIT_SECONDS") \\
WFB_LINK_ID=$(printf '%q' "$WFB_LINK_ID") WFB_RADIO_PORT=$(printf '%q' "$WFB_RADIO_PORT") FWMARK=$(printf '%q' "$FWMARK") MCS=$(printf '%q' "$MCS") PAYLOAD_LEN=$(printf '%q' "$PAYLOAD_LEN") PAYLOAD_MARKER=$(printf '%q' "$PAYLOAD_MARKER") bash -s
$remote_script
EOF
  fi
  exit 0
fi

if (( LOCAL_RUN == 1 )); then
  log "running $MODE smoke locally from $HW_DEPLOY_PATH"
  MODE="$MODE" \
  RUN_ID="$RUN_ID" \
  HW_DEPLOY_PATH="$HW_DEPLOY_PATH" \
  REMOTE_OUT_DIR="$REMOTE_OUT_DIR" \
  RADIO_COMMAND="$RADIO_COMMAND" \
  FIRMWARE="$FIRMWARE" \
  RADIO_RUN_CONFIG="$RADIO_RUN_CONFIG" \
  EFUSE_REPORT="$EFUSE_REPORT" \
  TX_POWER_MODE="$TX_POWER_MODE" \
  TX_POWER_INDEX="$TX_POWER_INDEX" \
  TX_POWER_PATH="$TX_POWER_PATH" \
  TX_POWER_EFUSE_LOGICAL_MAP="$TX_POWER_EFUSE_LOGICAL_MAP" \
  TX_POWER_SAFETY_PROFILE="$TX_POWER_SAFETY_PROFILE" \
  TX_POWER_MAX_INDEX="$TX_POWER_MAX_INDEX" \
  TX_CALIBRATION_PROFILE="$TX_CALIBRATION_PROFILE" \
  VID="$VID" \
  PID="$PID" \
  CHANNEL="$CHANNEL" \
  BANDWIDTH_MHZ="$BANDWIDTH_MHZ" \
  DURATION_MS="$DURATION_MS" \
  RX_TIMEOUT_MS="$RX_TIMEOUT_MS" \
  TX_BURST_LIMIT="$TX_BURST_LIMIT" \
  TX_DATAGRAMS="$TX_DATAGRAMS" \
  TX_BIND="$TX_BIND" \
  TX_INTERVAL_SEC="$TX_INTERVAL_SEC" \
  READY_WAIT_SECONDS="$READY_WAIT_SECONDS" \
  WFB_LINK_ID="$WFB_LINK_ID" \
  WFB_RADIO_PORT="$WFB_RADIO_PORT" \
  FWMARK="$FWMARK" \
  MCS="$MCS" \
  PAYLOAD_LEN="$PAYLOAD_LEN" \
  PAYLOAD_MARKER="$PAYLOAD_MARKER" \
  bash -s <<<"$remote_script"
  exit 0
fi

log "running $MODE smoke on $HW_MAC_HOST"
ssh "$HW_MAC_HOST" \
  MODE="$MODE" \
  RUN_ID="$RUN_ID" \
  HW_DEPLOY_PATH="$HW_DEPLOY_PATH" \
  REMOTE_OUT_DIR="$REMOTE_OUT_DIR" \
  RADIO_COMMAND="$RADIO_COMMAND" \
  FIRMWARE="$FIRMWARE" \
  RADIO_RUN_CONFIG="$RADIO_RUN_CONFIG" \
  EFUSE_REPORT="$EFUSE_REPORT" \
  TX_POWER_MODE="$TX_POWER_MODE" \
  TX_POWER_INDEX="$TX_POWER_INDEX" \
  TX_POWER_PATH="$TX_POWER_PATH" \
  TX_POWER_EFUSE_LOGICAL_MAP="$TX_POWER_EFUSE_LOGICAL_MAP" \
  TX_POWER_SAFETY_PROFILE="$TX_POWER_SAFETY_PROFILE" \
  TX_POWER_MAX_INDEX="$TX_POWER_MAX_INDEX" \
  TX_CALIBRATION_PROFILE="$TX_CALIBRATION_PROFILE" \
  VID="$VID" \
  PID="$PID" \
  CHANNEL="$CHANNEL" \
  BANDWIDTH_MHZ="$BANDWIDTH_MHZ" \
  DURATION_MS="$DURATION_MS" \
  RX_TIMEOUT_MS="$RX_TIMEOUT_MS" \
  TX_BURST_LIMIT="$TX_BURST_LIMIT" \
  TX_DATAGRAMS="$TX_DATAGRAMS" \
  TX_BIND="$TX_BIND" \
  TX_INTERVAL_SEC="$TX_INTERVAL_SEC" \
  READY_WAIT_SECONDS="$READY_WAIT_SECONDS" \
  WFB_LINK_ID="$WFB_LINK_ID" \
  WFB_RADIO_PORT="$WFB_RADIO_PORT" \
  FWMARK="$FWMARK" \
  MCS="$MCS" \
  PAYLOAD_LEN="$PAYLOAD_LEN" \
  PAYLOAD_MARKER="$PAYLOAD_MARKER" \
  bash -s <<<"$remote_script"
