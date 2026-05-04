#!/usr/bin/env bash
# shellcheck disable=SC2029,SC2088
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-rf-quality-close-range.sh [--dry-run] [--skip-report] [--out-dir DIR]

Runs the accepted close-range channel 36 HT20 RF-quality workflow across:
  local checkout -> hardware Mac (local or remote) -> Linux WFB peer

Configuration is via environment variables. Common overrides:
  HW_MAC_HOST=rownd@rownds-macbook-pro.tail5c793f.ts.net
  LOCAL_HW=1                 # run the hardware-Mac side on this checkout
  HW_MAC_HOST=local          # shorthand for LOCAL_HW=1
  HW_REPO_PATH=projects/arc/wfb-mac-radio-agent
  HW_DEPLOY=1 HW_DEPLOY_PATH=projects/arc/wfb-mac-radio-deploy
  MAC_RADIO_COMMAND=bridge-tx-listen
  LINUX_HOST=drone-2f389.local
  LINUX_SSH_JUMP=rownd@rownds-macbook-pro.tail5c793f.ts.net
  LINUX_SSH_NESTED=1         # use "ssh jump ssh linux" instead of ProxyJump
  LINUX_REMOTE_PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
  LINUX_REQUIRE_IW=0
  LINUX_REQUIRE_PEER_ISOLATION=1 LINUX_PEER_SETTLE_SECONDS=2
  LINUX_NM_UNMANAGE_IFACE=1 LINUX_FORCE_MONITOR=1
  MAC_LAN_IP=10.42.0.162
  FIRMWARE=/tmp/rtl8812aefw.bin
  EFUSE_REPORT=/tmp/wfb-remote-macos-efuse-dump.json
  EXPECTED_PAYLOADS=2000 PAYLOAD_LEN=1000 CHANNEL=36 BANDWIDTH_MHZ=20
  SOURCE_WARMUP_PAYLOADS=400

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
LOCAL_HW=${LOCAL_HW:-0}
case "$HW_MAC_HOST" in
  local|localhost|127.0.0.1)
    LOCAL_HW=1
    HW_MAC_HOST=local
    ;;
esac
HW_REPO_PATH_WAS_SET=${HW_REPO_PATH+x}
HW_REPO_PATH=${HW_REPO_PATH:-}
if [[ "$LOCAL_HW" == "1" && -z "${HW_REPO_PATH_WAS_SET:-}" ]]; then
  HW_REPO_PATH=$REPO_ROOT
elif [[ -z "$HW_REPO_PATH" ]]; then
  HW_REPO_PATH='projects/arc/wfb-mac-radio-agent'
fi
LINUX_HOST=${LINUX_HOST:-drone-2f389.local}
LINUX_SSH_JUMP=${LINUX_SSH_JUMP:-}
LINUX_SSH_NESTED=${LINUX_SSH_NESTED:-0}
LINUX_REMOTE_PATH=${LINUX_REMOTE_PATH:-/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin}
LINUX_REQUIRE_IW=${LINUX_REQUIRE_IW:-0}
LINUX_REQUIRE_PEER_ISOLATION=${LINUX_REQUIRE_PEER_ISOLATION:-1}
LINUX_PEER_SETTLE_SECONDS=${LINUX_PEER_SETTLE_SECONDS:-2}
LINUX_NM_UNMANAGE_IFACE=${LINUX_NM_UNMANAGE_IFACE:-1}
LINUX_FORCE_MONITOR=${LINUX_FORCE_MONITOR:-1}
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
SOURCE_WARMUP_PAYLOADS=${SOURCE_WARMUP_PAYLOADS:-400}
THEORETICAL_MAX_DATAGRAMS=${THEORETICAL_MAX_DATAGRAMS:-$(((EXPECTED_PAYLOADS * FEC_N + FEC_K - 1) / FEC_K))}
THEORETICAL_WARMUP_DATAGRAMS=$(((SOURCE_WARMUP_PAYLOADS * FEC_N + FEC_K - 1) / FEC_K))
THEORETICAL_TOTAL_DATAGRAMS=${THEORETICAL_TOTAL_DATAGRAMS:-$((THEORETICAL_MAX_DATAGRAMS + THEORETICAL_WARMUP_DATAGRAMS))}
MAX_DATAGRAMS=${MAX_DATAGRAMS:-$THEORETICAL_TOTAL_DATAGRAMS}
DATAGRAM_SHORTFALL_TOLERANCE=${DATAGRAM_SHORTFALL_TOLERANCE:-1}
PAYLOAD_LEN=${PAYLOAD_LEN:-1000}
PAYLOAD_MARKER=${PAYLOAD_MARKER:-RFQCLSEF}
PAYLOAD_INTERVAL_SEC=${PAYLOAD_INTERVAL_SEC:-0.0005}
RX_STARTUP_SECONDS=${RX_STARTUP_SECONDS:-3}
TX_STARTUP_SECONDS=${TX_STARTUP_SECONDS:-2}

LINK_ID=${LINK_ID:-0x000001}
RADIO_PORT=${RADIO_PORT:-0}
RADIO_PORT_HEX=${RADIO_PORT_HEX:-0x00}
TX_RATE=${TX_RATE:-mcs1}
TX_PROFILE=${TX_PROFILE:-linux-monitor}
TX_POWER_MODE=${TX_POWER_MODE:-efuse-derived}
TX_POWER_SAFETY_PROFILE=${TX_POWER_SAFETY_PROFILE:-linux-ch36-ht20}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}
if [[ -z "${CALIBRATION_MODE+x}" ]]; then
  if [[ "$TX_CALIBRATION_PROFILE" == "linux-parity-ch36-ht20" ]]; then
    CALIBRATION_MODE=targeted-linux-parity
  elif [[ "$TX_CALIBRATION_PROFILE" == "rtl8812a-lck" ]]; then
    CALIBRATION_MODE=runtime-approximation
  elif [[ "$TX_CALIBRATION_PROFILE" == "rtl8812a-runtime-iqk" ]]; then
    CALIBRATION_MODE=runtime-approximation
  elif [[ "$TX_CALIBRATION_PROFILE" == "rtl8812a-iqk-probe" ]]; then
    CALIBRATION_MODE=stop-gap-captured
  else
    CALIBRATION_MODE=stop-gap-captured
  fi
fi
PROFILE_KIND=${PROFILE_KIND:-close-range}
PROFILE_NAME=${PROFILE_NAME:-close-range-ch36-ht20-efuse-$RUN_ID}

RELAY_BIND_IP=${RELAY_BIND_IP:-$MAC_LAN_IP}
RELAY_PORT=${RELAY_PORT:-5610}
BRIDGE_BIND_HOST=${BRIDGE_BIND_HOST:-127.0.0.1}
BRIDGE_BIND_PORT=${BRIDGE_BIND_PORT:-5611}
BRIDGE_READY_WAIT_SECONDS=${BRIDGE_READY_WAIT_SECONDS:-90}
BRIDGE_START_DELAY=${BRIDGE_START_DELAY:-0}
BRIDGE_IDLE_TIMEOUT_MS=${BRIDGE_IDLE_TIMEOUT_MS:-60000}
BRIDGE_WAIT_SECONDS=${BRIDGE_WAIT_SECONDS:-140}
MAC_RADIO_COMMAND=${MAC_RADIO_COMMAND:-bridge-tx-listen}
RADIO_RUN_DURATION_MS=${RADIO_RUN_DURATION_MS:-$((BRIDGE_WAIT_SECONDS * 1000))}

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
  if [[ "$LOCAL_HW" != "1" ]]; then
    require_command scp
  fi
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

