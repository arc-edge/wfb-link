#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

RUN_LOCAL=${RUN_LOCAL:-1}
RUN_API_RADIO_SMOKE=${RUN_API_RADIO_SMOKE:-0}
RUN_API_TUNNEL_SMOKE=${RUN_API_TUNNEL_SMOKE:-0}
RUN_MANAGED_STREAMS_SMOKE=${RUN_MANAGED_STREAMS_SMOKE:-0}
RUN_LOADED_TUNNEL_GATE=${RUN_LOADED_TUNNEL_GATE:-0}
RUN_VIDEO_CONTROL_RADIO_GATE=${RUN_VIDEO_CONTROL_RADIO_GATE:-0}
RUN_RF_CLOSE_RANGE=${RUN_RF_CLOSE_RANGE:-0}
RUN_CALIBRATION_REGRESSION=${RUN_CALIBRATION_REGRESSION:-0}
MANAGED_STREAMS_SMOKE_REPEATS=${MANAGED_STREAMS_SMOKE_REPEATS:-1}

log() {
  printf '[prod-gate] %s\n' "$*" >&2
}

require_positive_integer() {
  local name=$1 value=$2
  if [[ ! "$value" =~ ^[1-9][0-9]*$ ]]; then
    printf '[prod-gate] error: %s must be a positive integer, got %q\n' "$name" "$value" >&2
    exit 1
  fi
}

run_managed_streams_smoke_repeats() {
  require_positive_integer MANAGED_STREAMS_SMOKE_REPEATS "$MANAGED_STREAMS_SMOKE_REPEATS"
  local repeat out_base run_id_base
  out_base=${MANAGED_STREAMS_SMOKE_OUT_BASE:-${OUT_DIR:-}}
  run_id_base=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
  for ((repeat = 1; repeat <= MANAGED_STREAMS_SMOKE_REPEATS; repeat++)); do
    log "managed raw application streams smoke repeat ${repeat}/${MANAGED_STREAMS_SMOKE_REPEATS}"
    if [[ -n "$out_base" ]]; then
      OUT_DIR="$out_base/repeat-$repeat" scripts/run-wfb-link-managed-streams-smoke.sh
    else
      RUN_ID="${run_id_base}-managed-r${repeat}-of-${MANAGED_STREAMS_SMOKE_REPEATS}" \
        scripts/run-wfb-link-managed-streams-smoke.sh
    fi
  done
}

if [[ "$RUN_LOCAL" == "1" ]]; then
  log "local Rust checks"
  cargo fmt --all -- --check
  cargo check --workspace --locked
  cargo test -p wfb-link -p wfb-tun --locked
  bash -n scripts/run-wfb-link-radio-smoke.sh
  bash -n scripts/run-wfb-link-tunnel-smoke.sh
  bash -n scripts/run-wfb-link-managed-streams-smoke.sh
  bash -n scripts/run-mac-wf-tun-profile-matrix.sh
  bash -n scripts/run-mac-wf-tun-recovery.sh
  bash -n scripts/run-radio-run-profile-matrix.sh
fi

if [[ "$RUN_API_RADIO_SMOKE" == "1" ]]; then
  log "product API radio smoke"
  scripts/run-wfb-link-radio-smoke.sh
fi

if [[ "$RUN_API_TUNNEL_SMOKE" == "1" ]]; then
  log "product API tunnel smoke"
  scripts/run-wfb-link-tunnel-smoke.sh
fi

if [[ "$RUN_MANAGED_STREAMS_SMOKE" == "1" ]]; then
  run_managed_streams_smoke_repeats
fi

if [[ "$RUN_LOADED_TUNNEL_GATE" == "1" ]]; then
  log "loaded tunnel side-load gate"
  PROFILE_SET=${PROFILE_SET:-loaded} scripts/run-mac-wf-tun-profile-matrix.sh
fi

if [[ "$RUN_VIDEO_CONTROL_RADIO_GATE" == "1" ]]; then
  log "video/control TDD radio gate"
  PROFILE_SET=${PROFILE_SET:-video-control-tdd} scripts/run-radio-run-profile-matrix.sh
fi

if [[ "$RUN_RF_CLOSE_RANGE" == "1" ]]; then
  log "receiver-backed close-range RF quality gate"
  scripts/run-rf-quality-close-range.sh
fi

if [[ "$RUN_CALIBRATION_REGRESSION" == "1" ]]; then
  log "calibration regression matrix"
  scripts/run-calibration-regression-matrix.sh
fi

log "complete"
