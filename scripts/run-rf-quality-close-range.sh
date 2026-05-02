#!/usr/bin/env bash
# shellcheck disable=SC2029,SC2088
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-rf-quality-close-range.sh [--dry-run] [--skip-report] [--out-dir DIR]

Runs the accepted close-range channel 36 HT20 RF-quality workflow across:
  local checkout -> hardware Mac -> Linux WFB peer

Configuration is via environment variables. Common overrides:
  HW_MAC_HOST=rownd@rownds-macbook-pro.tail5c793f.ts.net
  HW_REPO_PATH=projects/arc/wfb-mac-radio-agent
  HW_DEPLOY=1 HW_DEPLOY_PATH=projects/arc/wfb-mac-radio-deploy
  LINUX_HOST=drone-2f389.local
  MAC_LAN_IP=10.42.0.162
  FIRMWARE=/tmp/rtl8812aefw.bin
  EFUSE_REPORT=/tmp/wfb-remote-macos-efuse-dump.json
  EXPECTED_PAYLOADS=2000 PAYLOAD_LEN=1000 CHANNEL=36 BANDWIDTH_MHZ=20

Use --dry-run to print the remote command plan without claiming USB or
transmitting RF.
EOF
}

log() {
  printf '[rfq] %s\n' "$*" >&2
}

die() {
  printf '[rfq] error: %s\n' "$*" >&2
  exit 1
}

quote() {
  printf '%q' "$1"
}