export RUN_ID HW_MAC_HOST LOCAL_HW HW_REPO_PATH LINUX_HOST LINUX_SSH_JUMP LINUX_SSH_NESTED MAC_LAN_IP REMOTE_PREFIX
export LINUX_REMOTE_PATH LINUX_REQUIRE_IW LINUX_REQUIRE_PEER_ISOLATION LINUX_PEER_SETTLE_SECONDS LINUX_NM_UNMANAGE_IFACE LINUX_FORCE_MONITOR
export CHANNEL BANDWIDTH_MHZ FEC_K FEC_N EXPECTED_PAYLOADS SOURCE_WARMUP_PAYLOADS THEORETICAL_MAX_DATAGRAMS THEORETICAL_WARMUP_DATAGRAMS THEORETICAL_TOTAL_DATAGRAMS MAX_DATAGRAMS DATAGRAM_SHORTFALL_TOLERANCE
export PAYLOAD_LEN PAYLOAD_MARKER PAYLOAD_INTERVAL_SEC RX_STARTUP_SECONDS TX_STARTUP_SECONDS LINK_ID RADIO_PORT RADIO_PORT_HEX
export TX_RATE TX_PROFILE TX_POWER_MODE TX_POWER_SAFETY_PROFILE TX_CALIBRATION_PROFILE CALIBRATION_MODE PROFILE_KIND PROFILE_NAME
export RELAY_BIND_IP RELAY_PORT BRIDGE_BIND_HOST BRIDGE_BIND_PORT BRIDGE_READY_WAIT_SECONDS BRIDGE_START_DELAY BRIDGE_IDLE_TIMEOUT_MS BRIDGE_WAIT_SECONDS MAC_RADIO_COMMAND RADIO_RUN_DURATION_MS
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
  if [[ "$LOCAL_HW" == "1" ]]; then
    log "LOCAL_HW=1; using local checkout as hardware-Mac repo: $HW_REPO_PATH"
    return 0
  fi

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
    "RUN_ID", "HW_MAC_HOST", "LOCAL_HW", "HW_REPO_PATH", "LINUX_HOST", "LINUX_SSH_JUMP", "LINUX_SSH_NESTED", "MAC_LAN_IP",
    "REMOTE_PREFIX", "LINUX_REMOTE_PATH", "LINUX_REQUIRE_IW", "LINUX_REQUIRE_PEER_ISOLATION", "LINUX_PEER_SETTLE_SECONDS",
    "LINUX_NM_UNMANAGE_IFACE", "LINUX_FORCE_MONITOR", "CHANNEL", "BANDWIDTH_MHZ", "FEC_K", "FEC_N",
    "EXPECTED_PAYLOADS", "SOURCE_WARMUP_PAYLOADS", "THEORETICAL_MAX_DATAGRAMS", "THEORETICAL_WARMUP_DATAGRAMS", "THEORETICAL_TOTAL_DATAGRAMS",
    "MAX_DATAGRAMS", "DATAGRAM_SHORTFALL_TOLERANCE", "PAYLOAD_LEN", "PAYLOAD_MARKER",
    "RX_STARTUP_SECONDS", "TX_STARTUP_SECONDS",
    "LINK_ID", "RADIO_PORT", "TX_RATE", "TX_PROFILE", "TX_POWER_MODE",
    "TX_POWER_SAFETY_PROFILE", "TX_CALIBRATION_PROFILE", "CALIBRATION_MODE", "PROFILE_KIND",
    "PROFILE_NAME", "RELAY_BIND_IP", "RELAY_PORT", "BRIDGE_BIND_HOST",
    "BRIDGE_BIND_PORT", "BRIDGE_READY_WAIT_SECONDS", "BRIDGE_START_DELAY",
    "BRIDGE_IDLE_TIMEOUT_MS", "BRIDGE_WAIT_SECONDS", "MAC_RADIO_COMMAND",
    "RADIO_RUN_DURATION_MS", "IFACE", "WFB_SERVICE", "WFB_KEY",
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
  LOCAL_HW=$LOCAL_HW HW_DEPLOY=$HW_DEPLOY HW_DEPLOY_PATH=$HW_DEPLOY_PATH SYNC_HW_REPO=$SYNC_HW_REPO
  MAC_RADIO_COMMAND=$MAC_RADIO_COMMAND RADIO_RUN_DURATION_MS=$RADIO_RUN_DURATION_MS
  TX_POWER_MODE=$TX_POWER_MODE TX_CALIBRATION_PROFILE=$TX_CALIBRATION_PROFILE CALIBRATION_MODE=$CALIBRATION_MODE
  $(if [[ "$LOCAL_HW" == "1" ]]; then printf 'run relay/radio locally from %s\n' "$dry_bridge_path"; elif [[ "$HW_DEPLOY" == "1" ]]; then printf 'rsync local checkout to %s:%s\n' "$HW_MAC_HOST" "$HW_DEPLOY_PATH"; else printf 'no local deploy sync\n'; fi)
  $(if [[ "$LOCAL_HW" == "1" ]]; then printf 'local'; else printf 'ssh %s' "$(quote "$HW_MAC_HOST")"; fi) '<start UDP relay $RELAY_BIND_IP:$RELAY_PORT -> $BRIDGE_BIND_HOST:$BRIDGE_BIND_PORT>'
  $(if [[ "$LOCAL_HW" == "1" ]]; then printf 'local'; else printf 'ssh %s' "$(quote "$HW_MAC_HOST")"; fi) '<cd $dry_bridge_path && cargo run ... $MAC_RADIO_COMMAND --macos-usbhost --channel $CHANNEL --bandwidth $BANDWIDTH_MHZ --bind $BRIDGE_BIND_HOST:$BRIDGE_BIND_PORT --max-datagrams $MAX_DATAGRAMS>'
  $(if [[ "$LOCAL_HW" == "1" ]]; then printf 'local'; else printf 'ssh %s' "$(quote "$HW_MAC_HOST")"; fi) '<wait up to ${BRIDGE_READY_WAIT_SECONDS}s for ${REMOTE_PREFIX}-bridge-ready.json before Linux traffic>'

Linux peer:
  LINUX_REMOTE_PATH=$LINUX_REMOTE_PATH LINUX_REQUIRE_IW=$LINUX_REQUIRE_IW
  LINUX_REQUIRE_PEER_ISOLATION=$LINUX_REQUIRE_PEER_ISOLATION LINUX_PEER_SETTLE_SECONDS=$LINUX_PEER_SETTLE_SECONDS
  LINUX_NM_UNMANAGE_IFACE=$LINUX_NM_UNMANAGE_IFACE LINUX_FORCE_MONITOR=$LINUX_FORCE_MONITOR
  $(if [[ "$LOCAL_HW" == "1" && -n "$LINUX_SSH_JUMP" && "$LINUX_SSH_NESTED" == "1" ]]; then printf 'ssh %s ssh %s' "$(quote "$LINUX_SSH_JUMP")" "$(quote "$LINUX_HOST")"; elif [[ "$LOCAL_HW" == "1" && -n "$LINUX_SSH_JUMP" ]]; then printf 'ssh -J %s %s' "$(quote "$LINUX_SSH_JUMP")" "$(quote "$LINUX_HOST")"; elif [[ "$LOCAL_HW" == "1" ]]; then printf 'ssh %s' "$(quote "$LINUX_HOST")"; else printf 'ssh %s ssh %s' "$(quote "$HW_MAC_HOST")" "$(quote "$LINUX_HOST")"; fi) '<preflight commands; stop $WFB_SERVICE if docker exists; set channel with iw if available; start tcpdump/wfb_rx/wfb_tx; generate $SOURCE_WARMUP_PAYLOADS warmup payloads and $EXPECTED_PAYLOADS measured payloads>'

Local collection:
  $(if [[ "$LOCAL_HW" == "1" ]]; then printf 'copy local hardware reports from %s-*' "$REMOTE_PREFIX"; else printf 'scp hardware Mac reports from %s-*' "$REMOTE_PREFIX"; fi)
  $(if [[ "$LOCAL_HW" == "1" && -n "$LINUX_SSH_JUMP" && "$LINUX_SSH_NESTED" == "1" ]]; then printf 'stream Linux artifacts through nested jump via %s' "$LINUX_SSH_JUMP"; elif [[ "$LOCAL_HW" == "1" && -n "$LINUX_SSH_JUMP" ]]; then printf 'stream Linux artifacts through ProxyJump via %s' "$LINUX_SSH_JUMP"; elif [[ "$LOCAL_HW" == "1" ]]; then printf 'stream Linux artifacts through direct ssh'; else printf 'stream Linux artifacts through nested ssh via %s' "$HW_MAC_HOST"; fi)
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

hw_exec() {
  local cmd=$1
  if [[ "$LOCAL_HW" == "1" ]]; then
    bash -lc "$cmd"
  else
    ssh "$HW_MAC_HOST" "$cmd"
  fi
}

linux_exec() {
  local inner=$1
  if [[ "$LOCAL_HW" == "1" ]]; then
    if [[ -n "$LINUX_SSH_JUMP" && "$LINUX_SSH_NESTED" == "1" ]]; then
      ssh "$LINUX_SSH_JUMP" "ssh $(quote "$LINUX_HOST") $inner"
    elif [[ -n "$LINUX_SSH_JUMP" ]]; then
      ssh -J "$LINUX_SSH_JUMP" "$LINUX_HOST" "$inner"
    else
      ssh "$LINUX_HOST" "$inner"
    fi
  else
    ssh "$HW_MAC_HOST" "ssh $(quote "$LINUX_HOST") $inner"
  fi
}

stop_hw_pid_file() {
  local pid_file=$1
  local label=$2
  hw_exec "pid_file=$(quote "$pid_file"); label=$(quote "$label"); if [[ -f \"\$pid_file\" ]]; then pid=\$(cat \"\$pid_file\" 2>/dev/null || true); if [[ -n \"\$pid\" ]]; then kill \"\$pid\" >/dev/null 2>&1 || true; fi; fi" \
    >"$OUT_DIR/cleanup-$label.log" 2>&1 || true
}

