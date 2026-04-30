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

When frames are parsed, each JSONL record includes:

- Unix millisecond timestamp
- frame length and raw 802.11 frame hex
- RSSI dBm
- channel number, frequency, and band
- frame type

## Boundaries

`rx-scan` does not run init, issue control writes, submit bulk-OUT TX frames, or transmit. It reports the selected 20/40/80 MHz metadata path, but RF reception still requires traffic on the tuned channel.
