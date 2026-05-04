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
  LINUX_LAN_IP=192.168.122.77
  LINK_ID=0x000001        # report/runtime value
  WFB_CLI_LINK_ID=1       # decimal value for Linux WFB-ng CLI; derived by default
  EXPECTED_PAYLOADS=80 SOURCE_WARMUP_PAYLOADS=20
  M2L_MIN_UNIQUE=80 L2M_MIN_UNIQUE=80
  TX_CALIBRATION_PROFILE=rtl8812a-runtime-iqk
  REQUIRE_CALIBRATION_SUCCESS=auto
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

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
OUT_DIR=${OUT_DIR:-/tmp/wfb-radio-run-duplex-$RUN_ID}
REMOTE_PREFIX=${REMOTE_PREFIX:-/tmp/wfb-radio-run-duplex-$RUN_ID-peer}

LINUX_HOST=${LINUX_HOST:-pi@drone-2f389.local}
MAC_LAN_IP=${MAC_LAN_IP:-192.168.122.84}
LINUX_LAN_IP=${LINUX_LAN_IP:-192.168.122.77}
LINUX_REMOTE_PATH=${LINUX_REMOTE_PATH:-/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin}
IFACE=${IFACE:-wfb0}
WFB_SERVICE=${WFB_SERVICE:-arc-wfb-link-1}
WFB_KEY=${WFB_KEY:-/var/lib/arc/wfb/drone.key}

CHANNEL=${CHANNEL:-36}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
LINK_ID=${LINK_ID:-0x000001}
WFB_CLI_LINK_ID=${WFB_CLI_LINK_ID:-$(printf '%d' "$((LINK_ID))")}
M2L_RADIO_PORT=${M2L_RADIO_PORT:-0}
L2M_RADIO_PORT=${L2M_RADIO_PORT:-1}
FEC_K=${FEC_K:-8}
FEC_N=${FEC_N:-12}
EXPECTED_PAYLOADS=${EXPECTED_PAYLOADS:-80}
M2L_MIN_UNIQUE=${M2L_MIN_UNIQUE:-$EXPECTED_PAYLOADS}
L2M_MIN_UNIQUE=${L2M_MIN_UNIQUE:-$EXPECTED_PAYLOADS}
MIN_RADIO_RX_FORWARDED=${MIN_RADIO_RX_FORWARDED:-1}
REQUIRE_CALIBRATION_SUCCESS=${REQUIRE_CALIBRATION_SUCCESS:-auto}
export M2L_MIN_UNIQUE L2M_MIN_UNIQUE MIN_RADIO_RX_FORWARDED REQUIRE_CALIBRATION_SUCCESS
SOURCE_WARMUP_PAYLOADS=${SOURCE_WARMUP_PAYLOADS:-20}
PAYLOAD_LEN=${PAYLOAD_LEN:-1000}
PAYLOAD_INTERVAL_SEC=${PAYLOAD_INTERVAL_SEC:-0.003}
M2L_MARKER=${M2L_MARKER:-M2LRSMK1}
L2M_MARKER=${L2M_MARKER:-L2MRSMK1}
M2L_WARMUP_MARKER=${M2L_WARMUP_MARKER:-M2LWARM1}
L2M_WARMUP_MARKER=${L2M_WARMUP_MARKER:-L2MWARM1}

FIRMWARE=${FIRMWARE:-/tmp/rtl8812aefw.bin}
EFUSE_REPORT=${EFUSE_REPORT:-/tmp/wfb-remote-macos-efuse-dump.json}
TX_POWER_MODE=${TX_POWER_MODE:-efuse-derived}
TX_POWER_SAFETY_PROFILE=${TX_POWER_SAFETY_PROFILE:-linux-ch36-ht20}
TX_CALIBRATION_PROFILE=${TX_CALIBRATION_PROFILE:-current-default}

