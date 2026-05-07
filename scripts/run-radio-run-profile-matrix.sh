#!/usr/bin/env bash
# shellcheck disable=SC2029
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-radio-run-profile-matrix.sh [--dry-run] [--out-dir DIR]

Runs a small production radio-run profile matrix and writes ranked JSON and
Markdown summaries. The wrapped smoke can run on this Mac or on a remote
hardware Mac.

Common configuration:
  HW_MAC_HOST=rownd@rownds-macbook-pro.tail5c793f.ts.net
  LOCAL_HW=0                 # set to 1 or HW_MAC_HOST=local for local adapter
  HW_DEPLOY=1                # rsync local checkout to HW_DEPLOY_PATH first
  HW_DEPLOY_PATH=projects/arc/wfb-mac-radio-deploy
  LINUX_HOST=pi@drone-2f389.local
  LINUX_LAN_IP=10.42.0.1    # Linux peer LAN IP visible to remote hardware Mac
  MAC_LAN_IP=10.42.0.162     # remote Mac LAN IP visible to Linux peer
  PROFILE_SET=short          # short, range, or minimal
  ENABLE_M2L=1 ENABLE_L2M=1
  RADIO_COMMAND=service       # service or diagnostic
  REPEATS=1
  EXPECTED_PAYLOADS=80 SOURCE_WARMUP_PAYLOADS=100 SOURCE_TAIL_PAYLOADS=auto
  SESSION_ACQUIRE_MODE=observed SESSION_ACQUIRE_TIMEOUT_SECONDS=15
  DUPLEX_TRAFFIC_MODE=simultaneous TDD_FIRST_DIRECTION=l2m TDD_GUARD_SEC=2.0
  AIRTIME_MODE=continuous AIRTIME_TDD_FIRST_WINDOW=rx
  MATRIX_OUT_DIR=/tmp/wfb-radio-profile-matrix

Set PROFILE_FILE to a pipe-delimited profile list:
  name|description|m2l_k|m2l_n|l2m_k|l2m_n|m2l_mcs|l2m_mcs|interval_sec|m2l_min_pct|l2m_min_pct
  name|description|m2l_k|m2l_n|l2m_k|l2m_n|m2l_mcs|l2m_mcs|interval_sec|m2l_min_pct|l2m_min_pct|m2l_interval_sec|l2m_interval_sec|m2l_expected_payloads|l2m_expected_payloads
EOF
}

log() {
  printf '[matrix] %s\n' "$*" >&2
}

