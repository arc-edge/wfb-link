## Why

Bridge-loop RX packet accounting and WFB forwarding still live in
`wfb-radio-diag`, even though loop planning, TX ingress, scheduling, and TX
datagram handling are now runtime-owned. Moving the RX packet callback into
`wfb-radio-runtime` leaves diagnostics as file/report adapters and gets the
production flow closer to a real runtime service.

## What Changes

- Add runtime-owned RX forward runtime lifecycle, including UDP socket binding,
  forwarding counters, forwarded byte counts, and report-neutral snapshots.
- Add runtime-owned RX packet outcome processing for parsed frame/drop/tail
  counters, PHY/RSSI/SNR/noise metadata counters, frame type counters, and WFB
  forwarding.
- Refactor `bridge-run` to call the runtime RX packet handler from the existing
  runtime loop executor callback.
- Keep PCAP writing, JSONL frame records, and diagnostic report formatting in
  `wfb-radio-diag`.

## Capabilities

### New Capabilities

- `runtime-rx-handler`: Runtime-owned processing for parsed production RX
  packet outcomes and WFB forwarding.

### Modified Capabilities

- `runtime-bridge-loop`: Runtime bridge-loop ownership expands from scheduling,
  TX ingress, and TX datagram handling to RX packet outcome handling.
- `wfb-radio-runtime`: Production full-flow behavior uses runtime-owned RX
  packet accounting and WFB forwarding while diagnostic file output remains
  adapted outside runtime.

## Impact

- Extends `crates/wfb-radio-runtime` with RX forward runtime types, RX packet
  outcome telemetry, handler functions, and unit tests.
- Refactors `crates/wfb-radio-diag` bridge-run RX callback to adapt runtime
  outcomes into existing diagnostic report fields and continue writing optional
  PCAP/JSONL outputs.
- Keeps `wfb-bridge` as the lower-level WFB payload filtering and forward
  datagram construction dependency.
