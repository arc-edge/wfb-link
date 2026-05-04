## Why

Production TX ingress is runtime-owned, but the bridge loop scheduler still
lives in `wfb-radio-diag`. Moving the scheduler into `wfb-radio-runtime`
continues the production cutover without changing packet parsing, USB
submission, or hardware init behavior.

## What Changes

- Add a runtime-owned bridge loop executor for stop conditions, TX burst
  cadence, max-datagram handling, and RX timeout calculation.
- Keep packet-specific TX/RX work in diagnostic callbacks for this slice.
- Route the existing bridge loop through the runtime executor.
- Preserve existing `bridge-run`, `runtime-flow`, and `radio-run` report shape
  and hardware behavior.

## Capabilities

### New Capabilities

- `runtime-loop-executor`: Runtime-owned scheduler for production WFB bridge
  loop execution.

### Modified Capabilities

- `runtime-bridge-loop`: Runtime bridge-loop ownership expands from planning
  and TX ingress to loop scheduling and stop-condition handling.
- `wfb-radio-runtime`: Production full-flow behavior uses runtime-owned bridge
  loop scheduling while the remaining packet handlers are adapted from
  diagnostics.

## Impact

- Extends `crates/wfb-radio-runtime` with a callback-driven loop executor and
  tests.
- Refactors `crates/wfb-radio-diag` bridge loop to call the runtime executor.
- Updates OpenSpec docs and runtime boundary docs.
