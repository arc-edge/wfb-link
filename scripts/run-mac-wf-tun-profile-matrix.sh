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
  PROFILE_SET=short             # short, latency, throughput, or minimal
  REPEATS=1

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

WFB_KEY=${WFB_KEY:-}
PEER_IP=${PEER_IP:-10.5.0.2}
SSH_USER=${SSH_USER:-pi}
SSH_KEY=${SSH_KEY:-}
SSH_CONNECT_TIMEOUT=${SSH_CONNECT_TIMEOUT:-30}
PING_COUNT=${PING_COUNT:-8}
PING_INTERVAL=${PING_INTERVAL:-0.25}
SSH_DD_BLOCK_SIZE=${SSH_DD_BLOCK_SIZE:-1024}
SSH_DD_COUNT=${SSH_DD_COUNT:-256}
CUSTOM_PROBE_COMMAND=${CUSTOM_PROBE_COMMAND:-}

CHANNEL=${CHANNEL:-161}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
MCS=${MCS:-1}
FEC_K=${FEC_K:-2}
FEC_N=${FEC_N:-4}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}

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
      printf 'OUT_DIR=%q WFB_KEY=%q PEER_IP=%q AIRTIME_TDD_RX_WINDOW_MS=%q AIRTIME_TDD_TX_WINDOW_MS=%q AIRTIME_TDD_GUARD_MS=%q TUN_SETTLE_SECONDS=%q TUN_PROBE_COMMAND=%q %q\n' \
        "$run_dir" "$WFB_KEY" "$PEER_IP" "$rx_ms" "$tx_ms" "$guard_ms" "$settle_seconds" "$probe_command" "$RECOVERY_SCRIPT"
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
python3 - <<'PY'
import json
import os
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
runs = []
for line in manifest_path.read_text(encoding="utf-8").splitlines():
    if not line.strip():
        continue
    entry = json.loads(line)
    run_dir = Path(entry["run_dir"])
    summary = read_json(run_dir / "summary.json")
    tunnel = summary.get("tunnel") if isinstance(summary, dict) else None
    radio = summary.get("radio") if isinstance(summary, dict) else None
    probe = summary.get("probe") if isinstance(summary, dict) else None
    probe_status = probe.get("status") if isinstance(probe, dict) else None
    probe_log_tail = probe.get("log_tail") if isinstance(probe, dict) else None
    counters = tunnel.get("counters") if isinstance(tunnel, dict) else {}
    probe_bytes = None
    if isinstance(probe_log_tail, str):
        for probe_line in reversed(probe_log_tail.splitlines()):
            stripped = probe_line.strip()
            if stripped.isdigit():
                probe_bytes = int(stripped)
                break
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
    }
    runs.append(run)

pass_count = sum(1 for run in runs if run["status"] == 0 and run["result"] == "pass")
summary = {
    "schema": "wfb_mac_wf_tun_profile_matrix/v1",
    "run_id": os.environ["RUN_ID"],
    "out_dir": str(out_dir),
    "run_count": len(runs),
    "pass_count": pass_count,
    "fail_count": len(runs) - pass_count,
    "result": "pass" if runs and pass_count == len(runs) else "fail",
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
    f"- Runs: {pass_count}/{len(runs)} passed",
    f"- Artifacts: `{out_dir}`",
    "",
    "| Profile | Repeat | Probe | TDD rx/tx/guard ms | Result | Probe | Tun in/out | Tunnel dg in/out |",
    "|---|---:|---|---:|---|---:|---:|---:|",
]
for run in runs:
    probe_detail = ""
    if run.get("probe_duration_s") is not None:
        probe_detail = f"{run['probe_duration_s']:.3f}s"
    if run.get("probe_bytes") is not None:
        probe_detail = f"{probe_detail} {run['probe_bytes']}B".strip()
    lines.append(
        "| {profile} | {repeat} | {probe_kind} | {rx_window_ms}/{tx_window_ms}/{guard_ms} | {result} | {probe_detail} | {tun_packets_in}/{tun_packets_out} | {tunnel_datagrams_in}/{tunnel_datagrams_out} |".format(
            probe_detail=probe_detail,
            **run,
        )
    )
lines.append("")
md_path.write_text("\n".join(lines), encoding="utf-8")
PY

log "matrix artifacts: $MATRIX_OUT_DIR"
if (( failures != 0 )); then
  exit 1
fi
