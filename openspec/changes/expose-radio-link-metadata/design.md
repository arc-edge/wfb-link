## Overview

The runtime already has the raw material for ROW-161:

```text
RTL8812AU RX descriptor
  -> radio_core::RxFrame
     - rssi_dbm + source/valid flag
     - snr_db/noise_dbm when PHY status supports it
     - channel/rate/bandwidth/PHY evidence
  -> wfb-radio-runtime aggregate rx.signal
  -> radio-run report / health file
  -> wfb-link LinkHealth / Android final result
```

The missing layer is a product contract that keeps the raw metadata, derives a
simple quality indicator, and preserves signal by stream/radio port when WFB
frames can be attributed.

## Current Gaps

- `RuntimeRxSignalSummary` only tracks `sample_count`, `min`, `max`, and
  `average` for aggregate RX RSSI/SNR/noise.
- `ProductionRuntimeRxForwardSnapshot` tracks per-forward counters and
  `last_rx_unix_ms`, but not the matched frames' signal metadata.
- `LinkRxHealth` flattens aggregate RSSI/SNR/noise to averages only.
- `LinkStreamRxHealth` exposes forwarded frames/bytes/timestamp only.
- Android SDK exposes aggregate RX signal only from the terminal runtime report;
  it has no quality indicator and no source/staleness semantics.
- The runtime health file is written during validation, initialization, ready,
  and final exit states, but not as a periodic running health snapshot.

## Proposed Data Model

Use one common signal shape for aggregate RX and stream-level RX:

```text
RadioLinkSignalSummary
  state: unsupported | unknown | fresh | stale | disconnected
  quality_level: null | 0 | 1 | 2 | 3 | 4
  quality_label: unknown | none | poor | weak | fair | good | excellent
  quality_basis: rssi_dbm_average | rssi_dbm_last | unsupported | no_valid_samples
  last_sample_unix_ms: null | u64
  rssi_dbm: Metric
  snr_db: Metric
  noise_dbm: Metric
  metadata:
    rssi_source
    phy_status_frames
    rssi_valid_frames
    snr_frames
    noise_frames
    last_channel
    last_frequency_mhz
    last_bandwidth_mhz
    last_mcs_index
    last_rx_rate
```

`Metric` should keep existing fields and add `last`:

```text
Metric
  sample_count
  last
  min
  max
  average
```

The exact Rust/Java type names can follow local conventions, but the serialized
field names should stay snake_case in runtime/link JSON and Java should expose
camelCase wrappers.

## Quality Semantics

Quality is a UI convenience, not the source of truth. Raw values remain
available for debugging.

Initial RSSI-derived mapping:

```text
RSSI >= -50 dBm  -> 4 excellent
RSSI >= -60 dBm  -> 3 good
RSSI >= -70 dBm  -> 2 fair
RSSI >= -80 dBm  -> 1 weak
RSSI <  -80 dBm  -> 0 poor
```

Use `average` over the current accumulated/windowed summary when available;
fall back to `last` if needed. Do not derive a quality level from fallback or
invalid RSSI. SNR/noise remain debug fields for the first slice; they can tune
quality later after field validation.

State semantics:

- `unsupported`: the backend cannot produce radio signal metadata.
- `unknown`: the backend supports metadata but has no valid samples yet.
- `fresh`: at least one valid sample exists and the last sample is inside the
  documented freshness window.
- `stale`: valid samples exist, but the last sample is older than the freshness
  window while the runtime is otherwise running.
- `disconnected`: the runtime is stopped/failed or the stream is known down.

The first freshness window should be conservative and documented, for example
3 seconds. Consumers can still inspect `last_sample_unix_ms` directly.

## Stream Attribution

Aggregate signal should observe all parsed RX frames with valid signal. Stream
signal should observe only frames that match the configured WFB `link_id` and
`radio_port` for a forward target:

```text
rx.signal                         all parsed valid RX frames
rx.rx_forwards[N].signal          frames matched to that configured WFB stream
wfb-link LinkRxHealth.signal      aggregate product view
LinkStreamRxHealth.signal         named telemetry/video stream view
```

This keeps telemetry and video separately visible when they are separate WFB
radio ports, while still documenting that they are measured by the same local
receiver/radio unless a peer reports remote observations.

Remote/uplink RSSI is explicitly out of scope for this slice unless peer
metadata already exists. The contract should leave room for a future
`remote_signal` block.

## Health Snapshot Cadence

The production runtime should write running health snapshots at a bounded
cadence, initially 1 Hz, when a health file is configured. This gives UI and
support tools a stable polling target without per-packet streaming.

The loop already accumulates RX/TX telemetry in memory. The implementation can
write a service-health snapshot after loop iterations when:

- a health file is configured,
- at least the configured interval has elapsed since the last write, and
- the runtime has entered the bridge loop.

Snapshot write failures should follow existing health-writer behavior:
structured runtime error on required writes before startup/finalization, and
best-effort handling during the running loop if preserving RF flow is more
important than the supervisor artifact.

## Implementation Notes

- Extend runtime signal summaries first, because report JSON, `wfb-link`, and
  Android all parse from that shape.
- Add signal aggregation to `ProductionRuntimeRxForwardRuntime` after
  `build_rx_forward_datagram` confirms a frame matched the forward config.
- Keep existing serialized fields for compatibility; add fields rather than
  rename the current aggregate averages.
- `wfb-link` can keep `rssi_average_dbm`, `snr_average_db`, and
  `noise_average_dbm` while adding a structured `signal` block.
- Android can keep `result.rxSignal` while expanding it with `quality` and
  `last` fields. A per-stream Android model can follow when native Android
  supports multiple RX streams.

## Open Questions

- Should `quality_level` use 0..4 or 0..5? The Linear ticket suggested a bar
  indicator; 0..4 maps naturally to five visual states including empty/poor.
- Should stale be computed in runtime JSON, product wrappers, or both? Runtime
  JSON has the timestamp and lifecycle, so computing it there reduces duplicate
  client logic.
- Should periodic health writes be enabled only when `health_file` is set, or
  should embedded `LinkHandle::health()` eventually read an in-memory snapshot
  without a file?
