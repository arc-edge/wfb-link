## Why

`bridge-tx-bench` is the local saturation and RF-quality stress workload, but it still owns raw USB transport handles for both generated-frame TX and exact packet replay. Production readiness needs this path to exercise the same runtime session accounting and endpoint selection used by the combined bridge loop.

## What Changes

- Add runtime session support for descriptor-prefixed raw TX packet submission.
- Route generated WFB benchmark submissions through a session-backed radio adapter.
- Route packet override replay through the runtime session instead of direct bulk-OUT writes.

## Capabilities

### Modified Capabilities

- `wfb-radio-bridge`: TX benchmark uses runtime session I/O for generated WFB submissions and exact packet replay.

## Impact

- Affects `bridge-tx-bench` internals and runtime TX helper surface.
- No intended CLI or JSON schema changes.
