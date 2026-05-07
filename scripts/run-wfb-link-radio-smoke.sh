#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
OUT_DIR=${OUT_DIR:-/tmp/wfb-link-radio-smoke-$RUN_ID}
RADIO_CONFIG=${RADIO_CONFIG:-$REPO_ROOT/configs/radio-run-video-control-tdd.toml}
WFB_LINK_READY_TIMEOUT_S=${WFB_LINK_READY_TIMEOUT_S:-90}
WFB_LINK_HOLD_SECONDS=${WFB_LINK_HOLD_SECONDS:-4}
WFB_LINK_TX_DATAGRAMS=${WFB_LINK_TX_DATAGRAMS:-12}
WFB_LINK_TX_INTERVAL_US=${WFB_LINK_TX_INTERVAL_US:-1000}
WFB_LINK_TX_LINK_ID=${WFB_LINK_TX_LINK_ID:-1}
WFB_LINK_TX_RADIO_PORT=${WFB_LINK_TX_RADIO_PORT:-1}
WFB_LINK_TX_MCS=${WFB_LINK_TX_MCS:-2}
WFB_LINK_TX_BANDWIDTH_MHZ=${WFB_LINK_TX_BANDWIDTH_MHZ:-20}
WFB_LINK_TX_PAYLOAD_LEN=${WFB_LINK_TX_PAYLOAD_LEN:-256}

die() {
  printf '[wfb-link-radio-smoke] error: %s\n' "$*" >&2
  exit 1
}