env_assignments() {
  local name
  for name in "$@"; do
    printf '%s=%q ' "$name" "${!name}"
  done
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
DRY_RUN=0
SKIP_REPORT=0
OUT_DIR=${OUT_DIR:-/tmp/wfb-rfq-close-range-$RUN_ID}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --skip-report)
      SKIP_REPORT=1
      shift
      ;;
    --out-dir)
      [[ $# -ge 2 ]] || die "--out-dir requires a path"
      OUT_DIR=$2
      shift 2
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

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO_ROOT"

HW_MAC_HOST=${HW_MAC_HOST:-rownd@rownds-macbook-pro.tail5c793f.ts.net}
HW_REPO_PATH=${HW_REPO_PATH:-}
if [[ -z "$HW_REPO_PATH" ]]; then
  HW_REPO_PATH='projects/arc/wfb-mac-radio-agent'
fi
LINUX_HOST=${LINUX_HOST:-drone-2f389.local}
MAC_LAN_IP=${MAC_LAN_IP:-10.42.0.162}
REMOTE_PREFIX=${REMOTE_PREFIX:-/tmp/wfb-rfq-auto-$RUN_ID}
SYNC_HW_REPO=${SYNC_HW_REPO:-0}
HW_DEPLOY=${HW_DEPLOY:-0}
HW_DEPLOY_PATH=${HW_DEPLOY_PATH:-projects/arc/wfb-mac-radio-deploy}
ALLOW_DEPLOY_OVER_WORKTREE=${ALLOW_DEPLOY_OVER_WORKTREE:-0}

CHANNEL=${CHANNEL:-36}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
FEC_K=${FEC_K:-8}
FEC_N=${FEC_N:-12}
EXPECTED_PAYLOADS=${EXPECTED_PAYLOADS:-2000}
MAX_DATAGRAMS=${MAX_DATAGRAMS:-$(((EXPECTED_PAYLOADS * FEC_N + FEC_K - 1) / FEC_K))}
PAYLOAD_LEN=${PAYLOAD_LEN:-1000}
PAYLOAD_MARKER=${PAYLOAD_MARKER:-RFQCLSEF}
PAYLOAD_INTERVAL_SEC=${PAYLOAD_INTERVAL_SEC:-0.0005}

LINK_ID=${LINK_ID:-0x000001}
RADIO_PORT=${RADIO_PORT:-0}
RADIO_PORT_HEX=${RADIO_PORT_HEX:-0x00}
TX_RATE=${TX_RATE:-mcs1}
TX_PROFILE=${TX_PROFILE:-linux-monitor}
TX_POWER_MODE=${TX_POWER_MODE:-efuse-derived}
TX_POWER_SAFETY_PROFILE=${TX_POWER_SAFETY_PROFILE:-linux-ch36-ht20}
CALIBRATION_MODE=${CALIBRATION_MODE:-stop-gap-captured}
PROFILE_KIND=${PROFILE_KIND:-close-range}
PROFILE_NAME=${PROFILE_NAME:-close-range-ch36-ht20-efuse-$RUN_ID}

RELAY_BIND_IP=${RELAY_BIND_IP:-$MAC_LAN_IP}
RELAY_PORT=${RELAY_PORT:-5610}
BRIDGE_BIND_HOST=${BRIDGE_BIND_HOST:-127.0.0.1}
BRIDGE_BIND_PORT=${BRIDGE_BIND_PORT:-5611}
BRIDGE_START_DELAY=${BRIDGE_START_DELAY:-20}
BRIDGE_IDLE_TIMEOUT_MS=${BRIDGE_IDLE_TIMEOUT_MS:-60000}
BRIDGE_WAIT_SECONDS=${BRIDGE_WAIT_SECONDS:-140}

IFACE=${IFACE:-wfb0}
WFB_SERVICE=${WFB_SERVICE:-arc-wfb-link-1}
WFB_KEY=${WFB_KEY:-/var/lib/arc/wfb/drone.key}
LINUX_SOURCE_PORT=${LINUX_SOURCE_PORT:-5600}
LINUX_RX_PORT=${LINUX_RX_PORT:-5800}
TCPDUMP_SECONDS=${TCPDUMP_SECONDS:-95}
RX_SECONDS=${RX_SECONDS:-95}
TX_SECONDS=${TX_SECONDS:-85}
COUNTER_SECONDS=${COUNTER_SECONDS:-95}

FIRMWARE=${FIRMWARE:-/tmp/rtl8812aefw.bin}
EFUSE_REPORT=${EFUSE_REPORT:-/tmp/wfb-remote-macos-efuse-dump.json}
LINUX_BASELINE=${LINUX_BASELINE:-fixtures/rf-quality/current-close-range-20mhz-linux-baseline.json}

require_nonempty() {
  local name=$1
  [[ -n "${!name}" ]] || die "$name is required"
}

for setting in HW_MAC_HOST HW_REPO_PATH LINUX_HOST MAC_LAN_IP FIRMWARE EFUSE_REPORT LINUX_BASELINE; do
  require_nonempty "$setting"
done

if (( PAYLOAD_LEN < ${#PAYLOAD_MARKER} + 4 )); then
  die "PAYLOAD_LEN must be at least marker length + 4 sequence bytes"
fi

if (( DRY_RUN == 0 )); then
  require_command ssh
  require_command scp
  require_command python3
  require_command cargo
  if [[ "$HW_DEPLOY" == "1" ]]; then
    require_command rsync
  fi
  [[ -f "$LINUX_BASELINE" ]] || die "Linux baseline not found: $LINUX_BASELINE"
fi

mkdir -p "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd)
MISSING_ARTIFACTS="$OUT_DIR/missing-artifacts.txt"
: >"$MISSING_ARTIFACTS"

export RUN_ID HW_MAC_HOST HW_REPO_PATH LINUX_HOST MAC_LAN_IP REMOTE_PREFIX
export CHANNEL BANDWIDTH_MHZ FEC_K FEC_N EXPECTED_PAYLOADS MAX_DATAGRAMS
export PAYLOAD_LEN PAYLOAD_MARKER PAYLOAD_INTERVAL_SEC LINK_ID RADIO_PORT RADIO_PORT_HEX
export TX_RATE TX_PROFILE TX_POWER_MODE TX_POWER_SAFETY_PROFILE CALIBRATION_MODE PROFILE_KIND PROFILE_NAME
export RELAY_BIND_IP RELAY_PORT BRIDGE_BIND_HOST BRIDGE_BIND_PORT BRIDGE_START_DELAY BRIDGE_IDLE_TIMEOUT_MS BRIDGE_WAIT_SECONDS
export IFACE WFB_SERVICE WFB_KEY LINUX_SOURCE_PORT LINUX_RX_PORT TCPDUMP_SECONDS RX_SECONDS TX_SECONDS COUNTER_SECONDS
export FIRMWARE EFUSE_REPORT LINUX_BASELINE OUT_DIR SYNC_HW_REPO HW_DEPLOY HW_DEPLOY_PATH ALLOW_DEPLOY_OVER_WORKTREE

normalize_remote_path_for_guard() {
  local path=$1
  path=${path%/}
  case "$path" in
    "~/"*) path=${path#~/} ;;
  esac
  printf '%s' "$path"
}

deploy_hw_repo() {
  local repo_guard deploy_guard remote_cmd rsync_target
  repo_guard=$(normalize_remote_path_for_guard "$HW_REPO_PATH")
  deploy_guard=$(normalize_remote_path_for_guard "$HW_DEPLOY_PATH")
  if [[ "$repo_guard" == "$deploy_guard" && "$ALLOW_DEPLOY_OVER_WORKTREE" != "1" ]]; then
    die "HW_DEPLOY_PATH matches HW_REPO_PATH; set a separate deploy path or ALLOW_DEPLOY_OVER_WORKTREE=1"
  fi

  remote_cmd="$(env_assignments HW_DEPLOY_PATH) bash -s"
  log "creating hardware-Mac deploy directory: $HW_DEPLOY_PATH"
  ssh "$HW_MAC_HOST" "$remote_cmd" <<'MAC_DEPLOY_DIR'
set -euo pipefail
repo=$HW_DEPLOY_PATH
case "$repo" in
  "~/"*) repo="$HOME/${repo#~/}" ;;
  /*) ;;
  *) repo="$HOME/$repo" ;;
esac
mkdir -p "$repo"
MAC_DEPLOY_DIR

  rsync_target="${HW_MAC_HOST}:${HW_DEPLOY_PATH%/}/"
  log "deploying local checkout to hardware Mac: $rsync_target"
  rsync -az --delete \
    --exclude '.git/' \
    --exclude 'target/' \
    --exclude '.DS_Store' \
    --exclude '.direnv/' \
    "$REPO_ROOT/" "$rsync_target"
  HW_REPO_PATH=$HW_DEPLOY_PATH
  export HW_REPO_PATH
}

write_config() {
  python3 - "$OUT_DIR/run-config.json" <<'PY'
import json
import os
import sys

keys = [
    "RUN_ID", "HW_MAC_HOST", "HW_REPO_PATH", "LINUX_HOST", "MAC_LAN_IP",
    "REMOTE_PREFIX", "CHANNEL", "BANDWIDTH_MHZ", "FEC_K", "FEC_N",
    "EXPECTED_PAYLOADS", "MAX_DATAGRAMS", "PAYLOAD_LEN", "PAYLOAD_MARKER",
    "LINK_ID", "RADIO_PORT", "TX_RATE", "TX_PROFILE", "TX_POWER_MODE",
    "TX_POWER_SAFETY_PROFILE", "CALIBRATION_MODE", "PROFILE_KIND",
    "PROFILE_NAME", "RELAY_BIND_IP", "RELAY_PORT", "BRIDGE_BIND_HOST",
    "BRIDGE_BIND_PORT", "IFACE", "WFB_SERVICE", "WFB_KEY",
    "LINUX_SOURCE_PORT", "LINUX_RX_PORT", "FIRMWARE", "EFUSE_REPORT",
    "LINUX_BASELINE", "SYNC_HW_REPO", "HW_DEPLOY", "HW_DEPLOY_PATH",
    "ALLOW_DEPLOY_OVER_WORKTREE",
]
config = {key.lower(): os.environ.get(key, "") for key in keys}
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(config, fh, indent=2)
    fh.write("\n")
PY
}

print_dry_run() {
  local dry_bridge_path=$HW_REPO_PATH
  if [[ "$HW_DEPLOY" == "1" ]]; then
    dry_bridge_path=$HW_DEPLOY_PATH
  fi
  write_config
  cat <<EOF
Dry run only. No remote processes will be started and no RF will be transmitted.

Configuration written to:
  $OUT_DIR/run-config.json

Hardware Mac:
  HW_DEPLOY=$HW_DEPLOY HW_DEPLOY_PATH=$HW_DEPLOY_PATH SYNC_HW_REPO=$SYNC_HW_REPO
  $(if [[ "$HW_DEPLOY" == "1" ]]; then printf 'rsync local checkout to %s:%s\n' "$HW_MAC_HOST" "$HW_DEPLOY_PATH"; else printf 'no local deploy sync\n'; fi)
  ssh $(quote "$HW_MAC_HOST") '<start UDP relay $RELAY_BIND_IP:$RELAY_PORT -> $BRIDGE_BIND_HOST:$BRIDGE_BIND_PORT>'
  ssh $(quote "$HW_MAC_HOST") '<cd $dry_bridge_path && cargo run ... bridge-tx-listen --macos-usbhost --channel $CHANNEL --bandwidth $BANDWIDTH_MHZ --bind $BRIDGE_BIND_HOST:$BRIDGE_BIND_PORT --max-datagrams $MAX_DATAGRAMS>'

Linux peer through hardware Mac:
  ssh $(quote "$HW_MAC_HOST") 'ssh $(quote "$LINUX_HOST") <stop $WFB_SERVICE; iw dev $IFACE set channel $CHANNEL HT${BANDWIDTH_MHZ}; start tcpdump/wfb_rx/wfb_tx; generate $EXPECTED_PAYLOADS payloads>'

Local collection:
  scp hardware Mac reports from $REMOTE_PREFIX-*
  stream Linux artifacts through nested ssh via $HW_MAC_HOST
  cargo run -p wfb-radio-diag -- --json --report $OUT_DIR/rf-quality-report.json rf-quality-report ...
EOF
}

if (( DRY_RUN == 1 )); then
  print_dry_run
  exit 0
fi

if [[ "$HW_DEPLOY" == "1" ]]; then
  deploy_hw_repo
  export HW_REPO_PATH
fi

write_config
log "output directory: $OUT_DIR"
log "remote artifact prefix: $REMOTE_PREFIX"

STARTED_RELAY=0
STARTED_BRIDGE=0
STARTED_LINUX=0
CLEANUP_ACTIVE=0

ssh_hw() {
  ssh "$HW_MAC_HOST" "$@"
}

ssh_linux_via_hw() {
  local inner
  inner="$1"
  ssh "$HW_MAC_HOST" "ssh $(quote "$LINUX_HOST") $inner"
}

stop_hw_pid_file() {
  local pid_file=$1
  local label=$2
  ssh "$HW_MAC_HOST" "pid_file=$(quote "$pid_file"); label=$(quote "$label"); if [[ -f \"\$pid_file\" ]]; then pid=\$(cat \"\$pid_file\" 2>/dev/null || true); if [[ -n \"\$pid\" ]]; then kill \"\$pid\" >/dev/null 2>&1 || true; fi; fi" \
    >"$OUT_DIR/cleanup-$label.log" 2>&1 || true
}

restore_linux_peer() {
  local remote_cmd
  remote_cmd="$(env_assignments WFB_SERVICE LINUX_SOURCE_PORT LINUX_RX_PORT MAC_LAN_IP RELAY_PORT IFACE) bash -s"
  ssh "$HW_MAC_HOST" "ssh $(quote "$LINUX_HOST") $remote_cmd" >"$OUT_DIR/cleanup-linux-restore.log" 2>&1 <<'LINUX_RESTORE' || true
set +e
sudo -n pkill -f "wfb_tx .* -u ${LINUX_SOURCE_PORT} ${MAC_LAN_IP}:${RELAY_PORT}" >/dev/null 2>&1 || true
sudo -n pkill -f "wfb_rx .* -u ${LINUX_RX_PORT} ${IFACE}" >/dev/null 2>&1 || true
sudo -n docker restart "$WFB_SERVICE" >/dev/null 2>&1 || sudo -n docker start "$WFB_SERVICE" >/dev/null 2>&1 || true
sudo -n docker ps --filter "name=$WFB_SERVICE" --format '{{.Names}} {{.Status}}'
LINUX_RESTORE
}

cleanup() {
  local status=$?
  if (( CLEANUP_ACTIVE == 1 )); then
    exit "$status"
  fi
  CLEANUP_ACTIVE=1
  if (( STARTED_BRIDGE == 1 )); then
    stop_hw_pid_file "$REMOTE_PREFIX-bridge.pid" bridge
  fi
  if (( STARTED_RELAY == 1 )); then
    stop_hw_pid_file "$REMOTE_PREFIX-relay.pid" relay
  fi
  if (( STARTED_LINUX == 1 )); then
    restore_linux_peer
  fi
  exit "$status"
}
trap cleanup EXIT INT TERM

start_relay() {
  local remote_cmd
  remote_cmd="$(env_assignments REMOTE_PREFIX RELAY_BIND_IP RELAY_PORT BRIDGE_BIND_HOST BRIDGE_BIND_PORT) bash -s"
  log "starting hardware-Mac UDP relay"
  ssh "$HW_MAC_HOST" "$remote_cmd" <<'MAC_RELAY'
set -euo pipefail
cat > "${REMOTE_PREFIX}-relay.py" <<'PY'
import socket
import sys
import time

bind_ip = sys.argv[1]
bind_port = int(sys.argv[2])
dst_ip = sys.argv[3]
dst_port = int(sys.argv[4])
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind((bind_ip, bind_port))
out = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
count = 0
started = time.time()
print(f"relay {bind_ip}:{bind_port} -> {dst_ip}:{dst_port}", flush=True)
while True:
    data, addr = sock.recvfrom(65535)
    out.sendto(data, (dst_ip, dst_port))
    count += 1
    if count == 1 or count % 250 == 0:
        elapsed = max(time.time() - started, 0.001)
        print(f"forwarded={count} from={addr[0]}:{addr[1]} rate={count/elapsed:.2f}/s", flush=True)
PY
nohup python3 "${REMOTE_PREFIX}-relay.py" "$RELAY_BIND_IP" "$RELAY_PORT" "$BRIDGE_BIND_HOST" "$BRIDGE_BIND_PORT" > "${REMOTE_PREFIX}-relay.log" 2>&1 &
pid=$!
echo "$pid" > "${REMOTE_PREFIX}-relay.pid"
sleep 1
kill -0 "$pid"
MAC_RELAY
  STARTED_RELAY=1
}

start_bridge() {
  local remote_cmd
  remote_cmd="$(env_assignments REMOTE_PREFIX HW_REPO_PATH SYNC_HW_REPO FIRMWARE CHANNEL BANDWIDTH_MHZ BRIDGE_BIND_HOST BRIDGE_BIND_PORT MAX_DATAGRAMS BRIDGE_IDLE_TIMEOUT_MS TX_POWER_MODE EFUSE_REPORT TX_POWER_SAFETY_PROFILE) bash -s"
  log "starting hardware-Mac bridge listener"
  ssh "$HW_MAC_HOST" "$remote_cmd" <<'MAC_BRIDGE'
set -euo pipefail
repo=$HW_REPO_PATH
case "$repo" in
  "~/"*) repo="$HOME/${repo#~/}" ;;
  /*) ;;
  *) repo="$HOME/$repo" ;;
esac
if [[ "$SYNC_HW_REPO" == "1" ]]; then
  git -C "$repo" pull --ff-only
fi
cd "$repo"
nohup cargo run -p wfb-radio-diag -- --json \
  --report "${REMOTE_PREFIX}-listen.json" \
  bridge-tx-listen \
  --macos-usbhost \
  --vid 0x0bda --pid 0x8812 \
  --init-before-tx \
  --firmware "$FIRMWARE" \
  --channel "$CHANNEL" --bandwidth "$BANDWIDTH_MHZ" \
  --bind "${BRIDGE_BIND_HOST}:${BRIDGE_BIND_PORT}" \
  --max-datagrams "$MAX_DATAGRAMS" \
  --idle-timeout-ms "$BRIDGE_IDLE_TIMEOUT_MS" \
  --tx-power-mode "$TX_POWER_MODE" \
  --tx-power-efuse-report "$EFUSE_REPORT" \
  --tx-power-safety-profile "$TX_POWER_SAFETY_PROFILE" \
  --i-understand-this-transmits \
  > "${REMOTE_PREFIX}-bridge.log" 2>&1 &
pid=$!
echo "$pid" > "${REMOTE_PREFIX}-bridge.pid"
sleep 2
kill -0 "$pid"
MAC_BRIDGE
  STARTED_BRIDGE=1
}

run_linux_peer() {
  local remote_cmd
  remote_cmd="$(env_assignments REMOTE_PREFIX IFACE CHANNEL BANDWIDTH_MHZ WFB_SERVICE WFB_KEY RADIO_PORT FEC_K FEC_N LINUX_SOURCE_PORT LINUX_RX_PORT MAC_LAN_IP RELAY_PORT EXPECTED_PAYLOADS PAYLOAD_LEN PAYLOAD_MARKER PAYLOAD_INTERVAL_SEC TCPDUMP_SECONDS RX_SECONDS TX_SECONDS COUNTER_SECONDS) bash -s"
  log "running Linux peer sender/receiver through $HW_MAC_HOST -> $LINUX_HOST"
  STARTED_LINUX=1
  ssh "$HW_MAC_HOST" "ssh $(quote "$LINUX_HOST") $remote_cmd" <<'LINUX_RUN'
set -euo pipefail

setup_log="${REMOTE_PREFIX}-setup.log"
restore_log="${REMOTE_PREFIX}-restore.log"
summary_json="${REMOTE_PREFIX}-summary.json"
counter_json="${REMOTE_PREFIX}-counter.json"

case "$BANDWIDTH_MHZ" in
  20) iw_bandwidth=HT20 ;;
  40) iw_bandwidth=HT40+ ;;
  *) echo "unsupported BANDWIDTH_MHZ=$BANDWIDTH_MHZ" > "$setup_log"; exit 2 ;;
esac

cleanup_linux() {
  set +e
  sudo -n pkill -f "wfb_tx .* -u ${LINUX_SOURCE_PORT} ${MAC_LAN_IP}:${RELAY_PORT}" >/dev/null 2>&1 || true
  sudo -n pkill -f "wfb_rx .* -u ${LINUX_RX_PORT} ${IFACE}" >/dev/null 2>&1 || true
  sudo -n pkill -f "tcpdump -i ${IFACE} .*${REMOTE_PREFIX}-rf.pcap" >/dev/null 2>&1 || true
  if [[ -n "${counter_pid:-}" ]]; then kill "$counter_pid" >/dev/null 2>&1 || true; fi
  {
    date
    sudo -n docker start "$WFB_SERVICE" || true
    sudo -n docker ps --filter "name=$WFB_SERVICE" --format '{{.Names}} {{.Status}}'
    ps -eo pid,user,comm,args | grep -Ei 'arc-wfb|wfb' | grep -v grep || true
  } > "$restore_log" 2>&1
}
trap cleanup_linux EXIT INT TERM

rm -f "${REMOTE_PREFIX}"-{setup,restore,summary,counter,source,rx,tx,tcpdump}.log \
  "${REMOTE_PREFIX}"-{summary,counter}.json \
  "${REMOTE_PREFIX}-rf.pcap"

{
  date
  sudo -n docker stop "$WFB_SERVICE" || true
  sudo -n iw dev "$IFACE" set channel "$CHANNEL" "$iw_bandwidth"
  iw dev "$IFACE" info || true
  ip addr show "$IFACE" || true
} > "$setup_log" 2>&1

python3 - "$counter_json" "$LINUX_RX_PORT" "$EXPECTED_PAYLOADS" "$PAYLOAD_MARKER" "$COUNTER_SECONDS" <<'PY' &
import json
import socket
import sys
import time

out_path = sys.argv[1]
port = int(sys.argv[2])
expected = int(sys.argv[3])
marker = sys.argv[4].encode("ascii")
deadline = time.monotonic() + float(sys.argv[5])
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", port))
sock.settimeout(1.0)
unique = set()
total = 0
payload_bytes = 0
first_seq = None
last_seq = None
started = time.time()
while time.monotonic() < deadline and len(unique) < expected:
    try:
        data, _ = sock.recvfrom(65535)
    except socket.timeout:
        continue
    total += 1
    payload_bytes += len(data)
    if data.startswith(marker) and len(data) >= len(marker) + 4:
        seq = int.from_bytes(data[len(marker):len(marker) + 4], "big")
        unique.add(seq)
        first_seq = seq if first_seq is None else min(first_seq, seq)
        last_seq = seq if last_seq is None else max(last_seq, seq)
report = {
    "expected_payloads": expected,
    "recovered_payloads": len(unique),
    "total_datagrams": total,
    "payload_bytes": payload_bytes,
    "first_sequence": first_seq,
    "last_sequence": last_seq,
    "elapsed_seconds": time.time() - started,
}
with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2)
    fh.write("\n")
PY
counter_pid=$!

sudo -n timeout "$TCPDUMP_SECONDS" tcpdump -i "$IFACE" -s 256 -w "${REMOTE_PREFIX}-rf.pcap" > "${REMOTE_PREFIX}-tcpdump.log" 2>&1 &
tcpdump_pid=$!

sudo -n timeout "$RX_SECONDS" \
  wfb_rx -K "$WFB_KEY" -p "$RADIO_PORT" -c 127.0.0.1 -u "$LINUX_RX_PORT" "$IFACE" \
  > "${REMOTE_PREFIX}-rx.log" 2>&1 &
rx_pid=$!

sleep 2

sudo -n timeout "$TX_SECONDS" \
  wfb_tx -d -K "$WFB_KEY" -p "$RADIO_PORT" -B "$BANDWIDTH_MHZ" -k "$FEC_K" -n "$FEC_N" \
  -u "$LINUX_SOURCE_PORT" "${MAC_LAN_IP}:${RELAY_PORT}" \
  > "${REMOTE_PREFIX}-tx.log" 2>&1 &
tx_pid=$!

sleep 2

python3 - "$LINUX_SOURCE_PORT" "$EXPECTED_PAYLOADS" "$PAYLOAD_LEN" "$PAYLOAD_MARKER" "$PAYLOAD_INTERVAL_SEC" <<'PY' > "${REMOTE_PREFIX}-source.log" 2>&1
import socket
import sys
import time

port = int(sys.argv[1])
count = int(sys.argv[2])
payload_len = int(sys.argv[3])
marker = sys.argv[4].encode("ascii")
interval = float(sys.argv[5])
prefix_len = len(marker) + 4
if payload_len < prefix_len:
    raise SystemExit("payload too short")
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
for i in range(count):
    fill = bytes([65 + (i % 26)]) * (payload_len - prefix_len)
    payload = marker + i.to_bytes(4, "big") + fill
    sock.sendto(payload, ("127.0.0.1", port))
    if interval > 0:
        time.sleep(interval)
print(f"sent={count} payload_len={payload_len}")
PY

wait "$tx_pid" || true
wait "$counter_pid" || true
sudo -n kill "$rx_pid" >/dev/null 2>&1 || true
sudo -n kill "$tcpdump_pid" >/dev/null 2>&1 || true
wait "$rx_pid" >/dev/null 2>&1 || true
wait "$tcpdump_pid" >/dev/null 2>&1 || true

python3 - "$summary_json" "$counter_json" "$setup_log" "$restore_log" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
counter_path = Path(sys.argv[2])
counter = {}
if counter_path.exists():
    counter = json.loads(counter_path.read_text())
summary = {
    "counter": counter,
    "artifacts": {
        "counter": str(counter_path),
        "setup_log": sys.argv[3],
        "restore_log": sys.argv[4],
    },
}
summary_path.write_text(json.dumps(summary, indent=2) + "\n")
PY
LINUX_RUN
}

wait_for_bridge() {
  local remote_cmd
  remote_cmd="$(env_assignments REMOTE_PREFIX BRIDGE_WAIT_SECONDS) bash -s"
  log "waiting for bridge listener to finish"
  if ! ssh "$HW_MAC_HOST" "$remote_cmd" >"$OUT_DIR/bridge-wait.log" 2>&1 <<'MAC_WAIT'
set -euo pipefail
pid_file="${REMOTE_PREFIX}-bridge.pid"
if [[ ! -f "$pid_file" ]]; then
  echo "missing bridge pid file: $pid_file"
  exit 2
fi
pid=$(cat "$pid_file")
for ((i = 0; i < BRIDGE_WAIT_SECONDS; i++)); do
  if ! kill -0 "$pid" >/dev/null 2>&1; then
    echo "bridge exited after ${i}s"
    exit 0
  fi
  sleep 1
done
echo "bridge still running after ${BRIDGE_WAIT_SECONDS}s; terminating"
kill "$pid" >/dev/null 2>&1 || true
exit 124
MAC_WAIT
  then
    log "bridge did not finish cleanly; continuing to collect artifacts"
  fi
}

copy_hw_artifact() {
  local remote_path=$1
  local name=${2:-$(basename "$remote_path")}
  if scp -q "$HW_MAC_HOST:$remote_path" "$OUT_DIR/$name" >/dev/null 2>&1; then
    log "collected hardware Mac artifact: $name"
  else
    printf 'hardware-mac:%s\n' "$remote_path" >>"$MISSING_ARTIFACTS"
  fi
}

copy_linux_artifact() {
  local remote_path=$1
  local name=${2:-$(basename "$remote_path")}
  if ssh "$HW_MAC_HOST" "ssh $(quote "$LINUX_HOST") cat $(quote "$remote_path")" >"$OUT_DIR/$name" 2>/dev/null; then
    log "collected Linux artifact: $name"
  else
    rm -f "$OUT_DIR/$name"
    printf 'linux:%s\n' "$remote_path" >>"$MISSING_ARTIFACTS"
  fi
}

collect_artifacts() {
  log "collecting artifacts"
  copy_hw_artifact "${REMOTE_PREFIX}-listen.json"
  copy_hw_artifact "${REMOTE_PREFIX}-bridge.log"
  copy_hw_artifact "${REMOTE_PREFIX}-relay.log"
  copy_hw_artifact "$EFUSE_REPORT" "$(basename "$EFUSE_REPORT")"

  copy_linux_artifact "${REMOTE_PREFIX}-rf.pcap"
  copy_linux_artifact "${REMOTE_PREFIX}-rx.log"
  copy_linux_artifact "${REMOTE_PREFIX}-tx.log"
  copy_linux_artifact "${REMOTE_PREFIX}-counter.json"
  copy_linux_artifact "${REMOTE_PREFIX}-source.log"
  copy_linux_artifact "${REMOTE_PREFIX}-setup.log"
  copy_linux_artifact "${REMOTE_PREFIX}-restore.log"
  copy_linux_artifact "${REMOTE_PREFIX}-summary.json"
  copy_linux_artifact "${REMOTE_PREFIX}-tcpdump.log"
}

counter_recovered() {
  local path=$1
  python3 - "$path" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
if not path.exists():
    raise SystemExit(1)
print(json.loads(path.read_text()).get("recovered_payloads", ""))
PY
}

generate_report() {
  if (( SKIP_REPORT == 1 )); then
    log "skipping rf-quality-report (--skip-report)"
    return 0
  fi

  local mac_report
  local counter_report
  local efuse_report
  local recovered
  local receiver_args=()

  mac_report="$OUT_DIR/$(basename "${REMOTE_PREFIX}-listen.json")"
  counter_report="$OUT_DIR/$(basename "${REMOTE_PREFIX}-counter.json")"
  efuse_report="$OUT_DIR/$(basename "$EFUSE_REPORT")"

  if [[ ! -f "$mac_report" || ! -f "$counter_report" || ! -f "$efuse_report" ]]; then
    {
      echo "rf-quality-report skipped because required inputs are missing"
      [[ -f "$mac_report" ]] || echo "missing: $mac_report"
      [[ -f "$counter_report" ]] || echo "missing: $counter_report"
      [[ -f "$efuse_report" ]] || echo "missing: $efuse_report"
    } > "$OUT_DIR/rf-quality-report-skipped.txt"
    log "rf-quality-report skipped; see rf-quality-report-skipped.txt"
    return 0
  fi

  recovered=$(counter_recovered "$counter_report")
  [[ -n "$recovered" ]] || die "counter report did not include recovered_payloads"

  for artifact in \
    "$OUT_DIR/$(basename "${REMOTE_PREFIX}-rf.pcap")" \
    "$OUT_DIR/$(basename "${REMOTE_PREFIX}-rx.log")" \
    "$OUT_DIR/$(basename "${REMOTE_PREFIX}-tx.log")" \
    "$counter_report"; do
    [[ -f "$artifact" ]] && receiver_args+=(--receiver-artifact "$artifact")
  done

  log "generating rf-quality report with recovered_payloads=$recovered"
  cargo run -p wfb-radio-diag -- --json \
    --report "$OUT_DIR/rf-quality-report.json" \
    rf-quality-report \
    --profile-name "$PROFILE_NAME" \
    --profile-kind "$PROFILE_KIND" \
    --channel "$CHANNEL" --bandwidth "$BANDWIDTH_MHZ" \
    --tx-rate "$TX_RATE" \
    --tx-profile "$TX_PROFILE" \
    --tx-power-mode "$TX_POWER_MODE" \
    --calibration-mode "$CALIBRATION_MODE" \
    --wfb-link-id "$LINK_ID" \
    --wfb-radio-port "$RADIO_PORT_HEX" \
    --fec-k "$FEC_K" --fec-n "$FEC_N" \
    --payload-len "$PAYLOAD_LEN" \
    --expected-payloads "$EXPECTED_PAYLOADS" \
    --recovered-payloads "$recovered" \
    --mac-report "$mac_report" \
    --efuse-report "$efuse_report" \
    --linux-baseline "$LINUX_BASELINE" \
    "${receiver_args[@]}"
}

start_relay
start_bridge
log "waiting ${BRIDGE_START_DELAY}s before Linux traffic"
sleep "$BRIDGE_START_DELAY"
run_linux_peer
STARTED_LINUX=0
wait_for_bridge
stop_hw_pid_file "$REMOTE_PREFIX-relay.pid" relay
STARTED_RELAY=0
collect_artifacts
generate_report

STARTED_BRIDGE=0
STARTED_LINUX=0
trap - EXIT INT TERM
log "done: $OUT_DIR"
