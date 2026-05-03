## Why

`bridge-tx-once` now uses runtime session TX, but the UDP listener still submits through raw transport handles. The listener is the next TX production path to move before the combined RX/TX bridge loop.

## What Changes

- Route `bridge-tx-listen` live submissions through the session-backed `RadioTx` adapter.
- Keep pre-TX diagnostic register operations and reports unchanged.

## Capabilities

### Modified Capabilities
- `wfb-radio-bridge`: UDP bridge TX listener uses runtime session I/O for frame injection.

## Impact

- Affects `bridge-tx-listen` internals only.
- No intended CLI or JSON schema changes.
