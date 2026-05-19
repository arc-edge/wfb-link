#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-mac-wf-tun-profile-matrix.sh [--out-dir DIR] [--dry-run]

Runs repeatable short-range macOS wf_tun recovery profiles on the hardware Mac.
Each profile wraps scripts/run-mac-wf-tun-recovery.sh and writes a matrix JSON
and Markdown summary.

Common configuration:
  WFB_KEY=/Users/rownd/.config/arc/wfb/gs.key
  SSH_KEY=/Users/rownd/.ssh/id_ed25519_drone
  PEER_IP=10.5.0.2
  MATRIX_OUT_DIR=/tmp/wfb-mac-wf-tun-profile-matrix
  PROFILE_SET=short             # minimal, short, latency, throughput, soak, loaded
  REPEATS=1
  MATRIX_ENFORCE_THRESHOLDS=1
  PING_MAX_LOSS_PCT=0 PING_MAX_AVG_MS=500 PING_MAX_MAX_MS=1500
  SSH_MAX_DURATION_S=5 SSH_DD_MAX_DURATION_S=10
  TX_INGRESS_MAX_PENDING=64 TX_INGRESS_MAX_PENDING_PCT=10
  DATA_LOAD_MODE=duplex          # optional: none, m2l, l2m, or duplex
  DATA_LOAD_PRE_PROBE_SECONDS=0  # optional warmup delay after data sources start

Set PROFILE_FILE to a pipe-delimited profile list:
  name|description|rx_window_ms|tx_window_ms|guard_ms|probe_kind|settle_seconds

Probe kinds:
  ping       ICMP ping to PEER_IP
  ssh        SSH hostname probe through the tunnel
  ssh-dd     SSH download probe through the tunnel
  custom     Use CUSTOM_PROBE_COMMAND
EOF
}

log() {
  printf '[tun-matrix] %s\n' "$*" >&2
}

die() {
  printf '[tun-matrix] error: %s\n' "$*" >&2
  exit 1
}

quote() {
  printf '%q' "$1"
}

join_quoted() {
  local arg
  for arg in "$@"; do
    printf '%q ' "$arg"
  done
}

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
DRY_RUN=0
MATRIX_OUT_DIR=${MATRIX_OUT_DIR:-/tmp/wfb-mac-wf-tun-profile-matrix-$RUN_ID}

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

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

RECOVERY_SCRIPT=${RECOVERY_SCRIPT:-$REPO_ROOT/scripts/run-mac-wf-tun-recovery.sh}
PROFILE_SET=${PROFILE_SET:-short}
PROFILE_FILE=${PROFILE_FILE:-}
REPEATS=${REPEATS:-1}
MATRIX_CONTINUE_ON_FAIL=${MATRIX_CONTINUE_ON_FAIL:-1}
LOADED_PROFILE_DEFAULTS=0
if [[ -z "$PROFILE_FILE" && "$PROFILE_SET" == "loaded" ]]; then
  LOADED_PROFILE_DEFAULTS=1
fi

WFB_KEY=${WFB_KEY:-}
PEER_IP=${PEER_IP:-10.5.0.2}
SSH_USER=${SSH_USER:-pi}
SSH_KEY=${SSH_KEY:-}
SSH_CONNECT_TIMEOUT=${SSH_CONNECT_TIMEOUT:-30}
PING_COUNT=${PING_COUNT:-8}
PING_INTERVAL=${PING_INTERVAL:-0.25}
SSH_DD_BLOCK_SIZE=${SSH_DD_BLOCK_SIZE:-1024}
SSH_DD_COUNT=${SSH_DD_COUNT:-256}
SSH_DD_MIN_BYTES=${SSH_DD_MIN_BYTES:-$((SSH_DD_BLOCK_SIZE * SSH_DD_COUNT))}
CUSTOM_PROBE_COMMAND=${CUSTOM_PROBE_COMMAND:-}
MATRIX_ENFORCE_THRESHOLDS=${MATRIX_ENFORCE_THRESHOLDS:-1}
PING_MAX_LOSS_PCT=${PING_MAX_LOSS_PCT:-0}
PING_MAX_AVG_MS=${PING_MAX_AVG_MS:-500}
PING_MAX_MAX_MS=${PING_MAX_MAX_MS:-1500}
SSH_MAX_DURATION_S=${SSH_MAX_DURATION_S:-5}
SSH_DD_MAX_DURATION_S=${SSH_DD_MAX_DURATION_S:-10}
TUNNEL_MAX_DROPPED_PACKETS=${TUNNEL_MAX_DROPPED_PACKETS:-0}
TUNNEL_MAX_CORRUPT_MESSAGES=${TUNNEL_MAX_CORRUPT_MESSAGES:-0}
TUNNEL_MAX_TRUNCATED_MESSAGES=${TUNNEL_MAX_TRUNCATED_MESSAGES:-0}
TX_MAX_FAILED_SUBMISSIONS=${TX_MAX_FAILED_SUBMISSIONS:-0}
TX_INGRESS_MAX_QUEUE_SEND_FAILED=${TX_INGRESS_MAX_QUEUE_SEND_FAILED:-0}
TX_INGRESS_MAX_PENDING=${TX_INGRESS_MAX_PENDING:-64}
TX_INGRESS_MAX_PENDING_PCT=${TX_INGRESS_MAX_PENDING_PCT:-10}
RADIO_REQUIRE_PASS=${RADIO_REQUIRE_PASS:-1}

