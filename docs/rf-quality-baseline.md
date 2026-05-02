# RF Quality Baseline

This procedure captures the Linux-side evidence needed before calling a macOS run range-ready. The baseline is receiver-backed: the important output is not USB submission, but recovered WFB payloads, receiver artifacts, channel parameters, and command lines that can be compared to a macOS RF-quality report.

## Fixed 20 MHz Profile

Use this profile first for close-range sanity and long-distance comparison:

- Adapter class: RTL8812AU/AWUS036ACH-class radio on the Linux peer.
- Channel: 36, 20 MHz.
- TX rate/profile: WFB HT MCS1 traffic using the Linux monitor-injection descriptor shape.
- Key: `/var/lib/arc/wfb/drone.key` on the current bench.
- Primary video-like port: WFB radio port `0`, FEC `k=8`, `n=12`, UDP source `5600`.
- Telemetry-like lower-rate ports on the current bench: ports `1`, `3`, and RX ports `2`, `4`.
- Payload size for the accepted close-range reference: exactly 1,000 byte source payloads, 2,000 expected decoded source payloads.

Current Linux service commands observed on `drone-2f389.local`:

```sh
wfb_tx -K /var/lib/arc/wfb/drone.key -p 0 -B 20 -k 8 -n 12 -u 5600 wfb0
wfb_tx -K /var/lib/arc/wfb/drone.key -p 1 -B 20 -k 1 -n 3 -u 14660 wfb0
wfb_rx -K /var/lib/arc/wfb/drone.key -p 2 -c 127.0.0.1 -u 14661 wfb0
wfb_tx -K /var/lib/arc/wfb/drone.key -p 3 -B 20 -k 2 -n 4 -u 5601 wfb0
wfb_rx -K /var/lib/arc/wfb/drone.key -p 4 -c 127.0.0.1 -u 5701 wfb0
```

For a controlled single-port baseline, stop or isolate unrelated WFB services, pin both radios to the same channel/bandwidth, and run only the port under test. Record the exact commands in the baseline JSON.

## Capture Helper

`scripts/capture-linux-baseline.sh` collects the portable part of a Linux baseline:

```sh
OUT_DIR=/tmp/wfb-rf-baseline-$(date +%Y%m%d-%H%M%S) \
IFACE=wfb0 \
CHANNEL=36 \
BANDWIDTH_MHZ=20 \
LINK_ID=0x000001 \
RADIO_PORT=0x00 \
FEC_K=8 \
FEC_N=12 \
PAYLOAD_LEN=1000 \
SOURCE_PAYLOADS=2000 \
RECOVERED_PAYLOADS=1999 \
SUBMITTED_DATAGRAMS=3000 \
THROUGHPUT_MBPS=0.787644 \
TX_RATE=mcs1 \
TX_PROFILE=linux-monitor \
WFB_TX_CMD='wfb_tx -K /var/lib/arc/wfb/drone.key -p 0 -B 20 -k 8 -n 12 -u 5600 wfb0' \
WFB_RX_CMD='wfb_rx -K /var/lib/arc/wfb/drone.key -p 0 -c 127.0.0.1 -u 5800 wfb0' \
./scripts/capture-linux-baseline.sh
```

The helper writes `linux-baseline.json` plus command, process, link, and optional packet-capture artifacts. It does not assume `iw`, `tcpdump`, `lsusb`, or `docker` exist; missing tools are recorded as missing artifacts rather than failing the run.

## Current Close-Range Baseline

The current close-range 20 MHz reference is the sustained 1,000-byte run from May 1, 2026:

- macOS bridge artifact: `/tmp/wfb-agent-sust1000-listen.json`
- RF capture: `/tmp/mac-sust1000-rf.pcap`
- receiver-side capture: `/tmp/mac-sust1000-rx-lo.pcap`
- Linux TX log: `/tmp/wfb-tx-sust1000.log` on `drone-2f389.local`
- Submitted distributor datagrams: 3,000
- Recovered decoded source payloads: 1,999 of 2,000
- macOS bridge datagram throughput: about `0.787644` Mbps

The repo fixture `fixtures/rf-quality/current-close-range-20mhz-linux-baseline.json` stores these artifact paths and parameters in the format accepted by `rf-quality-report`.

EFUSE-derived TXAGC testing is opt-in. For channel 36 HT20 comparisons against
the current Linux baseline, run Mac TX commands with:

```bash
--tx-power-mode efuse-derived \
--tx-power-efuse-report /tmp/wfb-live-efuse-dump.json \
--tx-power-safety-profile linux-ch36-ht20
```

The mode computes per-path/per-rate TXAGC values from the decoded EFUSE
TX-power region, applies the Linux channel-36 HT20 safety caps, writes the
TXAGC registers before TX, and records the full lane-by-lane calculation in the
Mac report. See `docs/rtl8812au-efuse-tx-power.md`.

Build an RF-quality envelope from the current artifacts:

```sh
cargo run -p wfb-radio-diag -- --json \
  --report /tmp/wfb-rf-quality-close-range-20mhz.json \
  rf-quality-report \
  --profile-name current-close-range-20mhz \
  --profile-kind close-range \
  --channel 36 --bandwidth 20 \
  --tx-rate mcs1 \
  --tx-profile linux-monitor \
  --tx-power-mode current-default \
  --calibration-mode stop-gap-captured \
  --wfb-link-id 0x000001 \
  --wfb-radio-port 0x00 \
  --fec-k 8 --fec-n 12 \
  --payload-len 1000 \
  --expected-payloads 2000 \
  --recovered-payloads 1999 \
  --mac-report /tmp/wfb-agent-sust1000-listen.json \
  --linux-baseline fixtures/rf-quality/current-close-range-20mhz-linux-baseline.json \
  --receiver-artifact /tmp/mac-sust1000-rf.pcap \
  --receiver-artifact /tmp/mac-sust1000-rx-lo.pcap
```

The comparison is valid only when channel, bandwidth, TX rate/profile, WFB link/radio port, FEC settings, and payload length match. If any of those fields differ, the report still emits successfully but marks the comparison invalid or degraded.