RADIO_BIND_PORT=${RADIO_BIND_PORT:-5611}
RADIO_BIND=${RADIO_BIND:-0.0.0.0:$RADIO_BIND_PORT}
LINUX_M2L_SOURCE_PORT=${LINUX_M2L_SOURCE_PORT:-5600}
LINUX_L2M_SOURCE_PORT=${LINUX_L2M_SOURCE_PORT:-5621}
M2L_COUNTER_PORT=${M2L_COUNTER_PORT:-5900}
L2M_AGG_PORT=${L2M_AGG_PORT:-5801}
L2M_COUNTER_PORT=${L2M_COUNTER_PORT:-5911}
RADIO_RUN_DURATION_MS=${RADIO_RUN_DURATION_MS:-55000}
RADIO_READY_WAIT_SECONDS=${RADIO_READY_WAIT_SECONDS:-90}
RX_TIMEOUT_MS=${RX_TIMEOUT_MS:-20}
TX_BURST_LIMIT=${TX_BURST_LIMIT:-4}
COUNTER_SECONDS=${COUNTER_SECONDS:-50}
PEER_WAIT_SECONDS=${PEER_WAIT_SECONDS:-35}

for cmd in cargo python3 ssh scp; do
  require_command "$cmd"
done
[[ -f "$FIRMWARE" ]] || die "firmware not found: $FIRMWARE"
[[ -f "$EFUSE_REPORT" ]] || die "EFUSE report not found: $EFUSE_REPORT"

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO_ROOT"
mkdir -p "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd)

RADIO_PID=
cleanup() {
  set +e
  if [[ -n "${RADIO_PID:-}" ]]; then
    kill "$RADIO_PID" >/dev/null 2>&1 || true
  fi
  ssh "$LINUX_HOST" "REMOTE_PREFIX='$REMOTE_PREFIX' IFACE='$IFACE' WFB_SERVICE='$WFB_SERVICE' bash -s" <<'REMOTE_CLEANUP' >/dev/null 2>&1 || true
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH
sudo -n pkill -f "$REMOTE_PREFIX" || true
sudo -n pkill -f "wfb_rx .*5801|wfb_rx .*5900|wfb_rx .*5911|wfb_tx .*5621|wfb_tx .*5600" || true
sudo -n pkill -f "tcpdump -i $IFACE" || true
sudo -n docker start "$WFB_SERVICE" || true
REMOTE_CLEANUP
}
trap cleanup EXIT INT TERM

prepare_peer() {
  log "preparing Linux peer $LINUX_HOST on channel $CHANNEL"
  ssh "$LINUX_HOST" \
    "REMOTE_PREFIX='$REMOTE_PREFIX' LINUX_REMOTE_PATH='$LINUX_REMOTE_PATH' IFACE='$IFACE' CHANNEL='$CHANNEL' WFB_SERVICE='$WFB_SERVICE' bash -s" <<'REMOTE_PREP'
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
rm -rf "$REMOTE_PREFIX"
mkdir -p "$REMOTE_PREFIX"
sudo -n docker stop "$WFB_SERVICE" >/dev/null 2>&1 || true
sudo -n pkill -f "wfb_rx .*${IFACE}|wfb_tx .*${IFACE}|tcpdump -i ${IFACE}" >/dev/null 2>&1 || true
sudo -n pkill -f "wfb_rx .*5801|wfb_rx .*5900|wfb_rx .*5911|wfb_tx .*5621|wfb_tx .*5600" >/dev/null 2>&1 || true
sudo -n nmcli dev set "$IFACE" managed no >/dev/null 2>&1 || true
sudo -n nmcli dev set "p2p-dev-$IFACE" managed no >/dev/null 2>&1 || true
sudo -n ip link set "$IFACE" down
sudo -n iw dev "$IFACE" set type monitor
sudo -n ip link set "$IFACE" up
sudo -n iw dev "$IFACE" set channel "$CHANNEL" HT20
sudo -n iw dev "$IFACE" info > "$REMOTE_PREFIX/channel-state-before.txt" 2>&1 || true
REMOTE_PREP
}

