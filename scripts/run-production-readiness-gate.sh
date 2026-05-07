#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

RUN_LOCAL=${RUN_LOCAL:-1}
RUN_API_TUNNEL_SMOKE=${RUN_API_TUNNEL_SMOKE:-0}
RUN_LOADED_TUNNEL_GATE=${RUN_LOADED_TUNNEL_GATE:-0}
RUN_VIDEO_CONTROL_RADIO_GATE=${RUN_VIDEO_CONTROL_RADIO_GATE:-0}
RUN_RF_CLOSE_RANGE=${RUN_RF_CLOSE_RANGE:-0}
RUN_CALIBRATION_REGRESSION=${RUN_CALIBRATION_REGRESSION:-0}

log() {
  printf '[prod-gate] %s\n' "$*" >&2
}

if [[ "$RUN_LOCAL" == "1" ]]; then
  log "local Rust checks"
  cargo fmt --check
  cargo check --workspace
  cargo test -p wfb-link -p wfb-tun
  bash -n scripts/run-wfb-link-tunnel-smoke.sh
  bash -n scripts/run-mac-wf-tun-profile-matrix.sh
  bash -n scripts/run-mac-wf-tun-recovery.sh
  bash -n scripts/run-radio-run-profile-matrix.sh
fi

if [[ "$RUN_API_TUNNEL_SMOKE" == "1" ]]; then
  log "product API tunnel smoke"
  scripts/run-wfb-link-tunnel-smoke.sh
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
