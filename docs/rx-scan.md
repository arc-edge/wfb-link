# RX Scan

`rx-scan` is the bounded live RX diagnostic. It assumes `init` has already completed on the requested channel, then claims the adapter, reads the RTL8812AU bulk-IN endpoint until the duration expires, parses RX descriptors, counts frame types and drops, and optionally writes parsed 802.11 frames to PCAP and JSONL frame records.

## Command

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-rx-scan.json rx-scan \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --duration-ms 1500 --timeout-ms 100 \
  --pcap /tmp/wfb-live-rx-scan.pcap \
  --frame-jsonl /tmp/wfb-live-rx-frames.jsonl
```

On macOS 26, add `--macos-usbhost --vid 0x0bda --pid 0x8812` to use the retained IOUSBHost interface session instead of libusb.

For WFB RX bridge verification, add `--init-before-rx --monitor-opmode-before-rx` so the command performs init and then switches `REG_RCR`/RX filter maps into a no-link monitor receive mode before the bulk-IN loop. The same-session bridge init uses the Linux RTL8812AU LLT-before-firmware order because that order is required for WFB-shaped data TX to radiate reliably on the current AWUS036ACH bench. Add `--wfb-link-id`, `--wfb-radio-port`, and optional `--rx-aggregator HOST:PORT` to count matching WFB frames and forward them with WFB-ng's `wrxfwd_t` header:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-rx-forward.json rx-scan \
  --macos-usbhost --vid 0x0bda --pid 0x8812 \
  --init-before-rx --monitor-opmode-before-rx \
  --firmware /tmp/rtl8812aefw.bin \
  --channel 36 --bandwidth 20 \
  --duration-ms 8000 \
  --wfb-link-id 0x000000 --wfb-radio-port 0x03 \
  --rx-aggregator 127.0.0.1:5700 \
  --i-understand-this-writes-registers
```

## Live Result

Live run on April 30, 2026 with `0x0bda:0x8812` on macOS 15.7.4 passed as a bounded USB read test:

- Channel: 36, 5180 MHz, 20 MHz bandwidth.
- Bulk IN endpoint: `0x81`.
- Duration: 1,500 ms.
- Bulk-IN read attempts: 14 timeouts.
- Captured data: 0 buffers, 0 bytes, 0 parsed frames, 0 drops.
- PCAP: `/tmp/wfb-live-rx-scan.pcap`, header-only.
- Frame JSONL: `/tmp/wfb-live-rx-frames.jsonl`, empty because no frames were captured.
- Report: `/tmp/wfb-live-rx-scan.json`.

This proves the Mac can claim the already-initialized adapter and run a non-blocking bulk-IN loop. It does not yet prove RF reception; that needs traffic on the tuned channel, preferably from the Linux WFB peer.

A later April 30, 2026 run after 40 MHz init also passed as a bounded USB read test:

- Channel: 36, 5180 MHz, 40 MHz bandwidth.
- Bulk-IN read attempts: 10 timeouts over 1,000 ms.
- Captured data: 0 buffers, 0 bytes, 0 parsed frames, 0 drops.
- PCAP: `/tmp/wfb-live-rx-scan-40mhz.pcap`, header-only.
- Frame JSONL: `/tmp/wfb-live-rx-frames-40mhz.jsonl`, empty because no frames were captured.
- Report: `/tmp/wfb-live-rx-scan-40mhz.json`.

A later April 30, 2026 run after 80 MHz init also passed as a bounded USB read test:

- Channel: 36, 5180 MHz, 80 MHz bandwidth.
- Bulk-IN read attempts: 10 timeouts over 1,000 ms.
- Captured data: 0 buffers, 0 bytes, 0 parsed frames, 0 drops.
- PCAP: `/tmp/wfb-live-rx-scan-80mhz.pcap`, header-only.
- Frame JSONL: `/tmp/wfb-live-rx-frames-80mhz.jsonl`, empty because no frames were captured.
- Report: `/tmp/wfb-live-rx-scan-80mhz.json`.

The remote macOS 26 retained IOUSBHost path also passed as a bounded USB read test:

- Command: `rx-scan --macos-usbhost --vid 0x0bda --pid 0x8812 --channel 36 --bandwidth 20`.
- Bulk-IN read attempts: 10 clean timeouts over 1,000 ms on endpoint `0x81`.
- Captured data: 0 buffers, 0 bytes, 0 parsed frames, 0 drops.
- PCAP: `/tmp/wfb-remote-macos-rx-scan-usbhost.pcap`, header-only.
- Frame JSONL: `/tmp/wfb-remote-macos-rx-scan-usbhost.jsonl`, empty because no frames were captured.
- Report: `/tmp/wfb-remote-macos-rx-scan-usbhost.json`.

