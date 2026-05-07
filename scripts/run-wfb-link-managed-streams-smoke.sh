#!/usr/bin/env bash
# shellcheck disable=SC2029
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-wfb-link-managed-streams-smoke.sh

Runs a receiver-backed managed raw application multi-stream smoke:

  Linux -> Mac video-down:      remote wfb_tx -> RF -> ManagedWfbStreamsBackend -> local UDP counter
  Linux -> Mac telemetry-down:  remote wfb_tx -> RF -> ManagedWfbStreamsBackend -> local UDP counter
  Mac -> Linux control-up:      local UDP source -> ManagedWfbStreamsBackend -> RF -> remote wfb_rx/counter

Common overrides:

  WFB_KEY=/path/to/gs.key
  LINUX_HOST=pi@drone-2f389.local
  LINUX_WFB_KEY=/var/lib/arc/wfb/drone.key
  IFACE=wfb0 CHANNEL=36 LINK_ID=0x000001
  VIDEO_EXPECTED_PAYLOADS=80 TELEMETRY_EXPECTED_PAYLOADS=40 CONTROL_EXPECTED_PAYLOADS=40
  SOURCE_WARMUP_PAYLOADS=20
  VIDEO_MCS=0 VIDEO_FEC_K=2 VIDEO_FEC_N=16 VIDEO_INTERVAL_SEC=0.040
  TELEMETRY_MCS=0 TELEMETRY_FEC_K=2 TELEMETRY_FEC_N=16 TELEMETRY_INTERVAL_SEC=0.080
  CONTROL_MCS=0 CONTROL_FEC_K=2 CONTROL_FEC_N=16 CONTROL_INTERVAL_SEC=0.080
  OUT_DIR=/tmp/wfb-link-managed-streams-smoke

Set PREPARE_LINUX_PEER=0 only when the peer is already in monitor mode on the
correct channel.
EOF
}

log() {
  printf '[managed-streams-smoke] %s\n' "$*" >&2
}

die() {
  printf '[managed-streams-smoke] error: %s\n' "$*" >&2
  exit 1
}

