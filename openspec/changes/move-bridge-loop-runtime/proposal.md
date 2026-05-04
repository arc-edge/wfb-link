## Why

`radio-run` now exposes a production command and runtime-owned report, but the
live WFB RX/TX loop still executes through diagnostic bridge internals. Moving
the loop boundary into `wfb-radio-runtime` is the next production cutover step
because it reduces diagnostic coupling without changing the hardware-proven
behavior.

## What Changes

- Add runtime-owned WFB bridge-loop configuration, socket binding plan,
  validation, and telemetry helpers.
- Move production RX forwarding and TX UDP ingress planning out of diagnostic
  structs and into `wfb-radio-runtime`.
- Keep the existing diagnostic bridge implementation as an execution adapter
  until the runtime crate owns the full loop.
- Add tests that the production loop surface accepts valid WFB routing and
  rejects invalid routing/bounds before USB open or socket binding.
- Preserve the existing `radio-run` command and report shape while routing more
  of its setup through runtime-owned helpers.

## Capabilities

### New Capabilities

- `runtime-bridge-loop`: Runtime-owned WFB bridge-loop planning and telemetry
  boundary for production RX/TX operation.

### Modified Capabilities

- `wfb-radio-runtime`: Production full-flow behavior gains runtime-owned loop
  planning and validation for WFB TX ingress and RX forwarding before execution
  is fully moved out of diagnostics.

## Impact

- Extends `crates/wfb-radio-runtime` with WFB loop plan/config helpers.
- May add a runtime dependency on `wfb-bridge` for WFB channel ID and forwarding
  config types.
- Updates `wfb-radio-diag` so `radio-run` uses runtime-owned loop planning
  before adapting into the diagnostic bridge loop.
- Updates tests and runtime boundary documentation.