A later May 1, 2026 remote macOS 26 run with `--init-before-rx --monitor-opmode-before-rx` verified the WFB RX bridge path against the Linux peer:

- Linux source: stock `wfb_tx` on `drone-2f389.local:wfb0`, channel 36/HT20.
- WFB filter: link ID `0x000000`, radio port `0x03`.
- Mac capture: 848 parsed frames, including 147 data frames.
- WFB bridge: 21 matched frames, 21 forwarded datagrams, 0 send failures.
- Aggregator socket: 21 UDP datagrams, 1,425 bytes.
- Reports/artifacts: `/tmp/wfb-agent-rx-forward-linux-wfbtx.json`, `/tmp/wfb-agent-rx-forward-linux-wfbtx.jsonl`, `/tmp/wfb-agent-rx-forward-linux-wfbtx.pcap`, `/tmp/wfb-agent-agg-rx-monitor.json`.

When frames are parsed, each JSONL record includes:

- Unix millisecond timestamp
- frame length and raw 802.11 frame hex
- RSSI dBm plus `rssi_dbm_valid` and `rssi_dbm_source`; fallback RSSI remains
  present for compatibility but is marked invalid when PHY status was absent
- nullable `noise_dbm`, `snr_db`, and `snr_db_source` fields. For
  RTL8812AU OFDM/HT/VHT PHY-status records, the parser uses the PHYDM 1st-type
  layout to select the strongest path, expose signed SNR, and derive a noise
  estimate from RSSI minus SNR. Short, CCK, or no-PHY-status records leave these
  fields null.
- channel number, frequency, and band
- PHY-status evidence: `phy_status`, `driver_info_size`, `rx_shift`,
  `raw_phy_status_len`, and bounded `raw_phy_status_hex`
- RTL8812AU RX descriptor rate metadata: raw rate byte, decoded CCK/OFDM/HT/VHT rate, raw bandwidth field, decoded bandwidth in MHz, SGI, LDPC, and STBC flags
- frame type

A later May 1, 2026 remote macOS 26 run used the RX descriptor metadata to cross-check HT40 behavior while the Linux peer sent stock WFB traffic on channel 36/HT40+:

- Linux source: `wfb_tx -B 40 -k 1 -n 3` on `drone-2f389.local:wfb0`, pinned to channel 36/HT40+.
- Mac capture: `rx-scan --macos-usbhost --init-before-rx --monitor-opmode-before-rx --channel 36 --bandwidth 40 --frame-jsonl /tmp/wfb-agent-rxmeta40a.jsonl`.
- Parsed frames: 893, including 130 data frames.
- WFB bridge: 95 matched radio-port `0x03` frames, 95 forwarded datagrams, 0 send failures.
- Descriptor metadata for the matched WFB-shaped data burst: 95 MCS1 records with `rx_bandwidth_raw=0`, decoded as 20 MHz, no SGI, no LDPC, and no STBC.
- Report/artifacts: `/tmp/wfb-agent-rxmeta40a.json`, `/tmp/wfb-agent-rxmeta40a.jsonl`, `/tmp/wfb-agent-rxmeta40a.log`.

This independently confirms that the current HT40+ WFB flow is operating on an HT40-tuned channel but the received WFB data frames are still reported by the RTL8812AU RX descriptor as 20 MHz PPDUs.

A May 3, 2026 remote macOS 26 RX metadata smoke verified adapter-side SNR/noise
extraction from live RTL8812AU PHY status:

- Mac capture: `rx-scan --macos-usbhost --init-before-rx
  --monitor-opmode-before-rx --channel 36 --bandwidth 20 --frame-jsonl
  /tmp/wfb-snr-rx.jsonl`.
- Parsed frames: 486 records, 398 bulk-IN buffers, 211,482 bulk bytes, and 2
  read timeouts.
- SNR/noise metadata: 400 records reported
  `snr_db_source=rtl8812_phy_status_best_path` with non-null `snr_db` and
  `noise_dbm`; 86 records had no usable PHY-status SNR and kept those fields
  null.
- Example decoded records included OFDM6/OFDM12 frames with RSSI/SNR/noise such
  as `-70 dBm / 9 dB / -79 dBm` and `-26 dBm / 25 dB / -51 dBm`.
- Report/artifacts: `/tmp/wfb-snr-rx.json`, `/tmp/wfb-snr-rx.jsonl`, and
  `/tmp/wfb-snr-rx.log` on the hardware Mac.

## Boundaries

Without `--init-before-rx` or `--monitor-opmode-before-rx`, `rx-scan` does not run init or issue control writes. It never submits bulk-OUT TX frames or transmits. It reports the selected 20/40/80 MHz metadata path, but RF reception still requires traffic on the tuned channel.