start_radio() {
  log "starting local radio-run production loop"
  local tx_power_args=()
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

  cargo run -p wfb-radio-diag -- --json \
    --report "$OUT_DIR/radio-run.json" \
    radio-run \
    --macos-usbhost \
    --vid 0x0bda --pid 0x8812 \
    --firmware "$FIRMWARE" \
    --channel "$CHANNEL" --bandwidth "$BANDWIDTH_MHZ" \
    --bind "$RADIO_BIND" \
    --ready-file "$OUT_DIR/radio-ready.json" \
    --duration-ms "$RADIO_RUN_DURATION_MS" \
    --rx-timeout-ms "$RX_TIMEOUT_MS" \
    --tx-burst-limit "$TX_BURST_LIMIT" \
    --max-datagrams 0 \
    "${tx_power_args[@]}" \
    --tx-calibration-profile "$TX_CALIBRATION_PROFILE" \
    "${write_auth_arg[@]}" \
    --wfb-link-id "$LINK_ID" \
    --wfb-radio-port "$L2M_RADIO_PORT" \
    --rx-aggregator "$LINUX_LAN_IP:$L2M_AGG_PORT" \
    --i-understand-this-transmits \
    > "$OUT_DIR/radio-run.log" 2>&1 &
  RADIO_PID=$!
}

wait_for_radio_ready() {
  log "waiting for radio ready marker"
  for _ in $(seq 1 "$RADIO_READY_WAIT_SECONDS"); do
    if [[ -f "$OUT_DIR/radio-ready.json" ]]; then
      cp "$OUT_DIR/radio-ready.json" "$OUT_DIR/radio-ready-observed.json"
      return 0
    fi
    if ! kill -0 "$RADIO_PID" >/dev/null 2>&1; then
      tail -120 "$OUT_DIR/radio-run.log" >&2 || true
      die "radio-run exited before ready"
    fi
    sleep 1
  done
  tail -120 "$OUT_DIR/radio-run.log" >&2 || true
  die "radio ready marker timed out"
}