die() {
  printf '[matrix] error: %s\n' "$*" >&2
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
MATRIX_OUT_DIR=${MATRIX_OUT_DIR:-/tmp/wfb-radio-profile-matrix-$RUN_ID}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --out-dir)
      [[ $# -ge 2 ]] || die "--out-dir requires a path"
      MATRIX_OUT_DIR=$2
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
HW_DEPLOY=${HW_DEPLOY:-1}
HW_DEPLOY_PATH=${HW_DEPLOY_PATH:-projects/arc/wfb-mac-radio-deploy}
HW_DEPLOY_DELETE=${HW_DEPLOY_DELETE:-0}
HW_REPO_PATH=${HW_REPO_PATH:-projects/arc/wfb-mac-radio-agent}
SYNC_REALTEK_REF=${SYNC_REALTEK_REF:-1}
AUTO_FETCH_REALTEK_REF=${AUTO_FETCH_REALTEK_REF:-1}
REALTEK_REF_LOCAL_PATH=${REALTEK_REF_LOCAL_PATH:-/tmp/wfb-ref-rtl8812au}
REALTEK_REF_REMOTE_PATH=${REALTEK_REF_REMOTE_PATH:-/tmp/wfb-ref-rtl8812au}
REALTEK_REF_REPO=${REALTEK_REF_REPO:-https://github.com/aircrack-ng/rtl8812au.git}
REALTEK_REF_REQUIRED_FILE=${REALTEK_REF_REQUIRED_FILE:-hal/phydm/rtl8812a/halhwimg8812a_mac.c}
SSH_OPTS=${SSH_OPTS:-"-o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=2"}
# shellcheck disable=SC2206
SSH_OPTS_ARRAY=($SSH_OPTS)
if [[ "$LOCAL_HW" == "1" ]]; then
  HW_DEPLOY=0
  HW_REPO_PATH=$REPO_ROOT
fi

LINUX_HOST=${LINUX_HOST:-pi@drone-2f389.local}
LINUX_REMOTE_PATH=${LINUX_REMOTE_PATH:-/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin}
if [[ "$LOCAL_HW" == "1" ]]; then
  LINUX_LAN_IP=${LINUX_LAN_IP:-192.168.122.77}
  MAC_LAN_IP=${MAC_LAN_IP:-192.168.122.84}
else
  LINUX_LAN_IP=${LINUX_LAN_IP:-auto}
  MAC_LAN_IP=${MAC_LAN_IP:-auto}
fi

CHANNEL=${CHANNEL:-36}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
TX_POWER_MODE=${TX_POWER_MODE:-current-default}
TX_POWER_SAFETY_PROFILE=${TX_POWER_SAFETY_PROFILE:-linux-ch36-ht20}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}
REQUIRE_CALIBRATION_SUCCESS=${REQUIRE_CALIBRATION_SUCCESS:-auto}
DECRYPT_FAILURE_GATE=${DECRYPT_FAILURE_GATE:-post-session}
AUTO_EFUSE_DUMP=${AUTO_EFUSE_DUMP:-1}
EXPECTED_PAYLOADS=${EXPECTED_PAYLOADS:-80}
ENABLE_M2L=${ENABLE_M2L:-1}
ENABLE_L2M=${ENABLE_L2M:-1}
SOURCE_WARMUP_PAYLOADS=${SOURCE_WARMUP_PAYLOADS:-100}
SOURCE_TAIL_PAYLOADS=${SOURCE_TAIL_PAYLOADS:-auto}
SESSION_ACQUIRE_MODE=${SESSION_ACQUIRE_MODE:-observed}
SESSION_ACQUIRE_TIMEOUT_SECONDS=${SESSION_ACQUIRE_TIMEOUT_SECONDS:-15}
SESSION_ACQUIRE_POLL_SECONDS=${SESSION_ACQUIRE_POLL_SECONDS:-0.2}
PAYLOAD_LEN=${PAYLOAD_LEN:-1000}
DUPLEX_TRAFFIC_MODE=${DUPLEX_TRAFFIC_MODE:-simultaneous}
TDD_FIRST_DIRECTION=${TDD_FIRST_DIRECTION:-l2m}
TDD_GUARD_SEC=${TDD_GUARD_SEC:-2.0}
AIRTIME_MODE=${AIRTIME_MODE:-continuous}
AIRTIME_TDD_FIRST_WINDOW=${AIRTIME_TDD_FIRST_WINDOW:-rx}
AIRTIME_TDD_RX_WINDOW_MS=${AIRTIME_TDD_RX_WINDOW_MS:-1000}
AIRTIME_TDD_TX_WINDOW_MS=${AIRTIME_TDD_TX_WINDOW_MS:-1000}
AIRTIME_TDD_GUARD_MS=${AIRTIME_TDD_GUARD_MS:-0}
AIRTIME_TDD_START_DELAY_MS=${AIRTIME_TDD_START_DELAY_MS:-0}
COUNTER_SECONDS=${COUNTER_SECONDS:-55}
PEER_WAIT_SECONDS=${PEER_WAIT_SECONDS:-40}
RADIO_RUN_DURATION_MS=${RADIO_RUN_DURATION_MS:-60000}
RADIO_READY_WAIT_SECONDS=${RADIO_READY_WAIT_SECONDS:-90}
RX_TIMEOUT_MS=${RX_TIMEOUT_MS:-20}
TX_BURST_LIMIT=${TX_BURST_LIMIT:-4}
RADIO_COMMAND=${RADIO_COMMAND:-service}
MATRIX_SUSTAINED_PAYLOADS=${MATRIX_SUSTAINED_PAYLOADS:-200}
PROFILE_SET=${PROFILE_SET:-short}
PROFILE_FILE=${PROFILE_FILE:-}
REPEATS=${REPEATS:-1}

FIRMWARE=${FIRMWARE:-/tmp/rtl8812aefw.bin}
EFUSE_REPORT=${EFUSE_REPORT:-/tmp/wfb-remote-macos-efuse-dump.json}
LINK_ID=${LINK_ID:-0x000001}
WFB_CLI_LINK_ID=${WFB_CLI_LINK_ID:-$(printf '%d' "$((LINK_ID))")}
M2L_RADIO_PORT=${M2L_RADIO_PORT:-0}
L2M_RADIO_PORT=${L2M_RADIO_PORT:-1}
RADIO_BIND_PORT=${RADIO_BIND_PORT:-5611}
LINUX_M2L_SOURCE_PORT=${LINUX_M2L_SOURCE_PORT:-5600}
LINUX_L2M_SOURCE_PORT=${LINUX_L2M_SOURCE_PORT:-5621}
M2L_COUNTER_PORT=${M2L_COUNTER_PORT:-5900}
L2M_AGG_PORT=${L2M_AGG_PORT:-5801}
L2M_COUNTER_PORT=${L2M_COUNTER_PORT:-5911}
IFACE=${IFACE:-wfb0}
WFB_SERVICE=${WFB_SERVICE:-arc-wfb-link-1}
WFB_KEY=${WFB_KEY:-/var/lib/arc/wfb/drone.key}

require_command python3
if (( DRY_RUN == 0 )); then
  require_command ssh
  require_command scp
  if [[ "$LOCAL_HW" != "1" && "$HW_DEPLOY" == "1" ]]; then
    require_command rsync
  fi
fi

if (( REPEATS < 1 )); then
  die "REPEATS must be >= 1"
fi
case "$RADIO_COMMAND" in
  service|diagnostic) ;;
  diag) RADIO_COMMAND=diagnostic ;;
  *) die "invalid RADIO_COMMAND=$RADIO_COMMAND (expected service or diagnostic)" ;;
esac

mkdir -p "$MATRIX_OUT_DIR/runs"
MATRIX_OUT_DIR=$(cd "$MATRIX_OUT_DIR" && pwd)
REMOTE_MATRIX_OUT_DIR=${REMOTE_MATRIX_OUT_DIR:-/tmp/wfb-radio-profile-matrix-$RUN_ID}

profile_lines() {
  if [[ -n "$PROFILE_FILE" ]]; then
    cat "$PROFILE_FILE"
    return
  fi
  case "$PROFILE_SET" in
    minimal)
      cat <<'EOF'
baseline-8x12-mcs1|Default production smoke profile|8|12|8|12|1|1|0.003|95|95
EOF
      ;;
    short)
      cat <<'EOF'
baseline-8x12-mcs1|Default production smoke profile|8|12|8|12|1|1|0.003|95|95
symmetric-4x12-mcs1-20ms|Symmetric stronger FEC and slower source cadence|4|12|4|12|1|1|0.020|95|90
duplex-m2l5x12-l2m3x12-mcs2-20ms|Accepted short-range duplex sustained candidate|5|12|3|12|1|2|0.020|95|90
EOF
      ;;
    range)
      cat <<'EOF'
baseline-8x12-mcs1|Default production smoke profile|8|12|8|12|1|1|0.003|95|95
symmetric-4x12-mcs1-20ms|Symmetric stronger FEC and slower source cadence|4|12|4|12|1|1|0.020|95|90
duplex-m2l5x12-l2m3x12-mcs2-20ms|Accepted short-range duplex sustained candidate|5|12|3|12|1|2|0.020|95|90
asym-4x12-3x12-mcs2-20ms|Higher-overhead Mac TX candidate; failed one sustained repeat|4|12|3|12|1|2|0.020|95|90
asym-4x12-4x10-mcs2-20ms|Lower-overhead reverse MCS2 candidate|4|12|4|10|1|2|0.020|95|85
EOF
      ;;
    *)
      die "unknown PROFILE_SET: $PROFILE_SET"
      ;;
  esac
}

