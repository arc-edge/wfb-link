#!/usr/bin/env bash
# shellcheck disable=SC2029
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/run-calibration-regression-matrix.sh [--dry-run] [--out-dir DIR]

Runs the accepted short-range duplex tuple while varying only RTL8812AU
TX-power/calibration mode and direction. The goal is to quarantine modes that
create WFB decrypt failures and isolate whether the failure appears in
Mac-to-Linux, Linux-to-Mac, or only full duplex.

Common configuration:
  HW_MAC_HOST=rownd@rownds-macbook-pro.tail5c793f.ts.net
  HW_DEPLOY=1
  LINUX_HOST=pi@drone-2f389.local
  LINUX_LAN_IP=auto
  MAC_LAN_IP=10.42.0.162
  VARIANT_SET=isolation       # baseline, duplex, isolation, or full
  REPEATS=1
  EXPECTED_PAYLOADS=1000 SOURCE_WARMUP_PAYLOADS=100
  CALIBRATION_OUT_DIR=/tmp/wfb-radio-calibration-regression

The matrix profile is fixed by default to the current accepted short-range
production-smoke tuple:
  M2L 5/12 MCS1, L2M 3/12 MCS2, 20 ms source interval
EOF
}

log() {
  printf '[calibration-matrix] %s\n' "$*" >&2
}

die() {
  printf '[calibration-matrix] error: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
DRY_RUN=0
CALIBRATION_OUT_DIR=${CALIBRATION_OUT_DIR:-/tmp/wfb-radio-calibration-regression-$RUN_ID}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --out-dir)
      [[ $# -ge 2 ]] || die "--out-dir requires a path"
      CALIBRATION_OUT_DIR=$2
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

require_command python3

VARIANT_SET=${VARIANT_SET:-isolation}
REPEATS=${REPEATS:-1}
EXPECTED_PAYLOADS=${EXPECTED_PAYLOADS:-1000}
SOURCE_WARMUP_PAYLOADS=${SOURCE_WARMUP_PAYLOADS:-100}
MATRIX_SUSTAINED_PAYLOADS=${MATRIX_SUSTAINED_PAYLOADS:-200}
MATRIX_PROFILE_NAME=${MATRIX_PROFILE_NAME:-duplex-m2l5x12-l2m3x12-mcs2-20ms}
MATRIX_PROFILE_DESCRIPTION=${MATRIX_PROFILE_DESCRIPTION:-Accepted short-range duplex sustained candidate}
M2L_FEC_K=${M2L_FEC_K:-5}
M2L_FEC_N=${M2L_FEC_N:-12}
L2M_FEC_K=${L2M_FEC_K:-3}
L2M_FEC_N=${L2M_FEC_N:-12}
M2L_MCS=${M2L_MCS:-1}
L2M_MCS=${L2M_MCS:-2}
PAYLOAD_INTERVAL_SEC=${PAYLOAD_INTERVAL_SEC:-0.020}
M2L_MIN_PCT=${M2L_MIN_PCT:-95}
L2M_MIN_PCT=${L2M_MIN_PCT:-90}

HW_MAC_HOST=${HW_MAC_HOST:-rownd@rownds-macbook-pro.tail5c793f.ts.net}
LOCAL_HW=${LOCAL_HW:-0}
HW_DEPLOY=${HW_DEPLOY:-1}
HW_DEPLOY_PATH=${HW_DEPLOY_PATH:-projects/arc/wfb-mac-radio-deploy}
LINUX_HOST=${LINUX_HOST:-pi@drone-2f389.local}
LINUX_LAN_IP=${LINUX_LAN_IP:-auto}
MAC_LAN_IP=${MAC_LAN_IP:-10.42.0.162}
TX_POWER_SAFETY_PROFILE=${TX_POWER_SAFETY_PROFILE:-linux-ch36-ht20}
REQUIRE_CALIBRATION_SUCCESS=${REQUIRE_CALIBRATION_SUCCESS:-auto}
DECRYPT_FAILURE_GATE=${DECRYPT_FAILURE_GATE:-post-session}
AUTO_EFUSE_DUMP=${AUTO_EFUSE_DUMP:-1}

mkdir -p "$CALIBRATION_OUT_DIR"
CALIBRATION_OUT_DIR=$(cd "$CALIBRATION_OUT_DIR" && pwd)
PROFILE_FILE_PATH="$CALIBRATION_OUT_DIR/accepted-profile.tsv"
VARIANT_FILE_PATH="$CALIBRATION_OUT_DIR/variants.tsv"

cat > "$PROFILE_FILE_PATH" <<EOF
$MATRIX_PROFILE_NAME|$MATRIX_PROFILE_DESCRIPTION|$M2L_FEC_K|$M2L_FEC_N|$L2M_FEC_K|$L2M_FEC_N|$M2L_MCS|$L2M_MCS|$PAYLOAD_INTERVAL_SEC|$M2L_MIN_PCT|$L2M_MIN_PCT
EOF

variant_lines() {
  case "$VARIANT_SET" in
    baseline)
      cat <<'EOF'
baseline-current-default-duplex|current-default|current-default|1|1|Production-control tuple; this must stay clean.
EOF
      ;;
    duplex)
      cat <<'EOF'
baseline-current-default-duplex|current-default|current-default|1|1|Production-control tuple; this must stay clean.
runtime-iqk-duplex|current-default|rtl8812a-runtime-iqk|1|1|Runtime IQK under full-duplex load.
efuse-derived-duplex|efuse-derived|current-default|1|1|EFUSE TXAGC under full-duplex load.
EOF
      ;;
    isolation)
      cat <<'EOF'