CHANNEL=${CHANNEL:-161}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
MCS=${MCS:-1}
FEC_K=${FEC_K:-2}
FEC_N=${FEC_N:-4}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}
TX_POWER_MODE=${TX_POWER_MODE:-current-default}
TX_POWER_INDEX=${TX_POWER_INDEX:-}
TX_POWER_PATH=${TX_POWER_PATH:-both}
if (( LOADED_PROFILE_DEFAULTS == 1 )); then
  TX_MIN_INTERVAL_US=${TX_MIN_INTERVAL_US:-700}
  DATA_LOAD_MODE=${DATA_LOAD_MODE:-duplex}
  DATA_LOAD_INTERVAL_SEC=${DATA_LOAD_INTERVAL_SEC:-0.040}
else
  TX_MIN_INTERVAL_US=${TX_MIN_INTERVAL_US:-0}
  DATA_LOAD_MODE=${DATA_LOAD_MODE:-none}
  DATA_LOAD_INTERVAL_SEC=${DATA_LOAD_INTERVAL_SEC:-0.020}
fi
DATA_LOAD_EXPECTED_PAYLOADS=${DATA_LOAD_EXPECTED_PAYLOADS:-100}
DATA_LOAD_MIN_M2L_UNIQUE=${DATA_LOAD_MIN_M2L_UNIQUE:-$DATA_LOAD_EXPECTED_PAYLOADS}
DATA_LOAD_MIN_L2M_UNIQUE=${DATA_LOAD_MIN_L2M_UNIQUE:-$DATA_LOAD_EXPECTED_PAYLOADS}
DATA_LOAD_PAYLOAD_LEN=${DATA_LOAD_PAYLOAD_LEN:-512}
DATA_LOAD_WARMUP_PAYLOADS=${DATA_LOAD_WARMUP_PAYLOADS:-20}
DATA_LOAD_TAIL_PAYLOADS=${DATA_LOAD_TAIL_PAYLOADS:-8}
DATA_LOAD_PRE_PROBE_SECONDS=${DATA_LOAD_PRE_PROBE_SECONDS:-0}
DATA_LOAD_COUNTER_SECONDS=${DATA_LOAD_COUNTER_SECONDS:-20}
DATA_LOAD_MCS=${DATA_LOAD_MCS:-1}
DATA_LOAD_FEC_K=${DATA_LOAD_FEC_K:-2}
DATA_LOAD_FEC_N=${DATA_LOAD_FEC_N:-4}
DATA_M2L_RADIO_PORT=${DATA_M2L_RADIO_PORT:-6}
DATA_L2M_RADIO_PORT=${DATA_L2M_RADIO_PORT:-7}
DATA_LOAD_LINUX_HOST=${DATA_LOAD_LINUX_HOST:-pi@drone-2f389.local}
DATA_LOAD_LINUX_WFB_KEY=${DATA_LOAD_LINUX_WFB_KEY:-/var/lib/arc/wfb/drone.key}
DATA_LOAD_IFACE=${DATA_LOAD_IFACE:-wfb0}
DATA_LOAD_REQUIRE_PASS=${DATA_LOAD_REQUIRE_PASS:-1}

if (( REPEATS < 1 )); then
  die "REPEATS must be >= 1"
fi
if [[ ! -x "$RECOVERY_SCRIPT" ]]; then
  die "missing executable recovery script: $RECOVERY_SCRIPT"
fi
if [[ -z "$WFB_KEY" || ! -r "$WFB_KEY" ]]; then
  die "set WFB_KEY to a readable GS-side WFB-NG keypair file"
fi

mkdir -p "$MATRIX_OUT_DIR/runs"
MATRIX_OUT_DIR=$(cd "$MATRIX_OUT_DIR" && pwd)
MANIFEST="$MATRIX_OUT_DIR/manifest.jsonl"
: >"$MANIFEST"

