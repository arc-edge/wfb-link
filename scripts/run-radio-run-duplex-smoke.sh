#!/usr/bin/env bash
# shellcheck disable=SC2029
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-radio-run-duplex-smoke.sh

Runs a bounded production radio-run full-duplex smoke with the RTL8812AU
attached to this Mac:
  Mac -> Linux: Linux wfb_tx -d feeds local radio-run TX; Linux wfb_rx counts.
  Linux -> Mac: Linux wfb_tx transmits RF; radio-run forwards to Linux wfb_rx -a.

Configuration is via environment variables. Common overrides:
  LINUX_HOST=pi@drone-2f389.local
  MAC_LAN_IP=192.168.122.84
  LINUX_LAN_IP=192.168.122.77  # or auto, resolved from Linux route to MAC_LAN_IP
  LINK_ID=0x000001        # report/runtime value
  WFB_CLI_LINK_ID=1       # decimal value for Linux WFB-ng CLI; derived by default
  EXPECTED_PAYLOADS=80 SOURCE_WARMUP_PAYLOADS=100
  SOURCE_TAIL_PAYLOADS=auto
  SESSION_ACQUIRE_MODE=observed SESSION_ACQUIRE_TIMEOUT_SECONDS=15
  SESSION_ACQUIRE_SETTLE_SECONDS=1
  DUPLEX_TRAFFIC_MODE=simultaneous   # simultaneous, phased, or tdd
  TDD_FIRST_DIRECTION=l2m TDD_GUARD_SEC=2.0
  AIRTIME_MODE=continuous            # continuous or tdd runtime TX gating
  AIRTIME_TDD_FIRST_WINDOW=rx AIRTIME_TDD_RX_WINDOW_MS=7000
  AIRTIME_TDD_TX_WINDOW_MS=20000 AIRTIME_TDD_GUARD_MS=500
  M2L_SOURCE_PHASE_SEC=0 L2M_SOURCE_PHASE_SEC=0
  PAYLOAD_INTERVAL_SEC=0.020
  M2L_PAYLOAD_INTERVAL_SEC=0.100 L2M_PAYLOAD_INTERVAL_SEC=0.005
  M2L_EXPECTED_PAYLOADS=100 L2M_EXPECTED_PAYLOADS=2000
  ENABLE_M2L=1 ENABLE_L2M=1
  M2L_FEC_K=3 M2L_FEC_N=12 L2M_FEC_K=3 L2M_FEC_N=12
  M2L_MCS=1 L2M_MCS=1
  M2L_MIN_UNIQUE=80 L2M_MIN_UNIQUE=80
  WFB_RX_RCV_BUF_BYTES=8388608 WFB_RX_SND_BUF_BYTES=8388608
  LINUX_UDP_BUF_SYSCTL_BYTES=8388608
  MAX_M2L_DECRYPT_FAILURES=0 MAX_L2M_DECRYPT_FAILURES=0
  DECRYPT_FAILURE_GATE=post-session
  TX_POWER_MODE=current-default
  TX_CALIBRATION_PROFILE=rtl8812a-runtime-iqk
  REQUIRE_CALIBRATION_SUCCESS=auto
  AUTO_EFUSE_DUMP=1
  M2L_INGRESS_MODE=ssh-udp-relay
  RADIO_COMMAND=service       # service or diagnostic
  RADIO_RUN_CONFIG=configs/radio-run-robust-short-range.toml
  OUT_DIR=/tmp/wfb-radio-run-duplex-smoke
EOF
}

log() {
  printf '[duplex] %s\n' "$*" >&2
}

die() {
  printf '[duplex] error: %s\n' "$*" >&2
  exit 1
}

quote() {
  printf '%q' "$1"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
OUT_DIR=${OUT_DIR:-/tmp/wfb-radio-run-duplex-$RUN_ID}
REMOTE_PREFIX=${REMOTE_PREFIX:-/tmp/wfb-radio-run-duplex-$RUN_ID-peer}

LINUX_HOST=${LINUX_HOST:-pi@drone-2f389.local}
MAC_LAN_IP=${MAC_LAN_IP:-192.168.122.84}
LINUX_LAN_IP=${LINUX_LAN_IP:-192.168.122.77}
LINUX_LAN_IP_REQUESTED=$LINUX_LAN_IP
LINUX_REMOTE_PATH=${LINUX_REMOTE_PATH:-/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin}
SSH_OPTS=${SSH_OPTS:-"-o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=2"}
# shellcheck disable=SC2206
SSH_OPTS_ARRAY=($SSH_OPTS)
IFACE=${IFACE:-wfb0}
WFB_SERVICE=${WFB_SERVICE:-arc-wfb-link-1}
WFB_KEY=${WFB_KEY:-/var/lib/arc/wfb/drone.key}

CHANNEL=${CHANNEL:-36}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
LINK_ID=${LINK_ID:-0x000001}
WFB_CLI_LINK_ID=${WFB_CLI_LINK_ID:-$(printf '%d' "$((LINK_ID))")}
M2L_RADIO_PORT=${M2L_RADIO_PORT:-0}
L2M_RADIO_PORT=${L2M_RADIO_PORT:-1}
FEC_K=${FEC_K:-3}
FEC_N=${FEC_N:-12}
M2L_FEC_K=${M2L_FEC_K:-$FEC_K}
M2L_FEC_N=${M2L_FEC_N:-$FEC_N}
L2M_FEC_K=${L2M_FEC_K:-$FEC_K}
L2M_FEC_N=${L2M_FEC_N:-$FEC_N}
M2L_MCS=${M2L_MCS:-1}
L2M_MCS=${L2M_MCS:-1}
EXPECTED_PAYLOADS=${EXPECTED_PAYLOADS:-80}
ENABLE_M2L=${ENABLE_M2L:-1}
ENABLE_L2M=${ENABLE_L2M:-1}
M2L_EXPECTED_PAYLOADS=${M2L_EXPECTED_PAYLOADS:-$EXPECTED_PAYLOADS}
L2M_EXPECTED_PAYLOADS=${L2M_EXPECTED_PAYLOADS:-$EXPECTED_PAYLOADS}
M2L_MIN_UNIQUE=${M2L_MIN_UNIQUE:-$M2L_EXPECTED_PAYLOADS}
L2M_MIN_UNIQUE=${L2M_MIN_UNIQUE:-$L2M_EXPECTED_PAYLOADS}
MIN_RADIO_RX_FORWARDED=${MIN_RADIO_RX_FORWARDED:-1}
MAX_M2L_DECRYPT_FAILURES=${MAX_M2L_DECRYPT_FAILURES:-0}
MAX_L2M_DECRYPT_FAILURES=${MAX_L2M_DECRYPT_FAILURES:-0}
DECRYPT_FAILURE_GATE=${DECRYPT_FAILURE_GATE:-post-session}
REQUIRE_CALIBRATION_SUCCESS=${REQUIRE_CALIBRATION_SUCCESS:-auto}
export M2L_FEC_K M2L_FEC_N L2M_FEC_K L2M_FEC_N M2L_MCS L2M_MCS EXPECTED_PAYLOADS M2L_EXPECTED_PAYLOADS L2M_EXPECTED_PAYLOADS ENABLE_M2L ENABLE_L2M M2L_MIN_UNIQUE L2M_MIN_UNIQUE MIN_RADIO_RX_FORWARDED MAX_M2L_DECRYPT_FAILURES MAX_L2M_DECRYPT_FAILURES DECRYPT_FAILURE_GATE REQUIRE_CALIBRATION_SUCCESS
SOURCE_WARMUP_PAYLOADS=${SOURCE_WARMUP_PAYLOADS:-100}
SOURCE_TAIL_PAYLOADS=${SOURCE_TAIL_PAYLOADS:-auto}
SESSION_ACQUIRE_MODE=${SESSION_ACQUIRE_MODE:-observed}
SESSION_ACQUIRE_TIMEOUT_SECONDS=${SESSION_ACQUIRE_TIMEOUT_SECONDS:-15}
SESSION_ACQUIRE_POLL_SECONDS=${SESSION_ACQUIRE_POLL_SECONDS:-0.2}
SESSION_ACQUIRE_SETTLE_SECONDS=${SESSION_ACQUIRE_SETTLE_SECONDS:-1}
PAYLOAD_LEN=${PAYLOAD_LEN:-1000}
PAYLOAD_INTERVAL_SEC=${PAYLOAD_INTERVAL_SEC:-0.020}
M2L_PAYLOAD_INTERVAL_SEC=${M2L_PAYLOAD_INTERVAL_SEC:-$PAYLOAD_INTERVAL_SEC}
L2M_PAYLOAD_INTERVAL_SEC=${L2M_PAYLOAD_INTERVAL_SEC:-$PAYLOAD_INTERVAL_SEC}
DUPLEX_TRAFFIC_MODE=${DUPLEX_TRAFFIC_MODE:-simultaneous}
case "$DUPLEX_TRAFFIC_MODE" in
  simultaneous|phased|tdd) ;;
  *) die "invalid DUPLEX_TRAFFIC_MODE=$DUPLEX_TRAFFIC_MODE (expected simultaneous, phased, or tdd)" ;;
esac
TDD_FIRST_DIRECTION=${TDD_FIRST_DIRECTION:-l2m}
case "$TDD_FIRST_DIRECTION" in
  m2l|l2m) ;;
  *) die "invalid TDD_FIRST_DIRECTION=$TDD_FIRST_DIRECTION (expected m2l or l2m)" ;;
esac
TDD_GUARD_SEC=${TDD_GUARD_SEC:-2.0}
AIRTIME_MODE=${AIRTIME_MODE:-continuous}
case "$AIRTIME_MODE" in
  continuous|tdd) ;;
  *) die "invalid AIRTIME_MODE=$AIRTIME_MODE (expected continuous or tdd)" ;;
esac
AIRTIME_TDD_FIRST_WINDOW=${AIRTIME_TDD_FIRST_WINDOW:-rx}
case "$AIRTIME_TDD_FIRST_WINDOW" in
  rx|tx) ;;
  *) die "invalid AIRTIME_TDD_FIRST_WINDOW=$AIRTIME_TDD_FIRST_WINDOW (expected rx or tx)" ;;
