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
sudo -n nmcli dev set wfb0 managed no
sudo -n nmcli dev set p2p-dev-wfb0 managed no
sudo -n ip link set wfb0 down
sudo -n iw dev wfb0 set type monitor
sudo -n ip link set wfb0 up
sudo -n iw dev wfb0 set channel 36 HT20
```

The runner applies the same hardening by default with
`LINUX_NM_UNMANAGE_IFACE=1` and `LINUX_FORCE_MONITOR=1`. This prevents
NetworkManager scan bursts and catches cases where `wfb0` falls back to
`type managed` after the normal WFB service is stopped. The generated
`channel-state.json` records NetworkManager, monitor-mode, channel, and
bandwidth evidence.
The generated `pcap-channel-evidence.json` reads the Linux RF pcap with
`tcpdump`, counts radiotap frequency tags, and records whether captured frames
stayed on the requested frequency. `verified` means all frequency-tagged frames
matched the requested channel; `off_channel_frames` and
`requested_frequency_absent` mean the receiver evidence is not clean enough for
production or range promotion.

Restore:

```sh
sudo -n docker start arc-wfb-link-1
sudo -n docker ps --filter name=arc-wfb-link-1 --format '{{.Names}} {{.Status}}'
```

## Automated Close-Range Runner

Use the automation script for the accepted close-range sanity workflow when the
hardware Mac and Linux peer are reachable:

```sh
scripts/run-rf-quality-close-range.sh
```

The script runs from the local checkout, starts the hardware-Mac UDP relay and
`bridge-tx-listen`, controls the Linux `wfb0` sender/receiver through the
hardware Mac jump host, collects artifacts into a timestamped local `/tmp`
directory, restores the Linux WFB service, and generates an `rf-quality-report`
when the Mac report, EFUSE report, Linux baseline, and receiver counter are
available. The bridge writes a ready marker after init/calibration and before
traffic starts; the default automation no longer sleeps after that marker unless
`BRIDGE_START_DELAY` is explicitly set.
The report embeds Linux peer channel state and RF pcap channel evidence under
`macos.wfb_outcome.receiver_evidence`, so off-channel scanning can be diagnosed
from JSON without manually replaying the pcap.

Inspect the command plan without claiming USB or transmitting RF:

```sh
scripts/run-rf-quality-close-range.sh --dry-run
```

Common site-specific overrides:

```sh
HW_MAC_HOST=rownd@rownds-macbook-pro.tail5c793f.ts.net \
HW_REPO_PATH=projects/arc/wfb-mac-radio-agent \
LINUX_HOST=drone-2f389.local \
MAC_LAN_IP=10.42.0.162 \
scripts/run-rf-quality-close-range.sh
```

Set `SYNC_HW_REPO=1` only when the hardware-Mac checkout should be fast-forwarded
before the bridge starts. The manual commands below remain the fallback when a
single stage needs to be isolated.

If the hardware Mac cannot pull from GitHub or its working checkout is dirty,
use opt-in deploy sync. This copies the local checkout to a separate run
directory and starts the bridge from there:

```sh
HW_DEPLOY=1 \
HW_DEPLOY_PATH=projects/arc/wfb-mac-radio-deploy \
scripts/run-rf-quality-close-range.sh
```

Deploy sync excludes `.git`, `target`, and local transient files. By default it
refuses to deploy over `HW_REPO_PATH`; set `ALLOW_DEPLOY_OVER_WORKTREE=1` only
when overwriting that destination is intentional.

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
- `macos.wfb_outcome.receiver_evidence.pcap_channel_evidence.status=verified`
  means the Linux RF pcap stayed on the requested frequency when radiotap
  frequency tags were present. `off_channel_frames` or
  `requested_frequency_absent` marks the receiver-backed outcome outside the
  production margin.

## Rollback

All RF-quality controls are explicit. To roll back:

- Omit `--tx-power-mode efuse-derived` and use the existing bridge default.
- Omit `--observed-ppdu-*` fields when no wide-bandwidth evidence is being
  recorded.
- Restore the Linux WFB container after controlled runs.
- Keep generated RF-quality reports as evidence; do not overwrite accepted
  reports when debugging a failed run.