run_peer_traffic() {
  log "running peer TX/RX traffic"
  ssh "$LINUX_HOST" \
    "REMOTE_PREFIX='$REMOTE_PREFIX' LINUX_REMOTE_PATH='$LINUX_REMOTE_PATH' IFACE='$IFACE' WFB_KEY='$WFB_KEY' WFB_CLI_LINK_ID='$WFB_CLI_LINK_ID' MAC_LAN_IP='$MAC_LAN_IP' RADIO_BIND_PORT='$RADIO_BIND_PORT' M2L_RADIO_PORT='$M2L_RADIO_PORT' L2M_RADIO_PORT='$L2M_RADIO_PORT' FEC_K='$FEC_K' FEC_N='$FEC_N' EXPECTED_PAYLOADS='$EXPECTED_PAYLOADS' SOURCE_WARMUP_PAYLOADS='$SOURCE_WARMUP_PAYLOADS' PAYLOAD_LEN='$PAYLOAD_LEN' PAYLOAD_INTERVAL_SEC='$PAYLOAD_INTERVAL_SEC' M2L_MARKER='$M2L_MARKER' L2M_MARKER='$L2M_MARKER' M2L_WARMUP_MARKER='$M2L_WARMUP_MARKER' L2M_WARMUP_MARKER='$L2M_WARMUP_MARKER' LINUX_M2L_SOURCE_PORT='$LINUX_M2L_SOURCE_PORT' LINUX_L2M_SOURCE_PORT='$LINUX_L2M_SOURCE_PORT' M2L_COUNTER_PORT='$M2L_COUNTER_PORT' L2M_AGG_PORT='$L2M_AGG_PORT' L2M_COUNTER_PORT='$L2M_COUNTER_PORT' COUNTER_SECONDS='$COUNTER_SECONDS' PEER_WAIT_SECONDS='$PEER_WAIT_SECONDS' bash -s" <<'REMOTE_TRAFFIC'
set -euo pipefail
export PATH="$LINUX_REMOTE_PATH:$PATH"
cat > "$REMOTE_PREFIX/counter.py" <<'PY'
import json
import socket
import sys
import time
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
        seqs.add(int.from_bytes(data[idx + len(marker):idx + len(marker) + 4], "big"))
report = {
    "bind": f"{host}:{port}",
    "marker": marker.decode(),
    "expected": expected,
    "packets": packets,
    "bytes": bytes_total,
    "matched_datagrams": matched,
    "unique_sequences": len(seqs),
    "missing_sequences": [i for i in range(expected) if i not in seqs],
    "last_peer": last_peer,
    "duration_sec": time.time() - started,
}
out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY

python3 -u "$REMOTE_PREFIX/counter.py" 127.0.0.1 "$M2L_COUNTER_PORT" "$M2L_MARKER" "$EXPECTED_PAYLOADS" "$REMOTE_PREFIX/counter-m2l.json" "$COUNTER_SECONDS" > "$REMOTE_PREFIX/counter-m2l.log" 2>&1 &
echo $! > "$REMOTE_PREFIX/counter-m2l.pid"
python3 -u "$REMOTE_PREFIX/counter.py" 127.0.0.1 "$L2M_COUNTER_PORT" "$L2M_MARKER" "$EXPECTED_PAYLOADS" "$REMOTE_PREFIX/counter-l2m.json" "$COUNTER_SECONDS" > "$REMOTE_PREFIX/counter-l2m.log" 2>&1 &
echo $! > "$REMOTE_PREFIX/counter-l2m.pid"

sudo -n tcpdump -i "$IFACE" -s 256 -w "$REMOTE_PREFIX/rf.pcap" > "$REMOTE_PREFIX/tcpdump.log" 2>&1 &
echo $! > "$REMOTE_PREFIX/tcpdump.pid"
sudo -n timeout "$COUNTER_SECONDS" wfb_rx -K "$WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$M2L_RADIO_PORT" -c 127.0.0.1 -u "$M2L_COUNTER_PORT" "$IFACE" > "$REMOTE_PREFIX/wfb-rx-m2l.log" 2>&1 &
echo $! > "$REMOTE_PREFIX/wfb-rx-m2l.pid"
sudo -n timeout "$COUNTER_SECONDS" wfb_rx -a "$L2M_AGG_PORT" -K "$WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$L2M_RADIO_PORT" -c 127.0.0.1 -u "$L2M_COUNTER_PORT" > "$REMOTE_PREFIX/wfb-rx-l2m-agg.log" 2>&1 &
echo $! > "$REMOTE_PREFIX/wfb-rx-l2m-agg.pid"
sleep 3
sudo -n timeout "$COUNTER_SECONDS" wfb_tx -d -K "$WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$M2L_RADIO_PORT" -B 20 -k "$FEC_K" -n "$FEC_N" -u "$LINUX_M2L_SOURCE_PORT" "$MAC_LAN_IP:$RADIO_BIND_PORT" > "$REMOTE_PREFIX/wfb-tx-m2l-dist.log" 2>&1 &
echo $! > "$REMOTE_PREFIX/wfb-tx-m2l-dist.pid"
sudo -n timeout "$COUNTER_SECONDS" wfb_tx -K "$WFB_KEY" -i "$WFB_CLI_LINK_ID" -p "$L2M_RADIO_PORT" -B 20 -k "$FEC_K" -n "$FEC_N" -u "$LINUX_L2M_SOURCE_PORT" "$IFACE" > "$REMOTE_PREFIX/wfb-tx-l2m-rf.log" 2>&1 &
echo $! > "$REMOTE_PREFIX/wfb-tx-l2m-rf.pid"
sleep 2

python3 - <<'PY'
import os
import socket
import time

payload_len = int(os.environ["PAYLOAD_LEN"])
warmup = int(os.environ["SOURCE_WARMUP_PAYLOADS"])
expected = int(os.environ["EXPECTED_PAYLOADS"])
interval = float(os.environ["PAYLOAD_INTERVAL_SEC"])
source_m2l = ("127.0.0.1", int(os.environ["LINUX_M2L_SOURCE_PORT"]))
source_l2m = ("127.0.0.1", int(os.environ["LINUX_L2M_SOURCE_PORT"]))
sock_m2l = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock_l2m = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

def payload(marker, seq, fill):
    marker = marker.encode()
    return marker + seq.to_bytes(4, "big") + fill * (payload_len - len(marker) - 4)

for seq in range(warmup):
    sock_m2l.sendto(payload(os.environ["M2L_WARMUP_MARKER"], seq, b"w"), source_m2l)
    sock_l2m.sendto(payload(os.environ["L2M_WARMUP_MARKER"], seq, b"w"), source_l2m)
    time.sleep(interval)
for seq in range(expected):
    sock_m2l.sendto(payload(os.environ["M2L_MARKER"], seq, b"m"), source_m2l)
    sock_l2m.sendto(payload(os.environ["L2M_MARKER"], seq, b"l"), source_l2m)
    time.sleep(interval)
PY
printf 'sent warmup=%s marked=%s per direction link_cli=%s\n' "$SOURCE_WARMUP_PAYLOADS" "$EXPECTED_PAYLOADS" "$WFB_CLI_LINK_ID" > "$REMOTE_PREFIX/sources-done.txt"

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
  scp -r "$LINUX_HOST:$REMOTE_PREFIX" "$OUT_DIR/peer" >/dev/null
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

report = load(run / "radio-run.json")
m2l = load(run / "peer" / "counter-m2l.json")
l2m = load(run / "peer" / "counter-l2m.json")
rx = report.get("rx") or {}
tx = report.get("tx") or {}
calibration = report.get("tx_calibration_profile") or {}
runtime_iqk = calibration.get("runtime_iqk") or {}
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
require_calibration_success = os.environ["REQUIRE_CALIBRATION_SUCCESS"]
calibration_success_required = require_calibration_success in {"1", "true", "yes"}
if require_calibration_success == "auto":
    calibration_success_required = report.get("calibration_profile") == "rtl8812a_runtime_iqk"
failures = []
if report.get("result") != "pass":
    failures.append(f"radio_result={report.get('result')}")
if (tx.get("failed_submissions") or 0) != 0:
    failures.append(f"tx_failed_submissions={tx.get('failed_submissions')}")
if (tx.get("dropped_datagrams") or 0) != 0:
    failures.append(f"tx_dropped_datagrams={tx.get('dropped_datagrams')}")
if m2l_unique < m2l_min_unique:
    failures.append(f"m2l_unique_sequences={m2l_unique}<{m2l_min_unique}")
if l2m_unique < l2m_min_unique:
    failures.append(f"l2m_unique_sequences={l2m_unique}<{l2m_min_unique}")
if radio_rx_forwarded < min_radio_rx_forwarded:
    failures.append(f"radio_rx_forwarded={radio_rx_forwarded}<{min_radio_rx_forwarded}")
if calibration_success_required and runtime_iqk.get("status") != "success":
    failures.append(f"runtime_iqk_status={runtime_iqk.get('status')}")
summary = {
    "smoke_result": "fail" if failures else "pass",
    "failures": failures,
    "radio_result": report.get("result"),
    "stop_reason": report.get("stop_reason"),
    "tx": tx,
    "rx": rx,
    "radio_rx_forwarded_from_snapshots": radio_rx_forwarded,
    "radio_rx_forwards": rx_forwards,
    "calibration": {
        "profile": report.get("calibration_profile"),
        "class": report.get("calibration_class"),
        "evidence_source": report.get("calibration_evidence_source"),
        "receiver_backed_validation_required": report.get("receiver_backed_validation_required"),
        "runtime_iqk_status": runtime_iqk.get("status"),
        "runtime_iqk_cleanup_status": runtime_iqk.get("cleanup_status"),
        "runtime_iqk_fallback_stage_count": runtime_iqk.get("fallback_stage_count"),
        "calibration_success_required": calibration_success_required,
    },
    "m2l_min_unique": m2l_min_unique,
    "l2m_min_unique": l2m_min_unique,
    "m2l_counter": m2l,
    "l2m_counter": l2m,
}
(run / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(json.dumps(summary, indent=2, sort_keys=True))
if failures:
    sys.exit(1)
PY
}

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

log "output directory: $OUT_DIR"
prepare_peer
start_radio
wait_for_radio_ready
run_peer_traffic
wait "$RADIO_PID"
RADIO_PID=
collect_peer_artifacts
write_summary
trap - EXIT INT TERM
cleanup
log "done: $OUT_DIR"