esac
AIRTIME_TDD_RX_WINDOW_MS=${AIRTIME_TDD_RX_WINDOW_MS:-1000}
AIRTIME_TDD_TX_WINDOW_MS=${AIRTIME_TDD_TX_WINDOW_MS:-1000}
AIRTIME_TDD_GUARD_MS=${AIRTIME_TDD_GUARD_MS:-0}
AIRTIME_TDD_START_DELAY_MS=${AIRTIME_TDD_START_DELAY_MS:-0}
M2L_SOURCE_PHASE_SEC=${M2L_SOURCE_PHASE_SEC:-0}
L2M_SOURCE_PHASE_SEC=${L2M_SOURCE_PHASE_SEC:-0}
export SOURCE_WARMUP_PAYLOADS SOURCE_TAIL_PAYLOADS SESSION_ACQUIRE_MODE SESSION_ACQUIRE_TIMEOUT_SECONDS SESSION_ACQUIRE_POLL_SECONDS SESSION_ACQUIRE_SETTLE_SECONDS PAYLOAD_LEN PAYLOAD_INTERVAL_SEC M2L_PAYLOAD_INTERVAL_SEC L2M_PAYLOAD_INTERVAL_SEC DUPLEX_TRAFFIC_MODE TDD_FIRST_DIRECTION TDD_GUARD_SEC AIRTIME_MODE AIRTIME_TDD_FIRST_WINDOW AIRTIME_TDD_RX_WINDOW_MS AIRTIME_TDD_TX_WINDOW_MS AIRTIME_TDD_GUARD_MS AIRTIME_TDD_START_DELAY_MS M2L_SOURCE_PHASE_SEC L2M_SOURCE_PHASE_SEC
M2L_MARKER=${M2L_MARKER:-M2LRSMK1}
L2M_MARKER=${L2M_MARKER:-L2MRSMK1}
M2L_WARMUP_MARKER=${M2L_WARMUP_MARKER:-M2LWARM1}
L2M_WARMUP_MARKER=${L2M_WARMUP_MARKER:-L2MWARM1}
M2L_TAIL_MARKER=${M2L_TAIL_MARKER:-M2LTAIL1}
L2M_TAIL_MARKER=${L2M_TAIL_MARKER:-L2MTAIL1}

FIRMWARE=${FIRMWARE:-/tmp/rtl8812aefw.bin}
EFUSE_REPORT=${EFUSE_REPORT:-/tmp/wfb-remote-macos-efuse-dump.json}
AUTO_EFUSE_DUMP=${AUTO_EFUSE_DUMP:-1}
TX_POWER_MODE=${TX_POWER_MODE:-current-default}
TX_POWER_SAFETY_PROFILE=${TX_POWER_SAFETY_PROFILE:-linux-ch36-ht20}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}
RADIO_RUN_CONFIG=${RADIO_RUN_CONFIG:-configs/radio-run-robust-short-range.toml}
RADIO_COMMAND=${RADIO_COMMAND:-service}
export RADIO_RUN_CONFIG
case "$RADIO_COMMAND" in
  service|diagnostic) ;;
  diag) RADIO_COMMAND=diagnostic ;;
  *) die "invalid RADIO_COMMAND=$RADIO_COMMAND (expected service or diagnostic)" ;;
esac
export RADIO_COMMAND

RADIO_BIND_PORT=${RADIO_BIND_PORT:-5611}
M2L_INGRESS_MODE=${M2L_INGRESS_MODE:-ssh-udp-relay}
case "$M2L_INGRESS_MODE" in
  direct|ssh-udp-relay) ;;
  *) die "unsupported M2L_INGRESS_MODE=$M2L_INGRESS_MODE" ;;
esac
if [[ -z "${RADIO_BIND+x}" ]]; then
  if [[ "$M2L_INGRESS_MODE" == "ssh-udp-relay" ]]; then
    RADIO_BIND="127.0.0.1:$RADIO_BIND_PORT"
  else
    RADIO_BIND="0.0.0.0:$RADIO_BIND_PORT"
  fi
fi
if [[ -z "${M2L_DISTRIBUTOR_HOST+x}" ]]; then
  if [[ "$M2L_INGRESS_MODE" == "ssh-udp-relay" ]]; then
    M2L_DISTRIBUTOR_HOST=127.0.0.1
  else
    M2L_DISTRIBUTOR_HOST=$MAC_LAN_IP
  fi
fi
export M2L_INGRESS_MODE M2L_DISTRIBUTOR_HOST
LINUX_M2L_SOURCE_PORT=${LINUX_M2L_SOURCE_PORT:-5600}
LINUX_L2M_SOURCE_PORT=${LINUX_L2M_SOURCE_PORT:-5621}
M2L_COUNTER_PORT=${M2L_COUNTER_PORT:-5900}
L2M_AGG_PORT=${L2M_AGG_PORT:-5801}
L2M_COUNTER_PORT=${L2M_COUNTER_PORT:-5911}
WFB_RX_RCV_BUF_BYTES=${WFB_RX_RCV_BUF_BYTES:-8388608}
WFB_RX_SND_BUF_BYTES=${WFB_RX_SND_BUF_BYTES:-8388608}
LINUX_UDP_BUF_SYSCTL_BYTES=${LINUX_UDP_BUF_SYSCTL_BYTES:-8388608}
export WFB_RX_RCV_BUF_BYTES WFB_RX_SND_BUF_BYTES LINUX_UDP_BUF_SYSCTL_BYTES
RADIO_RUN_DURATION_MS=${RADIO_RUN_DURATION_MS:-55000}
RADIO_READY_WAIT_SECONDS=${RADIO_READY_WAIT_SECONDS:-90}
RX_TIMEOUT_MS=${RX_TIMEOUT_MS:-20}
TX_BURST_LIMIT=${TX_BURST_LIMIT:-4}
COUNTER_SECONDS=${COUNTER_SECONDS:-50}
PEER_WAIT_SECONDS=${PEER_WAIT_SECONDS:-$((COUNTER_SECONDS + 5))}
if (( PEER_WAIT_SECONDS < COUNTER_SECONDS + 5 )); then
  PEER_WAIT_SECONDS=$((COUNTER_SECONDS + 5))
fi

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

for cmd in cargo python3 ssh scp; do
  require_command "$cmd"
done
[[ -f "$FIRMWARE" ]] || die "firmware not found: $FIRMWARE"
if [[ "$TX_POWER_MODE" == "efuse-derived" ]]; then
  if [[ ! -f "$EFUSE_REPORT" && "$AUTO_EFUSE_DUMP" == "1" ]]; then
    log "EFUSE report missing; capturing $EFUSE_REPORT"
    cargo run -p wfb-radio-diag -- --json \
      --report "$EFUSE_REPORT" \
      macos-efuse-dump \
      --vid 0x0bda \
      --pid 0x8812 \
      --raw-out /tmp/wfb-remote-macos-efuse-raw.bin \
      --logical-map-out /tmp/wfb-remote-macos-efuse-logical.bin \
      --i-understand-this-writes-control-registers
  fi
  [[ -f "$EFUSE_REPORT" ]] || die "EFUSE report not found: $EFUSE_REPORT"
fi

preflight_linux_peer() {
  log "preflighting Linux peer $LINUX_HOST"
  local probe
  if ! probe=$(ssh -n "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "LINUX_REMOTE_PATH=$(quote "$LINUX_REMOTE_PATH") IFACE=$(quote "$IFACE") bash -s" <<'REMOTE_PREFLIGHT' 2>&1
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
missing=""
for cmd in sudo iw ip tcpdump python3 timeout wfb_rx wfb_tx; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    missing="$missing $cmd"
  fi
done
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
  log "Linux peer preflight passed: $(printf '%s\n' "$probe" | head -n 1)"
}

resolve_linux_lan_ip() {
  if [[ "$LINUX_LAN_IP" != "auto" ]]; then
    return
  fi
  local route_output
  local resolved
  route_output=$(ssh -n "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "ip -4 route get $(quote "$MAC_LAN_IP") 2>/dev/null" || true)
  resolved=$(printf '%s\n' "$route_output" | sed -n 's/.* src \([0-9][0-9.]*\).*/\1/p' | head -n 1)
  if [[ -z "$resolved" ]]; then
    die "could not resolve LINUX_LAN_IP from $LINUX_HOST route to MAC_LAN_IP=$MAC_LAN_IP; set LINUX_LAN_IP explicitly"
  fi
  LINUX_LAN_IP=$resolved
  log "resolved LINUX_LAN_IP=$LINUX_LAN_IP from Linux route to MAC_LAN_IP=$MAC_LAN_IP"
}

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO_ROOT"
mkdir -p "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd)
[[ -f "$RADIO_RUN_CONFIG" ]] || die "radio-run config not found: $RADIO_RUN_CONFIG"
RADIO_RUN_CONFIG=$(cd "$(dirname "$RADIO_RUN_CONFIG")" && pwd)/$(basename "$RADIO_RUN_CONFIG")

preflight_linux_peer
resolve_linux_lan_ip
export LINUX_HOST MAC_LAN_IP LINUX_LAN_IP LINUX_LAN_IP_REQUESTED

RADIO_PID=
M2L_RELAY_PID=
cleanup() {
  set +e
  if [[ -n "${RADIO_PID:-}" ]]; then
    kill "$RADIO_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${M2L_RELAY_PID:-}" ]]; then
    kill "$M2L_RELAY_PID" >/dev/null 2>&1 || true
    wait "$M2L_RELAY_PID" >/dev/null 2>&1 || true
    M2L_RELAY_PID=
  fi
  ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" "REMOTE_PREFIX='$REMOTE_PREFIX' IFACE='$IFACE' WFB_SERVICE='$WFB_SERVICE' RADIO_BIND_PORT='$RADIO_BIND_PORT' bash -s" <<'REMOTE_CLEANUP' >/dev/null 2>&1 || true
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH
sudo -n pkill -f "$REMOTE_PREFIX" || true
sudo -n pkill -f "[w]fb-m2l-udp-relay-" || true
sudo -n pkill -f "[p]ython3 -u - $RADIO_BIND_PORT" || true
sudo -n pkill -x wfb_rx || true
sudo -n pkill -x wfb_tx || true
sudo -n pkill -f "tcpdump -i $IFACE" || true
sudo -n docker start "$WFB_SERVICE" || true
REMOTE_CLEANUP
}
trap cleanup EXIT INT TERM

