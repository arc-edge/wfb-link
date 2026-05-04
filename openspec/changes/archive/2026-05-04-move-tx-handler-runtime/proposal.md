## Why

The bridge loop scheduler is runtime-owned, but one queued TX datagram is still
parsed, described, and submitted by `wfb-radio-diag`. Moving that TX step into
`wfb-radio-runtime` continues the production cutover and leaves diagnostics as a
report adapter instead of the owner of TX packet behavior.

## What Changes

- Add a runtime-owned bridge TX datagram handler that consumes one queued
  runtime TX datagram and submits it through `RuntimeRadioSession`.
- Return report-neutral TX step outcomes containing parsed datagram metadata,
  bridge counters, submit counters, byte counts, and stable failure labels.
- Refactor `bridge-run` to call the runtime TX handler from the existing runtime
  loop executor callback.
- Preserve existing diagnostic report shape, stop reason behavior, TX status
  handling, and radio-run smoke behavior.

## Capabilities

### New Capabilities

- `runtime-tx-handler`: Runtime-owned processing for one queued production WFB
  TX datagram.

### Modified Capabilities

- `runtime-bridge-loop`: Runtime bridge-loop ownership expands from scheduling
  and TX ingress to queued TX datagram handling.
- `wfb-radio-runtime`: Production full-flow behavior uses runtime-owned queued
  TX handling while RX packet output remains adapted from diagnostics.

## Impact

- Extends `crates/wfb-radio-runtime` with bridge TX handler config, metadata,
  outcome, error, and tests.
- Refactors `crates/wfb-radio-diag` bridge-run TX callback to adapt runtime
  handler outcomes into existing diagnostic report fields.
- Keeps `wfb-bridge` and `radio-core` as the lower-level frame parsing and
  descriptor construction dependencies.