baseline-current-default-duplex|current-default|current-default|1|1|Production-control tuple; this must stay clean.
runtime-iqk-m2l-only|current-default|rtl8812a-runtime-iqk|1|0|Runtime IQK with only Mac-to-Linux TX measured.
runtime-iqk-l2m-only|current-default|rtl8812a-runtime-iqk|0|1|Runtime IQK with only Linux-to-Mac RX measured.
efuse-derived-m2l-only|efuse-derived|current-default|1|0|EFUSE TXAGC with only Mac-to-Linux TX measured.
efuse-derived-l2m-only|efuse-derived|current-default|0|1|EFUSE TXAGC with only Linux-to-Mac RX measured.
EOF
      ;;
    full)
      cat <<'EOF'
baseline-current-default-duplex|current-default|current-default|1|1|Production-control tuple; this must stay clean.
runtime-iqk-duplex|current-default|rtl8812a-runtime-iqk|1|1|Runtime IQK under full-duplex load.
runtime-iqk-m2l-only|current-default|rtl8812a-runtime-iqk|1|0|Runtime IQK with only Mac-to-Linux TX measured.
runtime-iqk-l2m-only|current-default|rtl8812a-runtime-iqk|0|1|Runtime IQK with only Linux-to-Mac RX measured.
efuse-derived-duplex|efuse-derived|current-default|1|1|EFUSE TXAGC under full-duplex load.
efuse-derived-m2l-only|efuse-derived|current-default|1|0|EFUSE TXAGC with only Mac-to-Linux TX measured.
efuse-derived-l2m-only|efuse-derived|current-default|0|1|EFUSE TXAGC with only Linux-to-Mac RX measured.
EOF
      ;;
    *)
      die "unknown VARIANT_SET: $VARIANT_SET"
      ;;
  esac
}

variant_lines > "$VARIANT_FILE_PATH"