start_m2l_ingress_relay() {
  if [[ "$M2L_INGRESS_MODE" != "ssh-udp-relay" || "$ENABLE_M2L" != "1" ]]; then
    return 0
  fi
  log "starting M2L SSH UDP relay $LINUX_HOST:127.0.0.1:$RADIO_BIND_PORT -> 127.0.0.1:$RADIO_BIND_PORT"
  ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "RADIO_BIND_PORT=$(quote "$RADIO_BIND_PORT") bash -s" <<'REMOTE_RELAY_CLEANUP' >/dev/null 2>&1 || true
set +e
sudo -n pkill -f "[w]fb-m2l-udp-relay-" || true
sudo -n pkill -f "[p]ython3 -u - $RADIO_BIND_PORT" || true
REMOTE_RELAY_CLEANUP
  local remote_py="$OUT_DIR/m2l-udp-relay-remote.py"
  local local_py="$OUT_DIR/m2l-udp-relay-local.py"
  local remote_title="wfb-m2l-udp-relay-$RUN_ID"
  cat > "$remote_py" <<'PY'
import socket
import struct
import sys

port = int(sys.argv[1])
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", port))
out = sys.stdout.buffer
while True:
    data, _peer = sock.recvfrom(65535)
    out.write(struct.pack("!H", len(data)))
    out.write(data)
    out.flush()
PY
  cat > "$local_py" <<'PY'
import socket
import struct
import sys

port = int(sys.argv[1])
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
target = ("127.0.0.1", port)
inp = sys.stdin.buffer
while True:
    header = inp.read(2)
    if not header:
        break
    if len(header) != 2:
        raise SystemExit("truncated relay frame header")
    length = struct.unpack("!H", header)[0]
    data = inp.read(length)
    if len(data) != length:
        raise SystemExit("truncated relay frame payload")
    sock.sendto(data, target)
PY
  (
    ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
      "LINUX_REMOTE_PATH=$(quote "$LINUX_REMOTE_PATH") bash -c 'export PATH=\"\$0:\$PATH\"; exec -a \"\$1\" python3 -u - \"\$2\"' $(quote "$LINUX_REMOTE_PATH") $(quote "$remote_title") $(quote "$RADIO_BIND_PORT")" < "$remote_py" |
      python3 -u "$local_py" "$RADIO_BIND_PORT"
  ) > "$OUT_DIR/m2l-udp-relay.log" 2>&1 &
  M2L_RELAY_PID=$!
  sleep 1
  if ! kill -0 "$M2L_RELAY_PID" >/dev/null 2>&1; then
    cat "$OUT_DIR/m2l-udp-relay.log" >&2 || true
    die "M2L SSH UDP relay failed to start"
  fi
}

prepare_peer() {
  log "preparing Linux peer $LINUX_HOST on channel $CHANNEL"
  ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "REMOTE_PREFIX='$REMOTE_PREFIX' LINUX_REMOTE_PATH='$LINUX_REMOTE_PATH' IFACE='$IFACE' CHANNEL='$CHANNEL' WFB_SERVICE='$WFB_SERVICE' LINUX_UDP_BUF_SYSCTL_BYTES='$LINUX_UDP_BUF_SYSCTL_BYTES' bash -s" <<'REMOTE_PREP'
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
rm -rf "$REMOTE_PREFIX"
mkdir -p "$REMOTE_PREFIX"
SYSCTL_BIN=$(command -v sysctl || true)
if [[ -z "$SYSCTL_BIN" && -x /sbin/sysctl ]]; then
  SYSCTL_BIN=/sbin/sysctl
fi
if [[ -n "$SYSCTL_BIN" ]]; then
  sudo -n "$SYSCTL_BIN" -w \
    "net.core.rmem_max=$LINUX_UDP_BUF_SYSCTL_BYTES" \
    "net.core.wmem_max=$LINUX_UDP_BUF_SYSCTL_BYTES" \
    "net.core.rmem_default=$LINUX_UDP_BUF_SYSCTL_BYTES" \
    "net.core.wmem_default=$LINUX_UDP_BUF_SYSCTL_BYTES" \
    > "$REMOTE_PREFIX/sysctl-socket-buffers.txt" 2>&1 || true
fi
sudo -n docker stop "$WFB_SERVICE" >/dev/null 2>&1 || true
sudo -n pkill -x wfb_rx >/dev/null 2>&1 || true
sudo -n pkill -x wfb_tx >/dev/null 2>&1 || true
sudo -n pkill -f "tcpdump -i ${IFACE}" >/dev/null 2>&1 || true
sudo -n nmcli dev set "$IFACE" managed no >/dev/null 2>&1 || true
sudo -n nmcli dev set "p2p-dev-$IFACE" managed no >/dev/null 2>&1 || true
sudo -n ip link set "$IFACE" down
sudo -n iw dev "$IFACE" set type monitor
sudo -n ip link set "$IFACE" up
sudo -n iw dev "$IFACE" set channel "$CHANNEL" HT20
sudo -n iw dev "$IFACE" info > "$REMOTE_PREFIX/channel-state-before.txt" 2>&1 || true
ip -d link show "$IFACE" > "$REMOTE_PREFIX/link-state-before.txt" 2>&1 || true
sudo -n timeout 3 tcpdump -i "$IFACE" -L > "$REMOTE_PREFIX/pcap-linktypes-before.txt" 2>&1 || true
grep -q "type monitor" "$REMOTE_PREFIX/channel-state-before.txt"
grep -q "channel ${CHANNEL} " "$REMOTE_PREFIX/channel-state-before.txt"
grep -q "width: 20 MHz" "$REMOTE_PREFIX/channel-state-before.txt"
grep -q "link/ieee802.11/radiotap" "$REMOTE_PREFIX/link-state-before.txt"
grep -q "IEEE802_11_RADIO" "$REMOTE_PREFIX/pcap-linktypes-before.txt"
REMOTE_PREP
}

start_radio() {
  log "starting local radio-run production loop via $RADIO_COMMAND command"
  local tx_power_args=()
  local airtime_args=()
  local write_auth_arg=()
  if [[ "$TX_POWER_MODE" != "current-default" ]]; then
    tx_power_args+=(--tx-power-mode "$TX_POWER_MODE")
    if [[ "$TX_POWER_MODE" == "efuse-derived" ]]; then
      tx_power_args+=(
        --tx-power-efuse-report "$EFUSE_REPORT"
        --tx-power-safety-profile "$TX_POWER_SAFETY_PROFILE"
      )
    fi
  fi
  case "$TX_CALIBRATION_PROFILE" in
    linux-parity-ch36-ht20|rtl8812a-lck|rtl8812a-runtime-iqk)
      write_auth_arg+=(--i-understand-this-writes-registers)
      ;;
  esac
  if [[ "$AIRTIME_MODE" != "continuous" ]]; then
    if [[ "$RADIO_COMMAND" != "service" ]]; then
      die "runtime airtime gating requires RADIO_COMMAND=service"
    fi
    airtime_args+=(
      --airtime-mode "$AIRTIME_MODE"
      --airtime-tdd-first-window "$AIRTIME_TDD_FIRST_WINDOW"
      --airtime-tdd-rx-window-ms "$AIRTIME_TDD_RX_WINDOW_MS"
      --airtime-tdd-tx-window-ms "$AIRTIME_TDD_TX_WINDOW_MS"
      --airtime-tdd-guard-ms "$AIRTIME_TDD_GUARD_MS"
      --airtime-tdd-start-delay-ms "$AIRTIME_TDD_START_DELAY_MS"
    )
  fi

  case "$RADIO_COMMAND" in
    service)
      cargo run -p wfb-radio-service -- \
        --json \
        --report "$OUT_DIR/radio-run.json" \
        --config "$RADIO_RUN_CONFIG" \
        --firmware "$FIRMWARE" \
        --channel "$CHANNEL" --bandwidth "$BANDWIDTH_MHZ" \
        --bind "$RADIO_BIND" \
        --ready-file "$OUT_DIR/radio-ready.json" \
        --health-file "$OUT_DIR/radio-health.json" \
        --duration-ms "$RADIO_RUN_DURATION_MS" \
        --rx-timeout-ms "$RX_TIMEOUT_MS" \
        --tx-burst-limit "$TX_BURST_LIMIT" \
        --max-datagrams 0 \
        ${airtime_args[@]+"${airtime_args[@]}"} \
        ${tx_power_args[@]+"${tx_power_args[@]}"} \
        --tx-calibration-profile "$TX_CALIBRATION_PROFILE" \
        ${write_auth_arg[@]+"${write_auth_arg[@]}"} \
        --wfb-link-id "$LINK_ID" \
        --wfb-radio-port "$L2M_RADIO_PORT" \
        --rx-aggregator "$LINUX_LAN_IP:$L2M_AGG_PORT" \
        --rx-mcs-index "$L2M_MCS" \
        > "$OUT_DIR/radio-run.log" 2>&1 &
      ;;
    diagnostic)
      cargo run -p wfb-radio-diag -- --json \
        --report "$OUT_DIR/radio-run.json" \
        radio-run \
        --config "$RADIO_RUN_CONFIG" \
        --firmware "$FIRMWARE" \
        --channel "$CHANNEL" --bandwidth "$BANDWIDTH_MHZ" \
        --bind "$RADIO_BIND" \
        --ready-file "$OUT_DIR/radio-ready.json" \
        --health-file "$OUT_DIR/radio-health.json" \
        --duration-ms "$RADIO_RUN_DURATION_MS" \
        --rx-timeout-ms "$RX_TIMEOUT_MS" \
        --tx-burst-limit "$TX_BURST_LIMIT" \
        --max-datagrams 0 \
        ${tx_power_args[@]+"${tx_power_args[@]}"} \
        --tx-calibration-profile "$TX_CALIBRATION_PROFILE" \
        ${write_auth_arg[@]+"${write_auth_arg[@]}"} \
        --wfb-link-id "$LINK_ID" \
        --wfb-radio-port "$L2M_RADIO_PORT" \
        --rx-aggregator "$LINUX_LAN_IP:$L2M_AGG_PORT" \
        --rx-mcs-index "$L2M_MCS" \
        > "$OUT_DIR/radio-run.log" 2>&1 &
      ;;
  esac
  RADIO_PID=$!
}