deploy_remote_repo() {
  if [[ "$LOCAL_HW" == "1" || "$HW_DEPLOY" != "1" ]]; then
    return
  fi
  log "syncing checkout to $HW_MAC_HOST:$HW_DEPLOY_PATH"
  ssh -n "${SSH_OPTS_ARRAY[@]}" "$HW_MAC_HOST" "mkdir -p $(quote "$HW_DEPLOY_PATH")"
  local rsync_args=(-az --exclude target --exclude .git)
  if [[ "$HW_DEPLOY_DELETE" == "1" ]]; then
    rsync_args+=(--delete)
  fi
  rsync -e "ssh $SSH_OPTS" "${rsync_args[@]}" "$REPO_ROOT/" "$HW_MAC_HOST:$HW_DEPLOY_PATH/"
  HW_REPO_PATH=$HW_DEPLOY_PATH
  sync_realtek_ref
}

ensure_local_realtek_ref() {
  if [[ -f "$REALTEK_REF_LOCAL_PATH/$REALTEK_REF_REQUIRED_FILE" ]]; then
    return
  fi
  if [[ "$AUTO_FETCH_REALTEK_REF" != "1" ]]; then
    die "missing Realtek reference file: $REALTEK_REF_LOCAL_PATH/$REALTEK_REF_REQUIRED_FILE"
  fi
  require_command git
  local tmp_path="${REALTEK_REF_LOCAL_PATH}.tmp.$$"
  log "fetching Realtek reference source into $REALTEK_REF_LOCAL_PATH"
  rm -rf "$tmp_path"
  git clone --depth 100 "$REALTEK_REF_REPO" "$tmp_path"
  [[ -f "$tmp_path/$REALTEK_REF_REQUIRED_FILE" ]] || die "fetched Realtek reference is missing $REALTEK_REF_REQUIRED_FILE"
  rm -rf "$REALTEK_REF_LOCAL_PATH"
  mv "$tmp_path" "$REALTEK_REF_LOCAL_PATH"
}

sync_realtek_ref() {
  if [[ "$SYNC_REALTEK_REF" != "1" ]]; then
    return
  fi
  ensure_local_realtek_ref
  log "syncing Realtek reference to $HW_MAC_HOST:$REALTEK_REF_REMOTE_PATH"
  ssh -n "${SSH_OPTS_ARRAY[@]}" "$HW_MAC_HOST" "mkdir -p $(quote "$REALTEK_REF_REMOTE_PATH")"
  rsync -e "ssh $SSH_OPTS" -az --delete --exclude .git \
    "$REALTEK_REF_LOCAL_PATH/" "$HW_MAC_HOST:$REALTEK_REF_REMOTE_PATH/"
}