profile_lines() {
  if [[ -n "$PROFILE_FILE" ]]; then
    cat "$PROFILE_FILE"
    return
  fi

  case "$PROFILE_SET" in
    minimal)
      cat <<'EOF'
ssh-default|Baseline TDD tunnel SSH smoke|7000|20000|500|ssh|3
EOF
      ;;
    short)
      cat <<'EOF'
ssh-default|Baseline TDD tunnel SSH smoke|7000|20000|500|ssh|3
ping-1s|Symmetric one-second TDD ping latency probe|1000|1000|100|ping|3
ssh-1s|Symmetric one-second TDD SSH smoke|1000|1000|100|ssh|3
EOF
      ;;
    latency)
      cat <<'EOF'
ping-1s|Symmetric one-second TDD ping latency probe|1000|1000|100|ping|3
ping-500ms|Symmetric half-second TDD ping latency probe|500|500|50|ping|3
ssh-1s|Symmetric one-second TDD SSH smoke|1000|1000|100|ssh|3
EOF
      ;;
    throughput)
      cat <<'EOF'
ssh-dd-default|Baseline TDD SSH download probe|7000|20000|500|ssh-dd|3
ssh-dd-1s|Symmetric one-second TDD SSH download probe|1000|1000|100|ssh-dd|3
EOF
      ;;
    soak)
      cat <<'EOF'
ping-500ms|Symmetric half-second TDD ping production gate|500|500|50|ping|3
ssh-1s|Symmetric one-second TDD SSH production gate|1000|1000|100|ssh|3
ssh-dd-1s|Symmetric one-second TDD SSH download production gate|1000|1000|100|ssh-dd|3
EOF
      ;;
    loaded)
      cat <<'EOF'
ssh-dd-1s-load|Symmetric one-second TDD SSH download with duplex WFB side load|1000|1000|100|ssh-dd|3
EOF
      ;;
    *)
      die "unknown PROFILE_SET=$PROFILE_SET"
      ;;
  esac
}

build_ssh_prefix() {
  if [[ -z "$SSH_KEY" || ! -r "$SSH_KEY" ]]; then
    die "probe requires SSH_KEY to be set to a readable private key"
  fi
  join_quoted \
    ssh \
    -i "$SSH_KEY" \
    -o IdentitiesOnly=yes \
    -o BatchMode=yes \
    -o "ConnectTimeout=$SSH_CONNECT_TIMEOUT" \
    -o ServerAliveInterval=5 \
    -o ServerAliveCountMax=2 \
    -o StrictHostKeyChecking=no \
    -o "UserKnownHostsFile=$MATRIX_OUT_DIR/known_hosts" \
    "$SSH_USER@$PEER_IP"
}

build_probe_command() {
  local probe_kind=$1
  case "$probe_kind" in
    ping)
      printf 'ping -c %s -i %s %s' "$(quote "$PING_COUNT")" "$(quote "$PING_INTERVAL")" "$(quote "$PEER_IP")"
      ;;
    ssh)
      printf '%s%s' "$(build_ssh_prefix)" "$(quote hostname)"
      ;;
    ssh-dd)
      printf '%s%s | wc -c' \
        "$(build_ssh_prefix)" \
        "$(quote "dd if=/dev/zero bs=$SSH_DD_BLOCK_SIZE count=$SSH_DD_COUNT 2>/dev/null")"
      ;;
    custom)
      [[ -n "$CUSTOM_PROBE_COMMAND" ]] || die "CUSTOM_PROBE_COMMAND is required for custom probe profiles"
      printf '%s' "$CUSTOM_PROBE_COMMAND"
      ;;
    *)
      die "unknown probe_kind=$probe_kind"
      ;;
  esac
}

write_manifest_line() {
  local profile=$1 repeat=$2 run_dir=$3 status=$4 description=$5 rx_ms=$6 tx_ms=$7 guard_ms=$8 probe_kind=$9
  PROFILE="$profile" \
  REPEAT="$repeat" \
  RUN_DIR="$run_dir" \
  STATUS="$status" \
  DESCRIPTION="$description" \
  RX_MS="$rx_ms" \
  TX_MS="$tx_ms" \
  GUARD_MS="$guard_ms" \
  PROBE_KIND="$probe_kind" \
  python3 - <<'PY' >>"$MANIFEST"
import json
import os

print(json.dumps({
    "profile": os.environ["PROFILE"],
    "repeat": int(os.environ["REPEAT"]),
    "run_dir": os.environ["RUN_DIR"],
    "status": int(os.environ["STATUS"]),
    "description": os.environ["DESCRIPTION"],
    "rx_window_ms": int(os.environ["RX_MS"]),
    "tx_window_ms": int(os.environ["TX_MS"]),
    "guard_ms": int(os.environ["GUARD_MS"]),
    "probe_kind": os.environ["PROBE_KIND"],
}, sort_keys=True))
PY
}