write_radio_startup_failure_summary() {
  local reason=$1
  local radio_exit_status=${2:-}
  log "writing startup failure summary"
  python3 - "$OUT_DIR" "$reason" "$radio_exit_status" <<'PY'
import json
import os
import sys
from pathlib import Path

run = Path(sys.argv[1])
reason = sys.argv[2]
radio_exit_status = sys.argv[3] if len(sys.argv) > 3 and sys.argv[3] else None

def load(path):
    try:
        return json.loads(path.read_text())
    except Exception as exc:
        return {"error": str(exc), "path": str(path)}

def tail(path, limit=120):
    try:
        return path.read_text(errors="replace").splitlines()[-limit:]
    except Exception:
        return []

report = load(run / "radio-run.json")
health = load(run / "radio-health.json")
calibration = report.get("tx_calibration_profile") or {}
runtime_iqk = calibration.get("runtime_iqk") or {}
require_calibration_success = os.environ["REQUIRE_CALIBRATION_SUCCESS"]
calibration_success_required = require_calibration_success in {"1", "true", "yes"}
if require_calibration_success == "auto":
    calibration_success_required = report.get("calibration_profile") == "rtl8812a_runtime_iqk"

failures = [reason]
if isinstance(report, dict) and report.get("error"):
    failures.append(f"radio_report_error={report.get('error')}")
elif report.get("result") not in {None, "pass"}:
    failures.append(f"radio_result={report.get('result')}")
if calibration_success_required and runtime_iqk.get("status") not in {"completed", "success"}:
    failures.append(f"runtime_iqk_status={runtime_iqk.get('status')}")
if calibration_success_required and runtime_iqk.get("cleanup_status") != "restored":
    failures.append(f"runtime_iqk_cleanup_status={runtime_iqk.get('cleanup_status')}")
if calibration_success_required and runtime_iqk.get("selected_iqc_fill_applied") is not True:
    failures.append(
        f"runtime_iqk_selected_iqc_fill_applied={runtime_iqk.get('selected_iqc_fill_applied')}"
    )

summary = {
    "smoke_result": "fail",
    "failures": failures,
    "startup_failure_reason": reason,
    "network": {
        "linux_host": os.environ.get("LINUX_HOST"),
        "linux_lan_ip": os.environ.get("LINUX_LAN_IP"),
        "linux_lan_ip_requested": os.environ.get("LINUX_LAN_IP_REQUESTED"),
        "mac_lan_ip": os.environ.get("MAC_LAN_IP"),
    },
    "radio_exit_status": int(radio_exit_status) if radio_exit_status is not None else None,
    "radio_command": os.environ.get("RADIO_COMMAND"),
    "tx_power_mode": os.environ.get("TX_POWER_MODE"),
    "tx_power_safety_profile": os.environ.get("TX_POWER_SAFETY_PROFILE"),
    "tx_calibration_profile": os.environ.get("TX_CALIBRATION_PROFILE"),
    "radio_result": report.get("result"),
    "service_health": health,
    "stop_reason": report.get("stop_reason"),
    "calibration": {
        "profile": report.get("calibration_profile"),
        "class": report.get("calibration_class"),
        "evidence_source": report.get("calibration_evidence_source"),
        "receiver_backed_validation_required": report.get("receiver_backed_validation_required"),
        "runtime_iqk_status": runtime_iqk.get("status"),
        "runtime_iqk_cleanup_status": runtime_iqk.get("cleanup_status"),
        "runtime_iqk_sweep_index": runtime_iqk.get("sweep_index"),
        "runtime_iqk_sweep_count": runtime_iqk.get("sweep_count"),
        "runtime_iqk_selected_iqc_fill_applied": runtime_iqk.get("selected_iqc_fill_applied"),
        "runtime_iqk_selected_iqc_fill_register_count": runtime_iqk.get("selected_iqc_fill_register_count"),
        "calibration_success_required": calibration_success_required,
    },
    "radio_log_tail": tail(run / "radio-run.log"),
}
(run / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(json.dumps(summary, indent=2, sort_keys=True))
PY
}

wait_for_radio_ready() {
  log "waiting for radio ready marker"
  for _ in $(seq 1 "$RADIO_READY_WAIT_SECONDS"); do
    if [[ -f "$OUT_DIR/radio-ready.json" ]]; then
      cp "$OUT_DIR/radio-ready.json" "$OUT_DIR/radio-ready-observed.json"
      return 0
    fi
    if ! kill -0 "$RADIO_PID" >/dev/null 2>&1; then
      local radio_status=0
      wait "$RADIO_PID" || radio_status=$?
      RADIO_PID=
      tail -120 "$OUT_DIR/radio-run.log" >&2 || true
      write_radio_startup_failure_summary "radio_run_exited_before_ready" "$radio_status" || true
      die "radio-run exited before ready; summary written to $OUT_DIR/summary.json"
    fi
    sleep 1
  done
  local radio_status=0
  kill "$RADIO_PID" >/dev/null 2>&1 || true
  wait "$RADIO_PID" || radio_status=$?
  RADIO_PID=
  tail -120 "$OUT_DIR/radio-run.log" >&2 || true
  write_radio_startup_failure_summary "radio_ready_marker_timed_out" "$radio_status" || true
  die "radio ready marker timed out; summary written to $OUT_DIR/summary.json"
}

run_peer_traffic() {
  log "running peer TX/RX traffic"
  ssh "${SSH_OPTS_ARRAY[@]}" "$LINUX_HOST" \
    "REMOTE_PREFIX='$REMOTE_PREFIX' LINUX_REMOTE_PATH='$LINUX_REMOTE_PATH' IFACE='$IFACE' CHANNEL='$CHANNEL' WFB_KEY='$WFB_KEY' WFB_CLI_LINK_ID='$WFB_CLI_LINK_ID' MAC_LAN_IP='$MAC_LAN_IP' M2L_DISTRIBUTOR_HOST='$M2L_DISTRIBUTOR_HOST' RADIO_BIND_PORT='$RADIO_BIND_PORT' M2L_RADIO_PORT='$M2L_RADIO_PORT' L2M_RADIO_PORT='$L2M_RADIO_PORT' M2L_FEC_K='$M2L_FEC_K' M2L_FEC_N='$M2L_FEC_N' L2M_FEC_K='$L2M_FEC_K' L2M_FEC_N='$L2M_FEC_N' M2L_MCS='$M2L_MCS' L2M_MCS='$L2M_MCS' EXPECTED_PAYLOADS='$EXPECTED_PAYLOADS' M2L_EXPECTED_PAYLOADS='$M2L_EXPECTED_PAYLOADS' L2M_EXPECTED_PAYLOADS='$L2M_EXPECTED_PAYLOADS' ENABLE_M2L='$ENABLE_M2L' ENABLE_L2M='$ENABLE_L2M' SOURCE_WARMUP_PAYLOADS='$SOURCE_WARMUP_PAYLOADS' SOURCE_TAIL_PAYLOADS='$SOURCE_TAIL_PAYLOADS' SESSION_ACQUIRE_MODE='$SESSION_ACQUIRE_MODE' SESSION_ACQUIRE_TIMEOUT_SECONDS='$SESSION_ACQUIRE_TIMEOUT_SECONDS' SESSION_ACQUIRE_POLL_SECONDS='$SESSION_ACQUIRE_POLL_SECONDS' SESSION_ACQUIRE_SETTLE_SECONDS='$SESSION_ACQUIRE_SETTLE_SECONDS' PAYLOAD_LEN='$PAYLOAD_LEN' PAYLOAD_INTERVAL_SEC='$PAYLOAD_INTERVAL_SEC' M2L_PAYLOAD_INTERVAL_SEC='$M2L_PAYLOAD_INTERVAL_SEC' L2M_PAYLOAD_INTERVAL_SEC='$L2M_PAYLOAD_INTERVAL_SEC' DUPLEX_TRAFFIC_MODE='$DUPLEX_TRAFFIC_MODE' TDD_FIRST_DIRECTION='$TDD_FIRST_DIRECTION' TDD_GUARD_SEC='$TDD_GUARD_SEC' M2L_SOURCE_PHASE_SEC='$M2L_SOURCE_PHASE_SEC' L2M_SOURCE_PHASE_SEC='$L2M_SOURCE_PHASE_SEC' M2L_MARKER='$M2L_MARKER' L2M_MARKER='$L2M_MARKER' M2L_WARMUP_MARKER='$M2L_WARMUP_MARKER' L2M_WARMUP_MARKER='$L2M_WARMUP_MARKER' M2L_TAIL_MARKER='$M2L_TAIL_MARKER' L2M_TAIL_MARKER='$L2M_TAIL_MARKER' LINUX_M2L_SOURCE_PORT='$LINUX_M2L_SOURCE_PORT' LINUX_L2M_SOURCE_PORT='$LINUX_L2M_SOURCE_PORT' M2L_COUNTER_PORT='$M2L_COUNTER_PORT' L2M_AGG_PORT='$L2M_AGG_PORT' L2M_COUNTER_PORT='$L2M_COUNTER_PORT' WFB_RX_RCV_BUF_BYTES='$WFB_RX_RCV_BUF_BYTES' WFB_RX_SND_BUF_BYTES='$WFB_RX_SND_BUF_BYTES' COUNTER_SECONDS='$COUNTER_SECONDS' PEER_WAIT_SECONDS='$PEER_WAIT_SECONDS' bash -s" <<'REMOTE_TRAFFIC'
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
enabled() {
  case "$1" in
    0|false|False|FALSE|no|No|NO) return 1 ;;
    *) return 0 ;;
  esac
}

write_disabled_counter() {
  local path=$1
  local host=$2
  local port=$3
  local marker=$4
  local expected=$5
  python3 - "$path" "$host" "$port" "$marker" "$expected" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
host = sys.argv[2]
port = int(sys.argv[3])
marker = sys.argv[4]
expected = int(sys.argv[5])
report = {
    "bind": f"{host}:{port}",
    "bytes": 0,
    "disabled": True,
    "duration_sec": 0.0,
    "expected": expected,
    "last_peer": None,
    "marker": marker,
    "matched_datagrams": 0,
    "missing_sequences": [],
    "missing_sequences_omitted": True,
    "packets": 0,
    "unique_sequences": 0,
}
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY
}

cat > "$REMOTE_PREFIX/counter.py" <<'PY'
import json
import socket
import sys
import time
from collections import Counter
from pathlib import Path

host = sys.argv[1]
port = int(sys.argv[2])
marker = sys.argv[3].encode()
expected = int(sys.argv[4])
out = Path(sys.argv[5])
duration = float(sys.argv[6])

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
seq_counts = Counter()
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
        seq = int.from_bytes(data[idx + len(marker):idx + len(marker) + 4], "big")
        seqs.add(seq)
        seq_counts[seq] += 1
duplicate_sequences = {str(seq): count for seq, count in sorted(seq_counts.items()) if count > 1}
sequence_counts = {str(seq): count for seq, count in sorted(seq_counts.items())}
report = {
    "bind": f"{host}:{port}",
    "marker": marker.decode(),
    "expected": expected,
    "packets": packets,
    "bytes": bytes_total,
    "matched_datagrams": matched,
    "unique_sequences": len(seqs),
    "missing_sequences": [i for i in range(expected) if i not in seqs],
    "duplicate_sequence_count": len(duplicate_sequences),
    "duplicate_sequences": duplicate_sequences,
    "sequence_counts": sequence_counts,
    "max_sequence_count": max(seq_counts.values(), default=0),
    "min_seen_sequence_count": min(seq_counts.values(), default=0),
    "last_peer": last_peer,
    "duration_sec": time.time() - started,
}
out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY

sudo -n ip link set "$IFACE" down
sudo -n iw dev "$IFACE" set type monitor
sudo -n ip link set "$IFACE" up
sudo -n iw dev "$IFACE" set channel "$CHANNEL" HT20
sudo -n iw dev "$IFACE" info > "$REMOTE_PREFIX/channel-state-traffic.txt" 2>&1 || true
ip -d link show "$IFACE" > "$REMOTE_PREFIX/link-state-traffic.txt" 2>&1 || true
sudo -n timeout 3 tcpdump -i "$IFACE" -L > "$REMOTE_PREFIX/pcap-linktypes-traffic.txt" 2>&1 || true
grep -q "type monitor" "$REMOTE_PREFIX/channel-state-traffic.txt"
grep -q "channel ${CHANNEL} " "$REMOTE_PREFIX/channel-state-traffic.txt"
grep -q "width: 20 MHz" "$REMOTE_PREFIX/channel-state-traffic.txt"
grep -q "link/ieee802.11/radiotap" "$REMOTE_PREFIX/link-state-traffic.txt"
grep -q "IEEE802_11_RADIO" "$REMOTE_PREFIX/pcap-linktypes-traffic.txt"

STDBUF_PREFIX=()
if command -v stdbuf >/dev/null 2>&1; then
  STDBUF_PREFIX=(stdbuf -oL -eL)
fi

if enabled "$ENABLE_M2L"; then
  python3 -u "$REMOTE_PREFIX/counter.py" 127.0.0.1 "$M2L_COUNTER_PORT" "$M2L_MARKER" "$M2L_EXPECTED_PAYLOADS" "$REMOTE_PREFIX/counter-m2l.json" "$COUNTER_SECONDS" > "$REMOTE_PREFIX/counter-m2l.log" 2>&1 &
  echo $! > "$REMOTE_PREFIX/counter-m2l.pid"
else
  write_disabled_counter "$REMOTE_PREFIX/counter-m2l.json" 127.0.0.1 "$M2L_COUNTER_PORT" "$M2L_MARKER" "$M2L_EXPECTED_PAYLOADS"
fi
if enabled "$ENABLE_L2M"; then
  python3 -u "$REMOTE_PREFIX/counter.py" 127.0.0.1 "$L2M_COUNTER_PORT" "$L2M_MARKER" "$L2M_EXPECTED_PAYLOADS" "$REMOTE_PREFIX/counter-l2m.json" "$COUNTER_SECONDS" > "$REMOTE_PREFIX/counter-l2m.log" 2>&1 &
  echo $! > "$REMOTE_PREFIX/counter-l2m.pid"
else
  write_disabled_counter "$REMOTE_PREFIX/counter-l2m.json" 127.0.0.1 "$L2M_COUNTER_PORT" "$L2M_MARKER" "$L2M_EXPECTED_PAYLOADS"
fi

sudo -n tcpdump -i "$IFACE" -y IEEE802_11_RADIO -s 256 -w "$REMOTE_PREFIX/rf.pcap" > "$REMOTE_PREFIX/tcpdump.log" 2>&1 &
echo $! > "$REMOTE_PREFIX/tcpdump.pid"
if enabled "$ENABLE_M2L"; then
  sudo -n timeout "$COUNTER_SECONDS" "${STDBUF_PREFIX[@]}" wfb_rx -R "$WFB_RX_RCV_BUF_BYTES" -s "$WFB_RX_SND_BUF_BYTES" -K "$WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$M2L_RADIO_PORT" -c 127.0.0.1 -u "$M2L_COUNTER_PORT" "$IFACE" > "$REMOTE_PREFIX/wfb-rx-m2l.log" 2>&1 &
  echo $! > "$REMOTE_PREFIX/wfb-rx-m2l.pid"
else
  : > "$REMOTE_PREFIX/wfb-rx-m2l.log"
fi
if enabled "$ENABLE_L2M"; then
  sudo -n timeout "$COUNTER_SECONDS" "${STDBUF_PREFIX[@]}" wfb_rx -a "$L2M_AGG_PORT" -R "$WFB_RX_RCV_BUF_BYTES" -s "$WFB_RX_SND_BUF_BYTES" -K "$WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$L2M_RADIO_PORT" -c 127.0.0.1 -u "$L2M_COUNTER_PORT" > "$REMOTE_PREFIX/wfb-rx-l2m-agg.log" 2>&1 &
  echo $! > "$REMOTE_PREFIX/wfb-rx-l2m-agg.pid"
else
  : > "$REMOTE_PREFIX/wfb-rx-l2m-agg.log"
fi
sleep 3
if enabled "$ENABLE_M2L" && ! kill -0 "$(cat "$REMOTE_PREFIX/wfb-rx-m2l.pid")" >/dev/null 2>&1; then
  cat "$REMOTE_PREFIX/wfb-rx-m2l.log" >&2 || true
  exit 21
fi
if enabled "$ENABLE_M2L" && grep -qi "unknown encapsulation" "$REMOTE_PREFIX/wfb-rx-m2l.log"; then
  cat "$REMOTE_PREFIX/wfb-rx-m2l.log" >&2 || true
  exit 22
fi
if enabled "$ENABLE_M2L"; then
  sudo -n timeout "$COUNTER_SECONDS" wfb_tx -d -K "$WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$M2L_RADIO_PORT" -B 20 -M "$M2L_MCS" -k "$M2L_FEC_K" -n "$M2L_FEC_N" -u "$LINUX_M2L_SOURCE_PORT" "$M2L_DISTRIBUTOR_HOST:$RADIO_BIND_PORT" > "$REMOTE_PREFIX/wfb-tx-m2l-dist.log" 2>&1 &
  echo $! > "$REMOTE_PREFIX/wfb-tx-m2l-dist.pid"
else
  : > "$REMOTE_PREFIX/wfb-tx-m2l-dist.log"
fi
if enabled "$ENABLE_L2M"; then
  sudo -n timeout "$COUNTER_SECONDS" wfb_tx -K "$WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$L2M_RADIO_PORT" -B 20 -M "$L2M_MCS" -k "$L2M_FEC_K" -n "$L2M_FEC_N" -u "$LINUX_L2M_SOURCE_PORT" "$IFACE" > "$REMOTE_PREFIX/wfb-tx-l2m-rf.log" 2>&1 &
  echo $! > "$REMOTE_PREFIX/wfb-tx-l2m-rf.pid"
else
  : > "$REMOTE_PREFIX/wfb-tx-l2m-rf.log"
fi
sleep 2

python3 - <<'PY'
import json
import os
import socket
import time
from pathlib import Path

def enabled(name):
    return os.environ.get(name, "1").lower() not in {"0", "false", "no"}

remote_prefix = Path(os.environ["REMOTE_PREFIX"])
payload_len = int(os.environ["PAYLOAD_LEN"])
warmup = int(os.environ["SOURCE_WARMUP_PAYLOADS"])
expected = int(os.environ["EXPECTED_PAYLOADS"])
default_interval = float(os.environ["PAYLOAD_INTERVAL_SEC"])
m2l_interval = float(os.environ.get("M2L_PAYLOAD_INTERVAL_SEC", default_interval))
l2m_interval = float(os.environ.get("L2M_PAYLOAD_INTERVAL_SEC", default_interval))
traffic_mode = os.environ.get("DUPLEX_TRAFFIC_MODE", "simultaneous")
tdd_first_direction = os.environ.get("TDD_FIRST_DIRECTION", "l2m")
tdd_guard = max(0.0, float(os.environ.get("TDD_GUARD_SEC", "2.0")))
m2l_phase = max(0.0, float(os.environ.get("M2L_SOURCE_PHASE_SEC", "0")))
l2m_phase = max(0.0, float(os.environ.get("L2M_SOURCE_PHASE_SEC", "0")))
session_mode = os.environ.get("SESSION_ACQUIRE_MODE", "observed")
session_timeout = float(os.environ.get("SESSION_ACQUIRE_TIMEOUT_SECONDS", "15"))
session_poll = float(os.environ.get("SESSION_ACQUIRE_POLL_SECONDS", "0.2"))
session_settle = float(os.environ.get("SESSION_ACQUIRE_SETTLE_SECONDS", "1"))
enable_m2l = enabled("ENABLE_M2L")
enable_l2m = enabled("ENABLE_L2M")
m2l_expected = int(os.environ.get("M2L_EXPECTED_PAYLOADS", expected))
l2m_expected = int(os.environ.get("L2M_EXPECTED_PAYLOADS", expected))
expected_by_direction = {
    "m2l": m2l_expected,
    "l2m": l2m_expected,
}
interval_by_direction = {
    "m2l": m2l_interval,
    "l2m": l2m_interval,
}
phase_by_direction = {
    "m2l": m2l_phase,
    "l2m": l2m_phase,
}
enabled_by_direction = {
    "m2l": enable_m2l,
    "l2m": enable_l2m,
}
for direction, is_enabled in enabled_by_direction.items():
    if is_enabled and interval_by_direction[direction] <= 0:
        raise SystemExit(f"{direction.upper()}_PAYLOAD_INTERVAL_SEC must be > 0")
    if is_enabled and expected_by_direction[direction] < 0:
        raise SystemExit(f"{direction.upper()}_EXPECTED_PAYLOADS must be >= 0")
active_intervals = [
    interval_by_direction[direction]
    for direction, is_enabled in enabled_by_direction.items()
    if is_enabled
]
min_active_interval = min(active_intervals or [default_interval])
m2l_fec_k = int(os.environ["M2L_FEC_K"])
l2m_fec_k = int(os.environ["L2M_FEC_K"])
tail_config = os.environ.get("SOURCE_TAIL_PAYLOADS", "auto").strip().lower()
if tail_config == "auto":
    tail = max([value for enabled_flag, value in [(enable_m2l, m2l_fec_k), (enable_l2m, l2m_fec_k)] if enabled_flag] or [0])
else:
    tail = max(0, int(tail_config))
source_m2l = ("127.0.0.1", int(os.environ["LINUX_M2L_SOURCE_PORT"]))
source_l2m = ("127.0.0.1", int(os.environ["LINUX_L2M_SOURCE_PORT"]))
sock_m2l = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock_l2m = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
session_logs = []
if enable_m2l:
    session_logs.append(("m2l", remote_prefix / "wfb-rx-m2l.log"))
if enable_l2m:
    session_logs.append(("l2m", remote_prefix / "wfb-rx-l2m-agg.log"))

def payload(marker, seq, fill):
    marker = marker.encode()
    return marker + seq.to_bytes(4, "big") + fill * (payload_len - len(marker) - 4)

def send_direction(direction, marker, seq, fill):
    if direction == "m2l":
        sock_m2l.sendto(payload(marker, seq, fill), source_m2l)
    else:
        sock_l2m.sendto(payload(marker, seq, fill), source_l2m)

def marker_for(direction, kind):
    return os.environ[f"{direction.upper()}_{kind}_MARKER"]