quote() {
  printf '%q' "$1"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

require_file() {
  local path=$1 label=$2
  [[ -e "$path" ]] || die "missing $label: $path"
}

is_truthy() {
  case "${1:-}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  "")
    ;;
  *)
    die "unknown argument: $1"
    ;;
esac

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
OUT_DIR=${OUT_DIR:-/tmp/wfb-link-managed-streams-smoke-$RUN_ID}
REMOTE_PREFIX=${REMOTE_PREFIX:-/tmp/wfb-link-managed-streams-smoke-$RUN_ID-peer}

RADIO_CONFIG=${RADIO_CONFIG:-$REPO_ROOT/configs/radio-run-video-control-tdd.toml}
WFB_KEY=${WFB_KEY:-}
WFB_TX_BIN=${WFB_TX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_tx}
WFB_RX_BIN=${WFB_RX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_rx}
WFB_LINK_READY_TIMEOUT_S=${WFB_LINK_READY_TIMEOUT_S:-90}
WFB_LINK_HOLD_SECONDS=${WFB_LINK_HOLD_SECONDS:-35}

LINUX_HOST=${LINUX_HOST:-pi@drone-2f389.local}
LINUX_REMOTE_PATH=${LINUX_REMOTE_PATH:-/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin}
SSH_OPTS=${SSH_OPTS:-"-o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=2"}
# shellcheck disable=SC2206
SSH_OPTS_ARRAY=($SSH_OPTS)
IFACE=${IFACE:-wfb0}
WFB_SERVICE=${WFB_SERVICE:-arc-wfb-link-1}
LINUX_WFB_KEY=${LINUX_WFB_KEY:-/var/lib/arc/wfb/drone.key}
PREPARE_LINUX_PEER=${PREPARE_LINUX_PEER:-1}
RESTART_WFB_SERVICE_ON_CLEANUP=${RESTART_WFB_SERVICE_ON_CLEANUP:-1}

CHANNEL=${CHANNEL:-36}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
LINK_ID=${LINK_ID:-0x000001}
WFB_CLI_LINK_ID=${WFB_CLI_LINK_ID:-$(printf '%d' "$((LINK_ID))")}
COUNTER_SECONDS=${COUNTER_SECONDS:-25}
COUNTER_SETTLE_SECONDS=${COUNTER_SETTLE_SECONDS:-2}
HELPER_SETTLE_SECONDS=${HELPER_SETTLE_SECONDS:-1}
PAYLOAD_LEN=${PAYLOAD_LEN:-1000}
SOURCE_WARMUP_PAYLOADS=${SOURCE_WARMUP_PAYLOADS:-20}

VIDEO_RADIO_PORT=${VIDEO_RADIO_PORT:-4}
TELEMETRY_RADIO_PORT=${TELEMETRY_RADIO_PORT:-5}
CONTROL_RADIO_PORT=${CONTROL_RADIO_PORT:-6}

VIDEO_APP_HOST=${VIDEO_APP_HOST:-127.0.0.1}
TELEMETRY_APP_HOST=${TELEMETRY_APP_HOST:-127.0.0.1}
CONTROL_APP_HOST=${CONTROL_APP_HOST:-127.0.0.1}
VIDEO_APP_PORT=${VIDEO_APP_PORT:-5804}
TELEMETRY_APP_PORT=${TELEMETRY_APP_PORT:-5805}
CONTROL_APP_PORT=${CONTROL_APP_PORT:-5606}

VIDEO_SOURCE_PORT=${VIDEO_SOURCE_PORT:-5624}
TELEMETRY_SOURCE_PORT=${TELEMETRY_SOURCE_PORT:-5625}
CONTROL_COUNTER_PORT=${CONTROL_COUNTER_PORT:-5906}

VIDEO_EXPECTED_PAYLOADS=${VIDEO_EXPECTED_PAYLOADS:-80}
TELEMETRY_EXPECTED_PAYLOADS=${TELEMETRY_EXPECTED_PAYLOADS:-40}
CONTROL_EXPECTED_PAYLOADS=${CONTROL_EXPECTED_PAYLOADS:-40}
VIDEO_MIN_UNIQUE=${VIDEO_MIN_UNIQUE:-$VIDEO_EXPECTED_PAYLOADS}
TELEMETRY_MIN_UNIQUE=${TELEMETRY_MIN_UNIQUE:-$TELEMETRY_EXPECTED_PAYLOADS}
CONTROL_MIN_UNIQUE=${CONTROL_MIN_UNIQUE:-$CONTROL_EXPECTED_PAYLOADS}

VIDEO_MARKER=${VIDEO_MARKER:-MSVID001}
TELEMETRY_MARKER=${TELEMETRY_MARKER:-MSTEL001}
CONTROL_MARKER=${CONTROL_MARKER:-MSCTL001}
VIDEO_WARMUP_MARKER=${VIDEO_WARMUP_MARKER:-MSVIDWRM}
TELEMETRY_WARMUP_MARKER=${TELEMETRY_WARMUP_MARKER:-MSTELWRM}
CONTROL_WARMUP_MARKER=${CONTROL_WARMUP_MARKER:-MSCTLWRM}

VIDEO_INTERVAL_SEC=${VIDEO_INTERVAL_SEC:-0.040}
TELEMETRY_INTERVAL_SEC=${TELEMETRY_INTERVAL_SEC:-0.080}
CONTROL_INTERVAL_SEC=${CONTROL_INTERVAL_SEC:-0.080}

VIDEO_MCS=${VIDEO_MCS:-0}
VIDEO_FEC_K=${VIDEO_FEC_K:-2}
VIDEO_FEC_N=${VIDEO_FEC_N:-16}
TELEMETRY_MCS=${TELEMETRY_MCS:-0}
TELEMETRY_FEC_K=${TELEMETRY_FEC_K:-2}
TELEMETRY_FEC_N=${TELEMETRY_FEC_N:-16}
CONTROL_MCS=${CONTROL_MCS:-0}
CONTROL_FEC_K=${CONTROL_FEC_K:-2}
CONTROL_FEC_N=${CONTROL_FEC_N:-16}

MAX_VIDEO_DECRYPT_FAILURES=${MAX_VIDEO_DECRYPT_FAILURES:-0}
MAX_TELEMETRY_DECRYPT_FAILURES=${MAX_TELEMETRY_DECRYPT_FAILURES:-0}
MAX_CONTROL_DECRYPT_FAILURES=${MAX_CONTROL_DECRYPT_FAILURES:-0}

MANAGED_PID=
REMOTE_SOURCE_PID=
PEER_FETCHED=0
local_pids=()

cleanup() {
  set +e
  for pid in "${local_pids[@]:-}"; do
    kill "$pid" >/dev/null 2>&1 || true
  done
  if [[ -n "${REMOTE_SOURCE_PID:-}" ]]; then
    kill "$REMOTE_SOURCE_PID" >/dev/null 2>&1 || true
    wait "$REMOTE_SOURCE_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${MANAGED_PID:-}" ]]; then
    kill "$MANAGED_PID" >/dev/null 2>&1 || true
    wait "$MANAGED_PID" >/dev/null 2>&1 || true
  fi
  fetch_peer_artifacts >/dev/null 2>&1 || true
  ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "REMOTE_PREFIX=$(quote "$REMOTE_PREFIX") IFACE=$(quote "$IFACE") WFB_SERVICE=$(quote "$WFB_SERVICE") RESTART_WFB_SERVICE_ON_CLEANUP=$(quote "$RESTART_WFB_SERVICE_ON_CLEANUP") bash -s" <<'REMOTE_CLEANUP' >/dev/null 2>&1 || true
