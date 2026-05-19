#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/prepare-long-range-tailnet-field.sh [--env-file PATH] [--out-dir DIR] [--skip-drone]

Prepares a long-range WFB field-test kit for a Mac ground station and a drone
reachable over a tailnet. The script is safe to run before the drone is online:
it records local readiness, writes resolved settings, prints the exact runner
commands to use later, and runs drone-side preflight only when DRONE_HOST is set
and reachable.

Common environment:
  DRONE_HOST=pi@drone-name.tailnet.ts.net
  LOCAL_WFB_KEY=/path/to/gs.key
  DRONE_WFB_KEY=/var/lib/arc/wfb/drone.key
  CHANNEL=165 BANDWIDTH_MHZ=20 LINK_ID=0x000000
  RADIO_CONFIG=configs/radio-run-long-range-field.toml

Options:
  --env-file PATH   Source a field env file before resolving defaults.
  --out-dir DIR     Artifact directory. Default: /tmp/wfb-long-range-field-$RUN_ID
  --skip-drone      Do not attempt SSH/tailnet drone checks.
  --strict          Exit nonzero when required local or remote checks are missing.
  -h, --help        Show this help.
EOF
}

log() {
  printf '[long-range-prep] %s\n' "$*" >&2
}

warn() {
  printf '[long-range-prep] warn: %s\n' "$*" >&2
  WARNINGS+=("$*")
  WARNINGS_COUNT=$((WARNINGS_COUNT + 1))
}

die() {
  printf '[long-range-prep] error: %s\n' "$*" >&2
  exit 1
}