def has_session(path):
    try:
        lines = path.read_text(errors="replace").splitlines()
    except Exception:
        return False
    return any("\tSESSION" in line or " SESSION" in line for line in lines)

def session_state():
    observed = {name: has_session(path) for name, path in session_logs}
    missing = [name for name, seen in observed.items() if not seen]
    return observed, missing

warmup_sent = 0
warmup_sent_by_direction = {"m2l": 0, "l2m": 0}
warmup_started_at = time.time()
warmup_schedule = []
for direction in ("m2l", "l2m"):
    if not enabled_by_direction[direction]:
        continue
    for seq in range(warmup):
        warmup_schedule.append((
            warmup_started_at + seq * interval_by_direction[direction],
            direction,
            seq,
        ))
warmup_schedule.sort()
for send_at, direction, seq in warmup_schedule:
    delay = send_at - time.time()
    if delay > 0:
        time.sleep(delay)
    send_direction(direction, marker_for(direction, "WARMUP"), seq, b"w")
    warmup_sent_by_direction[direction] += 1
warmup_sent = max(warmup_sent_by_direction.values())
warmup_completed_at = time.time()
observed_sessions, missing_sessions = session_state()
timed_out = False
if session_mode in {"0", "false", "no", "off", "disabled", "warmup-only"}:
    session_mode = "disabled"
elif session_mode != "observed":
    session_mode = "observed"
if session_mode == "observed" and missing_sessions:
    deadline = time.time() + session_timeout
    next_poll = time.time()
    while time.time() < deadline:
        missing_set = set(missing_sessions)
        interval_candidates = []
        for direction in ("m2l", "l2m"):
            if direction not in missing_set:
                continue
            seq = warmup_sent_by_direction[direction]
            send_direction(direction, marker_for(direction, "WARMUP"), seq, b"w")
            warmup_sent_by_direction[direction] += 1
            interval_candidates.append(interval_by_direction[direction])
        warmup_sent = max(warmup_sent_by_direction.values())
        time.sleep(min(interval_candidates or [min_active_interval]))
        if time.time() >= next_poll:
            observed_sessions, missing_sessions = session_state()
            next_poll = time.time() + session_poll
            if not missing_sessions:
                break
    observed_sessions, missing_sessions = session_state()
    timed_out = bool(missing_sessions)
if not timed_out and not missing_sessions and session_settle > 0:
    time.sleep(session_settle)
measured_started_at = time.time()
(remote_prefix / "source-gate.json").write_text(json.dumps({
    "mode": session_mode,
    "status": "timed_out" if timed_out else ("acquired" if not missing_sessions else "skipped"),
    "acquired": not missing_sessions if session_mode == "observed" else None,
    "timed_out": timed_out,
    "required_sessions": [name for name, _ in session_logs],
    "observed_sessions": observed_sessions,
    "missing_sessions": missing_sessions,
    "configured_warmup_payloads": warmup,
    "configured_tail_payloads": tail_config,
    "warmup_payloads": warmup_sent,
    "warmup_payloads_by_direction": warmup_sent_by_direction,
    "tail_payloads": tail,
    "expected_payloads": expected,
    "expected_payloads_by_direction": expected_by_direction,
    "payload_interval_sec": default_interval,
    "payload_interval_sec_by_direction": interval_by_direction,
    "timeout_sec": session_timeout,
    "poll_sec": session_poll,
    "settle_sec": session_settle if not timed_out and not missing_sessions else 0,
    "delay_after_warmup_sec": measured_started_at - warmup_completed_at,
    "traffic_mode": traffic_mode,
    "tdd_first_direction": tdd_first_direction,
    "tdd_guard_sec": tdd_guard,
    "source_phase_sec": {
        "m2l": m2l_phase,
        "l2m": l2m_phase,
    },
}, indent=2, sort_keys=True) + "\n")
source_started_at = time.time()
schedule = []
source_events = []
def append_direction_schedule(block_start, direction, count):
    interval = interval_by_direction[direction]
    for seq in range(count):
        schedule.append((block_start + seq * interval, direction, seq))

if traffic_mode == "tdd":
    enabled_order = []
    if tdd_first_direction == "m2l":
        preferred = ["m2l", "l2m"]
    else:
        preferred = ["l2m", "m2l"]
    for direction in preferred:
        if direction == "m2l" and enable_m2l:
            enabled_order.append(direction)
        if direction == "l2m" and enable_l2m:
            enabled_order.append(direction)
    block_offset = 0.0
    for block_index, direction in enumerate(enabled_order):
        count = expected_by_direction[direction]
        interval = interval_by_direction[direction]
        block_start = source_started_at + block_offset
        append_direction_schedule(block_start, direction, count)
        block_offset += count * interval
        if block_index != len(enabled_order) - 1:
            block_offset += tdd_guard
else:
    if enable_m2l:
        append_direction_schedule(source_started_at + phase_by_direction["m2l"], "m2l", m2l_expected)
    if enable_l2m:
        append_direction_schedule(source_started_at + phase_by_direction["l2m"], "l2m", l2m_expected)
schedule.sort()
for send_at, direction, seq in schedule:
    delay = send_at - time.time()
    if delay > 0:
        time.sleep(delay)
    if direction == "m2l":
        sock_m2l.sendto(payload(os.environ["M2L_MARKER"], seq, b"m"), source_m2l)
    else:
        sock_l2m.sendto(payload(os.environ["L2M_MARKER"], seq, b"l"), source_l2m)
    sent_at = time.time()
    source_events.append({
        "direction": direction,
        "sequence": seq,
        "scheduled_offset_sec": round(send_at - source_started_at, 6),
        "sent_offset_sec": round(sent_at - source_started_at, 6),
        "lateness_sec": round(sent_at - send_at, 6),
    })
(remote_prefix / "source-events.jsonl").write_text(
    "".join(json.dumps(event, sort_keys=True) + "\n" for event in source_events)
)
direction_counts = {}
max_lateness = {}
for event in source_events:
    direction = event["direction"]
    direction_counts[direction] = direction_counts.get(direction, 0) + 1
    max_lateness[direction] = max(max_lateness.get(direction, 0.0), event["lateness_sec"])
marked_completed_at = time.time()
tail_started_at = marked_completed_at
tail_schedule = []
for direction in ("m2l", "l2m"):
    if not enabled_by_direction[direction]:
        continue
    for seq in range(tail):
        tail_schedule.append((tail_started_at + seq * interval_by_direction[direction], direction, seq))
tail_schedule.sort()
for send_at, direction, seq in tail_schedule:
    delay = send_at - time.time()
    if delay > 0:
        time.sleep(delay)
    send_direction(direction, marker_for(direction, "TAIL"), seq, b"t")
tail_completed_at = time.time()
(remote_prefix / "source-summary.json").write_text(json.dumps({
    "source_started_at": source_started_at,
    "expected_payloads": expected,
    "expected_payloads_by_direction": expected_by_direction,
    "payload_interval_sec": default_interval,
    "payload_interval_sec_by_direction": interval_by_direction,
    "tail_payloads": tail,
    "traffic_mode": traffic_mode,
    "tdd_first_direction": tdd_first_direction,
    "tdd_guard_sec": tdd_guard,
    "source_phase_sec": {
        "m2l": m2l_phase,
        "l2m": l2m_phase,
    },
    "marked_source_events": len(source_events),
    "direction_counts": direction_counts,
    "max_lateness_sec": max_lateness,
    "marked_duration_sec": round(marked_completed_at - source_started_at, 6),
    "tail_duration_sec": round(tail_completed_at - tail_started_at, 6),
    "duration_sec": round(tail_completed_at - source_started_at, 6),
}, indent=2, sort_keys=True) + "\n")
PY
printf 'sent configured_warmup=%s configured_tail=%s marked=%s m2l_expected=%s l2m_expected=%s m2l_interval=%s l2m_interval=%s enable_m2l=%s enable_l2m=%s link_cli=%s\n' "$SOURCE_WARMUP_PAYLOADS" "$SOURCE_TAIL_PAYLOADS" "$EXPECTED_PAYLOADS" "$M2L_EXPECTED_PAYLOADS" "$L2M_EXPECTED_PAYLOADS" "$M2L_PAYLOAD_INTERVAL_SEC" "$L2M_PAYLOAD_INTERVAL_SEC" "$ENABLE_M2L" "$ENABLE_L2M" "$WFB_CLI_LINK_ID" > "$REMOTE_PREFIX/sources-done.txt"

for _ in $(seq 1 "$PEER_WAIT_SECONDS"); do
  [[ -f "$REMOTE_PREFIX/counter-m2l.json" && -f "$REMOTE_PREFIX/counter-l2m.json" ]] && break
  sleep 1
done
sudo -n iw dev "$IFACE" info > "$REMOTE_PREFIX/channel-state-after.txt" 2>&1 || true
if [[ -f "$REMOTE_PREFIX/tcpdump.pid" ]]; then
  sudo -n kill -INT "$(cat "$REMOTE_PREFIX/tcpdump.pid")" >/dev/null 2>&1 || true
  sleep 1
fi
REMOTE_TRAFFIC
}

collect_peer_artifacts() {
  log "collecting peer artifacts"
  rm -rf "$OUT_DIR/peer"
  scp "${SSH_OPTS_ARRAY[@]}" -r "$LINUX_HOST:$REMOTE_PREFIX" "$OUT_DIR/peer" >/dev/null
}