set +e
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH
if [[ -d "$REMOTE_PREFIX" ]]; then
  for pidfile in "$REMOTE_PREFIX"/*.pid; do
    [[ -e "$pidfile" ]] || continue
    kill "$(cat "$pidfile")" >/dev/null 2>&1 || true
  done
fi
sudo -n pkill -f "$REMOTE_PREFIX" || true
sudo -n pkill -x wfb_rx || true
sudo -n pkill -x wfb_tx || true
if [[ "$RESTART_WFB_SERVICE_ON_CLEANUP" == "1" ]]; then
  sudo -n docker start "$WFB_SERVICE" || true
fi
REMOTE_CLEANUP
}
trap cleanup EXIT INT TERM

preflight() {
  require_command cargo
  require_command python3
  require_command ssh
  require_command scp
  require_file "$RADIO_CONFIG" "radio config"
  require_file "$WFB_TX_BIN" "wfb_tx binary"
  require_file "$WFB_RX_BIN" "wfb_rx binary"
  [[ -n "$WFB_KEY" && -r "$WFB_KEY" ]] || die "set WFB_KEY to a readable local WFB-NG key"

  log "preflighting Linux peer $LINUX_HOST"
  local probe
  if ! probe=$(ssh -n "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "LINUX_REMOTE_PATH=$(quote "$LINUX_REMOTE_PATH") IFACE=$(quote "$IFACE") PREPARE_LINUX_PEER=$(quote "$PREPARE_LINUX_PEER") bash -s" <<'REMOTE_PREFLIGHT' 2>&1
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
missing=""
for cmd in sudo ip python3 timeout wfb_rx wfb_tx; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    missing="$missing $cmd"
  fi
done
if [[ "$PREPARE_LINUX_PEER" == "1" ]] && ! command -v iw >/dev/null 2>&1; then
  missing="$missing iw"
fi
if [[ ! -e "/sys/class/net/$IFACE" ]]; then
  missing="$missing iface:$IFACE"
fi
if [[ -n "$missing" ]]; then
  printf 'missing_required=%s\n' "$missing"
  exit 42
fi
printf 'hostname=%s\n' "$(hostname)"
ip -br link show "$IFACE" 2>/dev/null || true
REMOTE_PREFLIGHT
  ); then
    die "Linux peer preflight failed for $LINUX_HOST: $probe"
  fi
  printf '%s\n' "$probe" > "$OUT_DIR/peer-preflight.txt"
}

write_local_helpers() {
  cat > "$OUT_DIR/udp-counter.py" <<'PY'
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

  cat > "$OUT_DIR/udp-source.py" <<'PY'
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
payload_len = int(sys.argv[7])
interval = float(sys.argv[8])
out = Path(sys.argv[9])

if len(marker) + 4 > payload_len or len(warmup_marker) + 4 > payload_len:
    raise SystemExit("marker plus sequence does not fit payload_len")

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
target = (host, port)
started = time.time()

def payload(prefix, seq):
    fill_len = payload_len - len(prefix) - 4
    return prefix + seq.to_bytes(4, "big") + bytes([(seq + i) % 251 for i in range(fill_len)])

for seq in range(warmup):
    sock.sendto(payload(warmup_marker, seq), target)
    time.sleep(interval)
for seq in range(expected):
    sock.sendto(payload(marker, seq), target)
    time.sleep(interval)

out.write_text(json.dumps({
    "duration_s": time.time() - started,
    "expected": expected,
    "interval_s": interval,
    "marker": marker.decode("ascii"),
    "payload_len": payload_len,
    "warmup": warmup,
    "warmup_marker": warmup_marker.decode("ascii"),
    "target": f"{host}:{port}",
}, indent=2, sort_keys=True) + "\n")
PY
}

prepare_linux_peer() {
  log "preparing Linux peer $LINUX_HOST"
  ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "REMOTE_PREFIX=$(quote "$REMOTE_PREFIX") LINUX_REMOTE_PATH=$(quote "$LINUX_REMOTE_PATH") IFACE=$(quote "$IFACE") CHANNEL=$(quote "$CHANNEL") WFB_SERVICE=$(quote "$WFB_SERVICE") PREPARE_LINUX_PEER=$(quote "$PREPARE_LINUX_PEER") bash -s" <<'REMOTE_PREP'
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
rm -rf "$REMOTE_PREFIX"
mkdir -p "$REMOTE_PREFIX"
sudo -n docker stop "$WFB_SERVICE" >/dev/null 2>&1 || true
sudo -n pkill -x wfb_rx >/dev/null 2>&1 || true
sudo -n pkill -x wfb_tx >/dev/null 2>&1 || true
if [[ "$PREPARE_LINUX_PEER" == "1" ]]; then
  sudo -n nmcli dev set "$IFACE" managed no >/dev/null 2>&1 || true
  sudo -n nmcli dev set "p2p-dev-$IFACE" managed no >/dev/null 2>&1 || true
  sudo -n ip link set "$IFACE" down
  sudo -n iw dev "$IFACE" set type monitor
  sudo -n ip link set "$IFACE" up
  sudo -n iw dev "$IFACE" set channel "$CHANNEL" HT20
fi
ip -d link show "$IFACE" > "$REMOTE_PREFIX/link-state-before.txt" 2>&1 || true
if command -v iw >/dev/null 2>&1; then
  sudo -n iw dev "$IFACE" info > "$REMOTE_PREFIX/channel-state-before.txt" 2>&1 || true
fi
REMOTE_PREP
}

install_remote_helpers() {
  log "installing remote helper scripts"
  scp -q "$OUT_DIR/udp-counter.py" "$LINUX_HOST:$REMOTE_PREFIX/udp-counter.py"
  scp -q "$OUT_DIR/udp-source.py" "$LINUX_HOST:$REMOTE_PREFIX/udp-source.py"
}

start_local_counters() {
  log "starting local RX counters"
  python3 "$OUT_DIR/udp-counter.py" "$VIDEO_APP_HOST" "$VIDEO_APP_PORT" "$VIDEO_MARKER" "$VIDEO_EXPECTED_PAYLOADS" "$COUNTER_SECONDS" "$OUT_DIR/video-down-counter.json" >"$OUT_DIR/video-down-counter.log" 2>&1 &
  local_pids+=("$!")
  python3 "$OUT_DIR/udp-counter.py" "$TELEMETRY_APP_HOST" "$TELEMETRY_APP_PORT" "$TELEMETRY_MARKER" "$TELEMETRY_EXPECTED_PAYLOADS" "$COUNTER_SECONDS" "$OUT_DIR/telemetry-down-counter.json" >"$OUT_DIR/telemetry-down-counter.log" 2>&1 &
  local_pids+=("$!")
  sleep 0.3
  for pid in "${local_pids[@]}"; do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      die "local counter failed to start; see $OUT_DIR/*-counter.log"
    fi
  done
}

start_remote_helpers() {
  log "starting remote WFB helpers"
  ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "REMOTE_PREFIX=$(quote "$REMOTE_PREFIX") LINUX_REMOTE_PATH=$(quote "$LINUX_REMOTE_PATH") IFACE=$(quote "$IFACE") LINUX_WFB_KEY=$(quote "$LINUX_WFB_KEY") WFB_CLI_LINK_ID=$(quote "$WFB_CLI_LINK_ID") COUNTER_SECONDS=$(quote "$COUNTER_SECONDS") BANDWIDTH_MHZ=$(quote "$BANDWIDTH_MHZ") VIDEO_RADIO_PORT=$(quote "$VIDEO_RADIO_PORT") TELEMETRY_RADIO_PORT=$(quote "$TELEMETRY_RADIO_PORT") CONTROL_RADIO_PORT=$(quote "$CONTROL_RADIO_PORT") VIDEO_SOURCE_PORT=$(quote "$VIDEO_SOURCE_PORT") TELEMETRY_SOURCE_PORT=$(quote "$TELEMETRY_SOURCE_PORT") CONTROL_COUNTER_PORT=$(quote "$CONTROL_COUNTER_PORT") CONTROL_MARKER=$(quote "$CONTROL_MARKER") CONTROL_EXPECTED_PAYLOADS=$(quote "$CONTROL_EXPECTED_PAYLOADS") VIDEO_MCS=$(quote "$VIDEO_MCS") VIDEO_FEC_K=$(quote "$VIDEO_FEC_K") VIDEO_FEC_N=$(quote "$VIDEO_FEC_N") TELEMETRY_MCS=$(quote "$TELEMETRY_MCS") TELEMETRY_FEC_K=$(quote "$TELEMETRY_FEC_K") TELEMETRY_FEC_N=$(quote "$TELEMETRY_FEC_N") bash -s" <<'REMOTE_HELPERS'
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
prefix=$REMOTE_PREFIX
nohup python3 "$prefix/udp-counter.py" 127.0.0.1 "$CONTROL_COUNTER_PORT" "$CONTROL_MARKER" "$CONTROL_EXPECTED_PAYLOADS" "$COUNTER_SECONDS" "$prefix/control-up-counter.json" > "$prefix/control-up-counter.log" 2>&1 &
echo $! > "$prefix/control-up-counter.pid"
nohup sudo -n timeout "$COUNTER_SECONDS" wfb_rx -K "$LINUX_WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$CONTROL_RADIO_PORT" -c 127.0.0.1 -u "$CONTROL_COUNTER_PORT" "$IFACE" > "$prefix/control-up-wfb-rx.log" 2>&1 &
echo $! > "$prefix/control-up-wfb-rx.pid"
nohup sudo -n timeout "$COUNTER_SECONDS" wfb_tx -K "$LINUX_WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$VIDEO_RADIO_PORT" -B "$BANDWIDTH_MHZ" -M "$VIDEO_MCS" -k "$VIDEO_FEC_K" -n "$VIDEO_FEC_N" -u "$VIDEO_SOURCE_PORT" "$IFACE" > "$prefix/video-down-wfb-tx.log" 2>&1 &
echo $! > "$prefix/video-down-wfb-tx.pid"
nohup sudo -n timeout "$COUNTER_SECONDS" wfb_tx -K "$LINUX_WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$TELEMETRY_RADIO_PORT" -B "$BANDWIDTH_MHZ" -M "$TELEMETRY_MCS" -k "$TELEMETRY_FEC_K" -n "$TELEMETRY_FEC_N" -u "$TELEMETRY_SOURCE_PORT" "$IFACE" > "$prefix/telemetry-down-wfb-tx.log" 2>&1 &
echo $! > "$prefix/telemetry-down-wfb-tx.pid"
REMOTE_HELPERS
}

start_managed_link() {
  log "starting ManagedWfbStreamsBackend"
  OUT_DIR="$OUT_DIR" \
  WFB_KEY="$WFB_KEY" \
  WFB_TX_BIN="$WFB_TX_BIN" \
  WFB_RX_BIN="$WFB_RX_BIN" \
  WFB_LINK_READY_TIMEOUT_S="$WFB_LINK_READY_TIMEOUT_S" \
  WFB_LINK_HOLD_SECONDS="$WFB_LINK_HOLD_SECONDS" \
  LINK_ID="$LINK_ID" \
  VIDEO_DOWN_UDP="$VIDEO_APP_HOST:$VIDEO_APP_PORT" \
  TELEMETRY_DOWN_UDP="$TELEMETRY_APP_HOST:$TELEMETRY_APP_PORT" \
  CONTROL_UP_UDP="$CONTROL_APP_HOST:$CONTROL_APP_PORT" \
  CONTROL_BANDWIDTH_MHZ="$BANDWIDTH_MHZ" \
  CONTROL_MCS="$CONTROL_MCS" \
  CONTROL_FEC_K="$CONTROL_FEC_K" \
  CONTROL_FEC_N="$CONTROL_FEC_N" \
  cargo run -p wfb-link --example managed-streams-link -- "$RADIO_CONFIG" \
    >"$OUT_DIR/managed-link.stdout.log" \
    2>"$OUT_DIR/managed-link.stderr.log" &
  MANAGED_PID=$!
}

wait_for_managed_ready() {
  log "waiting for managed backend readiness"
  for _ in $(seq 1 "$WFB_LINK_READY_TIMEOUT_S"); do
    if grep -q '"ready_file"' "$OUT_DIR/managed-link.stdout.log" 2>/dev/null; then
      return 0
    fi
    if ! kill -0 "$MANAGED_PID" >/dev/null 2>&1; then
      tail -120 "$OUT_DIR/managed-link.stderr.log" >&2 || true
      die "managed backend exited before ready"
    fi
    sleep 1
  done
  die "timed out waiting for managed backend readiness"
}

start_sources() {
  log "starting marked payload sources"
  python3 "$OUT_DIR/udp-source.py" "$CONTROL_APP_HOST" "$CONTROL_APP_PORT" "$CONTROL_MARKER" "$CONTROL_WARMUP_MARKER" "$CONTROL_EXPECTED_PAYLOADS" "$SOURCE_WARMUP_PAYLOADS" "$PAYLOAD_LEN" "$CONTROL_INTERVAL_SEC" "$OUT_DIR/control-up-source.json" >"$OUT_DIR/control-up-source.log" 2>&1 &
  local_pids+=("$!")

  ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "REMOTE_PREFIX=$(quote "$REMOTE_PREFIX") VIDEO_SOURCE_PORT=$(quote "$VIDEO_SOURCE_PORT") TELEMETRY_SOURCE_PORT=$(quote "$TELEMETRY_SOURCE_PORT") VIDEO_MARKER=$(quote "$VIDEO_MARKER") TELEMETRY_MARKER=$(quote "$TELEMETRY_MARKER") VIDEO_WARMUP_MARKER=$(quote "$VIDEO_WARMUP_MARKER") TELEMETRY_WARMUP_MARKER=$(quote "$TELEMETRY_WARMUP_MARKER") VIDEO_EXPECTED_PAYLOADS=$(quote "$VIDEO_EXPECTED_PAYLOADS") TELEMETRY_EXPECTED_PAYLOADS=$(quote "$TELEMETRY_EXPECTED_PAYLOADS") SOURCE_WARMUP_PAYLOADS=$(quote "$SOURCE_WARMUP_PAYLOADS") PAYLOAD_LEN=$(quote "$PAYLOAD_LEN") VIDEO_INTERVAL_SEC=$(quote "$VIDEO_INTERVAL_SEC") TELEMETRY_INTERVAL_SEC=$(quote "$TELEMETRY_INTERVAL_SEC") bash -s" <<'REMOTE_SOURCES' &
set -euo pipefail
prefix=$REMOTE_PREFIX
python3 "$prefix/udp-source.py" 127.0.0.1 "$VIDEO_SOURCE_PORT" "$VIDEO_MARKER" "$VIDEO_WARMUP_MARKER" "$VIDEO_EXPECTED_PAYLOADS" "$SOURCE_WARMUP_PAYLOADS" "$PAYLOAD_LEN" "$VIDEO_INTERVAL_SEC" "$prefix/video-down-source.json" > "$prefix/video-down-source.log" 2>&1 &
video_pid=$!
python3 "$prefix/udp-source.py" 127.0.0.1 "$TELEMETRY_SOURCE_PORT" "$TELEMETRY_MARKER" "$TELEMETRY_WARMUP_MARKER" "$TELEMETRY_EXPECTED_PAYLOADS" "$SOURCE_WARMUP_PAYLOADS" "$PAYLOAD_LEN" "$TELEMETRY_INTERVAL_SEC" "$prefix/telemetry-down-source.json" > "$prefix/telemetry-down-source.log" 2>&1 &
telemetry_pid=$!
wait "$video_pid"
wait "$telemetry_pid"
REMOTE_SOURCES
  REMOTE_SOURCE_PID=$!
}

wait_for_run_completion() {
  local source_status=0
  local remote_source_status=0
  for pid in "${local_pids[@]:-}"; do
    wait "$pid" || source_status=$?
  done
  local_pids=()
  if [[ -n "${REMOTE_SOURCE_PID:-}" ]]; then
    wait "$REMOTE_SOURCE_PID" || remote_source_status=$?
    REMOTE_SOURCE_PID=
  fi
  if (( source_status != 0 || remote_source_status != 0 )); then
    log "one or more payload sources/counters exited non-zero"
  fi

  sleep "$COUNTER_SETTLE_SECONDS"

  local deadline=$((SECONDS + WFB_LINK_HOLD_SECONDS + 20))
  local managed_status=0
  while kill -0 "$MANAGED_PID" >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then
      kill "$MANAGED_PID" >/dev/null 2>&1 || true
      managed_status=124
      break
    fi
    sleep 1
  done
  if [[ -n "${MANAGED_PID:-}" ]]; then
    wait "$MANAGED_PID" || managed_status=$?
    MANAGED_PID=
  fi
  printf '%s\n' "$managed_status" > "$OUT_DIR/managed-link.exit-status"
}

fetch_peer_artifacts() {
  if (( PEER_FETCHED == 1 )); then
    return 0
  fi
  mkdir -p "$OUT_DIR"
  rm -rf "$OUT_DIR/peer"
  scp -q -r "$LINUX_HOST:$REMOTE_PREFIX" "$OUT_DIR/peer" >/dev/null 2>&1 || true
  PEER_FETCHED=1
}

write_summary() {
  fetch_peer_artifacts
  log "writing summary"
  OUT_DIR="$OUT_DIR" \
  VIDEO_MIN_UNIQUE="$VIDEO_MIN_UNIQUE" \
  TELEMETRY_MIN_UNIQUE="$TELEMETRY_MIN_UNIQUE" \
  CONTROL_MIN_UNIQUE="$CONTROL_MIN_UNIQUE" \
  MAX_VIDEO_DECRYPT_FAILURES="$MAX_VIDEO_DECRYPT_FAILURES" \
  MAX_TELEMETRY_DECRYPT_FAILURES="$MAX_TELEMETRY_DECRYPT_FAILURES" \
  MAX_CONTROL_DECRYPT_FAILURES="$MAX_CONTROL_DECRYPT_FAILURES" \
  python3 - <<'PY'
import json
import os
import sys
from pathlib import Path

out = Path(os.environ["OUT_DIR"])

def load(path):
    try:
        return json.loads(path.read_text())
    except Exception as exc:
        return {"error": str(exc), "path": str(path)}

def parse_json_stream(path):
    try:
        data = path.read_text(encoding="utf-8")
    except Exception as exc:
        return [], [f"{path.name}: {exc}"]
    decoder = json.JSONDecoder()
    pos = 0
    objects = []
    failures = []
    while True:
        while pos < len(data) and data[pos].isspace():
            pos += 1
        if pos >= len(data):
            break
        try:
            value, pos = decoder.raw_decode(data, pos)
        except Exception as exc:
            failures.append(f"{path.name}: JSON parse failed at {pos}: {exc}")
            break
        objects.append(value)
    return objects, failures

def decrypt_counts(*paths):
    total = 0
    post_session = 0
    saw_session = False
    for path in paths:
        try:
            for line in path.read_text(errors="replace").splitlines():
                if "\tSESSION\t" in line or line.startswith("SESSION"):
                    saw_session = True
                if "Unable to decrypt" in line:
                    total += 1
                    if saw_session:
                        post_session += 1
        except Exception:
            pass
    return {"total": total, "post_session": post_session, "saw_session": saw_session}

def unique(counter):
    try:
        return int(counter.get("unique_sequences") or 0)
    except Exception:
        return 0

objects, parse_failures = parse_json_stream(out / "managed-link.stdout.log")
ready = objects[0] if len(objects) > 0 else {}
health = objects[1] if len(objects) > 1 else {}
report = objects[2] if len(objects) > 2 else {}
backend = report.get("backend") if isinstance(report, dict) else {}
managed = backend.get("managed_wfb_streams") if isinstance(backend, dict) else {}
radio = managed.get("radio") if isinstance(managed, dict) else {}

managed_status = load(out / "managed-link.exit-status")
if isinstance(managed_status, dict):
    managed_exit_status = None
else:
    try:
        managed_exit_status = int((out / "managed-link.exit-status").read_text().strip())
    except Exception:
        managed_exit_status = None

video = load(out / "video-down-counter.json")
telemetry = load(out / "telemetry-down-counter.json")
control = load(out / "peer" / "control-up-counter.json")
video_source = load(out / "peer" / "video-down-source.json")
telemetry_source = load(out / "peer" / "telemetry-down-source.json")
control_source = load(out / "control-up-source.json")

video_decrypt = decrypt_counts(
    out / "wfb-rx-video-down.stderr.log",
    out / "wfb-rx-video-down.stdout.log",
)
telemetry_decrypt = decrypt_counts(
    out / "wfb-rx-telemetry-down.stderr.log",
    out / "wfb-rx-telemetry-down.stdout.log",
)
control_decrypt = decrypt_counts(out / "peer" / "control-up-wfb-rx.log")

failures = list(parse_failures)
if len(objects) != 3:
    failures.append(f"managed_stdout_json_objects={len(objects)}, expected 3")
if managed_exit_status not in {0, None}:
    failures.append(f"managed_exit_status={managed_exit_status}")
if health.get("ready") is not True:
    failures.append(f"health.ready={health.get('ready')!r}")
if report.get("lifecycle") != "stopped":
    failures.append(f"report.lifecycle={report.get('lifecycle')!r}")
if radio.get("result") != "pass":
    failures.append(f"radio.result={radio.get('result')!r}")

checks = [
    ("video_down", video, int(os.environ["VIDEO_MIN_UNIQUE"])),
    ("telemetry_down", telemetry, int(os.environ["TELEMETRY_MIN_UNIQUE"])),
    ("control_up", control, int(os.environ["CONTROL_MIN_UNIQUE"])),
]
for name, counter, minimum in checks:
    if "error" in counter:
        failures.append(f"{name}_counter_error={counter.get('error')}")
    elif unique(counter) < minimum:
        failures.append(f"{name}_unique_sequences={unique(counter)}<{minimum}")

decrypt_checks = [
    ("video_down", video_decrypt["post_session"], int(os.environ["MAX_VIDEO_DECRYPT_FAILURES"])),
    ("telemetry_down", telemetry_decrypt["post_session"], int(os.environ["MAX_TELEMETRY_DECRYPT_FAILURES"])),
    ("control_up", control_decrypt["post_session"], int(os.environ["MAX_CONTROL_DECRYPT_FAILURES"])),
]
for name, observed, maximum in decrypt_checks:
    if observed > maximum:
        failures.append(f"{name}_decrypt_failures={observed}>{maximum}")

summary = {
    "schema": "wfb_link_managed_streams_smoke/v1",
    "result": "fail" if failures else "pass",
    "failures": failures,
    "managed_exit_status": managed_exit_status,
    "ready": ready,
    "health": {
        "lifecycle": health.get("lifecycle"),
        "ready": health.get("ready"),
        "tx": health.get("tx"),
        "rx": health.get("rx"),
        "streams": health.get("streams"),
    },
    "report": {
        "lifecycle": report.get("lifecycle"),
        "radio_result": radio.get("result"),
        "radio_stop_reason": radio.get("stop_reason"),
        "backend": managed,
    },
    "streams": {
        "video_down": {
            "counter": video,
            "source": video_source,
            "decrypt_failures": video_decrypt,
        },
        "telemetry_down": {
            "counter": telemetry,
            "source": telemetry_source,
            "decrypt_failures": telemetry_decrypt,
        },
        "control_up": {
            "counter": control,
            "source": control_source,
            "decrypt_failures": control_decrypt,
        },
    },
}
(out / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
if failures:
    for failure in failures:
        print(f"[managed-streams-smoke] {failure}", file=sys.stderr)
    sys.exit(1)
PY
}

mkdir -p "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd)
printf '[managed-streams-smoke] artifacts: %s\n' "$OUT_DIR" >&2

preflight
write_local_helpers
prepare_linux_peer
install_remote_helpers
start_managed_link
wait_for_managed_ready
start_local_counters
start_remote_helpers
sleep "$HELPER_SETTLE_SECONDS"
start_sources
wait_for_run_completion
write_summary

printf '[managed-streams-smoke] complete: %s\n' "$OUT_DIR" >&2