failures=0
while IFS='|' read -r name description rx_ms tx_ms guard_ms probe_kind settle_seconds; do
  [[ -z "${name// }" || "${name:0:1}" == "#" ]] && continue
  for repeat in $(seq 1 "$REPEATS"); do
    run_dir="$MATRIX_OUT_DIR/runs/${name}-r${repeat}"
    mkdir -p "$run_dir"
    probe_command=$(build_probe_command "$probe_kind")

    log "profile=$name repeat=$repeat probe=$probe_kind rx=${rx_ms}ms tx=${tx_ms}ms guard=${guard_ms}ms"
    if (( DRY_RUN == 1 )); then
      printf 'OUT_DIR=%q WFB_KEY=%q PEER_IP=%q DATA_LOAD_MODE=%q DATA_LOAD_PRE_PROBE_SECONDS=%q TX_MIN_INTERVAL_US=%q TX_POWER_MODE=%q TX_POWER_INDEX=%q TX_POWER_PATH=%q AIRTIME_TDD_RX_WINDOW_MS=%q AIRTIME_TDD_TX_WINDOW_MS=%q AIRTIME_TDD_GUARD_MS=%q TUN_SETTLE_SECONDS=%q TUN_PROBE_COMMAND=%q %q\n' \
        "$run_dir" "$WFB_KEY" "$PEER_IP" "$DATA_LOAD_MODE" "$DATA_LOAD_PRE_PROBE_SECONDS" "$TX_MIN_INTERVAL_US" "$TX_POWER_MODE" "$TX_POWER_INDEX" "$TX_POWER_PATH" "$rx_ms" "$tx_ms" "$guard_ms" "$settle_seconds" "$probe_command" "$RECOVERY_SCRIPT"
      status=0
    else
      if OUT_DIR="$run_dir" \
        RUN_ID="${RUN_ID}-${name}-r${repeat}" \
        WFB_KEY="$WFB_KEY" \
        PEER_IP="$PEER_IP" \
        CHANNEL="$CHANNEL" \
        BANDWIDTH_MHZ="$BANDWIDTH_MHZ" \
        MCS="$MCS" \
        FEC_K="$FEC_K" \
        FEC_N="$FEC_N" \
        TX_CALIBRATION_PROFILE="$TX_CALIBRATION_PROFILE" \
        TX_MIN_INTERVAL_US="$TX_MIN_INTERVAL_US" \
        TX_POWER_MODE="$TX_POWER_MODE" \
        TX_POWER_INDEX="$TX_POWER_INDEX" \
        TX_POWER_PATH="$TX_POWER_PATH" \
        DATA_LOAD_MODE="$DATA_LOAD_MODE" \
        DATA_LOAD_EXPECTED_PAYLOADS="$DATA_LOAD_EXPECTED_PAYLOADS" \
        DATA_LOAD_MIN_M2L_UNIQUE="$DATA_LOAD_MIN_M2L_UNIQUE" \
        DATA_LOAD_MIN_L2M_UNIQUE="$DATA_LOAD_MIN_L2M_UNIQUE" \
        DATA_LOAD_PAYLOAD_LEN="$DATA_LOAD_PAYLOAD_LEN" \
        DATA_LOAD_INTERVAL_SEC="$DATA_LOAD_INTERVAL_SEC" \
        DATA_LOAD_WARMUP_PAYLOADS="$DATA_LOAD_WARMUP_PAYLOADS" \
        DATA_LOAD_TAIL_PAYLOADS="$DATA_LOAD_TAIL_PAYLOADS" \
        DATA_LOAD_PRE_PROBE_SECONDS="$DATA_LOAD_PRE_PROBE_SECONDS" \
        DATA_LOAD_COUNTER_SECONDS="$DATA_LOAD_COUNTER_SECONDS" \
        DATA_LOAD_MCS="$DATA_LOAD_MCS" \
        DATA_LOAD_FEC_K="$DATA_LOAD_FEC_K" \
        DATA_LOAD_FEC_N="$DATA_LOAD_FEC_N" \
        DATA_M2L_RADIO_PORT="$DATA_M2L_RADIO_PORT" \
        DATA_L2M_RADIO_PORT="$DATA_L2M_RADIO_PORT" \
        DATA_LOAD_LINUX_HOST="$DATA_LOAD_LINUX_HOST" \
        DATA_LOAD_LINUX_WFB_KEY="$DATA_LOAD_LINUX_WFB_KEY" \
        DATA_LOAD_IFACE="$DATA_LOAD_IFACE" \
        DATA_LOAD_REQUIRE_PASS="$DATA_LOAD_REQUIRE_PASS" \
        AIRTIME_MODE=tdd \
        AIRTIME_TDD_FIRST_WINDOW=rx \
        AIRTIME_TDD_RX_WINDOW_MS="$rx_ms" \
        AIRTIME_TDD_TX_WINDOW_MS="$tx_ms" \
        AIRTIME_TDD_GUARD_MS="$guard_ms" \
        TUN_SETTLE_SECONDS="$settle_seconds" \
        TUN_PROBE_COMMAND="$probe_command" \
        "$RECOVERY_SCRIPT" < /dev/null; then
        status=0
      else
        status=$?
      fi
    fi

    write_manifest_line "$name" "$repeat" "$run_dir" "$status" "$description" "$rx_ms" "$tx_ms" "$guard_ms" "$probe_kind"
    if (( status != 0 )); then
      failures=$((failures + 1))
      if [[ "$MATRIX_CONTINUE_ON_FAIL" != "1" ]]; then
        break 2
      fi
    fi
  done
