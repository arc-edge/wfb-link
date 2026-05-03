## Why

`bridge-run` now owns a runtime radio session and uses it for TX, but RX still calls the raw bulk-IN transport and reparses frames in the diagnostic command. Moving RX reads through the runtime helper makes the combined bridge loop use runtime-owned I/O in both directions.

## What Changes

- Preserve incomplete-tail RX parser outcomes in `RuntimeRadioSession::read_rx_packets`.
- Process runtime-parsed RX packets in `bridge-run` for PCAP, JSONL, and WFB forwarding.
- Keep bridge-run JSON counters and RX report fields stable.

## Capabilities

### Modified Capabilities

- `wfb-radio-bridge`: combined RX/TX bridge loop uses runtime session I/O for RX reads and TX submissions.

## Impact

- Affects `bridge-run` internals and runtime RX helper result detail.
- No intended CLI or JSON schema changes.
