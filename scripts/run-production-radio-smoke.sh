#!/usr/bin/env bash
# shellcheck disable=SC2029
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-production-radio-smoke.sh [--mode rx-only|tx-positive|both] [--dry-run] [--skip-deploy]

Runs repeatable `radio-run` production smokes on the hardware Mac.

Configuration is via environment variables:
  HW_MAC_HOST=rownd@100.104.12.123
  HW_DEPLOY_PATH=projects/arc/wfb-mac-radio-snr-deploy
  FIRMWARE=/tmp/rtl8812aefw.bin
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
HW_DEPLOY_PATH=${HW_DEPLOY_PATH:-projects/arc/wfb-mac-radio-snr-deploy}
FIRMWARE=${FIRMWARE:-/tmp/rtl8812aefw.bin}
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

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

if (( DRY_RUN == 0 )); then
  require_command ssh
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
  printf '[prod-smoke:remote] %s\n' "$*" >&2
}

die() {
  printf '[prod-smoke:remote] error: %s\n' "$*" >&2
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

run_radio_smoke() {
  local smoke_mode=$1
  local report="$REMOTE_OUT_DIR/radio-run-$smoke_mode.json"
  local ready="$REMOTE_OUT_DIR/radio-run-$smoke_mode-ready.json"
  local log_file="$REMOTE_OUT_DIR/radio-run-$smoke_mode.log"
  local max_datagrams=0
  local duration_ms=$DURATION_MS

  rm -f "$report" "$ready" "$log_file"
  if [[ "$smoke_mode" == "tx-positive" ]]; then
    max_datagrams=$TX_DATAGRAMS
    duration_ms=$((DURATION_MS < 3500 ? 3500 : DURATION_MS))
  fi

  log "starting radio-run $smoke_mode report=$report"
  set +e
  ./target/debug/wfb-radio-diag --json --report "$report" radio-run \
    --macos-usbhost \
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
    --i-understand-this-transmits \
    >"$log_file" 2>&1 &
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

  SMOKE_MODE="$smoke_mode" REPORT="$report" EXPECT_TX="$([[ "$smoke_mode" == "tx-positive" ]] && echo 1 || echo 0)" \
    TX_DATAGRAMS="$TX_DATAGRAMS" python3 - <<'PY'
import json
import os
import sys

report_path = os.environ["REPORT"]
mode = os.environ["SMOKE_MODE"]
expect_tx = os.environ["EXPECT_TX"] == "1"
expected = int(os.environ["TX_DATAGRAMS"])
with open(report_path, "r", encoding="utf-8") as handle:
    report = json.load(handle)

result = report.get("result")
tx = report.get("tx", {})
rx = report.get("rx", {})
datagrams = int(tx.get("datagrams_received", 0))
submitted = int(tx.get("submitted_frames", 0))
failed = int(tx.get("failed_submissions", 0))
dropped = int(tx.get("dropped_datagrams", 0))

print(
    f"{mode}: result={result} stop={report.get('stop_reason')} "
    f"tx_datagrams={datagrams} submitted={submitted} failed={failed} dropped={dropped} "
    f"rx_buffers={rx.get('buffers_read', 0)} rx_frames={rx.get('parsed_frames', 0)}"
)

if result != "pass":
    print(json.dumps(report.get("error"), indent=2), file=sys.stderr)
    sys.exit(2)
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
mkdir -p "$REMOTE_OUT_DIR"
log "building wfb-radio-diag"
cargo build -p wfb-radio-diag

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
  cat <<EOF
ssh $HW_MAC_HOST with:
MODE=$(printf '%q' "$MODE") RUN_ID=$(printf '%q' "$RUN_ID") HW_DEPLOY_PATH=$(printf '%q' "$HW_DEPLOY_PATH") REMOTE_OUT_DIR=$(printf '%q' "$REMOTE_OUT_DIR") \\
FIRMWARE=$(printf '%q' "$FIRMWARE") VID=$(printf '%q' "$VID") PID=$(printf '%q' "$PID") CHANNEL=$(printf '%q' "$CHANNEL") BANDWIDTH_MHZ=$(printf '%q' "$BANDWIDTH_MHZ") \\
DURATION_MS=$(printf '%q' "$DURATION_MS") RX_TIMEOUT_MS=$(printf '%q' "$RX_TIMEOUT_MS") TX_BURST_LIMIT=$(printf '%q' "$TX_BURST_LIMIT") \\
TX_DATAGRAMS=$(printf '%q' "$TX_DATAGRAMS") TX_BIND=$(printf '%q' "$TX_BIND") TX_INTERVAL_SEC=$(printf '%q' "$TX_INTERVAL_SEC") READY_WAIT_SECONDS=$(printf '%q' "$READY_WAIT_SECONDS") \\
WFB_LINK_ID=$(printf '%q' "$WFB_LINK_ID") WFB_RADIO_PORT=$(printf '%q' "$WFB_RADIO_PORT") FWMARK=$(printf '%q' "$FWMARK") MCS=$(printf '%q' "$MCS") PAYLOAD_LEN=$(printf '%q' "$PAYLOAD_LEN") PAYLOAD_MARKER=$(printf '%q' "$PAYLOAD_MARKER") bash -s
$remote_script
EOF
  exit 0
fi

log "running $MODE smoke on $HW_MAC_HOST"
ssh "$HW_MAC_HOST" \
  MODE="$MODE" \
  RUN_ID="$RUN_ID" \
  HW_DEPLOY_PATH="$HW_DEPLOY_PATH" \
  REMOTE_OUT_DIR="$REMOTE_OUT_DIR" \
  FIRMWARE="$FIRMWARE" \
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