done < <(profile_lines)

MANIFEST="$MANIFEST" \
MATRIX_OUT_DIR="$MATRIX_OUT_DIR" \
RUN_ID="$RUN_ID" \
DRY_RUN="$DRY_RUN" \
MATRIX_ENFORCE_THRESHOLDS="$MATRIX_ENFORCE_THRESHOLDS" \
PING_MAX_LOSS_PCT="$PING_MAX_LOSS_PCT" \
PING_MAX_AVG_MS="$PING_MAX_AVG_MS" \
PING_MAX_MAX_MS="$PING_MAX_MAX_MS" \
SSH_MAX_DURATION_S="$SSH_MAX_DURATION_S" \
SSH_DD_MIN_BYTES="$SSH_DD_MIN_BYTES" \
SSH_DD_MAX_DURATION_S="$SSH_DD_MAX_DURATION_S" \
TUNNEL_MAX_DROPPED_PACKETS="$TUNNEL_MAX_DROPPED_PACKETS" \
TUNNEL_MAX_CORRUPT_MESSAGES="$TUNNEL_MAX_CORRUPT_MESSAGES" \
TUNNEL_MAX_TRUNCATED_MESSAGES="$TUNNEL_MAX_TRUNCATED_MESSAGES" \
TX_MAX_FAILED_SUBMISSIONS="$TX_MAX_FAILED_SUBMISSIONS" \
TX_INGRESS_MAX_QUEUE_SEND_FAILED="$TX_INGRESS_MAX_QUEUE_SEND_FAILED" \
TX_INGRESS_MAX_PENDING="$TX_INGRESS_MAX_PENDING" \
TX_INGRESS_MAX_PENDING_PCT="$TX_INGRESS_MAX_PENDING_PCT" \
RADIO_REQUIRE_PASS="$RADIO_REQUIRE_PASS" \
DATA_LOAD_REQUIRE_PASS="$DATA_LOAD_REQUIRE_PASS" \
python3 - <<'PY'
import json
import os
import re
import sys
from pathlib import Path


def read_json(path):
    try:
        with open(path, "r", encoding="utf-8") as fh:
            return json.load(fh)
    except FileNotFoundError:
        return None
    except json.JSONDecodeError as exc:
        return {"parse_error": str(exc), "path": str(path)}


