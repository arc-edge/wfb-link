#!/usr/bin/env bash
set -euo pipefail

OUT_DIR=${OUT_DIR:-/tmp/wfb-rf-baseline}
IFACE=${IFACE:-wfb0}
CHANNEL=${CHANNEL:-36}
BANDWIDTH_MHZ=${BANDWIDTH_MHZ:-20}
LINK_ID=${LINK_ID:-0x000001}
RADIO_PORT=${RADIO_PORT:-0x00}
FEC_K=${FEC_K:-8}
FEC_N=${FEC_N:-12}
PAYLOAD_LEN=${PAYLOAD_LEN:-1000}
SOURCE_PAYLOADS=${SOURCE_PAYLOADS:-}
RECOVERED_PAYLOADS=${RECOVERED_PAYLOADS:-}
SUBMITTED_DATAGRAMS=${SUBMITTED_DATAGRAMS:-}
THROUGHPUT_MBPS=${THROUGHPUT_MBPS:-}
TX_RATE=${TX_RATE:-mcs1}
TX_PROFILE=${TX_PROFILE:-linux-monitor}
WFB_KEY=${WFB_KEY:-/var/lib/arc/wfb/drone.key}
WFB_TX_CMD=${WFB_TX_CMD:-}
WFB_RX_CMD=${WFB_RX_CMD:-}
TCPDUMP_SECONDS=${TCPDUMP_SECONDS:-0}
TCPDUMP_IFACE=${TCPDUMP_IFACE:-$IFACE}

mkdir -p "$OUT_DIR"

capture_cmd() {
  local name=$1
  shift
  if command -v "$1" >/dev/null 2>&1; then
    "$@" >"$OUT_DIR/$name.txt" 2>&1 || true
  else
    printf '%s not found\n' "$1" >"$OUT_DIR/$name.txt"
  fi
}

capture_cmd uname uname -a
capture_cmd ip-link ip -br link
capture_cmd ip-addr ip addr show "$IFACE"
capture_cmd iw-dev iw dev
capture_cmd lsusb lsusb
capture_cmd processes ps -eo pid,user,comm,args

if command -v docker >/dev/null 2>&1; then
  docker ps --format '{{.Names}} {{.Status}} {{.Image}}' >"$OUT_DIR/docker-ps.txt" 2>&1 || true
else
  printf 'docker not found\n' >"$OUT_DIR/docker-ps.txt"
fi

if [[ "$TCPDUMP_SECONDS" != "0" ]]; then
  if command -v tcpdump >/dev/null 2>&1; then
    timeout "$TCPDUMP_SECONDS" tcpdump -i "$TCPDUMP_IFACE" -s 256 -w "$OUT_DIR/receiver.pcap" >"$OUT_DIR/tcpdump.log" 2>&1 || true
  else
    printf 'tcpdump not found\n' >"$OUT_DIR/tcpdump.log"
  fi
fi

printf '%s\n' "$WFB_TX_CMD" >"$OUT_DIR/wfb-tx-command.txt"
printf '%s\n' "$WFB_RX_CMD" >"$OUT_DIR/wfb-rx-command.txt"

python3 - "$OUT_DIR" <<'PY'
import json
import os
import sys
from pathlib import Path

out_dir = Path(sys.argv[1])

def getenv(name, default=None):
    return os.environ.get(name, default)

def maybe_int(name):
    value = getenv(name, "")
    if value == "":
        return None
    return int(value, 0)

def maybe_float(name):
    value = getenv(name, "")
    if value == "":
        return None
    return float(value)

artifacts = []
for path in sorted(out_dir.iterdir()):
    if path.name == "linux-baseline.json":
        continue
    artifacts.append(str(path))

report = {
    "command": "linux-wfb-baseline",
    "profile": {
        "name": getenv("PROFILE_NAME", "linux-close-range-20mhz"),
        "channel": maybe_int("CHANNEL"),
        "bandwidth": f"{maybe_int('BANDWIDTH_MHZ')}MHz",
        "tx_rate": getenv("TX_RATE", "mcs1"),
        "tx_descriptor_profile": getenv("TX_PROFILE", "linux-monitor"),
        "wfb": {
            "link_id": getenv("LINK_ID", "0x000001"),
            "radio_port": getenv("RADIO_PORT", "0x00"),
            "fec_k": maybe_int("FEC_K"),
            "fec_n": maybe_int("FEC_N"),
        },
        "payload_len": maybe_int("PAYLOAD_LEN"),
        "source_payloads": maybe_int("SOURCE_PAYLOADS"),
    },
    "commands": {
        "wfb_tx": getenv("WFB_TX_CMD", ""),
        "wfb_rx": getenv("WFB_RX_CMD", ""),
    },
    "adapter": {
        "interface": getenv("IFACE", "wfb0"),
        "key_path": getenv("WFB_KEY", "/var/lib/arc/wfb/drone.key"),
    },
    "metrics": {
        "submitted_datagrams": maybe_int("SUBMITTED_DATAGRAMS"),
        "source_payloads": maybe_int("SOURCE_PAYLOADS"),
        "recovered_payloads": maybe_int("RECOVERED_PAYLOADS"),
        "throughput_mbps": maybe_float("THROUGHPUT_MBPS"),
    },
    "receiver_artifacts": artifacts,
}

(out_dir / "linux-baseline.json").write_text(json.dumps(report, indent=2) + "\n")
print(out_dir / "linux-baseline.json")
PY