restore_linux_peer() {
  local remote_cmd
  remote_cmd="$(env_assignments WFB_SERVICE LINUX_SOURCE_PORT LINUX_RX_PORT MAC_LAN_IP RELAY_PORT IFACE LINUX_REMOTE_PATH) bash -s"
  linux_exec "$remote_cmd" >"$OUT_DIR/cleanup-linux-restore.log" 2>&1 <<'LINUX_RESTORE' || true
set +e
export PATH="$LINUX_REMOTE_PATH:$PATH"
resolve_cmd() {
  command -v "$1" 2>/dev/null || return 1
}
sudo_bin=$(resolve_cmd sudo || true)
pkill_bin=$(resolve_cmd pkill || true)
docker_bin=$(resolve_cmd docker || true)
if [[ -n "$sudo_bin" && -n "$pkill_bin" ]]; then
  "$sudo_bin" -n "$pkill_bin" -f "wfb_tx .* -u ${LINUX_SOURCE_PORT} ${MAC_LAN_IP}:${RELAY_PORT}" >/dev/null 2>&1 || true
  "$sudo_bin" -n "$pkill_bin" -f "wfb_rx .* -u ${LINUX_RX_PORT} ${IFACE}" >/dev/null 2>&1 || true
fi
if [[ -n "$sudo_bin" && -n "$docker_bin" ]]; then
  "$sudo_bin" -n "$docker_bin" restart "$WFB_SERVICE" >/dev/null 2>&1 || "$sudo_bin" -n "$docker_bin" start "$WFB_SERVICE" >/dev/null 2>&1 || true
  "$sudo_bin" -n "$docker_bin" ps --filter "name=$WFB_SERVICE" --format '{{.Names}} {{.Status}}'
else
  echo "docker or sudo unavailable during restore; skipped service restart"
fi
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
  hw_exec "$remote_cmd" <<'MAC_RELAY'
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
  remote_cmd="$(env_assignments REMOTE_PREFIX HW_REPO_PATH SYNC_HW_REPO FIRMWARE CHANNEL BANDWIDTH_MHZ BRIDGE_BIND_HOST BRIDGE_BIND_PORT MAX_DATAGRAMS BRIDGE_IDLE_TIMEOUT_MS BRIDGE_WAIT_SECONDS MAC_RADIO_COMMAND RADIO_RUN_DURATION_MS TX_POWER_MODE EFUSE_REPORT TX_POWER_SAFETY_PROFILE TX_CALIBRATION_PROFILE) bash -s"
  log "starting hardware-Mac $MAC_RADIO_COMMAND listener"
  hw_exec "$remote_cmd" <<'MAC_BRIDGE'
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
write_auth_arg=
case "$TX_CALIBRATION_PROFILE" in
  linux-parity-ch36-ht20|rtl8812a-lck|rtl8812a-runtime-iqk)
    write_auth_arg=--i-understand-this-writes-registers
    ;;
esac
tx_power_args=()
if [[ "$TX_POWER_MODE" != "current-default" ]]; then
  tx_power_args+=(--tx-power-mode "$TX_POWER_MODE")
  if [[ "$TX_POWER_MODE" == "efuse-derived" ]]; then
    tx_power_args+=(
      --tx-power-efuse-report "$EFUSE_REPORT"
      --tx-power-safety-profile "$TX_POWER_SAFETY_PROFILE"
    )
  fi
fi
case "$MAC_RADIO_COMMAND" in
  bridge-tx-listen)
    nohup cargo run -p wfb-radio-diag -- --json \
      --report "${REMOTE_PREFIX}-listen.json" \
      bridge-tx-listen \
      --macos-usbhost \
      --vid 0x0bda --pid 0x8812 \
      --init-before-tx \
      --firmware "$FIRMWARE" \
      --channel "$CHANNEL" --bandwidth "$BANDWIDTH_MHZ" \
      --bind "${BRIDGE_BIND_HOST}:${BRIDGE_BIND_PORT}" \
      --ready-file "${REMOTE_PREFIX}-bridge-ready.json" \
      --max-datagrams "$MAX_DATAGRAMS" \
      --idle-timeout-ms "$BRIDGE_IDLE_TIMEOUT_MS" \
      ${tx_power_args[@]+"${tx_power_args[@]}"} \
      --tx-calibration-profile "$TX_CALIBRATION_PROFILE" \
      --i-understand-this-transmits \
      ${write_auth_arg:+"$write_auth_arg"} \
      > "${REMOTE_PREFIX}-bridge.log" 2>&1 &
    ;;
  radio-run)
    nohup cargo run -p wfb-radio-diag -- --json \
      --report "${REMOTE_PREFIX}-listen.json" \
      radio-run \
      --macos-usbhost \
      --vid 0x0bda --pid 0x8812 \
      --firmware "$FIRMWARE" \
      --channel "$CHANNEL" --bandwidth "$BANDWIDTH_MHZ" \
      --bind "${BRIDGE_BIND_HOST}:${BRIDGE_BIND_PORT}" \
      --ready-file "${REMOTE_PREFIX}-bridge-ready.json" \
      --max-datagrams "$MAX_DATAGRAMS" \
      --duration-ms "$RADIO_RUN_DURATION_MS" \
      ${tx_power_args[@]+"${tx_power_args[@]}"} \
      --tx-calibration-profile "$TX_CALIBRATION_PROFILE" \
      --i-understand-this-transmits \
      ${write_auth_arg:+"$write_auth_arg"} \
      > "${REMOTE_PREFIX}-bridge.log" 2>&1 &
    ;;
  *)
    echo "unsupported MAC_RADIO_COMMAND=$MAC_RADIO_COMMAND" >&2
    exit 2
    ;;
esac
pid=$!
echo "$pid" > "${REMOTE_PREFIX}-bridge.pid"
sleep 2
kill -0 "$pid"
MAC_BRIDGE
  STARTED_BRIDGE=1
}

wait_for_bridge_ready() {
  local remote_cmd
  remote_cmd="$(env_assignments REMOTE_PREFIX BRIDGE_READY_WAIT_SECONDS) bash -s"
  log "waiting for bridge ready marker"
  hw_exec "$remote_cmd" >"$OUT_DIR/bridge-ready-wait.log" 2>&1 <<'MAC_WAIT_READY'
set -euo pipefail
ready_file="${REMOTE_PREFIX}-bridge-ready.json"
pid_file="${REMOTE_PREFIX}-bridge.pid"
for ((i = 0; i < BRIDGE_READY_WAIT_SECONDS; i++)); do
  if [[ -f "$ready_file" ]]; then
    echo "bridge ready marker observed after ${i}s"
    cat "$ready_file"
    exit 0
  fi
  if [[ -f "$pid_file" ]]; then
    pid=$(cat "$pid_file" 2>/dev/null || true)
    if [[ -n "$pid" ]] && ! kill -0 "$pid" >/dev/null 2>&1; then
      echo "bridge exited before writing ready marker"
      exit 2
    fi
  fi
  sleep 1
done
echo "timed out waiting for bridge ready marker: $ready_file"
exit 124
MAC_WAIT_READY
}