manifest_path = Path(os.environ["MANIFEST"])
out_dir = Path(os.environ["MATRIX_OUT_DIR"])
dry_run = os.environ["DRY_RUN"] == "1"
thresholds = {
    "enforce": os.environ["MATRIX_ENFORCE_THRESHOLDS"] == "1" and not dry_run,
    "ping_max_loss_pct": float(os.environ["PING_MAX_LOSS_PCT"]),
    "ping_max_avg_ms": float(os.environ["PING_MAX_AVG_MS"]),
    "ping_max_max_ms": float(os.environ["PING_MAX_MAX_MS"]),
    "ssh_max_duration_s": float(os.environ["SSH_MAX_DURATION_S"]),
    "ssh_dd_min_bytes": int(os.environ["SSH_DD_MIN_BYTES"]),
    "ssh_dd_max_duration_s": float(os.environ["SSH_DD_MAX_DURATION_S"]),
    "tunnel_max_dropped_packets": int(os.environ["TUNNEL_MAX_DROPPED_PACKETS"]),
    "tunnel_max_corrupt_messages": int(os.environ["TUNNEL_MAX_CORRUPT_MESSAGES"]),
    "tunnel_max_truncated_messages": int(os.environ["TUNNEL_MAX_TRUNCATED_MESSAGES"]),
    "tx_max_failed_submissions": int(os.environ["TX_MAX_FAILED_SUBMISSIONS"]),
    "tx_ingress_max_queue_send_failed": int(os.environ["TX_INGRESS_MAX_QUEUE_SEND_FAILED"]),
    "tx_ingress_max_pending": int(os.environ["TX_INGRESS_MAX_PENDING"]),
    "tx_ingress_max_pending_pct": float(os.environ["TX_INGRESS_MAX_PENDING_PCT"]),
    "radio_require_pass": os.environ["RADIO_REQUIRE_PASS"] == "1",
    "data_load_require_pass": os.environ["DATA_LOAD_REQUIRE_PASS"] == "1",
}


def parse_ping_metrics(log_tail):
    if not isinstance(log_tail, str):
        return {}
    metrics = {}
    loss_match = re.search(r"(\d+)\s+packets transmitted,\s+(\d+)\s+packets received,\s+([0-9.]+)% packet loss", log_tail)
    if loss_match:
        metrics["packets_transmitted"] = int(loss_match.group(1))
        metrics["packets_received"] = int(loss_match.group(2))
        metrics["packet_loss_pct"] = float(loss_match.group(3))
    rtt_match = re.search(r"(?:round-trip|rtt) min/avg/max/(?:stddev|mdev) = ([0-9.]+)/([0-9.]+)/([0-9.]+)/([0-9.]+) ms", log_tail)
    if rtt_match:
        metrics["rtt_min_ms"] = float(rtt_match.group(1))
        metrics["rtt_avg_ms"] = float(rtt_match.group(2))
        metrics["rtt_max_ms"] = float(rtt_match.group(3))
        metrics["rtt_stddev_ms"] = float(rtt_match.group(4))
    return metrics


def parse_numeric_tail(log_tail):
    if not isinstance(log_tail, str):
        return None
    for line in reversed(log_tail.splitlines()):
        stripped = line.strip()
        if stripped.isdigit():
            return int(stripped)
    return None


def run_acceptance(entry, run, counters, probe_metrics):
    if dry_run:
        return True, []
    reasons = []
    if run["status"] != 0:
        reasons.append(f"exit_status={run['status']}")
    if run["result"] != "pass":
        reasons.append(f"summary_result={run['result']}")
    if not run["probe_passed"]:
        reasons.append("probe_not_passed")
    if thresholds["radio_require_pass"] and run.get("radio_result") != "pass":
        reasons.append(f"radio_result={run.get('radio_result')}")
    if thresholds["data_load_require_pass"] and run.get("data_load_result") not in {None, "pass"}:
        reasons.append(f"data_load_result={run.get('data_load_result')}")
    if run.get("tx_failed_submissions") is not None and run["tx_failed_submissions"] > thresholds["tx_max_failed_submissions"]:
        reasons.append(f"tx_failed_submissions={run['tx_failed_submissions']}")
    if run.get("tx_ingress_queue_send_failed") is not None and run["tx_ingress_queue_send_failed"] > thresholds["tx_ingress_max_queue_send_failed"]:
        reasons.append(f"tx_ingress_queue_send_failed={run['tx_ingress_queue_send_failed']}")
    if run.get("tx_ingress_pending_datagrams") is not None:
        pending = run["tx_ingress_pending_datagrams"]
        if pending > thresholds["tx_ingress_max_pending"]:
            reasons.append(f"tx_ingress_pending_datagrams={pending}")
        ingress = run.get("tx_ingress_datagrams_received") or 0
        if ingress > 0:
            pending_pct = (pending * 100.0) / ingress
            if pending_pct > thresholds["tx_ingress_max_pending_pct"]:
                reasons.append(f"tx_ingress_pending_pct={pending_pct:.2f}")

    if int(counters.get("dropped_packets") or 0) > thresholds["tunnel_max_dropped_packets"]:
        reasons.append(f"tunnel_dropped_packets={counters.get('dropped_packets')}")
    if int(counters.get("corrupt_messages") or 0) > thresholds["tunnel_max_corrupt_messages"]:
        reasons.append(f"tunnel_corrupt_messages={counters.get('corrupt_messages')}")
    if int(counters.get("truncated_messages") or 0) > thresholds["tunnel_max_truncated_messages"]:
        reasons.append(f"tunnel_truncated_messages={counters.get('truncated_messages')}")

    probe_kind = entry["probe_kind"]
    duration_s = run.get("probe_duration_s")
    if probe_kind == "ping":
        loss_pct = probe_metrics.get("packet_loss_pct")
        avg_ms = probe_metrics.get("rtt_avg_ms")
        max_ms = probe_metrics.get("rtt_max_ms")
        if loss_pct is None:
            reasons.append("ping_loss_missing")
        elif loss_pct > thresholds["ping_max_loss_pct"]:
            reasons.append(f"ping_loss_pct={loss_pct}")
        if avg_ms is None:
            reasons.append("ping_avg_missing")
        elif avg_ms > thresholds["ping_max_avg_ms"]:
            reasons.append(f"ping_avg_ms={avg_ms}")
        if max_ms is None:
            reasons.append("ping_max_missing")
        elif max_ms > thresholds["ping_max_max_ms"]:
            reasons.append(f"ping_max_ms={max_ms}")
    elif probe_kind == "ssh":
        if duration_s is None:
            reasons.append("ssh_duration_missing")
        elif duration_s > thresholds["ssh_max_duration_s"]:
            reasons.append(f"ssh_duration_s={duration_s:.3f}")
    elif probe_kind == "ssh-dd":
        if run.get("probe_bytes") is None:
            reasons.append("ssh_dd_bytes_missing")
        elif run["probe_bytes"] < thresholds["ssh_dd_min_bytes"]:
            reasons.append(f"ssh_dd_bytes={run['probe_bytes']}")
        if duration_s is None:
            reasons.append("ssh_dd_duration_missing")
        elif duration_s > thresholds["ssh_dd_max_duration_s"]:
            reasons.append(f"ssh_dd_duration_s={duration_s:.3f}")
    return not reasons, reasons