run_variant() {
  local name=$1
  local tx_power_mode=$2
  local tx_calibration_profile=$3
  local enable_m2l=$4
  local enable_l2m=$5
  local note=$6
  local variant_out="$CALIBRATION_OUT_DIR/$name"
  local variant_run_id="${RUN_ID}-${name}"
  local matrix_args=(--out-dir "$variant_out")
  if (( DRY_RUN == 1 )); then
    matrix_args=(--dry-run "${matrix_args[@]}")
  fi

  log "variant=$name tx_power=$tx_power_mode calibration=$tx_calibration_profile m2l=$enable_m2l l2m=$enable_l2m"
  mkdir -p "$variant_out"
  printf '%s\n' "$note" > "$variant_out/variant-note.txt"

  env \
    RUN_ID="$variant_run_id" \
    REMOTE_MATRIX_OUT_DIR="/tmp/wfb-radio-profile-matrix-$variant_run_id" \
    MATRIX_OUT_DIR="$variant_out" \
    PROFILE_FILE="$PROFILE_FILE_PATH" \
    PROFILE_SET=minimal \
    REPEATS="$REPEATS" \
    EXPECTED_PAYLOADS="$EXPECTED_PAYLOADS" \
    SOURCE_WARMUP_PAYLOADS="$SOURCE_WARMUP_PAYLOADS" \
    MATRIX_SUSTAINED_PAYLOADS="$MATRIX_SUSTAINED_PAYLOADS" \
    HW_MAC_HOST="$HW_MAC_HOST" \
    LOCAL_HW="$LOCAL_HW" \
    HW_DEPLOY="$HW_DEPLOY" \
    HW_DEPLOY_PATH="$HW_DEPLOY_PATH" \
    LINUX_HOST="$LINUX_HOST" \
    LINUX_LAN_IP="$LINUX_LAN_IP" \
    MAC_LAN_IP="$MAC_LAN_IP" \
    TX_POWER_MODE="$tx_power_mode" \
    TX_POWER_SAFETY_PROFILE="$TX_POWER_SAFETY_PROFILE" \
    TX_CALIBRATION_PROFILE="$tx_calibration_profile" \
    REQUIRE_CALIBRATION_SUCCESS="$REQUIRE_CALIBRATION_SUCCESS" \
    DECRYPT_FAILURE_GATE="$DECRYPT_FAILURE_GATE" \
    AUTO_EFUSE_DUMP="$AUTO_EFUSE_DUMP" \
    ENABLE_M2L="$enable_m2l" \
    ENABLE_L2M="$enable_l2m" \
    scripts/run-radio-run-profile-matrix.sh "${matrix_args[@]}"
}