resolve_remote_lan_pair() {
  if [[ "$LOCAL_HW" == "1" || "$MAC_LAN_IP" != "auto" ]]; then
    return
  fi
  (( DRY_RUN == 0 )) || return
  log "resolving remote Mac/Linux LAN pair through $HW_MAC_HOST"
  local pair
  if ! pair=$(ssh "${SSH_OPTS_ARRAY[@]}" "$HW_MAC_HOST" \
    "LINUX_HOST=$(quote "$LINUX_HOST") LINUX_REMOTE_PATH=$(quote "$LINUX_REMOTE_PATH") bash -s" <<'REMOTE_RESOLVE'
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
linux_ips=$(ssh -n -o BatchMode=yes -o ConnectTimeout=10 "$LINUX_HOST" 'hostname -I' 2>/dev/null || true)
for linux_ip in $linux_ips; do
  if [[ -z "$linux_ip" || "$linux_ip" == 127.* || "$linux_ip" == 169.254.* || "$linux_ip" == 172.17.* || "$linux_ip" == 10.5.* || "$linux_ip" == fd* || "$linux_ip" == *:* ]]; then
    continue
  fi
  iface=$(route -n get "$linux_ip" 2>/dev/null | awk '/interface:/{print $2; exit}')
  [[ -n "$iface" ]] || continue
  mac_ip=$(ipconfig getifaddr "$iface" 2>/dev/null || true)
  [[ -n "$mac_ip" ]] || continue
  linux_src=$(ssh -n -o BatchMode=yes -o ConnectTimeout=10 "$LINUX_HOST" \
    "ip -4 route get '$mac_ip' 2>/dev/null | sed -n 's/.* src \\([0-9.]*\\).*/\\1/p' | head -n 1" 2>/dev/null || true)
  if [[ "$linux_src" == "$linux_ip" ]]; then
    printf '%s %s\n' "$mac_ip" "$linux_ip"
    exit 0
  fi
done
exit 42
REMOTE_RESOLVE
  ); then
    die "could not resolve reciprocal LAN pair from $HW_MAC_HOST to $LINUX_HOST"
  fi
  MAC_LAN_IP=${pair%% *}
  LINUX_LAN_IP=${pair##* }
  log "resolved MAC_LAN_IP=$MAC_LAN_IP LINUX_LAN_IP=$LINUX_LAN_IP"
}

ceil_percent() {
  local total=$1
  local pct=$2
  printf '%d\n' "$(((total * pct + 99) / 100))"
}

write_run_meta() {
  local path=$1
  python3 - "$path" <<'PY'
import json
import os
import sys
from pathlib import Path

keys = [
    "profile_name", "profile_description", "repeat_index", "out_dir",
    "remote_out_dir", "local_hw", "hw_mac_host", "linux_host",
    "linux_lan_ip", "mac_lan_ip",
    "channel", "bandwidth_mhz", "tx_power_mode", "tx_calibration_profile",
    "enable_m2l", "enable_l2m",
    "m2l_fec_k", "m2l_fec_n", "l2m_fec_k", "l2m_fec_n", "m2l_mcs",
    "l2m_mcs", "payload_interval_sec", "m2l_payload_interval_sec",
    "l2m_payload_interval_sec", "expected_payloads",
    "m2l_expected_payloads", "l2m_expected_payloads",
    "duplex_traffic_mode", "tdd_first_direction", "tdd_guard_sec",
    "source_warmup_payloads", "source_tail_payloads", "session_acquire_mode",
    "session_acquire_timeout_seconds", "session_acquire_poll_seconds",
    "payload_len", "m2l_min_unique",
    "l2m_min_unique", "counter_seconds", "peer_wait_seconds",
    "radio_run_duration_ms", "radio_command", "decrypt_failure_gate",
]
data = {key: os.environ.get(key.upper(), os.environ.get(key)) for key in keys}
for key in [
    "repeat_index", "channel", "bandwidth_mhz", "m2l_fec_k", "m2l_fec_n",
    "l2m_fec_k", "l2m_fec_n", "m2l_mcs", "l2m_mcs", "expected_payloads",
    "m2l_expected_payloads", "l2m_expected_payloads",
    "source_warmup_payloads", "payload_len", "m2l_min_unique",
    "l2m_min_unique", "counter_seconds", "peer_wait_seconds",
    "radio_run_duration_ms",
    "session_acquire_timeout_seconds",
]:
    if data.get(key) is not None:
        data[key] = int(data[key])
for key in ["payload_interval_sec", "m2l_payload_interval_sec", "l2m_payload_interval_sec"]:
    if data.get(key) is not None:
        data[key] = float(data[key])
if data.get("tdd_guard_sec") is not None:
    data["tdd_guard_sec"] = float(data["tdd_guard_sec"])
if data.get("session_acquire_poll_seconds") is not None:
    data["session_acquire_poll_seconds"] = float(data["session_acquire_poll_seconds"])
Path(sys.argv[1]).write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
PY
}

write_run_status() {
  local path=$1
  local status=$2
  printf '{"exit_status":%d}\n' "$status" > "$path"
}

run_one_profile() {
  local profile_name=$1
  local profile_description=$2
  local m2l_fec_k=$3
  local m2l_fec_n=$4
  local l2m_fec_k=$5
  local l2m_fec_n=$6
  local m2l_mcs=$7
  local l2m_mcs=$8
  local payload_interval_sec=$9
  shift 9
  local m2l_min_pct=$1
  local l2m_min_pct=$2
  local repeat_index=$3
  local m2l_payload_interval_sec=${4:-$payload_interval_sec}
  local l2m_payload_interval_sec=${5:-$payload_interval_sec}
  local m2l_expected_payloads=${6:-$EXPECTED_PAYLOADS}
  local l2m_expected_payloads=${7:-$EXPECTED_PAYLOADS}
  [[ -n "$m2l_payload_interval_sec" ]] || m2l_payload_interval_sec=$payload_interval_sec
  [[ -n "$l2m_payload_interval_sec" ]] || l2m_payload_interval_sec=$payload_interval_sec
  [[ -n "$m2l_expected_payloads" ]] || m2l_expected_payloads=$EXPECTED_PAYLOADS
  [[ -n "$l2m_expected_payloads" ]] || l2m_expected_payloads=$EXPECTED_PAYLOADS

  local run_name="${profile_name}-r${repeat_index}"
  local local_run_dir="$MATRIX_OUT_DIR/runs/$run_name"
  local remote_run_dir="$REMOTE_MATRIX_OUT_DIR/runs/$run_name"
  local m2l_min_unique
  local l2m_min_unique
  m2l_min_unique=$(ceil_percent "$m2l_expected_payloads" "$m2l_min_pct")
  l2m_min_unique=$(ceil_percent "$l2m_expected_payloads" "$l2m_min_pct")
  mkdir -p "$local_run_dir"

  PROFILE_NAME=$profile_name
  PROFILE_DESCRIPTION=$profile_description
  REPEAT_INDEX=$repeat_index
  OUT_DIR=$local_run_dir
  REMOTE_OUT_DIR=$remote_run_dir
  M2L_FEC_K=$m2l_fec_k
  M2L_FEC_N=$m2l_fec_n
  L2M_FEC_K=$l2m_fec_k
  L2M_FEC_N=$l2m_fec_n
  M2L_MCS=$m2l_mcs
  L2M_MCS=$l2m_mcs
  PAYLOAD_INTERVAL_SEC=$payload_interval_sec
  M2L_PAYLOAD_INTERVAL_SEC=$m2l_payload_interval_sec
  L2M_PAYLOAD_INTERVAL_SEC=$l2m_payload_interval_sec
  M2L_EXPECTED_PAYLOADS=$m2l_expected_payloads
  L2M_EXPECTED_PAYLOADS=$l2m_expected_payloads
  M2L_MIN_UNIQUE=$m2l_min_unique
  L2M_MIN_UNIQUE=$l2m_min_unique
  export PROFILE_NAME PROFILE_DESCRIPTION REPEAT_INDEX OUT_DIR REMOTE_OUT_DIR
  export LOCAL_HW HW_MAC_HOST LINUX_HOST LINUX_LAN_IP MAC_LAN_IP CHANNEL BANDWIDTH_MHZ
  export TX_POWER_MODE TX_CALIBRATION_PROFILE ENABLE_M2L ENABLE_L2M M2L_FEC_K
  export M2L_FEC_N L2M_FEC_K L2M_FEC_N M2L_MCS L2M_MCS PAYLOAD_INTERVAL_SEC M2L_PAYLOAD_INTERVAL_SEC L2M_PAYLOAD_INTERVAL_SEC EXPECTED_PAYLOADS M2L_EXPECTED_PAYLOADS L2M_EXPECTED_PAYLOADS
  export SOURCE_WARMUP_PAYLOADS SOURCE_TAIL_PAYLOADS SESSION_ACQUIRE_MODE SESSION_ACQUIRE_TIMEOUT_SECONDS SESSION_ACQUIRE_POLL_SECONDS PAYLOAD_LEN M2L_MIN_UNIQUE L2M_MIN_UNIQUE
  export DUPLEX_TRAFFIC_MODE TDD_FIRST_DIRECTION TDD_GUARD_SEC AIRTIME_MODE AIRTIME_TDD_FIRST_WINDOW AIRTIME_TDD_RX_WINDOW_MS AIRTIME_TDD_TX_WINDOW_MS AIRTIME_TDD_GUARD_MS AIRTIME_TDD_START_DELAY_MS
  export COUNTER_SECONDS PEER_WAIT_SECONDS RADIO_RUN_DURATION_MS RADIO_COMMAND DECRYPT_FAILURE_GATE
  write_run_meta "$local_run_dir/matrix-run-meta.json"

  log "profile=$profile_name repeat=$repeat_index m2l=${m2l_fec_k}/${m2l_fec_n}@mcs${m2l_mcs}/${m2l_payload_interval_sec}s/${m2l_expected_payloads} l2m=${l2m_fec_k}/${l2m_fec_n}@mcs${l2m_mcs}/${l2m_payload_interval_sec}s/${l2m_expected_payloads}"

  if (( DRY_RUN == 1 )); then
    write_run_status "$local_run_dir/matrix-run-status.json" 0
    return
  fi

  local status=0
  set +e
  if [[ "$LOCAL_HW" == "1" ]]; then
    env \
      OUT_DIR="$local_run_dir" \
      LINUX_HOST="$LINUX_HOST" \
      LINUX_LAN_IP="$LINUX_LAN_IP" \
      LINUX_REMOTE_PATH="$LINUX_REMOTE_PATH" \
      MAC_LAN_IP="$MAC_LAN_IP" \
      CHANNEL="$CHANNEL" \
      BANDWIDTH_MHZ="$BANDWIDTH_MHZ" \
      LINK_ID="$LINK_ID" \
      WFB_CLI_LINK_ID="$WFB_CLI_LINK_ID" \
      M2L_RADIO_PORT="$M2L_RADIO_PORT" \
      L2M_RADIO_PORT="$L2M_RADIO_PORT" \
      M2L_FEC_K="$M2L_FEC_K" \
      M2L_FEC_N="$M2L_FEC_N" \
      L2M_FEC_K="$L2M_FEC_K" \
      L2M_FEC_N="$L2M_FEC_N" \
      M2L_MCS="$M2L_MCS" \
      L2M_MCS="$L2M_MCS" \
      EXPECTED_PAYLOADS="$EXPECTED_PAYLOADS" \
      M2L_EXPECTED_PAYLOADS="$M2L_EXPECTED_PAYLOADS" \
      L2M_EXPECTED_PAYLOADS="$L2M_EXPECTED_PAYLOADS" \
      ENABLE_M2L="$ENABLE_M2L" \
      ENABLE_L2M="$ENABLE_L2M" \
      SOURCE_WARMUP_PAYLOADS="$SOURCE_WARMUP_PAYLOADS" \
      SOURCE_TAIL_PAYLOADS="$SOURCE_TAIL_PAYLOADS" \
      SESSION_ACQUIRE_MODE="$SESSION_ACQUIRE_MODE" \
      SESSION_ACQUIRE_TIMEOUT_SECONDS="$SESSION_ACQUIRE_TIMEOUT_SECONDS" \
      SESSION_ACQUIRE_POLL_SECONDS="$SESSION_ACQUIRE_POLL_SECONDS" \
      PAYLOAD_LEN="$PAYLOAD_LEN" \
      DUPLEX_TRAFFIC_MODE="$DUPLEX_TRAFFIC_MODE" \
      TDD_FIRST_DIRECTION="$TDD_FIRST_DIRECTION" \
      TDD_GUARD_SEC="$TDD_GUARD_SEC" \
      AIRTIME_MODE="$AIRTIME_MODE" \
      AIRTIME_TDD_FIRST_WINDOW="$AIRTIME_TDD_FIRST_WINDOW" \
      AIRTIME_TDD_RX_WINDOW_MS="$AIRTIME_TDD_RX_WINDOW_MS" \
      AIRTIME_TDD_TX_WINDOW_MS="$AIRTIME_TDD_TX_WINDOW_MS" \
      AIRTIME_TDD_GUARD_MS="$AIRTIME_TDD_GUARD_MS" \
      AIRTIME_TDD_START_DELAY_MS="$AIRTIME_TDD_START_DELAY_MS" \
      PAYLOAD_INTERVAL_SEC="$PAYLOAD_INTERVAL_SEC" \
      M2L_PAYLOAD_INTERVAL_SEC="$M2L_PAYLOAD_INTERVAL_SEC" \
      L2M_PAYLOAD_INTERVAL_SEC="$L2M_PAYLOAD_INTERVAL_SEC" \
      M2L_MIN_UNIQUE="$M2L_MIN_UNIQUE" \
      L2M_MIN_UNIQUE="$L2M_MIN_UNIQUE" \
      COUNTER_SECONDS="$COUNTER_SECONDS" \
      PEER_WAIT_SECONDS="$PEER_WAIT_SECONDS" \
      RADIO_RUN_DURATION_MS="$RADIO_RUN_DURATION_MS" \
      RADIO_COMMAND="$RADIO_COMMAND" \
      RADIO_READY_WAIT_SECONDS="$RADIO_READY_WAIT_SECONDS" \
      RX_TIMEOUT_MS="$RX_TIMEOUT_MS" \
      TX_BURST_LIMIT="$TX_BURST_LIMIT" \
      FIRMWARE="$FIRMWARE" \
      EFUSE_REPORT="$EFUSE_REPORT" \
      TX_POWER_MODE="$TX_POWER_MODE" \
      TX_POWER_SAFETY_PROFILE="$TX_POWER_SAFETY_PROFILE" \
      TX_CALIBRATION_PROFILE="$TX_CALIBRATION_PROFILE" \
      REQUIRE_CALIBRATION_SUCCESS="$REQUIRE_CALIBRATION_SUCCESS" \
      DECRYPT_FAILURE_GATE="$DECRYPT_FAILURE_GATE" \
      AUTO_EFUSE_DUMP="$AUTO_EFUSE_DUMP" \
      RADIO_BIND_PORT="$RADIO_BIND_PORT" \
      LINUX_M2L_SOURCE_PORT="$LINUX_M2L_SOURCE_PORT" \
      LINUX_L2M_SOURCE_PORT="$LINUX_L2M_SOURCE_PORT" \
      M2L_COUNTER_PORT="$M2L_COUNTER_PORT" \
      L2M_AGG_PORT="$L2M_AGG_PORT" \
      L2M_COUNTER_PORT="$L2M_COUNTER_PORT" \
      IFACE="$IFACE" \
      WFB_SERVICE="$WFB_SERVICE" \
      WFB_KEY="$WFB_KEY" \
      scripts/run-radio-run-duplex-smoke.sh
    status=$?
  else
    local remote_cmd
    OUT_DIR=$remote_run_dir
    remote_cmd="$(env_assignments OUT_DIR LINUX_HOST LINUX_LAN_IP LINUX_REMOTE_PATH MAC_LAN_IP CHANNEL BANDWIDTH_MHZ LINK_ID WFB_CLI_LINK_ID M2L_RADIO_PORT L2M_RADIO_PORT M2L_FEC_K M2L_FEC_N L2M_FEC_K L2M_FEC_N M2L_MCS L2M_MCS EXPECTED_PAYLOADS M2L_EXPECTED_PAYLOADS L2M_EXPECTED_PAYLOADS ENABLE_M2L ENABLE_L2M SOURCE_WARMUP_PAYLOADS SOURCE_TAIL_PAYLOADS SESSION_ACQUIRE_MODE SESSION_ACQUIRE_TIMEOUT_SECONDS SESSION_ACQUIRE_POLL_SECONDS PAYLOAD_LEN DUPLEX_TRAFFIC_MODE TDD_FIRST_DIRECTION TDD_GUARD_SEC AIRTIME_MODE AIRTIME_TDD_FIRST_WINDOW AIRTIME_TDD_RX_WINDOW_MS AIRTIME_TDD_TX_WINDOW_MS AIRTIME_TDD_GUARD_MS AIRTIME_TDD_START_DELAY_MS PAYLOAD_INTERVAL_SEC M2L_PAYLOAD_INTERVAL_SEC L2M_PAYLOAD_INTERVAL_SEC M2L_MIN_UNIQUE L2M_MIN_UNIQUE COUNTER_SECONDS PEER_WAIT_SECONDS RADIO_RUN_DURATION_MS RADIO_COMMAND RADIO_READY_WAIT_SECONDS RX_TIMEOUT_MS TX_BURST_LIMIT FIRMWARE EFUSE_REPORT TX_POWER_MODE TX_POWER_SAFETY_PROFILE TX_CALIBRATION_PROFILE REQUIRE_CALIBRATION_SUCCESS DECRYPT_FAILURE_GATE AUTO_EFUSE_DUMP RADIO_BIND_PORT LINUX_M2L_SOURCE_PORT LINUX_L2M_SOURCE_PORT M2L_COUNTER_PORT L2M_AGG_PORT L2M_COUNTER_PORT IFACE WFB_SERVICE WFB_KEY) scripts/run-radio-run-duplex-smoke.sh"
    ssh -n "${SSH_OPTS_ARRAY[@]}" "$HW_MAC_HOST" "cd $(quote "$HW_REPO_PATH") && $remote_cmd"
    status=$?
    rm -rf "$local_run_dir/remote-copy"
    mkdir -p "$local_run_dir/remote-copy"
    scp "${SSH_OPTS_ARRAY[@]}" -r "$HW_MAC_HOST:$remote_run_dir/." "$local_run_dir/" >/dev/null 2>&1
  fi
  set -e
  write_run_status "$local_run_dir/matrix-run-status.json" "$status"
  return 0
}

write_matrix_summary() {
  log "writing matrix summary"
  python3 - "$MATRIX_OUT_DIR" "$MATRIX_SUSTAINED_PAYLOADS" <<'PY'
import json
import statistics
import sys
from pathlib import Path

root = Path(sys.argv[1])
sustained_payloads = int(sys.argv[2])
runs = []

def load(path):
    try:
        return json.loads(path.read_text())
    except Exception as exc:
        return {"error": str(exc), "path": str(path)}

for run_dir in sorted((root / "runs").iterdir() if (root / "runs").exists() else []):
    if not run_dir.is_dir():
        continue
    meta = load(run_dir / "matrix-run-meta.json")
    status = load(run_dir / "matrix-run-status.json")
    summary = load(run_dir / "summary.json")
    m2l = summary.get("m2l_counter") or {}
    l2m = summary.get("l2m_counter") or {}
    dec = summary.get("peer_wfb_rx") or {}
    source_gate = summary.get("source_gate") or {}
    directions = summary.get("directions") or {}
    rx = summary.get("rx") or {}
    signal = rx.get("signal") or {}
    network = summary.get("network") or {}
    m2l_enabled = bool(directions.get("m2l_enabled", str(meta.get("enable_m2l", "1")).lower() not in {"0", "false", "no"}))
    l2m_enabled = bool(directions.get("l2m_enabled", str(meta.get("enable_l2m", "1")).lower() not in {"0", "false", "no"}))
    m2l_expected = int(m2l.get("expected") or meta.get("m2l_expected_payloads") or meta.get("expected_payloads") or 0)
    l2m_expected = int(l2m.get("expected") or meta.get("l2m_expected_payloads") or meta.get("expected_payloads") or 0)
    expected_payloads = max([
        value
        for value, enabled in [(m2l_expected, m2l_enabled), (l2m_expected, l2m_enabled)]
        if enabled
    ] or [m2l_expected, l2m_expected, int(meta.get("expected_payloads") or 0)])
    run = {
        "run_dir": str(run_dir),
        "profile": meta.get("profile_name") or run_dir.name,
        "description": meta.get("profile_description"),
        "repeat_index": meta.get("repeat_index"),
        "exit_status": status.get("exit_status"),
        "smoke_result": summary.get("smoke_result", "missing"),
        "radio_command": summary.get("radio_command") or meta.get("radio_command"),
        "failures": summary.get("failures") or ([] if "smoke_result" in summary else ["missing_summary"]),
        "expected_payloads": expected_payloads,
        "m2l_expected_payloads": m2l_expected,
        "l2m_expected_payloads": l2m_expected,
        "source_warmup_payloads": int(meta.get("source_warmup_payloads") or 0),
        "source_gate_status": source_gate.get("status"),
        "source_gate_acquired": source_gate.get("acquired"),
        "source_gate_missing_sessions": source_gate.get("missing_sessions") or [],
        "enable_m2l": meta.get("enable_m2l"),
        "enable_l2m": meta.get("enable_l2m"),
        "m2l_enabled": m2l_enabled,
        "l2m_enabled": l2m_enabled,
        "linux_lan_ip": network.get("linux_lan_ip") or meta.get("linux_lan_ip"),
        "linux_lan_ip_requested": network.get("linux_lan_ip_requested") or meta.get("linux_lan_ip"),
        "mac_lan_ip": network.get("mac_lan_ip") or meta.get("mac_lan_ip"),
        "m2l_unique": int(m2l.get("unique_sequences") or 0),
        "l2m_unique": int(l2m.get("unique_sequences") or 0),
        "m2l_decrypt_failures": int(dec.get("m2l_decrypt_failures") or 0),
        "l2m_decrypt_failures": int(dec.get("l2m_decrypt_failures") or 0),
        "m2l_decrypt_failures_total": int(dec.get("m2l_decrypt_failures_total") or dec.get("m2l_decrypt_failures") or 0),
        "l2m_decrypt_failures_total": int(dec.get("l2m_decrypt_failures_total") or dec.get("l2m_decrypt_failures") or 0),
        "m2l_decrypt_failures_before_session": int(dec.get("m2l_decrypt_failures_before_session") or 0),
        "l2m_decrypt_failures_before_session": int(dec.get("l2m_decrypt_failures_before_session") or 0),
        "m2l_decrypt_failures_after_session": int(dec.get("m2l_decrypt_failures_after_session") or dec.get("m2l_decrypt_failures") or 0),
        "l2m_decrypt_failures_after_session": int(dec.get("l2m_decrypt_failures_after_session") or dec.get("l2m_decrypt_failures") or 0),
        "tx_submitted_frames": int(((summary.get("tx") or {}).get("submitted_frames")) or 0),
        "tx_failed_submissions": int(((summary.get("tx") or {}).get("failed_submissions")) or 0),
        "tx_dropped_datagrams": int(((summary.get("tx") or {}).get("dropped_datagrams")) or 0),
        "rx_forwarded": int(summary.get("radio_rx_forwarded_from_snapshots") or 0),
        "link_profile": summary.get("link_profile") or {
            "m2l_fec_k": meta.get("m2l_fec_k"),
            "m2l_fec_n": meta.get("m2l_fec_n"),
            "l2m_fec_k": meta.get("l2m_fec_k"),
            "l2m_fec_n": meta.get("l2m_fec_n"),
            "m2l_mcs": meta.get("m2l_mcs"),
            "l2m_mcs": meta.get("l2m_mcs"),
            "payload_interval_sec": meta.get("payload_interval_sec"),
            "m2l_payload_interval_sec": meta.get("m2l_payload_interval_sec"),
            "l2m_payload_interval_sec": meta.get("l2m_payload_interval_sec"),
            "m2l_expected_payloads": meta.get("m2l_expected_payloads"),
            "l2m_expected_payloads": meta.get("l2m_expected_payloads"),
            "traffic_mode": meta.get("duplex_traffic_mode"),
            "tdd_first_direction": meta.get("tdd_first_direction"),
            "tdd_guard_sec": meta.get("tdd_guard_sec"),
        },
        "signal": signal,
    }
    run["m2l_recovery"] = run["m2l_unique"] / max(run["m2l_expected_payloads"], 1) if m2l_enabled else None
    run["l2m_recovery"] = run["l2m_unique"] / max(run["l2m_expected_payloads"], 1) if l2m_enabled else None
    enabled_recoveries = [
        value for value in [run["m2l_recovery"], run["l2m_recovery"]]
        if value is not None
    ]
    run["min_recovery"] = min(enabled_recoveries) if enabled_recoveries else 0.0
    run["decrypt_failures"] = run["m2l_decrypt_failures"] + run["l2m_decrypt_failures"]
    run["decrypt_failures_total"] = run["m2l_decrypt_failures_total"] + run["l2m_decrypt_failures_total"]
    run["pre_session_decrypt_failures"] = (
        run["m2l_decrypt_failures_before_session"]
        + run["l2m_decrypt_failures_before_session"]
    )
    run["post_session_decrypt_failures"] = (
        run["m2l_decrypt_failures_after_session"]
        + run["l2m_decrypt_failures_after_session"]
    )
    run["is_sustained"] = run["expected_payloads"] >= sustained_payloads
    run["accepted"] = (
        run["smoke_result"] == "pass"
        and run["is_sustained"]
        and run["decrypt_failures"] == 0
        and run["tx_failed_submissions"] == 0
        and run["tx_dropped_datagrams"] == 0
    )
    run["short_smoke_pass"] = (
        run["smoke_result"] == "pass"
        and not run["is_sustained"]
        and run["decrypt_failures"] == 0
    )
    runs.append(run)

groups = {}
for run in runs:
    groups.setdefault(run["profile"], []).append(run)

profiles = []
for profile, items in groups.items():
    expected_values = [item["expected_payloads"] for item in items]
    m2l_expected_values = [item["m2l_expected_payloads"] for item in items if item["m2l_enabled"]]
    l2m_expected_values = [item["l2m_expected_payloads"] for item in items if item["l2m_enabled"]]
    decrypt_total = sum(item["decrypt_failures"] for item in items)
    pass_count = sum(1 for item in items if item["smoke_result"] == "pass")
    accepted_count = sum(1 for item in items if item["accepted"])
    short_pass_count = sum(1 for item in items if item["short_smoke_pass"])
    m2l_values = [item["m2l_recovery"] for item in items if item["m2l_recovery"] is not None]
    l2m_values = [item["l2m_recovery"] for item in items if item["l2m_recovery"] is not None]
    min_values = [item["min_recovery"] for item in items]
    profile_summary = {
        "profile": profile,
        "description": items[0].get("description"),
        "runs": len(items),
        "pass_count": pass_count,
        "accepted_count": accepted_count,
        "short_smoke_pass_count": short_pass_count,
        "m2l_enabled_runs": len(m2l_values),
        "l2m_enabled_runs": len(l2m_values),
        "expected_payloads": sorted(set(expected_values)),
        "m2l_expected_payloads": sorted(set(m2l_expected_values)),
        "l2m_expected_payloads": sorted(set(l2m_expected_values)),
        "avg_m2l_recovery": statistics.fmean(m2l_values) if m2l_values else 0.0,
        "avg_l2m_recovery": statistics.fmean(l2m_values) if l2m_values else 0.0,
        "avg_min_recovery": statistics.fmean(min_values) if min_values else 0.0,
        "worst_m2l_recovery": min(m2l_values) if m2l_values else 0.0,
        "worst_l2m_recovery": min(l2m_values) if l2m_values else 0.0,
        "decrypt_failures": decrypt_total,
        "decrypt_failures_total": sum(item.get("decrypt_failures_total", item["decrypt_failures"]) for item in items),
        "pre_session_decrypt_failures": sum(item.get("pre_session_decrypt_failures", 0) for item in items),
        "post_session_decrypt_failures": sum(item.get("post_session_decrypt_failures", item["decrypt_failures"]) for item in items),
        "tx_failed_submissions": sum(item["tx_failed_submissions"] for item in items),
        "tx_dropped_datagrams": sum(item["tx_dropped_datagrams"] for item in items),
        "representative_link_profile": items[0].get("link_profile"),
        "status": "accepted" if accepted_count == len(items) else (
            "short_smoke_pass" if short_pass_count == len(items) else "failed"
        ),
    }
    profile_summary["rank_key"] = [
        profile_summary["accepted_count"],
        profile_summary["short_smoke_pass_count"],
        profile_summary["pass_count"],
        profile_summary["avg_min_recovery"],
        -profile_summary["decrypt_failures"],
    ]
    profiles.append(profile_summary)

profiles.sort(key=lambda item: item["rank_key"], reverse=True)
for idx, item in enumerate(profiles, 1):
    item["rank"] = idx
    item.pop("rank_key", None)

result = {
    "result": "pass" if any(item["status"] in {"accepted", "short_smoke_pass"} for item in profiles) else "fail",
    "sustained_payload_threshold": sustained_payloads,
    "profiles": profiles,
    "runs": runs,
}
(root / "matrix-summary.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")

lines = [
    "# radio-run Profile Matrix",
    "",
    f"Result: `{result['result']}`",
    f"Sustained payload threshold: `{sustained_payloads}`",
    "",
    "| Rank | Profile | Status | Runs | Avg M2L | Avg L2M | Worst M2L | Worst L2M | Decrypt Failures |",
    "|---:|---|---|---:|---:|---:|---:|---:|---:|",
]
def pct(value):
    return "n/a" if value is None else f"{value:.1%}"

for item in profiles:
    lines.append(
        "| {rank} | `{profile}` | {status} | {runs} | {avg_m2l} | {avg_l2m} | {worst_m2l} | {worst_l2m} | {decrypt} |".format(
            rank=item["rank"],
            profile=item["profile"],
            status=item["status"],
            runs=item["runs"],
            avg_m2l=pct(item["avg_m2l_recovery"] if item["m2l_enabled_runs"] else None),
            avg_l2m=pct(item["avg_l2m_recovery"] if item["l2m_enabled_runs"] else None),
            worst_m2l=pct(item["worst_m2l_recovery"] if item["m2l_enabled_runs"] else None),
            worst_l2m=pct(item["worst_l2m_recovery"] if item["l2m_enabled_runs"] else None),
            decrypt=item["decrypt_failures"],
        )
    )
lines.extend(["", "## Runs", ""])
for run in runs:
    lines.append(
        "- `{profile}` r{repeat}: command `{command}`, result `{result}`, M2L `{m2l}/{m2l_expected}`, L2M `{l2m}/{l2m_expected}`, decrypt `{decrypt}`, dir `{dir}`".format(
            profile=run["profile"],
            repeat=run.get("repeat_index"),
            command=run.get("radio_command"),
            result=run["smoke_result"],
            m2l=run["m2l_unique"],
            l2m=run["l2m_unique"],
            m2l_expected=run["m2l_expected_payloads"],
            l2m_expected=run["l2m_expected_payloads"],
            decrypt=run["decrypt_failures"],
            dir=run["run_dir"],
        )
    )
(root / "matrix-summary.md").write_text("\n".join(lines) + "\n")
print(json.dumps(result, indent=2, sort_keys=True))
PY
}

write_evidence_summary() {
  local summarizer=scripts/summarize-radio-run-evidence.py
  if [[ ! -x "$summarizer" ]]; then
    log "skipping evidence summary; missing executable $summarizer"
    return
  fi
  log "writing evidence summary"
  if ! "$summarizer" "$MATRIX_OUT_DIR" > "$MATRIX_OUT_DIR/evidence-summary.md"; then
    printf 'No run evidence found.\n' > "$MATRIX_OUT_DIR/evidence-summary.md"
  fi
  if ! "$summarizer" --json "$MATRIX_OUT_DIR" > "$MATRIX_OUT_DIR/evidence-summary.json"; then
    printf '{"run_count":0,"clean_count":0,"issue_count":0,"runs":[]}\n' > "$MATRIX_OUT_DIR/evidence-summary.json"
  fi
}

if (( DRY_RUN == 1 )); then
  log "dry run; matrix artifacts will be skeletal under $MATRIX_OUT_DIR"
else
  deploy_remote_repo
  resolve_remote_lan_pair
fi

profile_count=0
while IFS='|' read -r name description m2l_k m2l_n l2m_k l2m_n m2l_mcs l2m_mcs interval m2l_min_pct l2m_min_pct m2l_interval l2m_interval m2l_expected l2m_expected; do
  [[ -z "${name:-}" || "${name:0:1}" == "#" ]] && continue
  profile_count=$((profile_count + 1))
  repeat=1
  while (( repeat <= REPEATS )); do
    run_one_profile "$name" "$description" "$m2l_k" "$m2l_n" "$l2m_k" "$l2m_n" "$m2l_mcs" "$l2m_mcs" "$interval" "$m2l_min_pct" "$l2m_min_pct" "$repeat" "${m2l_interval:-}" "${l2m_interval:-}" "${m2l_expected:-}" "${l2m_expected:-}"
    repeat=$((repeat + 1))
  done
done < <(profile_lines)

if (( profile_count == 0 )); then
  die "profile matrix is empty"
fi

write_matrix_summary
write_evidence_summary
log "done: $MATRIX_OUT_DIR"
