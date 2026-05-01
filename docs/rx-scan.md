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
- RSSI dBm
- channel number, frequency, and band
- frame type

## Boundaries

Without `--init-before-rx` or `--monitor-opmode-before-rx`, `rx-scan` does not run init or issue control writes. It never submits bulk-OUT TX frames or transmits. It reports the selected 20/40/80 MHz metadata path, but RF reception still requires traffic on the tuned channel.