quote() {
  printf '%q' "$1"
}

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
ENV_FILE=${ENV_FILE:-}
OUT_DIR=${OUT_DIR:-/tmp/wfb-long-range-field-$RUN_ID}
SKIP_DRONE=0
STRICT_PREP=${STRICT_PREP:-0}
WARNINGS=()
WARNINGS_COUNT=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      [[ $# -ge 2 ]] || die "--env-file requires a path"
      ENV_FILE=$2
      shift 2
      ;;
    --out-dir)
      [[ $# -ge 2 ]] || die "--out-dir requires a path"
      OUT_DIR=$2
      shift 2
      ;;
    --skip-drone)
      SKIP_DRONE=1
      shift
      ;;
    --strict)
      STRICT_PREP=1
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

if [[ -n "$ENV_FILE" ]]; then
  [[ -r "$ENV_FILE" ]] || die "env file is not readable: $ENV_FILE"
  # shellcheck disable=SC1090
  set -a; . "$ENV_FILE"; set +a
fi

OUT_DIR=$(mkdir -p "$OUT_DIR" && cd "$OUT_DIR" && pwd)

DRONE_HOST=${DRONE_HOST:-}
DRONE_IFACE=${DRONE_IFACE:-${IFACE:-wfb0}}
DRONE_REMOTE_PATH=${DRONE_REMOTE_PATH:-/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin}
DRONE_WFB_KEY=${DRONE_WFB_KEY:-/var/lib/arc/wfb/drone.key}
LOCAL_WFB_KEY=${LOCAL_WFB_KEY:-${WFB_KEY:-}}
FIRMWARE=${FIRMWARE:-/tmp/rtl8812aefw.bin}
RADIO_CONFIG=${RADIO_CONFIG:-configs/radio-run-long-range-field.toml}
RADIO_SERVICE_BIN=${RADIO_SERVICE_BIN:-$REPO_ROOT/target/debug/wfb-radio-service}
WFB_TX_BIN=${WFB_TX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_tx}
WFB_RX_BIN=${WFB_RX_BIN:-$REPO_ROOT/target/wfb-ng-macos/bin/wfb_rx}
TUN_BIN=${TUN_BIN:-$REPO_ROOT/target/debug/wfb-tun-macos}
TUN_IMPL=${TUN_IMPL:-rust}
TUN_SCRIPT=${TUN_SCRIPT:-$REPO_ROOT/scripts/development/wfb-mac-wf-tun.py}
PYTHON=${PYTHON:-python3}
SSH_OPTS=${SSH_OPTS:-"-o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=2"}
# shellcheck disable=SC2206
SSH_OPTS_ARRAY=($SSH_OPTS)

CHANNEL=${CHANNEL:-165}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
LINK_ID=${LINK_ID:-0x000000}
WFB_CLI_LINK_ID=${WFB_CLI_LINK_ID:-$(printf '%d' "$((LINK_ID))")}
LOCAL_IP=${LOCAL_IP:-10.5.0.1}
PEER_IP=${PEER_IP:-10.5.0.2}
TUN_RX_RADIO_PORT=${TUN_RX_RADIO_PORT:-3}
TUN_TX_RADIO_PORT=${TUN_TX_RADIO_PORT:-4}
MCS=${MCS:-0}
FEC_K=${FEC_K:-2}
FEC_N=${FEC_N:-8}
TX_MIN_INTERVAL_US=${TX_MIN_INTERVAL_US:-700}
TX_BURST_LIMIT=${TX_BURST_LIMIT:-8}
TX_POWER_MODE=${TX_POWER_MODE:-manual-index}
TX_POWER_INDEX=${TX_POWER_INDEX:-0x20}
TX_POWER_PATH=${TX_POWER_PATH:-both}
AIRTIME_MODE=${AIRTIME_MODE:-tdd}
AIRTIME_TDD_FIRST_WINDOW=${AIRTIME_TDD_FIRST_WINDOW:-rx}
AIRTIME_TDD_RX_WINDOW_MS=${AIRTIME_TDD_RX_WINDOW_MS:-1000}
AIRTIME_TDD_TX_WINDOW_MS=${AIRTIME_TDD_TX_WINDOW_MS:-1000}
AIRTIME_TDD_GUARD_MS=${AIRTIME_TDD_GUARD_MS:-100}
AIRTIME_TDD_START_DELAY_MS=${AIRTIME_TDD_START_DELAY_MS:-0}

DATA_LOAD_MCS=${DATA_LOAD_MCS:-0}
DATA_LOAD_FEC_K=${DATA_LOAD_FEC_K:-2}
DATA_LOAD_FEC_N=${DATA_LOAD_FEC_N:-8}
DATA_M2L_RADIO_PORT=${DATA_M2L_RADIO_PORT:-6}
DATA_L2M_RADIO_PORT=${DATA_L2M_RADIO_PORT:-7}
DATA_LOAD_EXPECTED_PAYLOADS=${DATA_LOAD_EXPECTED_PAYLOADS:-100}
DATA_LOAD_MIN_M2L_UNIQUE=${DATA_LOAD_MIN_M2L_UNIQUE:-$DATA_LOAD_EXPECTED_PAYLOADS}
DATA_LOAD_MIN_L2M_UNIQUE=${DATA_LOAD_MIN_L2M_UNIQUE:-$DATA_LOAD_EXPECTED_PAYLOADS}
DATA_LOAD_COUNTER_SECONDS=${DATA_LOAD_COUNTER_SECONDS:-30}

M2L_RADIO_PORT=${M2L_RADIO_PORT:-0}
L2M_RADIO_PORT=${L2M_RADIO_PORT:-1}
M2L_MCS=${M2L_MCS:-0}
L2M_MCS=${L2M_MCS:-0}
M2L_FEC_K=${M2L_FEC_K:-2}
M2L_FEC_N=${M2L_FEC_N:-12}
L2M_FEC_K=${L2M_FEC_K:-2}
L2M_FEC_N=${L2M_FEC_N:-12}
M2L_PAYLOAD_INTERVAL_SEC=${M2L_PAYLOAD_INTERVAL_SEC:-0.100}
L2M_PAYLOAD_INTERVAL_SEC=${L2M_PAYLOAD_INTERVAL_SEC:-0.040}
M2L_EXPECTED_PAYLOADS=${M2L_EXPECTED_PAYLOADS:-100}
L2M_EXPECTED_PAYLOADS=${L2M_EXPECTED_PAYLOADS:-100}
SESSION_ACQUIRE_MODE=${SESSION_ACQUIRE_MODE:-observed}
DECRYPT_FAILURE_GATE=${DECRYPT_FAILURE_GATE:-post-session}
MAX_M2L_DECRYPT_FAILURES=${MAX_M2L_DECRYPT_FAILURES:-0}
MAX_L2M_DECRYPT_FAILURES=${MAX_L2M_DECRYPT_FAILURES:-0}

EXPECTED_LOCAL_WFB_KEY_SHA256=${EXPECTED_LOCAL_WFB_KEY_SHA256:-}
EXPECTED_DRONE_WFB_KEY_SHA256=${EXPECTED_DRONE_WFB_KEY_SHA256:-}

settings_env="$OUT_DIR/field-settings.env"
commands_script="$OUT_DIR/field-run-commands.sh"
loaded_ping_profile="$OUT_DIR/tunnel-loaded-ping.profile"
local_report="$OUT_DIR/local-preflight.txt"
drone_report="$OUT_DIR/drone-preflight.json"
fingerprints_report="$OUT_DIR/key-fingerprints.txt"

hash_file() {
  local path=$1
  if [[ ! -r "$path" ]]; then
    return 1
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  else
    return 1
  fi
}

check_command() {
  local cmd=$1
  local required=${2:-1}
  if command -v "$cmd" >/dev/null 2>&1; then
    printf 'ok command %s -> %s\n' "$cmd" "$(command -v "$cmd")" >>"$local_report"
  elif [[ "$required" == "1" ]]; then
    warn "missing required local command: $cmd"
    printf 'missing required command %s\n' "$cmd" >>"$local_report"
  else
    printf 'missing optional command %s\n' "$cmd" >>"$local_report"
  fi
}

check_file() {
  local path=$1
  local label=$2
  local required=${3:-1}
  if [[ -e "$path" ]]; then
    printf 'ok file %s -> %s\n' "$label" "$path" >>"$local_report"
  elif [[ "$required" == "1" ]]; then
    warn "missing required $label: $path"
    printf 'missing required file %s -> %s\n' "$label" "$path" >>"$local_report"
  else
    printf 'missing optional file %s -> %s\n' "$label" "$path" >>"$local_report"
  fi
}

write_export() {
  local name=$1
  local value=${!name}
  printf 'export %s=%q\n' "$name" "$value" >>"$settings_env"
}

write_settings_env() {
  cat >"$settings_env" <<EOF
# Generated by scripts/prepare-long-range-tailnet-field.sh
# Source this file only for the current field run.
EOF
  local names=(
    DRONE_HOST DRONE_IFACE DRONE_REMOTE_PATH DRONE_WFB_KEY LOCAL_WFB_KEY
    FIRMWARE RADIO_CONFIG RADIO_SERVICE_BIN WFB_TX_BIN WFB_RX_BIN TUN_BIN
    TUN_IMPL TUN_SCRIPT PYTHON SSH_OPTS CHANNEL BANDWIDTH_MHZ LINK_ID
    WFB_CLI_LINK_ID LOCAL_IP PEER_IP TUN_RX_RADIO_PORT TUN_TX_RADIO_PORT
    MCS FEC_K FEC_N TX_MIN_INTERVAL_US TX_BURST_LIMIT TX_POWER_MODE
    TX_POWER_INDEX TX_POWER_PATH AIRTIME_MODE
    AIRTIME_TDD_FIRST_WINDOW AIRTIME_TDD_RX_WINDOW_MS AIRTIME_TDD_TX_WINDOW_MS
    AIRTIME_TDD_GUARD_MS AIRTIME_TDD_START_DELAY_MS DATA_LOAD_MCS
    DATA_LOAD_FEC_K DATA_LOAD_FEC_N DATA_M2L_RADIO_PORT DATA_L2M_RADIO_PORT
    DATA_LOAD_EXPECTED_PAYLOADS DATA_LOAD_MIN_M2L_UNIQUE
    DATA_LOAD_MIN_L2M_UNIQUE DATA_LOAD_COUNTER_SECONDS M2L_RADIO_PORT
    L2M_RADIO_PORT M2L_MCS L2M_MCS M2L_FEC_K M2L_FEC_N L2M_FEC_K L2M_FEC_N
    M2L_PAYLOAD_INTERVAL_SEC L2M_PAYLOAD_INTERVAL_SEC M2L_EXPECTED_PAYLOADS
    L2M_EXPECTED_PAYLOADS SESSION_ACQUIRE_MODE DECRYPT_FAILURE_GATE
    MAX_M2L_DECRYPT_FAILURES MAX_L2M_DECRYPT_FAILURES
    EXPECTED_LOCAL_WFB_KEY_SHA256 EXPECTED_DRONE_WFB_KEY_SHA256
  )
  local name
  for name in "${names[@]}"; do
    write_export "$name"
  done
}

write_run_commands() {
  cat >"$loaded_ping_profile" <<'EOF'
ping-1s-load|Symmetric one-second TDD ping with duplex WFB side load|1000|1000|100|ping|3
EOF
  cat >"$commands_script" <<EOF
#!/usr/bin/env bash
set -euo pipefail
cd $(quote "$REPO_ROOT")
source $(quote "$settings_env")

# Build local binaries required by the macOS ground station.
cargo build -p wfb-radio-service
cargo build -p wfb-tun --bin wfb-tun-macos
scripts/build-wfb-ng-macos-codec.sh

# Re-run preflight immediately before any RF test.
scripts/prepare-long-range-tailnet-field.sh --env-file $(quote "$settings_env") --out-dir $(quote "$OUT_DIR/recheck")

# First-contact tunnel/auth gate. Uses the local GS-side key and a ping probe
# so the packet/decrypt gate does not depend on a separate tunnel SSH key.
PROFILE_FILE=$(quote "$loaded_ping_profile") \\
REPEATS=\${REPEATS:-1} \\
WFB_KEY="\$LOCAL_WFB_KEY" \\
CHANNEL="\$CHANNEL" \\
BANDWIDTH_MHZ="\$BANDWIDTH_MHZ" \\
LINK_ID="\$LINK_ID" \\
MCS="\$MCS" \\
FEC_K="\$FEC_K" \\
FEC_N="\$FEC_N" \\
TX_MIN_INTERVAL_US="\$TX_MIN_INTERVAL_US" \\
TX_BURST_LIMIT="\$TX_BURST_LIMIT" \\
TX_POWER_MODE="\$TX_POWER_MODE" \\
TX_POWER_INDEX="\$TX_POWER_INDEX" \\
TX_POWER_PATH="\$TX_POWER_PATH" \\
AIRTIME_MODE="\$AIRTIME_MODE" \\
AIRTIME_TDD_FIRST_WINDOW="\$AIRTIME_TDD_FIRST_WINDOW" \\
AIRTIME_TDD_RX_WINDOW_MS="\$AIRTIME_TDD_RX_WINDOW_MS" \\
AIRTIME_TDD_TX_WINDOW_MS="\$AIRTIME_TDD_TX_WINDOW_MS" \\
AIRTIME_TDD_GUARD_MS="\$AIRTIME_TDD_GUARD_MS" \\
AIRTIME_TDD_START_DELAY_MS="\$AIRTIME_TDD_START_DELAY_MS" \\
RADIO_CONFIG="\$RADIO_CONFIG" \\
DATA_LOAD_MODE=duplex \\
DATA_LOAD_LINUX_HOST="\$DRONE_HOST" \\
DATA_LOAD_LINUX_WFB_KEY="\$DRONE_WFB_KEY" \\
DATA_LOAD_IFACE="\$DRONE_IFACE" \\
DATA_LOAD_MCS="\$DATA_LOAD_MCS" \\
DATA_LOAD_FEC_K="\$DATA_LOAD_FEC_K" \\
DATA_LOAD_FEC_N="\$DATA_LOAD_FEC_N" \\
DATA_M2L_RADIO_PORT="\$DATA_M2L_RADIO_PORT" \\
DATA_L2M_RADIO_PORT="\$DATA_L2M_RADIO_PORT" \\
DATA_LOAD_EXPECTED_PAYLOADS="\$DATA_LOAD_EXPECTED_PAYLOADS" \\
DATA_LOAD_MIN_M2L_UNIQUE="\$DATA_LOAD_MIN_M2L_UNIQUE" \\
DATA_LOAD_MIN_L2M_UNIQUE="\$DATA_LOAD_MIN_L2M_UNIQUE" \\
DATA_LOAD_COUNTER_SECONDS="\$DATA_LOAD_COUNTER_SECONDS" \\
scripts/run-mac-wf-tun-profile-matrix.sh --out-dir $(quote "$OUT_DIR/tunnel-loaded")

if [[ "\${RUN_RAW_DUPLEX:-0}" != "1" ]]; then
  echo "Skipping controlled raw-duplex gate. Set RUN_RAW_DUPLEX=1 when the drone can tolerate temporary WFB peer process isolation." >&2
  exit 0
fi

# Raw WFB packet/decrypt gate. This runner uses WFB_KEY for the remote drone key.
PROFILE_SET=range \\
REPEATS=\${REPEATS:-1} \\
RADIO_RUN_CONFIG="\$RADIO_CONFIG" \\
LINUX_HOST="\$DRONE_HOST" \\
IFACE="\$DRONE_IFACE" \\
WFB_KEY="\$DRONE_WFB_KEY" \\
CHANNEL="\$CHANNEL" \\
BANDWIDTH_MHZ="\$BANDWIDTH_MHZ" \\
LINK_ID="\$LINK_ID" \\
WFB_CLI_LINK_ID="\$WFB_CLI_LINK_ID" \\
M2L_RADIO_PORT="\$M2L_RADIO_PORT" \\
L2M_RADIO_PORT="\$L2M_RADIO_PORT" \\
M2L_MCS="\$M2L_MCS" \\
L2M_MCS="\$L2M_MCS" \\
M2L_FEC_K="\$M2L_FEC_K" \\
M2L_FEC_N="\$M2L_FEC_N" \\
L2M_FEC_K="\$L2M_FEC_K" \\
L2M_FEC_N="\$L2M_FEC_N" \\
M2L_PAYLOAD_INTERVAL_SEC="\$M2L_PAYLOAD_INTERVAL_SEC" \\
L2M_PAYLOAD_INTERVAL_SEC="\$L2M_PAYLOAD_INTERVAL_SEC" \\
M2L_EXPECTED_PAYLOADS="\$M2L_EXPECTED_PAYLOADS" \\
L2M_EXPECTED_PAYLOADS="\$L2M_EXPECTED_PAYLOADS" \\
SESSION_ACQUIRE_MODE="\$SESSION_ACQUIRE_MODE" \\
DECRYPT_FAILURE_GATE="\$DECRYPT_FAILURE_GATE" \\
MAX_M2L_DECRYPT_FAILURES="\$MAX_M2L_DECRYPT_FAILURES" \\
MAX_L2M_DECRYPT_FAILURES="\$MAX_L2M_DECRYPT_FAILURES" \\
TX_BURST_LIMIT="\$TX_BURST_LIMIT" \\
TX_POWER_MODE="\$TX_POWER_MODE" \\
TX_POWER_INDEX="\$TX_POWER_INDEX" \\
TX_POWER_PATH="\$TX_POWER_PATH" \\
M2L_INGRESS_MODE=ssh-udp-relay \\
OUT_DIR=$(quote "$OUT_DIR/raw-duplex") \\
scripts/run-radio-run-profile-matrix.sh
EOF
  chmod +x "$commands_script"
}

run_local_preflight() {
  : >"$local_report"
  printf 'repo=%s\n' "$REPO_ROOT" >>"$local_report"
  printf 'generated_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >>"$local_report"
  printf 'channel=%s bandwidth_mhz=%s link_id=%s wfb_cli_link_id=%s\n' \
    "$CHANNEL" "$BANDWIDTH_MHZ" "$LINK_ID" "$WFB_CLI_LINK_ID" >>"$local_report"

  check_command cargo
  check_command "$PYTHON"
  check_command ssh
  check_command scp 0
  check_command tailscale 0
  check_command tcpdump 0
  check_command shasum 0
  check_command sha256sum 0

  check_file "$RADIO_CONFIG" "radio config"
  check_file "$FIRMWARE" "firmware"
  check_file "$RADIO_SERVICE_BIN" "wfb-radio-service binary"
  check_file "$WFB_TX_BIN" "wfb_tx binary"
  check_file "$WFB_RX_BIN" "wfb_rx binary"
  if [[ "$TUN_IMPL" == "rust" ]]; then
    check_file "$TUN_BIN" "wfb-tun-macos binary"
  else
    check_file "$TUN_SCRIPT" "tunnel script"
  fi
  if [[ -n "$LOCAL_WFB_KEY" ]]; then
    check_file "$LOCAL_WFB_KEY" "local GS WFB key"
  else
    warn "LOCAL_WFB_KEY is not set; tunnel/managed-stream runs cannot authenticate WFB payloads yet"
  fi
}

write_fingerprints() {
  : >"$fingerprints_report"
  local local_fp=
  if [[ -n "$LOCAL_WFB_KEY" ]] && local_fp=$(hash_file "$LOCAL_WFB_KEY"); then
    printf 'local_gs_key_sha256=%s  %s\n' "$local_fp" "$LOCAL_WFB_KEY" >>"$fingerprints_report"
    if [[ -n "$EXPECTED_LOCAL_WFB_KEY_SHA256" && "$EXPECTED_LOCAL_WFB_KEY_SHA256" != "$local_fp" ]]; then
      warn "local GS key fingerprint does not match EXPECTED_LOCAL_WFB_KEY_SHA256"
    fi
  else
    printf 'local_gs_key_sha256=unavailable\n' >>"$fingerprints_report"
  fi
  printf 'drone_key_sha256=see %s after remote preflight\n' "$drone_report" >>"$fingerprints_report"
  printf '\nNote: gs.key and drone.key are paired but normally not identical files. Do not expect their SHA-256 values to match.\n' >>"$fingerprints_report"
}

run_tailnet_probe() {
  if [[ -z "$DRONE_HOST" || "$SKIP_DRONE" == "1" ]]; then
    return
  fi
  local host_only=${DRONE_HOST##*@}
  if command -v tailscale >/dev/null 2>&1; then
    tailscale status >"$OUT_DIR/tailscale-status.txt" 2>&1 || true
    tailscale ping --c 1 --timeout 5s "$host_only" >"$OUT_DIR/tailscale-ping.txt" 2>&1 || true
  fi
}

run_drone_preflight() {
  if [[ "$SKIP_DRONE" == "1" ]]; then
    log "skipping drone preflight"
    return
  fi
  if [[ -z "$DRONE_HOST" ]]; then
    warn "DRONE_HOST is not set; drone-side SSH preflight is deferred"
    return
  fi

  log "preflighting drone over SSH: $DRONE_HOST"
  if ! ssh "${SSH_OPTS_ARRAY[@]}" "$DRONE_HOST" \
    "export PATH=$(quote "$DRONE_REMOTE_PATH"):\$PATH; export DRONE_IFACE=$(quote "$DRONE_IFACE") DRONE_WFB_KEY=$(quote "$DRONE_WFB_KEY") DRONE_REMOTE_PATH=$(quote "$DRONE_REMOTE_PATH") CHANNEL=$(quote "$CHANNEL") BANDWIDTH_MHZ=$(quote "$BANDWIDTH_MHZ") LINK_ID=$(quote "$LINK_ID") WFB_CLI_LINK_ID=$(quote "$WFB_CLI_LINK_ID"); python3 -" \
    >"$drone_report" 2>"$OUT_DIR/drone-preflight.stderr" <<'PY'
import json
import os
import shlex
import shutil
import subprocess
import time


def run(cmd, timeout=8, shell=False):
    try:
        proc = subprocess.run(
            cmd,
            shell=shell,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        return {
            "status": proc.returncode,
            "stdout": proc.stdout[-4000:],
            "stderr": proc.stderr[-4000:],
        }
    except Exception as exc:
        return {"status": -1, "stdout": "", "stderr": str(exc)}


path = os.environ["DRONE_REMOTE_PATH"] + ":" + os.environ.get("PATH", "")
os.environ["PATH"] = path
iface = os.environ["DRONE_IFACE"]
key = os.environ["DRONE_WFB_KEY"]
commands = {
    name: shutil.which(name)
    for name in (
        "sudo",
        "iw",
        "ip",
        "tcpdump",
        "timeout",
        "python3",
        "wfb_rx",
        "wfb_tx",
        "tailscale",
        "sha256sum",
        "shasum",
        "docker",
        "pkill",
    )
}
sudo_ok = False
sudo_probe = {"status": -1, "stdout": "", "stderr": "sudo missing"}
if commands["sudo"]:
    sudo_probe = run(["sudo", "-n", "true"])
    sudo_ok = sudo_probe["status"] == 0

key_readable = False
key_sha256 = None
key_probe = {"status": -1, "stdout": "", "stderr": "sudo unavailable"}
if sudo_ok:
    key_probe = run(["sudo", "-n", "test", "-r", key])
    key_readable = key_probe["status"] == 0
    if key_readable:
        key_cmd = (
            "if command -v sha256sum >/dev/null 2>&1; then "
            f"sha256sum {shlex.quote(key)}; "
            "elif command -v shasum >/dev/null 2>&1; then "
            f"shasum -a 256 {shlex.quote(key)}; "
            "else exit 127; fi"
        )
        key_hash_probe = run(["sudo", "-n", "sh", "-c", key_cmd])
        if key_hash_probe["status"] == 0 and key_hash_probe["stdout"].strip():
            key_sha256 = key_hash_probe["stdout"].strip().split()[0]

iface_exists = os.path.exists(f"/sys/class/net/{iface}")
iw_info = run(["iw", "dev", iface, "info"]) if commands["iw"] else None
ip_link = run(["ip", "link", "show", "dev", iface]) if commands["ip"] else None
tcpdump_version = run(["tcpdump", "--version"], timeout=4) if commands["tcpdump"] else None
wfb_rx_help = run(["wfb_rx", "--help"], timeout=4) if commands["wfb_rx"] else None
wfb_tx_help = run(["wfb_tx", "--help"], timeout=4) if commands["wfb_tx"] else None

report = {
    "schema": "wfb_long_range_drone_preflight/v1",
    "generated_at_unix": time.time(),
    "settings": {
        "iface": iface,
        "channel": int(os.environ["CHANNEL"], 0),
        "bandwidth_mhz": int(os.environ["BANDWIDTH_MHZ"], 0),
        "link_id": os.environ["LINK_ID"],
        "wfb_cli_link_id": int(os.environ["WFB_CLI_LINK_ID"], 0),
        "wfb_key": key,
    },
    "commands": commands,
    "sudo": {"ok": sudo_ok, "probe": sudo_probe},
    "wfb_key": {
        "readable_via_sudo": key_readable,
        "sha256": key_sha256,
        "probe": key_probe,
    },
    "iface": {
        "exists": iface_exists,
        "ip_link": ip_link,
        "iw_info": iw_info,
    },
    "tool_probes": {
        "tcpdump_version": tcpdump_version,
        "wfb_rx_help": wfb_rx_help,
        "wfb_tx_help": wfb_tx_help,
    },
}
print(json.dumps(report, indent=2, sort_keys=True))
PY
  then
    warn "drone SSH preflight failed; see $OUT_DIR/drone-preflight.stderr"
    return
  fi

  if [[ -n "$EXPECTED_DRONE_WFB_KEY_SHA256" ]]; then
    "$PYTHON" - "$drone_report" "$EXPECTED_DRONE_WFB_KEY_SHA256" <<'PY' || warn "unable to verify drone key fingerprint"
import json
import sys

path, expected = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)
actual = (data.get("wfb_key") or {}).get("sha256")
if actual != expected:
    print(f"drone key fingerprint mismatch: actual={actual} expected={expected}", file=sys.stderr)
    sys.exit(1)
PY
  fi
}

write_summary() {
  local warning_file="$OUT_DIR/warnings.txt"
  : >"$warning_file"
  local warning
  if (( WARNINGS_COUNT > 0 )); then
    for warning in "${WARNINGS[@]}"; do
      printf '%s\n' "$warning" >>"$warning_file"
    done
  fi
  cat <<EOF
Long-range field prep artifacts: $OUT_DIR
  settings:     $settings_env
  run commands: $commands_script
  local checks: $local_report
  drone checks: $drone_report
  key notes:    $fingerprints_report
  warnings:     $warning_file
EOF
}

write_settings_env
run_local_preflight
write_fingerprints
run_tailnet_probe
run_drone_preflight
write_run_commands
write_summary

if (( STRICT_PREP == 1 && WARNINGS_COUNT > 0 )); then
  exit 1
fi