runs = []
for line in manifest_path.read_text(encoding="utf-8").splitlines():
    if not line.strip():
        continue
    entry = json.loads(line)
    run_dir = Path(entry["run_dir"])
    summary = read_json(run_dir / "summary.json")
    tunnel = summary.get("tunnel") if isinstance(summary, dict) else None
    radio = summary.get("radio") if isinstance(summary, dict) else None
    radio_tx = radio.get("tx") if isinstance(radio, dict) and isinstance(radio.get("tx"), dict) else {}
    data_load = summary.get("data_load") if isinstance(summary, dict) else None
    probe = summary.get("probe") if isinstance(summary, dict) else None
    probe_status = probe.get("status") if isinstance(probe, dict) else None
    probe_log_tail = probe.get("log_tail") if isinstance(probe, dict) else None
    counters = tunnel.get("counters") if isinstance(tunnel, dict) else {}
    probe_metrics = parse_ping_metrics(probe_log_tail) if entry["probe_kind"] == "ping" else {}
    probe_bytes = parse_numeric_tail(probe_log_tail)
    run = {
        **entry,
        "summary_path": str(run_dir / "summary.json"),
        "result": summary.get("result") if isinstance(summary, dict) else "missing-summary",
        "probe_passed": probe.get("passed") if isinstance(probe, dict) else False,
        "probe_duration_s": probe_status.get("duration_s") if isinstance(probe_status, dict) else None,
        "probe_bytes": probe_bytes,
        "tun_packets_in": counters.get("tun_packets_in"),
        "tun_packets_out": counters.get("tun_packets_out"),
        "tunnel_datagrams_out": counters.get("tunnel_datagrams_out"),
        "tunnel_datagrams_in": counters.get("tunnel_datagrams_in"),
        "radio_result": radio.get("result") if isinstance(radio, dict) else None,
        "tx_ingress_datagrams_received": radio_tx.get("ingress_datagrams_received"),
        "tx_datagrams_processed": radio_tx.get("datagrams_received"),
        "tx_submitted_frames": radio_tx.get("submitted_frames"),
        "tx_failed_submissions": radio_tx.get("failed_submissions"),
        "tx_ingress_queue_send_failed": radio_tx.get("ingress_queue_send_failed"),
        "tx_ingress_pending_datagrams": radio_tx.get("ingress_pending_datagrams"),
        "data_load_result": data_load.get("result") if isinstance(data_load, dict) else None,
        "data_load_mode": data_load.get("mode") if isinstance(data_load, dict) else None,
        "tx_min_interval_us": (summary.get("settings") or {}).get("tx_min_interval_us") if isinstance(summary, dict) else None,
        "data_load_m2l_unique": ((data_load.get("m2l") or {}).get("counter") or {}).get("unique_sequences") if isinstance(data_load, dict) else None,
        "data_load_l2m_unique": ((data_load.get("l2m") or {}).get("counter") or {}).get("unique_sequences") if isinstance(data_load, dict) else None,
        "probe_metrics": probe_metrics,
        "tunnel_dropped_packets": counters.get("dropped_packets"),
        "tunnel_corrupt_messages": counters.get("corrupt_messages"),
        "tunnel_truncated_messages": counters.get("truncated_messages"),
    }
    accepted, reject_reasons = run_acceptance(entry, run, counters, probe_metrics)
    run["accepted"] = accepted
    run["reject_reasons"] = reject_reasons
    runs.append(run)

