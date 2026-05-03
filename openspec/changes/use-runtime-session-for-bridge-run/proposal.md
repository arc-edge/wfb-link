## Why

`bridge-run` is the combined RX/TX command that production use will converge on, but it still owns raw diagnostic USB handles while one-shot and listener TX already use runtime sessions. Moving `bridge-run` onto `RuntimeRadioSession` makes the runtime crate the owner of live radio I/O for the full bridge loop.

## What Changes

- Open `bridge-run` live USB transport into a `RuntimeRadioSession`.
- Route bridge-run TX submissions through the session-backed `RadioTx` adapter.
- Keep existing initialization, RX parsing, forwarding, and reports unchanged.

## Capabilities

### Modified Capabilities

- `wfb-radio-bridge`: combined RX/TX bridge loop uses runtime session I/O for frame injection.

## Impact

- Affects `bridge-run` internals only.
- No intended CLI or JSON schema changes.
