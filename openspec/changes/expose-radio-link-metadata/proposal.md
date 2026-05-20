## Why

Downstream consumers need more than a single RSSI average to render signal
quality and debug radio behavior. Arc Edge already extracts RTL8812AU RX signal
metadata, but the product/status contract does not expose a stable quality
indicator, source/validity semantics, or stream-level signal attribution for
telemetry and video radio ports.

## What Changes

- Define a radio-link metadata contract for aggregate RX state and
  stream/radio-port RX state.
- Add a normalized quality indicator alongside raw RSSI, SNR, noise, sample
  counts, min/max/average, last sample, source, and validity data.
- Attribute RX signal metadata to telemetry/video streams when frames match a
  configured WFB link ID and radio port.
- Preserve existing raw debug evidence, including channel, frequency,
  bandwidth, MCS/rate, and PHY-status coverage where available.
- Document unknown, unsupported, stale, disconnected, and invalid/fallback
  signal behavior.
- Define an operator-facing update cadence through periodic health snapshots
  instead of per-packet UI streaming.

## Capabilities

### New Capabilities

- `radio-link-metadata`: downstream-facing aggregate and stream-level radio
  metadata contract, including quality indicator and raw debug fields.

### Modified Capabilities

- `wfb-radio-runtime`: runtime RX telemetry will retain quality/source data and
  per-forward signal attribution.
- `production-runtime`: production health/status output will expose radio-link
  metadata on a documented snapshot cadence.
- `android-sdk-integration`: Android SDK result/status models will surface the
  radio-link metadata needed by product apps.

## Impact

- Affected Rust APIs: `wfb-radio-runtime` RX telemetry/report structs and
  `wfb-link` product health/report structs.
- Affected Java APIs: Android SDK signal/result models.
- Affected docs/specs: product link interface docs, Android integration docs,
  runtime boundary notes, and OpenSpec runtime/production requirements.
- Affected behavior: health artifacts should update periodically while the
  production loop runs so consumers can render current quality without parsing
  packet streams.