require_file() {
  local path=$1 label=$2
  [[ -e "$path" ]] || die "missing $label: $path"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

require_file "$RADIO_CONFIG" "radio config"
require_command cargo
require_command python3

mkdir -p "$OUT_DIR"
STDOUT_LOG="$OUT_DIR/wfb-link-radio.stdout.log"
STDERR_LOG="$OUT_DIR/wfb-link-radio.stderr.log"
SUMMARY_JSON="$OUT_DIR/summary.json"

printf '[wfb-link-radio-smoke] artifacts: %s\n' "$OUT_DIR" >&2
WFB_LINK_READY_TIMEOUT_S="$WFB_LINK_READY_TIMEOUT_S" \
WFB_LINK_HOLD_SECONDS="$WFB_LINK_HOLD_SECONDS" \
WFB_LINK_TX_DATAGRAMS="$WFB_LINK_TX_DATAGRAMS" \
WFB_LINK_TX_INTERVAL_US="$WFB_LINK_TX_INTERVAL_US" \
WFB_LINK_TX_LINK_ID="$WFB_LINK_TX_LINK_ID" \
WFB_LINK_TX_RADIO_PORT="$WFB_LINK_TX_RADIO_PORT" \
WFB_LINK_TX_MCS="$WFB_LINK_TX_MCS" \
WFB_LINK_TX_BANDWIDTH_MHZ="$WFB_LINK_TX_BANDWIDTH_MHZ" \
WFB_LINK_TX_PAYLOAD_LEN="$WFB_LINK_TX_PAYLOAD_LEN" \
cargo run -p wfb-link --example embed-radio-service -- "$RADIO_CONFIG" \
  >"$STDOUT_LOG" \
  2>"$STDERR_LOG"

STDOUT_LOG="$STDOUT_LOG" \
SUMMARY_JSON="$SUMMARY_JSON" \
EXPECTED_TX_DATAGRAMS="$WFB_LINK_TX_DATAGRAMS" \
EXPECTED_TDD_RX_WINDOW_MS=2200 \
EXPECTED_TDD_TX_WINDOW_MS=3600 \
EXPECTED_TDD_GUARD_MS=200 \
python3 - <<'PY'
import json
import os
import sys
from pathlib import Path

stdout_path = Path(os.environ["STDOUT_LOG"])
summary_path = Path(os.environ["SUMMARY_JSON"])
expected_tx = int(os.environ["EXPECTED_TX_DATAGRAMS"], 0)

data = stdout_path.read_text(encoding="utf-8")
decoder = json.JSONDecoder()
objects = []
pos = 0
while True:
    while pos < len(data) and data[pos].isspace():
        pos += 1
    if pos >= len(data):
        break
    value, pos = decoder.raw_decode(data, pos)
    objects.append(value)

failures = []
if len(objects) != 3:
    failures.append(f"expected 3 JSON objects on stdout, found {len(objects)}")
    ready, health, report = ({}, {}, {})
else:
    ready, health, report = objects

backend = report.get("backend", {})
runtime_report = {}
if isinstance(backend, dict):
    runtime_report = backend.get("macos_userspace_radio") or backend.get("MacosUserspaceRadio") or {}
airtime = runtime_report.get("airtime", {})
schedule = airtime.get("schedule", {})
tx = runtime_report.get("tx", {})
rx = runtime_report.get("rx", {})

if not ready.get("ready_file"):
    failures.append("ready_file missing")
if health.get("ready") is not True:
    failures.append(f"health.ready={health.get('ready')!r}")
if report.get("lifecycle") != "stopped":
    failures.append(f"lifecycle={report.get('lifecycle')!r}")
if runtime_report.get("result") != "pass":
    failures.append(f"runtime_result={runtime_report.get('result')!r}")
if runtime_report.get("stop_reason") != "signal":
    failures.append(f"stop_reason={runtime_report.get('stop_reason')!r}")

expected_schedule = {
    "mode": "tdd",
    "tdd_first_window": "rx",
    "tdd_rx_window_ms": int(os.environ["EXPECTED_TDD_RX_WINDOW_MS"], 0),
    "tdd_tx_window_ms": int(os.environ["EXPECTED_TDD_TX_WINDOW_MS"], 0),
    "tdd_guard_ms": int(os.environ["EXPECTED_TDD_GUARD_MS"], 0),
}
for key, expected in expected_schedule.items():
    if schedule.get(key) != expected:
        failures.append(f"airtime.schedule.{key}={schedule.get(key)!r}, expected {expected!r}")
if int(airtime.get("tx_gated_iterations") or 0) <= 0:
    failures.append("airtime.tx_gated_iterations did not advance")
if int(airtime.get("tx_allowed_iterations") or 0) <= 0:
    failures.append("airtime.tx_allowed_iterations did not advance")

if expected_tx > 0:
    datagrams = int(tx.get("datagrams_received") or 0)
    submitted = int(tx.get("submitted_frames") or 0)
    if datagrams < expected_tx:
        failures.append(f"tx.datagrams_received={datagrams}, expected >= {expected_tx}")
    if submitted < expected_tx:
        failures.append(f"tx.submitted_frames={submitted}, expected >= {expected_tx}")
if int(tx.get("failed_submissions") or 0) != 0:
    failures.append(f"tx.failed_submissions={tx.get('failed_submissions')}")
if int(tx.get("dropped_datagrams") or 0) != 0:
    failures.append(f"tx.dropped_datagrams={tx.get('dropped_datagrams')}")
if int(tx.get("ingress_queue_send_failed") or 0) != 0:
    failures.append(f"tx.ingress_queue_send_failed={tx.get('ingress_queue_send_failed')}")

summary = {
    "result": "pass" if not failures else "fail",
    "failures": failures,
    "ready": ready,
    "health": {
        "lifecycle": health.get("lifecycle"),
        "ready": health.get("ready"),
        "tx": health.get("tx"),
        "rx": health.get("rx"),
    },
    "report": {
        "lifecycle": report.get("lifecycle"),
        "runtime_result": runtime_report.get("result"),
        "stop_reason": runtime_report.get("stop_reason"),
        "airtime": airtime,
        "tx": tx,
        "rx": rx,
    },
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
if failures:
    for failure in failures:
        print(f"[wfb-link-radio-smoke] {failure}", file=sys.stderr)
    sys.exit(1)
PY

printf '[wfb-link-radio-smoke] complete: %s\n' "$OUT_DIR" >&2
