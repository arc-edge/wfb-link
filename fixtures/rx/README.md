# RX Bulk-IN Fixtures

`wfb-radio-diag rx-scan --fixture-bulk-in <path>` reads raw bytes exactly as they would arrive from the RTL8812AU bulk-IN endpoint.

Each file should contain one or more RTL8812AU RX aggregate records:

- 24-byte RTL8812AU RX descriptor.
- Optional driver info and shift bytes, as described by descriptor fields.
- IEEE 802.11 frame bytes plus the trailing FCS.
- Padding up to the chipset aggregation alignment when present.

The diagnostic parser does not synthesize metadata for these files. It runs the same descriptor parser used by the future live RX loop, emits counters, and optionally writes parsed 802.11 frames to PCAP.

Example:

```sh
cargo run -p wfb-radio-diag -- --json rx-scan \
  --channel 36 \
  --fixture-bulk-in /path/to/bulk-in.bin \
  --pcap /tmp/wfb-rx-fixture.pcap
```