write_summary() {
  log "writing summary"
  python3 - "$OUT_DIR" <<'PY'
import json
import os
import sys
from pathlib import Path

run = Path(sys.argv[1])

def load(path):
    try:
        return json.loads(path.read_text())
    except Exception as exc:
        return {"error": str(exc), "path": str(path)}

def count_lines(path, needle):
    try:
        return sum(1 for line in path.read_text(errors="replace").splitlines() if needle in line)
    except Exception:
        return 0

def read_text(path):
    try:
        return path.read_text(errors="replace")
    except Exception:
        return ""

def log_contains(path, needle):
    return needle in read_text(path)

def decrypt_stats(path):
    stats = {
        "total": 0,
        "before_session": 0,
        "after_session": 0,
        "session_observed": False,
    }
    try:
        lines = path.read_text(errors="replace").splitlines()
    except Exception:
        return stats
    for line in lines:
        if "\tSESSION" in line or " SESSION" in line:
            stats["session_observed"] = True
        if "Unable to decrypt" not in line:
            continue
        stats["total"] += 1
        if stats["session_observed"]:
            stats["after_session"] += 1
        else:
            stats["before_session"] += 1
    return stats

def compact_counter(counter):
    if not isinstance(counter, dict):
        return counter
    return {
        key: value
        for key, value in counter.items()
        if key not in {"sequence_counts", "duplicate_sequences"}
    }

def parse_iw_channel_state(path):
    try:
        text = path.read_text(errors="replace")
    except Exception as exc:
        return {"error": str(exc), "path": str(path)}
    state = {
        "path": str(path),
        "type": None,
        "channel": None,
        "frequency_mhz": None,
        "width_mhz": None,
        "txpower_dbm": None,
        "raw": text,
    }
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("type "):
            state["type"] = stripped.split(None, 1)[1]
        elif stripped.startswith("channel "):
            parts = stripped.split()
            if len(parts) >= 2:
                try:
                    state["channel"] = int(parts[1])
                except ValueError:
                    pass
            if "(" in stripped and "MHz" in stripped:
                try:
                    state["frequency_mhz"] = int(stripped.split("(", 1)[1].split("MHz", 1)[0].strip())
                except ValueError:
                    pass
            if "width:" in stripped and "MHz" in stripped.rsplit("width:", 1)[1]:
                try:
                    state["width_mhz"] = int(stripped.rsplit("width:", 1)[1].split("MHz", 1)[0].strip())
                except ValueError:
                    pass
        elif stripped.startswith("txpower "):
            try:
                state["txpower_dbm"] = float(stripped.split()[1])
            except (IndexError, ValueError):
                pass
    return state

def observation_matches(observations, link_id, radio_port):
    if not isinstance(observations, list):
        return False
    def int_field(observation, name, default=-1):
        value = observation.get(name)
        if value is None:
            return default
        try:
            return int(value)
        except (TypeError, ValueError):
            return default
    for observation in observations:
        if not isinstance(observation, dict):
            continue
        if int_field(observation, "count", 0) <= 0:
            continue
        if int_field(observation, "source_link_id") != link_id:
            continue
        if int_field(observation, "destination_link_id") != link_id:
            continue
        if int_field(observation, "source_radio_port") != radio_port:
            continue
        if int_field(observation, "destination_radio_port") != radio_port:
            continue
        return True
    return False

def metric_sample_count(signal, metric):
    values = signal.get(metric) if isinstance(signal, dict) else {}
    if not isinstance(values, dict):
        return 0
    return int(values.get("sample_count") or 0)

report = load(run / "radio-run.json")
health = load(run / "radio-health.json")
m2l = load(run / "peer" / "counter-m2l.json")
l2m = load(run / "peer" / "counter-l2m.json")
source_gate = load(run / "peer" / "source-gate.json")
source_summary = load(run / "peer" / "source-summary.json")
peer_channel_state = {
    "before": parse_iw_channel_state(run / "peer" / "channel-state-before.txt"),
    "traffic": parse_iw_channel_state(run / "peer" / "channel-state-traffic.txt"),
    "after": parse_iw_channel_state(run / "peer" / "channel-state-after.txt"),
}
rx = report.get("rx") or {}
tx = report.get("tx") or {}
calibration = report.get("tx_calibration_profile") or {}
runtime_iqk = calibration.get("runtime_iqk") or {}
sweep_summaries = runtime_iqk.get("sweep_summaries") or []
last_sweep_summary = sweep_summaries[-1] if sweep_summaries else {}
rx_forwards = rx.get("rx_forwards") or []
radio_rx_forwarded = sum(
    ((forward.get("counters") or {}).get("forwarded") or 0)
    for forward in rx_forwards
)
m2l_unique = int(m2l.get("unique_sequences") or 0)
l2m_unique = int(l2m.get("unique_sequences") or 0)
m2l_min_unique = int(os.environ["M2L_MIN_UNIQUE"])
l2m_min_unique = int(os.environ["L2M_MIN_UNIQUE"])
min_radio_rx_forwarded = int(os.environ["MIN_RADIO_RX_FORWARDED"])
max_m2l_decrypt_failures = int(os.environ["MAX_M2L_DECRYPT_FAILURES"])
max_l2m_decrypt_failures = int(os.environ["MAX_L2M_DECRYPT_FAILURES"])
decrypt_failure_gate = os.environ.get("DECRYPT_FAILURE_GATE", "post-session")
session_acquire_mode = os.environ.get("SESSION_ACQUIRE_MODE", "observed")
if session_acquire_mode in {"0", "false", "no", "off", "disabled", "warmup-only"}:
    session_acquire_mode = "disabled"
else:
    session_acquire_mode = "observed"
m2l_enabled = os.environ.get("ENABLE_M2L", "1").lower() not in {"0", "false", "no"}
l2m_enabled = os.environ.get("ENABLE_L2M", "1").lower() not in {"0", "false", "no"}
expected_payloads = int(os.environ.get("EXPECTED_PAYLOADS", 0))
m2l_expected_payloads = int(os.environ.get("M2L_EXPECTED_PAYLOADS", expected_payloads))
l2m_expected_payloads = int(os.environ.get("L2M_EXPECTED_PAYLOADS", expected_payloads))
expected_source_events = (
    m2l_expected_payloads * int(m2l_enabled)
    + l2m_expected_payloads * int(l2m_enabled)
)
m2l_decrypt_stats = decrypt_stats(run / "peer" / "wfb-rx-m2l.log")
l2m_decrypt_stats = decrypt_stats(run / "peer" / "wfb-rx-l2m-agg.log")
if decrypt_failure_gate == "total":
    m2l_decrypt_failures = m2l_decrypt_stats["total"]
    l2m_decrypt_failures = l2m_decrypt_stats["total"]
else:
    m2l_decrypt_failures = m2l_decrypt_stats["after_session"]
    l2m_decrypt_failures = l2m_decrypt_stats["after_session"]
m2l_unknown_encapsulation = count_lines(run / "peer" / "wfb-rx-m2l.log", "unknown encapsulation")
require_calibration_success = os.environ["REQUIRE_CALIBRATION_SUCCESS"]
calibration_success_required = require_calibration_success in {"1", "true", "yes"}
if require_calibration_success == "auto":
    calibration_success_required = report.get("calibration_profile") == "rtl8812a_runtime_iqk"
link_id = int(os.environ.get("WFB_CLI_LINK_ID") or "1")
m2l_radio_port = int(os.environ.get("M2L_RADIO_PORT", "0"))
l2m_radio_port = int(os.environ.get("L2M_RADIO_PORT", "1"))
l2m_source_port = os.environ.get("LINUX_L2M_SOURCE_PORT", "5621")
iface = os.environ.get("IFACE", "wfb0")
expected_channel = int(os.environ.get("CHANNEL", "36"))
health_counters = health.get("counters") if isinstance(health.get("counters"), dict) else {}
signal = rx.get("signal") if isinstance(rx.get("signal"), dict) else {}
peer_wfb0_monitor_ready = (
    peer_channel_state["traffic"].get("type") == "monitor"
    and peer_channel_state["traffic"].get("channel") == expected_channel
    and peer_channel_state["traffic"].get("width_mhz") == 20
)
l2m_peer_wfb_tx_on_iface = log_contains(
    run / "peer" / "wfb-tx-l2m-rf.log",
    f"Listen on {l2m_source_port} for {iface}",
)
l2m_mac_usb_rx_advanced = (
    int(health_counters.get("usb_bulk_in_reads") or 0) > 0
    and int(health_counters.get("rx_frames") or 0) > 0
)
l2m_mac_rx_wfb_channel_matched = observation_matches(
    rx.get("wfb_channel_observations"),
    link_id,
    l2m_radio_port,
)
l2m_mac_rx_signal_present = (
    metric_sample_count(signal, "snr_db") > 0
    and metric_sample_count(signal, "rssi_dbm") > 0
)
m2l_mac_tx_submitted = int(tx.get("submitted_frames") or 0) > 0
m2l_mac_tx_wfb_channel_matched = observation_matches(
    tx.get("wfb_channel_observations"),
    link_id,
    m2l_radio_port,
)
rf_proof = {
    "peer_wfb0_monitor_ready": peer_wfb0_monitor_ready,
    "l2m": {
        "enabled": l2m_enabled,
        "peer_wfb_tx_on_iface": l2m_peer_wfb_tx_on_iface,
        "mac_usb_rx_advanced": l2m_mac_usb_rx_advanced,
        "mac_rx_wfb_channel_matched": l2m_mac_rx_wfb_channel_matched,
        "mac_rx_signal_present": l2m_mac_rx_signal_present,
        "radio_rx_forwarded": radio_rx_forwarded,
        "wfb_rx_session_observed": l2m_decrypt_stats["session_observed"],
    },
    "m2l": {
        "enabled": m2l_enabled,
        "mac_tx_submitted": m2l_mac_tx_submitted,
        "mac_tx_wfb_channel_matched": m2l_mac_tx_wfb_channel_matched,
        "peer_wfb_rx_session_observed": m2l_decrypt_stats["session_observed"],
        "peer_counter_unique_sequences": m2l_unique,
    },
}
failures = []
if report.get("result") != "pass":
    failures.append(f"radio_result={report.get('result')}")
if health.get("lifecycle") != "exited_pass":
    failures.append(f"health_lifecycle={health.get('lifecycle')}")
if health.get("result") != "pass":
    failures.append(f"health_result={health.get('result')}")
if health.get("operator_action") != "monitor":
    failures.append(f"health_operator_action={health.get('operator_action')}")
traffic_channel = peer_channel_state["traffic"].get("channel")
traffic_width_mhz = peer_channel_state["traffic"].get("width_mhz")
traffic_type = peer_channel_state["traffic"].get("type")
if traffic_type != "monitor":
    failures.append(f"peer_wfb0_type={traffic_type}")
if traffic_channel != expected_channel:
    failures.append(f"peer_wfb0_channel={traffic_channel}!={expected_channel}")
if traffic_width_mhz != 20:
    failures.append(f"peer_wfb0_width_mhz={traffic_width_mhz}!=20")
if not peer_wfb0_monitor_ready:
    failures.append("rf_proof_peer_wfb0_monitor_ready=false")
if l2m_enabled:
    if not l2m_peer_wfb_tx_on_iface:
        failures.append("rf_proof_l2m_peer_wfb_tx_on_iface=false")
    if not l2m_mac_usb_rx_advanced:
        failures.append("rf_proof_l2m_mac_usb_rx_advanced=false")
    if not l2m_mac_rx_wfb_channel_matched:
        failures.append("rf_proof_l2m_mac_rx_wfb_channel_matched=false")
    if not l2m_mac_rx_signal_present:
        failures.append("rf_proof_l2m_mac_rx_signal_present=false")
    if not l2m_decrypt_stats["session_observed"]:
        failures.append("rf_proof_l2m_wfb_rx_session_observed=false")
if m2l_enabled:
    if not m2l_mac_tx_submitted:
        failures.append("rf_proof_m2l_mac_tx_submitted=false")
    if not m2l_mac_tx_wfb_channel_matched:
        failures.append("rf_proof_m2l_mac_tx_wfb_channel_matched=false")
    if not m2l_decrypt_stats["session_observed"]:
        failures.append("rf_proof_m2l_peer_wfb_rx_session_observed=false")
if (tx.get("failed_submissions") or 0) != 0:
    failures.append(f"tx_failed_submissions={tx.get('failed_submissions')}")
if (tx.get("dropped_datagrams") or 0) != 0:
    failures.append(f"tx_dropped_datagrams={tx.get('dropped_datagrams')}")
if m2l_enabled and m2l_unique < m2l_min_unique:
    failures.append(f"m2l_unique_sequences={m2l_unique}<{m2l_min_unique}")
if l2m_enabled and l2m_unique < l2m_min_unique:
    failures.append(f"l2m_unique_sequences={l2m_unique}<{l2m_min_unique}")
if l2m_enabled and radio_rx_forwarded < min_radio_rx_forwarded:
    failures.append(f"radio_rx_forwarded={radio_rx_forwarded}<{min_radio_rx_forwarded}")
if session_acquire_mode == "observed" and source_gate.get("acquired") is not True:
    failures.append(f"source_session_gate={source_gate.get('status', 'missing')}")
if expected_source_events and source_summary.get("marked_source_events") != expected_source_events:
    failures.append(
        f"source_marked_events={source_summary.get('marked_source_events')}!={expected_source_events}"
    )
if expected_source_events and not isinstance(source_summary.get("source_phase_sec"), dict):
    failures.append("source_phase_sec=missing")
if m2l_enabled and m2l_decrypt_failures > max_m2l_decrypt_failures:
    failures.append(f"m2l_decrypt_failures={m2l_decrypt_failures}>{max_m2l_decrypt_failures}")
if l2m_enabled and l2m_decrypt_failures > max_l2m_decrypt_failures:
    failures.append(f"l2m_decrypt_failures={l2m_decrypt_failures}>{max_l2m_decrypt_failures}")
if calibration_success_required and runtime_iqk.get("status") not in {"completed", "success"}:
    failures.append(f"runtime_iqk_status={runtime_iqk.get('status')}")
if calibration_success_required and runtime_iqk.get("cleanup_status") != "restored":
    failures.append(f"runtime_iqk_cleanup_status={runtime_iqk.get('cleanup_status')}")
if calibration_success_required and runtime_iqk.get("selected_iqc_fill_applied") is not True:
    failures.append(
        f"runtime_iqk_selected_iqc_fill_applied={runtime_iqk.get('selected_iqc_fill_applied')}"
    )
summary = {
    "smoke_result": "fail" if failures else "pass",
    "failures": failures,
    "network": {
        "linux_host": os.environ.get("LINUX_HOST"),
        "linux_lan_ip": os.environ.get("LINUX_LAN_IP"),
        "linux_lan_ip_requested": os.environ.get("LINUX_LAN_IP_REQUESTED"),
        "mac_lan_ip": os.environ.get("MAC_LAN_IP"),
    },
    "peer_channel_state": peer_channel_state,
    "radio_result": report.get("result"),
    "radio_command": os.environ.get("RADIO_COMMAND"),
    "tx_power_mode": os.environ.get("TX_POWER_MODE"),
    "tx_power_safety_profile": os.environ.get("TX_POWER_SAFETY_PROFILE"),
    "tx_calibration_profile": os.environ.get("TX_CALIBRATION_PROFILE"),
    "service_health": health,
    "stop_reason": report.get("stop_reason"),
    "tx": tx,
    "rx": rx,
    "radio_rx_forwarded_from_snapshots": radio_rx_forwarded,
    "radio_rx_forwards": rx_forwards,
    "rf_proof": rf_proof,
    "directions": {
        "m2l_enabled": m2l_enabled,
        "l2m_enabled": l2m_enabled,
    },
    "link_profile": {
        "m2l_fec_k": int(os.environ.get("M2L_FEC_K", 0)),
        "m2l_fec_n": int(os.environ.get("M2L_FEC_N", 0)),
        "l2m_fec_k": int(os.environ.get("L2M_FEC_K", 0)),
        "l2m_fec_n": int(os.environ.get("L2M_FEC_N", 0)),
        "m2l_mcs": int(os.environ.get("M2L_MCS", 0)),
        "l2m_mcs": int(os.environ.get("L2M_MCS", 0)),
        "payload_interval_sec": float(os.environ.get("PAYLOAD_INTERVAL_SEC", 0)),
        "m2l_payload_interval_sec": float(os.environ.get("M2L_PAYLOAD_INTERVAL_SEC", os.environ.get("PAYLOAD_INTERVAL_SEC", 0))),
        "l2m_payload_interval_sec": float(os.environ.get("L2M_PAYLOAD_INTERVAL_SEC", os.environ.get("PAYLOAD_INTERVAL_SEC", 0))),
        "payload_len": int(os.environ.get("PAYLOAD_LEN", 0)),
        "expected_payloads": expected_payloads,
        "m2l_expected_payloads": m2l_expected_payloads,
        "l2m_expected_payloads": l2m_expected_payloads,
        "traffic_mode": os.environ.get("DUPLEX_TRAFFIC_MODE"),
        "tdd_first_direction": os.environ.get("TDD_FIRST_DIRECTION"),
        "tdd_guard_sec": float(os.environ.get("TDD_GUARD_SEC", 0)),
        "airtime_mode": os.environ.get("AIRTIME_MODE"),
        "airtime_tdd_first_window": os.environ.get("AIRTIME_TDD_FIRST_WINDOW"),
        "airtime_tdd_rx_window_ms": int(os.environ.get("AIRTIME_TDD_RX_WINDOW_MS", 0)),
        "airtime_tdd_tx_window_ms": int(os.environ.get("AIRTIME_TDD_TX_WINDOW_MS", 0)),
        "airtime_tdd_guard_ms": int(os.environ.get("AIRTIME_TDD_GUARD_MS", 0)),
        "airtime_tdd_start_delay_ms": int(os.environ.get("AIRTIME_TDD_START_DELAY_MS", 0)),
        "runtime_airtime": report.get("airtime"),
        "radio_run_config": os.environ.get("RADIO_RUN_CONFIG"),
        "m2l_ingress_mode": os.environ.get("M2L_INGRESS_MODE"),
        "m2l_distributor_host": os.environ.get("M2L_DISTRIBUTOR_HOST"),
        "wfb_rx_rcv_buf_bytes": int(os.environ.get("WFB_RX_RCV_BUF_BYTES", 0)),
        "wfb_rx_snd_buf_bytes": int(os.environ.get("WFB_RX_SND_BUF_BYTES", 0)),
        "linux_udp_buf_sysctl_bytes": int(os.environ.get("LINUX_UDP_BUF_SYSCTL_BYTES", 0)),
    },
    "calibration": {
        "profile": report.get("calibration_profile"),
        "class": report.get("calibration_class"),
        "evidence_source": report.get("calibration_evidence_source"),
        "receiver_backed_validation_required": report.get("receiver_backed_validation_required"),
        "runtime_iqk_status": runtime_iqk.get("status"),
        "runtime_iqk_cleanup_status": runtime_iqk.get("cleanup_status"),
        "runtime_iqk_sweep_index": runtime_iqk.get("sweep_index"),
        "runtime_iqk_sweep_count": runtime_iqk.get("sweep_count"),
        "runtime_iqk_fallback_stage_count": last_sweep_summary.get("fallback_stage_count"),
        "runtime_iqk_selected_iqc_fill_applied": runtime_iqk.get("selected_iqc_fill_applied"),
        "runtime_iqk_selected_iqc_fill_register_count": runtime_iqk.get("selected_iqc_fill_register_count"),
        "calibration_success_required": calibration_success_required,
    },
    "m2l_min_unique": m2l_min_unique,
    "l2m_min_unique": l2m_min_unique,
    "max_m2l_decrypt_failures": max_m2l_decrypt_failures,
    "max_l2m_decrypt_failures": max_l2m_decrypt_failures,
    "decrypt_failure_gate": decrypt_failure_gate,
    "source_gate": source_gate,
    "source_summary": source_summary,
    "peer_wfb_rx": {
        "m2l_decrypt_failures": m2l_decrypt_failures,
        "m2l_decrypt_failures_total": m2l_decrypt_stats["total"],
        "m2l_decrypt_failures_before_session": m2l_decrypt_stats["before_session"],
        "m2l_decrypt_failures_after_session": m2l_decrypt_stats["after_session"],
        "m2l_session_observed": m2l_decrypt_stats["session_observed"],
        "l2m_decrypt_failures": l2m_decrypt_failures,
        "l2m_decrypt_failures_total": l2m_decrypt_stats["total"],
        "l2m_decrypt_failures_before_session": l2m_decrypt_stats["before_session"],
        "l2m_decrypt_failures_after_session": l2m_decrypt_stats["after_session"],
        "l2m_session_observed": l2m_decrypt_stats["session_observed"],
        "m2l_unknown_encapsulation": m2l_unknown_encapsulation,
    },
    "m2l_counter": compact_counter(m2l),
    "l2m_counter": compact_counter(l2m),
}
(run / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(json.dumps(summary, indent=2, sort_keys=True))
if failures:
    sys.exit(1)
PY
}

log "output directory: $OUT_DIR"
if ! prepare_peer; then
  collect_peer_artifacts || true
  write_radio_startup_failure_summary "linux_peer_preparation_failed" || true
  die "Linux peer preparation failed; partial artifacts copied to $OUT_DIR/peer"
fi
start_m2l_ingress_relay
start_radio
wait_for_radio_ready
if ! run_peer_traffic; then
  radio_status=0
  if [[ -n "${RADIO_PID:-}" ]]; then
    kill "$RADIO_PID" >/dev/null 2>&1 || true
    wait "$RADIO_PID" || radio_status=$?
    RADIO_PID=
  fi
  collect_peer_artifacts || true
  write_summary || write_radio_startup_failure_summary "linux_peer_traffic_failed" "$radio_status" || true
  die "Linux peer traffic failed; partial artifacts copied to $OUT_DIR/peer"
fi
radio_status=0
if wait "$RADIO_PID"; then
  RADIO_PID=
else
  radio_status=$?
  RADIO_PID=
  collect_peer_artifacts || true
  write_summary || true
  die "radio-run exited with status $radio_status; summary written to $OUT_DIR/summary.json"
fi
collect_peer_artifacts
write_summary
trap - EXIT INT TERM
cleanup
log "done: $OUT_DIR"
