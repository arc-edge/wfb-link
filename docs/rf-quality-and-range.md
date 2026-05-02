# RF Quality And Range Runbook

This is the operating runbook for deciding whether the macOS RTL8812AU WFB path
is ready for range work. Use it with `docs/rf-quality-baseline.md`,
`docs/rf-quality-range-profiles.md`, and
`docs/rtl8812au-calibration-state.md`.

## Baseline Setup

Use Linux WFB-ng on `drone-2f389.local` as the RF baseline. The comparison tuple
must match the macOS run:

- Adapter class: RTL8812AU/AWUS036ACH-class radio.
- Channel and bandwidth: start with channel 36, 20 MHz.
- TX rate/profile: fixed HT MCS1, Linux monitor-compatible descriptor shape.
- WFB tuple: link ID `0x000001`, radio port `0x00`, FEC `k=8,n=12`, 1,000
  byte source payloads.
- Key: `/var/lib/arc/wfb/drone.key`.
- Antenna setup, placement, and channel state: unchanged between Linux and Mac
  comparison runs.

Before controlled tests, stop or isolate normal WFB services and restore them
afterward:

```sh
sudo -n docker stop arc-wfb-link-1
sudo -n iw dev wfb0 set channel 36 HT20
```

Restore:

```sh
sudo -n docker start arc-wfb-link-1
sudo -n docker ps --filter name=arc-wfb-link-1 --format '{{.Names}} {{.Status}}'
```

## Mac Close-Range Command

The accepted close-range profile uses EFUSE-derived TX power and the current
stop-gap captured calibration label:

```sh
cargo run -p wfb-radio-diag -- --json \
  --report /tmp/wfb-rfq-close-efuse-listen.json \
  bridge-tx-listen \
  --macos-usbhost \
  --vid 0x0bda --pid 0x8812 \
  --init-before-tx \
  --firmware /tmp/rtl8812aefw.bin \
  --channel 36 --bandwidth 20 \
  --bind 127.0.0.1:5611 \
  --max-datagrams 3000 \
  --idle-timeout-ms 60000 \
  --tx-power-mode efuse-derived \
  --tx-power-efuse-report /tmp/wfb-remote-macos-efuse-dump.json \
  --tx-power-safety-profile linux-ch36-ht20 \
  --i-understand-this-transmits
```

If the Linux peer cannot send UDP directly into the Mac listener, relay hardware
Mac LAN traffic to localhost and point Linux `wfb_tx -d` at the relay:

```sh
python3 - <<'PY'
import socket
src = ("10.42.0.162", 5610)
dst = ("127.0.0.1", 5611)
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(src)
out = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
while True:
    data, _ = sock.recvfrom(4096)
    out.sendto(data, dst)
PY
```

## Linux Sender And Receiver

Use root for WFB commands when the key is root-readable:

```sh
sudo -n timeout 80 \
  wfb_rx -K /var/lib/arc/wfb/drone.key -p 0 -c 127.0.0.1 -u 5800 wfb0

sudo -n timeout 70 \
  wfb_tx -d -K /var/lib/arc/wfb/drone.key -p 0 -B 20 -k 8 -n 12 \
  -u 5600 10.42.0.162:5610
```

Capture RF evidence while the run is active:

```sh
sudo -n timeout 85 tcpdump -i wfb0 -w /tmp/rfq-close-efuse-rf.pcap
```

Generate 2,000 source payloads of exactly 1,000 bytes into Linux `wfb_tx`:

```sh
python3 - <<'PY'
import socket, time
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
for i in range(2000):
    payload = b'RFQCLSEF' + i.to_bytes(4, 'big') + b'A' * 988
    sock.sendto(payload, ('127.0.0.1', 5600))
    time.sleep(0.0005)
PY
```

## RF-Quality Envelope

Build the accepted close-range envelope:

```sh
cargo run -p wfb-radio-diag -- --json \
  --report /tmp/wfb-rfq-close-efuse-quality.json \
  rf-quality-report \
  --profile-name close-range-ch36-ht20-efuse-20260502 \
  --profile-kind close-range \
  --channel 36 --bandwidth 20 \
  --tx-rate mcs1 \
  --tx-profile linux-monitor \
  --tx-power-mode efuse-derived \
  --calibration-mode stop-gap-captured \
  --wfb-link-id 0x000001 \
  --wfb-radio-port 0x00 \
  --fec-k 8 --fec-n 12 \
  --payload-len 1000 \
  --expected-payloads 2000 \
  --recovered-payloads 2000 \
  --mac-report /tmp/wfb-rfq-close-efuse-listen.json \
  --efuse-report /tmp/wfb-remote-macos-efuse-dump.json \
  --linux-baseline fixtures/rf-quality/current-close-range-20mhz-linux-baseline.json \
  --receiver-artifact /tmp/rfq-close-efuse-rf.pcap \
  --receiver-artifact /tmp/rfq-close-efuse-rx.log \
  --receiver-artifact /tmp/rfq-close-efuse-tx.log
```

## Interpretation

- `acceptance.status=baseline_comparable` means the comparison tuple matches the
  Linux baseline and the report is valid for close-range RF-quality evidence.
- `comparison.outcome.acceptance_margin.status=within_margin` means payload
  loss is inside the configured Linux-baseline margin.
- `calibration.stop_gap=true` means close-range pass is not calibration parity.
  Keep this label visible until runtime IQK/LCK/RFE calibration is ported or
  measured unnecessary at range.
- `bandwidth_evidence.status=context_only_narrower_observed` means HT40/VHT80
  channel context was used but observed frames were narrower; do not claim wide
  PPDU range.

## Rollback

All RF-quality controls are explicit. To roll back:

- Omit `--tx-power-mode efuse-derived` and use the existing bridge default.
- Omit `--observed-ppdu-*` fields when no wide-bandwidth evidence is being
  recorded.
- Restore the Linux WFB container after controlled runs.
- Keep generated RF-quality reports as evidence; do not overwrite accepted
  reports when debugging a failed run.
