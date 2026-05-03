## Why

Runtime session TX/RX helpers exist, but diagnostic bridge commands still submit TX through raw transport handles. The first production-path adoption should be the bounded `bridge-tx-once` command because it exercises WFB datagram parsing and one live radio submit without changing long-running loop behavior.

## What Changes

- Add a diagnostic `RadioTx` adapter backed by `RuntimeRadioSession`.
- Route `bridge-tx-once` live submission through `RuntimeRadioSession::submit_80211_frame`.
- Preserve command output shape and validation behavior.

## Capabilities

### Modified Capabilities
- `wfb-radio-bridge`: Single-frame WFB bridge TX uses runtime session I/O.

## Impact

- Affects `bridge-tx-once` internals only.
- No intended CLI or JSON schema changes.