write_summary() {
  python3 - "$CALIBRATION_OUT_DIR" "$VARIANT_FILE_PATH" "$VARIANT_SET" "$DRY_RUN" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
variant_file = Path(sys.argv[2])
variant_set = sys.argv[3]
dry_run = sys.argv[4] == "1"

def load(path):
    try:
        return json.loads(path.read_text())
    except Exception as exc:
        return {"error": str(exc), "path": str(path)}

variants = []
for raw in variant_file.read_text().splitlines():
    if not raw or raw.startswith("#"):
        continue
    name, tx_power, calibration, enable_m2l, enable_l2m, note = raw.split("|", 5)
    matrix = load(root / name / "matrix-summary.json")
    profile = {}
    if isinstance(matrix.get("profiles"), list) and matrix["profiles"]:
        profile = matrix["profiles"][0]
    runs = matrix.get("runs") if isinstance(matrix.get("runs"), list) else []
    decrypt_failures = int(profile.get("decrypt_failures") or sum(int(run.get("decrypt_failures") or 0) for run in runs))
    decrypt_failures_total = int(profile.get("decrypt_failures_total") or sum(int(run.get("decrypt_failures_total") or run.get("decrypt_failures") or 0) for run in runs))
    pre_session_decrypt_failures = int(profile.get("pre_session_decrypt_failures") or sum(int(run.get("pre_session_decrypt_failures") or 0) for run in runs))
    post_session_decrypt_failures = int(profile.get("post_session_decrypt_failures") or sum(int(run.get("post_session_decrypt_failures") or run.get("decrypt_failures") or 0) for run in runs))
    tx_failures = int(profile.get("tx_failed_submissions") or 0) + int(profile.get("tx_dropped_datagrams") or 0)
    result = matrix.get("result", "missing")
    profile_status = profile.get("status", "missing")
    is_control = tx_power == "current-default" and calibration == "current-default"
    if dry_run:
        disposition = "dry-run"
    elif result == "pass" and profile_status == "accepted" and decrypt_failures == 0 and tx_failures == 0:
        disposition = "production-control-pass" if is_control else "experimental-pass-needs-soak"
    else:
        disposition = "production-control-failed" if is_control else "quarantined"
    variants.append({
        "name": name,
        "tx_power_mode": tx_power,
        "tx_calibration_profile": calibration,
        "enable_m2l": enable_m2l,
        "enable_l2m": enable_l2m,
        "note": note,
        "matrix_result": result,
        "profile_status": profile_status,
        "runs": int(profile.get("runs") or len(runs)),
        "pass_count": int(profile.get("pass_count") or 0),
        "accepted_count": int(profile.get("accepted_count") or 0),
        "decrypt_failures": decrypt_failures,
        "decrypt_failures_total": decrypt_failures_total,
        "pre_session_decrypt_failures": pre_session_decrypt_failures,
        "post_session_decrypt_failures": post_session_decrypt_failures,
        "tx_failures": tx_failures,
        "avg_m2l_recovery": profile.get("avg_m2l_recovery"),
        "avg_l2m_recovery": profile.get("avg_l2m_recovery"),
        "worst_m2l_recovery": profile.get("worst_m2l_recovery"),
        "worst_l2m_recovery": profile.get("worst_l2m_recovery"),
        "disposition": disposition,
        "matrix_dir": str(root / name),
    })

summary = {
    "result": "dry-run" if dry_run else (
        "fail" if any(v["disposition"] == "production-control-failed" for v in variants) else "pass"
    ),
    "variant_count": len(variants),
    "variant_set": variant_set,
    "variants": variants,
}
(root / "calibration-regression-summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

def pct(value):
    if value is None:
        return "n/a"
    return f"{float(value):.1%}"

lines = [
    "# Calibration Regression Matrix",
    "",
    f"Result: `{summary['result']}`",
    "",
    "| Variant | Power | Calibration | Direction | Disposition | Runs | Accepted | Avg M2L | Avg L2M | Post-Session Decrypt | Pre-Session Decrypt |",
    "|---|---|---|---|---|---:|---:|---:|---:|---:|---:|",
]
for variant in variants:
    direction = "duplex"
    if variant["enable_m2l"] in {"1", "true", "yes"} and variant["enable_l2m"] in {"0", "false", "no"}:
        direction = "m2l-only"
    elif variant["enable_m2l"] in {"0", "false", "no"} and variant["enable_l2m"] in {"1", "true", "yes"}:
        direction = "l2m-only"
    lines.append(
        "| `{name}` | `{power}` | `{cal}` | `{direction}` | {disp} | {runs} | {accepted} | {m2l} | {l2m} | {post_decrypt} | {pre_decrypt} |".format(
            name=variant["name"],
            power=variant["tx_power_mode"],
            cal=variant["tx_calibration_profile"],
            direction=direction,
            disp=variant["disposition"],
            runs=variant["runs"],
            accepted=variant["accepted_count"],
            m2l=pct(variant["avg_m2l_recovery"]),
            l2m=pct(variant["avg_l2m_recovery"]),
            post_decrypt=variant["post_session_decrypt_failures"],
            pre_decrypt=variant["pre_session_decrypt_failures"],
        )
    )
lines.extend(["", "## Notes", ""])
for variant in variants:
    lines.append(f"- `{variant['name']}`: {variant['note']} Artifacts: `{variant['matrix_dir']}`.")
(root / "calibration-regression-summary.md").write_text("\n".join(lines) + "\n")
print(json.dumps(summary, indent=2, sort_keys=True))
PY
}

while IFS='|' read -r name tx_power_mode tx_calibration_profile enable_m2l enable_l2m note; do
  [[ -z "${name:-}" || "${name:0:1}" == "#" ]] && continue
  run_variant "$name" "$tx_power_mode" "$tx_calibration_profile" "$enable_m2l" "$enable_l2m" "$note"
done < "$VARIANT_FILE_PATH"

write_summary
log "done: $CALIBRATION_OUT_DIR"