pass_count = sum(1 for run in runs if run["status"] == 0 and run["result"] == "pass")
accepted_count = sum(1 for run in runs if run["accepted"])
summary = {
    "schema": "wfb_mac_wf_tun_profile_matrix/v1",
    "run_id": os.environ["RUN_ID"],
    "out_dir": str(out_dir),
    "thresholds": thresholds,
    "run_count": len(runs),
    "pass_count": pass_count,
    "accepted_count": accepted_count,
    "fail_count": len(runs) - accepted_count,
    "result": "dry-run" if dry_run else ("pass" if runs and accepted_count == len(runs) else "fail"),
    "runs": runs,
}

json_path = out_dir / "matrix-summary.json"
with open(json_path.with_suffix(".json.tmp"), "w", encoding="utf-8") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)
    fh.write("\n")
os.replace(json_path.with_suffix(".json.tmp"), json_path)

md_path = out_dir / "matrix-summary.md"
lines = [
    "# macOS wf_tun Profile Matrix",
    "",
    f"- Result: {summary['result']}",
    f"- Runs: {accepted_count}/{len(runs)} accepted",
    f"- Artifacts: `{out_dir}`",
    "",
    "| Profile | Repeat | Probe | TDD rx/tx/guard ms | Accepted | Probe | Data Load | TX ingress/proc/sub/pending | Tun in/out | Tunnel dg in/out | Reject Reasons |",
    "|---|---:|---|---:|---|---:|---:|---:|---:|---:|---|",
]
for run in runs:
    probe_detail = ""
    if run.get("probe_duration_s") is not None:
        probe_detail = f"{run['probe_duration_s']:.3f}s"
    if run.get("probe_bytes") is not None:
        probe_detail = f"{probe_detail} {run['probe_bytes']}B".strip()
    metrics = run.get("probe_metrics") or {}
    if metrics.get("packet_loss_pct") is not None:
        probe_detail = f"{probe_detail} loss={metrics['packet_loss_pct']:.1f}% avg={metrics.get('rtt_avg_ms')}ms max={metrics.get('rtt_max_ms')}ms".strip()
    data_load_detail = ""
    if run.get("data_load_mode"):
        data_load_detail = f"{run.get('data_load_mode')} {run.get('data_load_result')}"
        if run.get("data_load_m2l_unique") is not None:
            data_load_detail += f" m2l={run.get('data_load_m2l_unique')}"
        if run.get("data_load_l2m_unique") is not None:
            data_load_detail += f" l2m={run.get('data_load_l2m_unique')}"
    tx_detail = ""
    if run.get("tx_ingress_datagrams_received") is not None:
        tx_detail = "{}/{}/{}/{}".format(
            run.get("tx_ingress_datagrams_received"),
            run.get("tx_datagrams_processed"),
            run.get("tx_submitted_frames"),
            run.get("tx_ingress_pending_datagrams"),
        )
    lines.append(
        "| {profile} | {repeat} | {probe_kind} | {rx_window_ms}/{tx_window_ms}/{guard_ms} | {accepted_label} | {probe_detail} | {data_load_detail} | {tx_detail} | {tun_packets_in}/{tun_packets_out} | {tunnel_datagrams_in}/{tunnel_datagrams_out} | {reject_reason_text} |".format(
            probe_detail=probe_detail,
            data_load_detail=data_load_detail,
            tx_detail=tx_detail,
            accepted_label="yes" if run.get("accepted") else "no",
            reject_reason_text=", ".join(run.get("reject_reasons") or []),
            **run,
        )
    )
lines.append("")
md_path.write_text("\n".join(lines), encoding="utf-8")
if thresholds["enforce"] and summary["result"] != "pass":
    sys.exit(1)
PY

log "matrix artifacts: $MATRIX_OUT_DIR"
if (( failures != 0 )); then
  exit 1
fi