run_linux_peer() {
  local remote_cmd
  remote_cmd="$(env_assignments REMOTE_PREFIX IFACE CHANNEL BANDWIDTH_MHZ WFB_SERVICE WFB_KEY LINK_ID RADIO_PORT FEC_K FEC_N LINUX_SOURCE_PORT LINUX_RX_PORT MAC_LAN_IP RELAY_PORT EXPECTED_PAYLOADS SOURCE_WARMUP_PAYLOADS PAYLOAD_LEN PAYLOAD_MARKER PAYLOAD_INTERVAL_SEC RX_STARTUP_SECONDS TX_STARTUP_SECONDS TCPDUMP_SECONDS RX_SECONDS TX_SECONDS COUNTER_SECONDS LINUX_REMOTE_PATH LINUX_REQUIRE_IW LINUX_REQUIRE_PEER_ISOLATION LINUX_PEER_SETTLE_SECONDS LINUX_NM_UNMANAGE_IFACE LINUX_FORCE_MONITOR) bash -s"
  if [[ "$LOCAL_HW" == "1" && -n "$LINUX_SSH_JUMP" && "$LINUX_SSH_NESTED" == "1" ]]; then
    log "running Linux peer sender/receiver through nested jump $LINUX_SSH_JUMP -> $LINUX_HOST"
  elif [[ "$LOCAL_HW" == "1" && -n "$LINUX_SSH_JUMP" ]]; then
    log "running Linux peer sender/receiver through ProxyJump $LINUX_SSH_JUMP -> $LINUX_HOST"
  elif [[ "$LOCAL_HW" == "1" ]]; then
    log "running Linux peer sender/receiver through direct ssh -> $LINUX_HOST"
  else
    log "running Linux peer sender/receiver through $HW_MAC_HOST -> $LINUX_HOST"
  fi
  STARTED_LINUX=1
  linux_exec "$remote_cmd" <<'LINUX_RUN'
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"

setup_log="${REMOTE_PREFIX}-setup.log"
restore_log="${REMOTE_PREFIX}-restore.log"
summary_json="${REMOTE_PREFIX}-summary.json"
counter_json="${REMOTE_PREFIX}-counter.json"
receiver_health_json="${REMOTE_PREFIX}-receiver-health.json"
restore_json="${REMOTE_PREFIX}-restore.json"
preflight_json="${REMOTE_PREFIX}-preflight.json"
preflight_log="${REMOTE_PREFIX}-preflight.log"
channel_state_json="${REMOTE_PREFIX}-channel-state.json"

resolve_cmd() {
  local name=$1
  local path dir
  if path=$(command -v "$name" 2>/dev/null); then
    printf '%s\n' "$path"
    return 0
  fi
  IFS=: read -r -a search_dirs <<< "$LINUX_REMOTE_PATH"
  for dir in "${search_dirs[@]}"; do
    [[ -n "$dir" && -x "$dir/$name" ]] || continue
    printf '%s\n' "$dir/$name"
    return 0
  done
  return 1
}

PYTHON3_BIN=$(resolve_cmd python3 || true)
SUDO_BIN=$(resolve_cmd sudo || true)
TIMEOUT_BIN=$(resolve_cmd timeout || true)
WFB_RX_BIN=$(resolve_cmd wfb_rx || true)
WFB_TX_BIN=$(resolve_cmd wfb_tx || true)
DOCKER_BIN=$(resolve_cmd docker || true)
IW_BIN=$(resolve_cmd iw || true)
IP_BIN=$(resolve_cmd ip || true)
TCPDUMP_BIN=$(resolve_cmd tcpdump || true)
PKILL_BIN=$(resolve_cmd pkill || true)
PS_BIN=$(resolve_cmd ps || true)
GREP_BIN=$(resolve_cmd grep || true)
DATE_BIN=$(resolve_cmd date || true)
NMCLI_BIN=$(resolve_cmd nmcli || true)

capture_wfb_process_matches() {
  if [[ -z "${PS_BIN:-}" || -z "${GREP_BIN:-}" ]]; then
    return 0
  fi
  "$PS_BIN" -eo pid,user,comm,args \
    | "$GREP_BIN" -E '(^|[[:space:]/])(arc-wfb-link|wfb_rx|wfb_tx)([[:space:]]|$)' \
    | "$GREP_BIN" -v grep || true
}

missing_required=()
[[ -n "$PYTHON3_BIN" ]] || missing_required+=(python3)
[[ -n "$SUDO_BIN" ]] || missing_required+=(sudo)
[[ -n "$TIMEOUT_BIN" ]] || missing_required+=(timeout)
[[ -n "$WFB_RX_BIN" ]] || missing_required+=(wfb_rx)
[[ -n "$WFB_TX_BIN" ]] || missing_required+=(wfb_tx)
missing_optional=()
[[ -n "$DOCKER_BIN" ]] || missing_optional+=(docker)
[[ -n "$IW_BIN" ]] || missing_optional+=(iw)
[[ -n "$IP_BIN" ]] || missing_optional+=(ip)
[[ -n "$TCPDUMP_BIN" ]] || missing_optional+=(tcpdump)
[[ -n "$PKILL_BIN" ]] || missing_optional+=(pkill)
[[ -n "$PS_BIN" ]] || missing_optional+=(ps)
[[ -n "$GREP_BIN" ]] || missing_optional+=(grep)
[[ -n "$DATE_BIN" ]] || missing_optional+=(date)
[[ -n "$NMCLI_BIN" ]] || missing_optional+=(nmcli)

sudo_noninteractive=unknown
iface_status=unknown
wfb_key_status=unknown
docker_service_state=unknown
preflight_process_matches=""
policy_blockers=()

if [[ -n "$SUDO_BIN" ]]; then
  if "$SUDO_BIN" -n true >/dev/null 2>&1; then
    sudo_noninteractive=ok
  else
    sudo_noninteractive=blocked
    policy_blockers+=(sudo_noninteractive_unavailable)
  fi
fi

if [[ -n "$IP_BIN" ]]; then
  if "$IP_BIN" link show "$IFACE" >/dev/null 2>&1; then
    iface_status=present
  else
    iface_status=missing
    policy_blockers+=(interface_missing)
  fi
fi

if [[ -n "$SUDO_BIN" && "$sudo_noninteractive" == "ok" ]]; then
  if "$SUDO_BIN" -n test -r "$WFB_KEY" >/dev/null 2>&1; then
    wfb_key_status=readable
  else
    wfb_key_status=unreadable
    policy_blockers+=(wfb_key_unreadable)
  fi
fi

if [[ -n "$SUDO_BIN" && "$sudo_noninteractive" == "ok" && -n "$DOCKER_BIN" ]]; then
  docker_service_state=$("$SUDO_BIN" -n "$DOCKER_BIN" ps -a --filter "name=$WFB_SERVICE" --format '{{.Names}} {{.Status}}' 2>/dev/null || true)
  [[ -n "$docker_service_state" ]] || docker_service_state=not_found
fi

if [[ -n "$PS_BIN" && -n "$GREP_BIN" ]]; then
  preflight_process_matches=$(capture_wfb_process_matches)
fi

preflight_status=ok
preflight_degraded=0
if (( ${#missing_required[@]} > 0 || ${#policy_blockers[@]} > 0 )); then
  preflight_status=blocked
elif [[ "$LINUX_REQUIRE_PEER_ISOLATION" == "1" && ( -z "$PS_BIN" || -z "$GREP_BIN" ) ]]; then
  preflight_status=blocked
  policy_blockers+=(peer_isolation_requires_ps_and_grep)
elif [[ -z "$IW_BIN" && "$LINUX_REQUIRE_IW" == "1" ]]; then
  preflight_status=blocked
  policy_blockers+=(iw_required_but_missing)
elif (( ${#missing_optional[@]} > 0 )); then
  preflight_status=degraded
  preflight_degraded=1
fi

{
  printf 'status=%s\n' "$preflight_status"
  printf 'linux_remote_path=%s\n' "$LINUX_REMOTE_PATH"
  printf 'linux_require_iw=%s\n' "$LINUX_REQUIRE_IW"
  printf 'linux_require_peer_isolation=%s\n' "$LINUX_REQUIRE_PEER_ISOLATION"
  printf 'linux_peer_settle_seconds=%s\n' "$LINUX_PEER_SETTLE_SECONDS"
  printf 'linux_nm_unmanage_iface=%s\n' "$LINUX_NM_UNMANAGE_IFACE"
  printf 'linux_force_monitor=%s\n' "$LINUX_FORCE_MONITOR"
  printf 'python3=%s\n' "${PYTHON3_BIN:-MISSING}"
  printf 'sudo=%s\n' "${SUDO_BIN:-MISSING}"
  printf 'timeout=%s\n' "${TIMEOUT_BIN:-MISSING}"
  printf 'wfb_rx=%s\n' "${WFB_RX_BIN:-MISSING}"
  printf 'wfb_tx=%s\n' "${WFB_TX_BIN:-MISSING}"
  printf 'docker=%s\n' "${DOCKER_BIN:-MISSING}"
  printf 'iw=%s\n' "${IW_BIN:-MISSING}"
  printf 'ip=%s\n' "${IP_BIN:-MISSING}"
  printf 'tcpdump=%s\n' "${TCPDUMP_BIN:-MISSING}"
  printf 'pkill=%s\n' "${PKILL_BIN:-MISSING}"
  printf 'ps=%s\n' "${PS_BIN:-MISSING}"
  printf 'grep=%s\n' "${GREP_BIN:-MISSING}"
  printf 'date=%s\n' "${DATE_BIN:-MISSING}"
  printf 'nmcli=%s\n' "${NMCLI_BIN:-MISSING}"
  printf 'sudo_noninteractive=%s\n' "$sudo_noninteractive"
  printf 'iface_status=%s\n' "$iface_status"
  printf 'wfb_key_status=%s\n' "$wfb_key_status"
  printf 'docker_service_state=%s\n' "$docker_service_state"
  printf 'preflight_wfb_process_matches=%s\n' "${preflight_process_matches:-}"
  printf 'missing_required=%s\n' "${missing_required[*]:-}"
  printf 'missing_optional=%s\n' "${missing_optional[*]:-}"
  printf 'policy_blockers=%s\n' "${policy_blockers[*]:-}"
} > "$preflight_log"

if [[ -n "$PYTHON3_BIN" ]]; then
  export preflight_status preflight_degraded
  export sudo_noninteractive iface_status wfb_key_status docker_service_state
  export preflight_process_matches LINUX_REQUIRE_PEER_ISOLATION LINUX_PEER_SETTLE_SECONDS
  export PYTHON3_BIN SUDO_BIN TIMEOUT_BIN WFB_RX_BIN WFB_TX_BIN DOCKER_BIN IW_BIN IP_BIN TCPDUMP_BIN PKILL_BIN PS_BIN GREP_BIN DATE_BIN NMCLI_BIN
  export LINUX_REMOTE_PATH LINUX_REQUIRE_IW LINUX_NM_UNMANAGE_IFACE LINUX_FORCE_MONITOR IFACE WFB_KEY WFB_SERVICE
  "$PYTHON3_BIN" - "$preflight_json" "${missing_required[*]:-}" "${missing_optional[*]:-}" "${policy_blockers[*]:-}" <<'PY'
import json
import os
import sys

commands = {
    "python3": os.environ.get("PYTHON3_BIN") or None,
    "sudo": os.environ.get("SUDO_BIN") or None,
    "timeout": os.environ.get("TIMEOUT_BIN") or None,
    "wfb_rx": os.environ.get("WFB_RX_BIN") or None,
    "wfb_tx": os.environ.get("WFB_TX_BIN") or None,
    "docker": os.environ.get("DOCKER_BIN") or None,
    "iw": os.environ.get("IW_BIN") or None,
    "ip": os.environ.get("IP_BIN") or None,
    "tcpdump": os.environ.get("TCPDUMP_BIN") or None,
    "pkill": os.environ.get("PKILL_BIN") or None,
    "ps": os.environ.get("PS_BIN") or None,
    "grep": os.environ.get("GREP_BIN") or None,
    "date": os.environ.get("DATE_BIN") or None,
    "nmcli": os.environ.get("NMCLI_BIN") or None,
}
missing_required = [name for name in sys.argv[2].split() if name]
missing_optional = [name for name in sys.argv[3].split() if name]
policy_blockers = [name for name in sys.argv[4].split() if name]
report = {
    "status": os.environ["preflight_status"],
    "degraded": os.environ["preflight_degraded"] == "1",
    "linux_remote_path": os.environ.get("LINUX_REMOTE_PATH", ""),
    "linux_require_iw": os.environ.get("LINUX_REQUIRE_IW", "") == "1",
    "linux_require_peer_isolation": os.environ.get("LINUX_REQUIRE_PEER_ISOLATION", "") == "1",
    "linux_peer_settle_seconds": float(os.environ.get("LINUX_PEER_SETTLE_SECONDS", "0") or 0),
    "linux_nm_unmanage_iface": os.environ.get("LINUX_NM_UNMANAGE_IFACE", "") == "1",
    "linux_force_monitor": os.environ.get("LINUX_FORCE_MONITOR", "") == "1",
    "interface": os.environ.get("IFACE", ""),
    "wfb_key": os.environ.get("WFB_KEY", ""),
    "wfb_service": os.environ.get("WFB_SERVICE", ""),
    "commands": commands,
    "sudo_noninteractive": os.environ.get("sudo_noninteractive"),
    "iface_status": os.environ.get("iface_status"),
    "wfb_key_status": os.environ.get("wfb_key_status"),
    "docker_service_state": os.environ.get("docker_service_state"),
    "preflight_wfb_process_match_count": len([line for line in os.environ.get("preflight_process_matches", "").splitlines() if line.strip()]),
    "preflight_wfb_process_matches": [
        line for line in os.environ.get("preflight_process_matches", "").splitlines() if line.strip()
    ],
    "missing_required": missing_required,
    "missing_optional": missing_optional,
    "policy_blockers": policy_blockers,
}
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2, sort_keys=True)
    fh.write("\n")
PY
else
  printf '{"status":"blocked","missing_required":["python3"]}\n' > "$preflight_json"
fi

if [[ "$preflight_status" == "blocked" ]]; then
  cat "$preflight_log" > "$setup_log"
  exit 2
fi

case "$BANDWIDTH_MHZ" in
  20) iw_bandwidth=HT20 ;;
  40) iw_bandwidth=HT40+ ;;
  *) echo "unsupported BANDWIDTH_MHZ=$BANDWIDTH_MHZ" > "$setup_log"; exit 2 ;;
esac

cleanup_linux_completed=0
cleanup_linux() {
  set +e
  if (( cleanup_linux_completed == 1 )); then
    return
  fi
  local restore_status=ok
  local restore_service_action=skipped
  local restore_service_state=unknown
  local restore_process_matches=""
  local restore_channel_iw_info=""
  if [[ -n "$SUDO_BIN" && -n "$PKILL_BIN" ]]; then
    "$SUDO_BIN" -n "$PKILL_BIN" -f "wfb_tx .* -u ${LINUX_SOURCE_PORT} ${MAC_LAN_IP}:${RELAY_PORT}" >/dev/null 2>&1 || true
    "$SUDO_BIN" -n "$PKILL_BIN" -f "wfb_rx .* -u ${LINUX_RX_PORT} ${IFACE}" >/dev/null 2>&1 || true
    "$SUDO_BIN" -n "$PKILL_BIN" -f "tcpdump -i ${IFACE} .*${REMOTE_PREFIX}-rf.pcap" >/dev/null 2>&1 || true
  fi
  if [[ -n "${counter_pid:-}" ]]; then kill "$counter_pid" >/dev/null 2>&1 || true; fi
  {
    if [[ -n "$DATE_BIN" ]]; then "$DATE_BIN"; else date; fi
    if [[ -n "$SUDO_BIN" && -n "$DOCKER_BIN" ]]; then
      restore_service_action=start
      "$SUDO_BIN" -n "$DOCKER_BIN" start "$WFB_SERVICE" || true
      restore_service_state=$("$SUDO_BIN" -n "$DOCKER_BIN" ps --filter "name=$WFB_SERVICE" --format '{{.Names}} {{.Status}}' 2>/dev/null || true)
      [[ -n "$restore_service_state" ]] || restore_service_state=not_running
      printf '%s\n' "$restore_service_state"
      if [[ "$restore_service_state" == "not_running" ]]; then
        restore_status=degraded
      fi
    else
      restore_status=degraded
      restore_service_action=skipped
      restore_service_state=unavailable
      echo "docker or sudo unavailable; skipped service restore"
    fi
    if [[ -n "$PS_BIN" && -n "$GREP_BIN" ]]; then
      restore_process_matches=$("$PS_BIN" -eo pid,user,comm,args | "$GREP_BIN" -Ei 'arc-wfb|wfb' | "$GREP_BIN" -v grep || true)
      printf '%s\n' "$restore_process_matches"
    fi
    if [[ -n "$IW_BIN" ]]; then
      restore_channel_iw_info=$("$IW_BIN" dev "$IFACE" info 2>&1 || true)
      printf '%s\n' "$restore_channel_iw_info"
    fi
  } > "$restore_log" 2>&1
  if [[ -n "$PYTHON3_BIN" ]]; then
    export restore_status restore_service_action restore_service_state restore_process_matches restore_channel_iw_info
    export WFB_SERVICE IFACE REMOTE_PREFIX
    "$PYTHON3_BIN" - "$restore_json" <<'PY' || true
import json
import os
import sys

process_matches = [
    line for line in os.environ.get("restore_process_matches", "").splitlines() if line.strip()
]
report = {
    "source": "scripts/run-rf-quality-close-range.sh",
    "remote_prefix": os.environ.get("REMOTE_PREFIX", ""),
    "status": os.environ.get("restore_status", "unknown"),
    "wfb_service": os.environ.get("WFB_SERVICE", ""),
    "interface": os.environ.get("IFACE", ""),
    "service_action": os.environ.get("restore_service_action", "unknown"),
    "service_state": os.environ.get("restore_service_state", "unknown"),
    "process_match_count": len(process_matches),
    "process_matches": process_matches,
    "post_restore_iw_info": [
        line for line in os.environ.get("restore_channel_iw_info", "").splitlines() if line.strip()
    ],
}
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2, sort_keys=True)
    fh.write("\n")
PY
  fi
  cleanup_linux_completed=1
}
trap cleanup_linux EXIT INT TERM

rm -f "${REMOTE_PREFIX}"-{setup,restore,summary,counter,source,rx,tx,tcpdump}.log \
  "${REMOTE_PREFIX}"-{summary,counter,receiver-health,restore,channel-state}.json \
  "${REMOTE_PREFIX}-rf.pcap"

channel_set_status=skipped
channel_iw_info=""
nm_unmanage_status=skipped
monitor_set_status=skipped
link_down_status=skipped
link_up_status=skipped
peer_isolation_status=unknown
peer_process_matches_before_stop=""
peer_process_matches_after_stop=""
{
  if [[ -n "$DATE_BIN" ]]; then "$DATE_BIN"; else date; fi
  cat "$preflight_log"
  if [[ -n "$PS_BIN" && -n "$GREP_BIN" ]]; then
    peer_process_matches_before_stop=$(capture_wfb_process_matches)
    printf 'peer_process_matches_before_stop:\n%s\n' "${peer_process_matches_before_stop:-<none>}"
  else
    peer_isolation_status=unverified
    echo "ps or grep unavailable; skipped peer process isolation check"
  fi
  if [[ -n "$SUDO_BIN" && -n "$DOCKER_BIN" ]]; then
    "$SUDO_BIN" -n "$DOCKER_BIN" stop "$WFB_SERVICE" || true
  else
    echo "docker or sudo unavailable; skipped service stop"
  fi
  if [[ -n "$SUDO_BIN" && -n "$PKILL_BIN" ]]; then
    "$SUDO_BIN" -n "$PKILL_BIN" -f "wfb_tx .* -u ${LINUX_SOURCE_PORT} ${MAC_LAN_IP}:${RELAY_PORT}" >/dev/null 2>&1 || true
    "$SUDO_BIN" -n "$PKILL_BIN" -f "wfb_rx .* -u ${LINUX_RX_PORT} ${IFACE}" >/dev/null 2>&1 || true
    "$SUDO_BIN" -n "$PKILL_BIN" -f "tcpdump -i ${IFACE} .*${REMOTE_PREFIX}-rf.pcap" >/dev/null 2>&1 || true
  else
    echo "sudo or pkill unavailable; skipped stale test process cleanup"
  fi
  if [[ -n "$PS_BIN" && -n "$GREP_BIN" ]]; then
    if [[ "${LINUX_PEER_SETTLE_SECONDS:-0}" != "0" ]]; then
      sleep "$LINUX_PEER_SETTLE_SECONDS"
    fi
    peer_process_matches_after_stop=$(capture_wfb_process_matches)
    printf 'peer_process_matches_after_stop:\n%s\n' "${peer_process_matches_after_stop:-<none>}"
    if [[ -z "$peer_process_matches_after_stop" ]]; then
      peer_isolation_status=ok
    else
      peer_isolation_status=residual_processes
    fi
  fi
  printf 'peer_isolation_status=%s\n' "$peer_isolation_status"
  if [[ "$LINUX_REQUIRE_PEER_ISOLATION" == "1" && "$peer_isolation_status" != "ok" ]]; then
    echo "peer isolation required but status=$peer_isolation_status"
  fi
  if [[ "$LINUX_NM_UNMANAGE_IFACE" == "1" ]]; then
    if [[ -n "$SUDO_BIN" && -n "$NMCLI_BIN" ]]; then
      if "$SUDO_BIN" -n "$NMCLI_BIN" dev set "$IFACE" managed no; then
        nm_unmanage_status=ok
      else
        nm_unmanage_status=failed
      fi
      "$SUDO_BIN" -n "$NMCLI_BIN" dev set "p2p-dev-${IFACE}" managed no >/dev/null 2>&1 || true
    else
      nm_unmanage_status=skipped_nmcli_or_sudo_missing
      echo "nmcli or sudo unavailable; skipped NetworkManager unmanaged guard"
    fi
  else
    nm_unmanage_status=disabled
  fi
  printf 'nm_unmanage_status=%s\n' "$nm_unmanage_status"
  if [[ "$LINUX_FORCE_MONITOR" == "1" ]]; then
    if [[ -n "$SUDO_BIN" && -n "$IW_BIN" && -n "$IP_BIN" ]]; then
      if "$SUDO_BIN" -n "$IP_BIN" link set "$IFACE" down; then
        link_down_status=ok
      else
        link_down_status=failed
      fi
      if "$SUDO_BIN" -n "$IW_BIN" dev "$IFACE" set type monitor; then
        monitor_set_status=ok
      else
        monitor_set_status=failed
      fi
      if "$SUDO_BIN" -n "$IP_BIN" link set "$IFACE" up; then
        link_up_status=ok
      else
        link_up_status=failed
      fi
    else
      monitor_set_status=skipped_iw_ip_or_sudo_missing
      echo "iw, ip, or sudo unavailable; skipped monitor-mode guard"
    fi
  else
    monitor_set_status=disabled
  fi
  printf 'link_down_status=%s\n' "$link_down_status"
  printf 'monitor_set_status=%s\n' "$monitor_set_status"
  printf 'link_up_status=%s\n' "$link_up_status"
  if [[ -n "$SUDO_BIN" && -n "$IW_BIN" ]]; then
    if "$SUDO_BIN" -n "$IW_BIN" dev "$IFACE" set channel "$CHANNEL" "$iw_bandwidth"; then
      channel_set_status=ok
    else
      channel_set_status=failed
    fi
    channel_iw_info=$("$IW_BIN" dev "$IFACE" info 2>&1 || true)
    printf '%s\n' "$channel_iw_info"
  else
    channel_set_status=skipped_iw_or_sudo_missing
    echo "iw or sudo unavailable; skipped channel set and channel-state evidence"
  fi
  if [[ -n "$IP_BIN" ]]; then
    "$IP_BIN" addr show "$IFACE" || true
  else
    echo "ip unavailable; skipped interface address evidence"
  fi
} > "$setup_log" 2>&1

if [[ -n "$PYTHON3_BIN" ]]; then
  export channel_set_status channel_iw_info CHANNEL BANDWIDTH_MHZ IFACE IW_BIN SUDO_BIN
  export nm_unmanage_status monitor_set_status link_down_status link_up_status LINUX_NM_UNMANAGE_IFACE LINUX_FORCE_MONITOR
  export peer_isolation_status peer_process_matches_before_stop peer_process_matches_after_stop LINUX_REQUIRE_PEER_ISOLATION LINUX_PEER_SETTLE_SECONDS
  "$PYTHON3_BIN" - "$channel_state_json" <<'PY'
import json
import os
import re
import sys

requested_channel = int(os.environ.get("CHANNEL", "0"), 0)
requested_bandwidth_mhz = int(os.environ.get("BANDWIDTH_MHZ", "0"), 0)
iw_info = os.environ.get("channel_iw_info", "")
observed_channel = None
observed_frequency_mhz = None
observed_width_mhz = None
observed_type = None
type_match = re.search(r"^\s*type\s+(\S+)", iw_info, re.MULTILINE)
if type_match:
    observed_type = type_match.group(1)
match = re.search(r"channel\s+(\d+)\s+\((\d+)\s+MHz\),\s+width:\s*(\d+)\s+MHz", iw_info)
if match:
    observed_channel = int(match.group(1))
    observed_frequency_mhz = int(match.group(2))
    observed_width_mhz = int(match.group(3))

set_status = os.environ.get("channel_set_status", "unknown")
force_monitor = os.environ.get("LINUX_FORCE_MONITOR", "") == "1"
monitor_set_status = os.environ.get("monitor_set_status", "unknown")
if set_status.startswith("skipped"):
    verify_status = "skipped"
elif force_monitor and monitor_set_status != "ok":
    verify_status = "monitor_set_failed"
elif force_monitor and observed_type != "monitor":
    verify_status = "monitor_mismatch"
elif set_status != "ok":
    verify_status = "set_failed"
elif observed_channel is None or observed_width_mhz is None:
    verify_status = "set_unverified"
elif observed_channel == requested_channel and observed_width_mhz == requested_bandwidth_mhz:
    verify_status = "verified"
else:
    verify_status = "mismatch"

report = {
    "source": "scripts/run-rf-quality-close-range.sh",
    "interface": os.environ.get("IFACE", ""),
    "requested_channel": requested_channel,
    "requested_bandwidth_mhz": requested_bandwidth_mhz,
    "set_status": set_status,
    "nm_unmanage_status": os.environ.get("nm_unmanage_status", "unknown"),
    "nm_unmanage_requested": os.environ.get("LINUX_NM_UNMANAGE_IFACE", "") == "1",
    "monitor_set_status": monitor_set_status,
    "monitor_force_requested": force_monitor,
    "link_down_status": os.environ.get("link_down_status", "unknown"),
    "link_up_status": os.environ.get("link_up_status", "unknown"),
    "verify_status": verify_status,
    "observed_type": observed_type,
    "observed_channel": observed_channel,
    "observed_frequency_mhz": observed_frequency_mhz,
    "observed_width_mhz": observed_width_mhz,
    "iw_available": bool(os.environ.get("IW_BIN")),
    "sudo_available": bool(os.environ.get("SUDO_BIN")),
    "iw_info": iw_info.splitlines(),
    "peer_isolation_status": os.environ.get("peer_isolation_status", "unknown"),
    "peer_isolation_required": os.environ.get("LINUX_REQUIRE_PEER_ISOLATION", "") == "1",
    "peer_settle_seconds": float(os.environ.get("LINUX_PEER_SETTLE_SECONDS", "0") or 0),
    "peer_process_match_count_before_stop": len([
        line for line in os.environ.get("peer_process_matches_before_stop", "").splitlines() if line.strip()
    ]),
    "peer_process_matches_before_stop": [
        line for line in os.environ.get("peer_process_matches_before_stop", "").splitlines() if line.strip()
    ],
    "peer_process_match_count_after_stop": len([
        line for line in os.environ.get("peer_process_matches_after_stop", "").splitlines() if line.strip()
    ]),
    "peer_process_matches_after_stop": [
        line for line in os.environ.get("peer_process_matches_after_stop", "").splitlines() if line.strip()
    ],
}
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2, sort_keys=True)
    fh.write("\n")
PY
fi

if [[ "$LINUX_REQUIRE_PEER_ISOLATION" == "1" && "$peer_isolation_status" != "ok" ]]; then
  exit 2
fi

"$PYTHON3_BIN" - "$counter_json" "$LINUX_RX_PORT" "$EXPECTED_PAYLOADS" "$PAYLOAD_MARKER" "$COUNTER_SECONDS" <<'PY' &
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

tcpdump_pid=
if [[ -n "$TCPDUMP_BIN" ]]; then
  "$SUDO_BIN" -n "$TIMEOUT_BIN" "$TCPDUMP_SECONDS" "$TCPDUMP_BIN" -i "$IFACE" -s 256 -w "${REMOTE_PREFIX}-rf.pcap" > "${REMOTE_PREFIX}-tcpdump.log" 2>&1 &
  tcpdump_pid=$!
else
  echo "tcpdump unavailable; skipped RF pcap capture" > "${REMOTE_PREFIX}-tcpdump.log"
fi

"$SUDO_BIN" -n "$TIMEOUT_BIN" "$RX_SECONDS" \
  "$WFB_RX_BIN" -K "$WFB_KEY" -i "$LINK_ID" -p "$RADIO_PORT" -c 127.0.0.1 -u "$LINUX_RX_PORT" "$IFACE" \
  > "${REMOTE_PREFIX}-rx.log" 2>&1 &
rx_pid=$!

sleep "$RX_STARTUP_SECONDS"

"$SUDO_BIN" -n "$TIMEOUT_BIN" "$TX_SECONDS" \
  "$WFB_TX_BIN" -d -K "$WFB_KEY" -i "$LINK_ID" -p "$RADIO_PORT" -B "$BANDWIDTH_MHZ" -k "$FEC_K" -n "$FEC_N" \
  -u "$LINUX_SOURCE_PORT" "${MAC_LAN_IP}:${RELAY_PORT}" \
  > "${REMOTE_PREFIX}-tx.log" 2>&1 &
tx_pid=$!

sleep "$TX_STARTUP_SECONDS"

"$PYTHON3_BIN" - "$LINUX_SOURCE_PORT" "$EXPECTED_PAYLOADS" "$SOURCE_WARMUP_PAYLOADS" "$PAYLOAD_LEN" "$PAYLOAD_MARKER" "$PAYLOAD_INTERVAL_SEC" <<'PY' > "${REMOTE_PREFIX}-source.log" 2>&1
import socket
import sys
import time

port = int(sys.argv[1])
count = int(sys.argv[2])
warmup_count = int(sys.argv[3])
payload_len = int(sys.argv[4])
marker = sys.argv[5].encode("ascii")
interval = float(sys.argv[6])
prefix_len = len(marker) + 4
if payload_len < prefix_len:
    raise SystemExit("payload too short")
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
warmup_prefix = b"WFBWARM"[:max(0, min(7, payload_len))]
for i in range(warmup_count):
    seq = i.to_bytes(4, "big")
    fill_len = max(0, payload_len - len(warmup_prefix) - len(seq))
    payload = warmup_prefix + seq + (b"W" * fill_len)
    sock.sendto(payload, ("127.0.0.1", port))
    if interval > 0:
        time.sleep(interval)
for i in range(count):
    fill = bytes([65 + (i % 26)]) * (payload_len - prefix_len)
    payload = marker + i.to_bytes(4, "big") + fill
    sock.sendto(payload, ("127.0.0.1", port))
    if interval > 0:
        time.sleep(interval)
print(f"warmup_sent={warmup_count} sent={count} payload_len={payload_len}")
PY

wait "$tx_pid" || true
wait "$counter_pid" || true
"$SUDO_BIN" -n kill "$rx_pid" >/dev/null 2>&1 || true
if [[ -n "$tcpdump_pid" ]]; then "$SUDO_BIN" -n kill "$tcpdump_pid" >/dev/null 2>&1 || true; fi
wait "$rx_pid" >/dev/null 2>&1 || true
if [[ -n "$tcpdump_pid" ]]; then wait "$tcpdump_pid" >/dev/null 2>&1 || true; fi

"$PYTHON3_BIN" - "$receiver_health_json" "${REMOTE_PREFIX}-rx.log" "$counter_json" "$EXPECTED_PAYLOADS" <<'PY'
import json
import sys
from pathlib import Path

out_path = Path(sys.argv[1])
rx_path = Path(sys.argv[2])
counter_path = Path(sys.argv[3])
expected = int(sys.argv[4])

rx_text = rx_path.read_text(errors="ignore") if rx_path.exists() else ""
counter = {}
if counter_path.exists():
    counter = json.loads(counter_path.read_text())

def parse_int(value, base=10):
    try:
        return int(value, base)
    except (TypeError, ValueError):
        return None

def parse_rx_antenna_reports(text):
    reports = []
    for line in text.splitlines():
        if "\tRX_ANT\t" not in line:
            continue
        fields = line.split("\t")
        if len(fields) < 5:
            continue
        channel = fields[2].split(":")
        metrics = fields[4].split(":")
        report = {
            "raw_line": line,
            "timestamp_ms": parse_int(fields[0]),
            "antenna_id_hex": f"0x{fields[3]}",
            "antenna_id": parse_int(fields[3], 16),
        }
        if len(channel) >= 3:
            report.update({
                "freq_mhz": parse_int(channel[0]),
                "mcs_index": parse_int(channel[1]),
                "bandwidth_mhz": parse_int(channel[2]),
            })
        if len(metrics) >= 7:
            report.update({
                "count_all": parse_int(metrics[0]),
                "rssi_min_dbm": parse_int(metrics[1]),
                "rssi_avg_dbm": parse_int(metrics[2]),
                "rssi_max_dbm": parse_int(metrics[3]),
                "snr_min_db": parse_int(metrics[4]),
                "snr_avg_db": parse_int(metrics[5]),
                "snr_max_db": parse_int(metrics[6]),
            })
        reports.append(report)
    return reports

def numeric_values(reports, key):
    return [report[key] for report in reports if isinstance(report.get(key), (int, float))]

def receiver_metadata_summary(reports):
    latest_by_antenna = {}
    for report in reports:
        key = (
            report.get("freq_mhz"),
            report.get("mcs_index"),
            report.get("bandwidth_mhz"),
            report.get("antenna_id_hex"),
        )
        latest_by_antenna[key] = report
    summary = {
        "report_count": len(reports),
        "latest_by_antenna": list(latest_by_antenna.values()),
    }
    for source_key, min_key, max_key in [
        ("rssi_avg_dbm", "rssi_avg_dbm_min", "rssi_avg_dbm_max"),
        ("snr_avg_db", "snr_avg_db_min", "snr_avg_db_max"),
    ]:
        values = numeric_values(reports, source_key)
        if values:
            summary[min_key] = min(values)
            summary[max_key] = max(values)
    return summary

pkt_lines = [line for line in rx_text.splitlines() if "\tPKT\t" in line]
nonzero_pkt_lines = []
for line in pkt_lines:
    fields = line.split("\t", 2)
    if len(fields) != 3:
        continue
    counters = [part for part in fields[2].split(":") if part]
    if any(part != "0" for part in counters):
        nonzero_pkt_lines.append(line)

recovered = int(counter.get("recovered_payloads") or 0)
session_observed = "\tSESSION\t" in rx_text
unable_decrypt_count = rx_text.count("Unable to decrypt packet")
rx_antenna_reports = parse_rx_antenna_reports(rx_text)
if recovered >= expected:
    status = "ok"
elif not session_observed and unable_decrypt_count > 0:
    status = "missing_session"
elif unable_decrypt_count > 0:
    status = "decrypt_errors"
elif recovered == 0:
    status = "no_payloads"
else:
    status = "partial_payloads"

report = {
    "source": "scripts/run-rf-quality-close-range.sh",
    "status": status,
    "session_observed": session_observed,
    "unable_decrypt_count": unable_decrypt_count,
    "rx_antenna_report_count": len(rx_antenna_reports),
    "rx_antenna_reports": rx_antenna_reports,
    "rx_antenna_summary": receiver_metadata_summary(rx_antenna_reports),
    "rx_pkt_line_count": len(pkt_lines),
    "rx_nonzero_pkt_line_count": len(nonzero_pkt_lines),
    "last_nonzero_pkt_line": nonzero_pkt_lines[-1] if nonzero_pkt_lines else None,
    "expected_payloads": expected,
    "recovered_payloads": recovered,
    "counter": counter,
}
out_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY

cleanup_linux

"$PYTHON3_BIN" - "$summary_json" "$counter_json" "$setup_log" "$restore_log" "$preflight_json" "$receiver_health_json" "$restore_json" "$channel_state_json" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
counter_path = Path(sys.argv[2])
counter = {}
if counter_path.exists():
    counter = json.loads(counter_path.read_text())
preflight = {}
preflight_path = Path(sys.argv[5])
if preflight_path.exists():
    preflight = json.loads(preflight_path.read_text())
receiver_health = {}
receiver_health_path = Path(sys.argv[6])
if receiver_health_path.exists():
    receiver_health = json.loads(receiver_health_path.read_text())
linux_restore = {}
restore_path = Path(sys.argv[7])
if restore_path.exists():
    linux_restore = json.loads(restore_path.read_text())
channel_state = {}
channel_state_path = Path(sys.argv[8])
if channel_state_path.exists():
    channel_state = json.loads(channel_state_path.read_text())
summary = {
    "preflight": preflight,
    "counter": counter,
    "receiver_health": receiver_health,
    "linux_restore": linux_restore,
    "channel_state": channel_state,
    "artifacts": {
        "counter": str(counter_path),
        "receiver_health": sys.argv[6],
        "linux_restore": sys.argv[7],
        "channel_state": sys.argv[8],
        "setup_log": sys.argv[3],
        "restore_log": sys.argv[4],
        "preflight": sys.argv[5],
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
  if ! hw_exec "$remote_cmd" >"$OUT_DIR/bridge-wait.log" 2>&1 <<'MAC_WAIT'
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
  if [[ "$LOCAL_HW" == "1" ]]; then
    if cp "$remote_path" "$OUT_DIR/$name" >/dev/null 2>&1; then
      log "collected local hardware artifact: $name"
      return 0
    fi
  elif scp -q "$HW_MAC_HOST:$remote_path" "$OUT_DIR/$name" >/dev/null 2>&1; then
    log "collected hardware Mac artifact: $name"
    return 0
  fi
  printf 'hardware-mac:%s\n' "$remote_path" >>"$MISSING_ARTIFACTS"
}

copy_linux_artifact() {
  local remote_path=$1
  local name=${2:-$(basename "$remote_path")}
  if linux_exec "cat $(quote "$remote_path")" >"$OUT_DIR/$name" 2>/dev/null; then
    log "collected Linux artifact: $name"
  else
    rm -f "$OUT_DIR/$name"
    printf 'linux:%s\n' "$remote_path" >>"$MISSING_ARTIFACTS"
  fi
}

collect_artifacts() {
  log "collecting artifacts"
  copy_hw_artifact "${REMOTE_PREFIX}-listen.json"
  copy_hw_artifact "${REMOTE_PREFIX}-bridge-ready.json"
  copy_hw_artifact "${REMOTE_PREFIX}-bridge.log"
  copy_hw_artifact "${REMOTE_PREFIX}-relay.log"
  copy_hw_artifact "$EFUSE_REPORT" "$(basename "$EFUSE_REPORT")"

  copy_linux_artifact "${REMOTE_PREFIX}-rf.pcap"
  copy_linux_artifact "${REMOTE_PREFIX}-rx.log"
  copy_linux_artifact "${REMOTE_PREFIX}-tx.log"
  copy_linux_artifact "${REMOTE_PREFIX}-counter.json"
  copy_linux_artifact "${REMOTE_PREFIX}-receiver-health.json"
  copy_linux_artifact "${REMOTE_PREFIX}-source.log"
  copy_linux_artifact "${REMOTE_PREFIX}-setup.log"
  copy_linux_artifact "${REMOTE_PREFIX}-preflight.json"
  copy_linux_artifact "${REMOTE_PREFIX}-preflight.log"
  copy_linux_artifact "${REMOTE_PREFIX}-channel-state.json"
  copy_linux_artifact "${REMOTE_PREFIX}-restore.json"
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
  local receiver_health_report
  local restore_state_report
  local channel_state_report
  local recovered
  local datagram_evidence
  local receiver_args=()

  mac_report="$OUT_DIR/$(basename "${REMOTE_PREFIX}-listen.json")"
  counter_report="$OUT_DIR/$(basename "${REMOTE_PREFIX}-counter.json")"
  receiver_health_report="$OUT_DIR/$(basename "${REMOTE_PREFIX}-receiver-health.json")"
  restore_state_report="$OUT_DIR/$(basename "${REMOTE_PREFIX}-restore.json")"
  channel_state_report="$OUT_DIR/$(basename "${REMOTE_PREFIX}-channel-state.json")"
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

  datagram_evidence="$OUT_DIR/datagram-evidence.json"
  python3 - "$datagram_evidence" "$mac_report" "$counter_report" "$receiver_health_report" "$restore_state_report" "$channel_state_report" <<'PY'
import json
import os
import sys
from pathlib import Path

out_path = Path(sys.argv[1])
mac_report = json.loads(Path(sys.argv[2]).read_text())
counter_report = json.loads(Path(sys.argv[3]).read_text())
receiver_health_path = Path(sys.argv[4])
receiver_health = {}
if receiver_health_path.exists():
    receiver_health = json.loads(receiver_health_path.read_text())
restore_state_path = Path(sys.argv[5])
linux_restore = {}
if restore_state_path.exists():
    linux_restore = json.loads(restore_state_path.read_text())
channel_state_path = Path(sys.argv[6])
channel_state = {}
if channel_state_path.exists():
    channel_state = json.loads(channel_state_path.read_text())

def env_int(name):
    value = os.environ.get(name)
    if value is None or value == "":
        return None
    return int(value, 0)

submit = mac_report.get("submit_counters") or {}
bridge = mac_report.get("bridge_counters") or {}
tx = mac_report.get("tx") or {}
submitted = submit.get("submitted")
if submitted is None:
    submitted = bridge.get("injected")
if submitted is None:
    submitted = tx.get("submitted_frames")
observed = mac_report.get("datagrams_received")
if observed is None:
    observed = tx.get("datagrams_received")
if observed is None:
    observed = submitted

expected_payloads = env_int("EXPECTED_PAYLOADS")
source_warmup_payloads = env_int("SOURCE_WARMUP_PAYLOADS") or 0
recovered = counter_report.get("recovered_payloads")
theoretical = env_int("THEORETICAL_MAX_DATAGRAMS")
theoretical_warmup = env_int("THEORETICAL_WARMUP_DATAGRAMS") or 0
theoretical_total = env_int("THEORETICAL_TOTAL_DATAGRAMS")
bridge_max = env_int("MAX_DATAGRAMS")
tolerance = env_int("DATAGRAM_SHORTFALL_TOLERANCE") or 0
shortfall = theoretical_total - observed if theoretical_total is not None and observed is not None else None
complete_payload_recovery = (
    expected_payloads is not None
    and recovered is not None
    and recovered >= expected_payloads
)
within_tolerance = (
    shortfall is not None
    and 0 < shortfall <= tolerance
    and complete_payload_recovery
)

report = {
    "source": "scripts/run-rf-quality-close-range.sh",
    "theoretical_max_datagrams": theoretical,
    "source_warmup_payloads": source_warmup_payloads,
    "theoretical_warmup_datagrams": theoretical_warmup,
    "theoretical_total_datagrams": theoretical_total,
    "bridge_max_datagrams": bridge_max,
    "observed_datagrams": observed,
    "submitted_datagrams": submitted,
    "datagram_shortfall": shortfall,
    "shortfall_tolerance": tolerance,
    "short_run_tolerance_applied": within_tolerance,
    "expected_payloads": expected_payloads,
    "recovered_payloads": recovered,
    "complete_payload_recovery": complete_payload_recovery,
    "receiver_health": receiver_health,
    "receiver_session_observed": receiver_health.get("session_observed"),
    "receiver_status": receiver_health.get("status"),
    "receiver_unable_decrypt_count": receiver_health.get("unable_decrypt_count"),
    "channel_state": channel_state,
    "linux_restore": linux_restore,
    "note": "short smoke runs can emit one fewer WFB datagram than the FEC ceiling while still recovering every source payload",
}
out_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY

  for artifact in \
    "$OUT_DIR/$(basename "${REMOTE_PREFIX}-rf.pcap")" \
    "$OUT_DIR/$(basename "${REMOTE_PREFIX}-rx.log")" \
    "$OUT_DIR/$(basename "${REMOTE_PREFIX}-tx.log")" \
    "$datagram_evidence" \
    "$receiver_health_report" \
    "$channel_state_report" \
    "$restore_state_report" \
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
wait_for_bridge_ready
if (( BRIDGE_START_DELAY > 0 )); then
  log "waiting ${BRIDGE_START_DELAY}s before Linux traffic"
  sleep "$BRIDGE_START_DELAY"
fi
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
