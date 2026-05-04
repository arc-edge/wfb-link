## Context

`wfb-radio-runtime` owns transport open policy, runtime session I/O, selected
init helpers, calibration policy labels, and production-shaped telemetry. The
current `runtime-flow` CLI still adapts `BridgeRunArgs` and `BridgeRunReport`
from `wfb-radio-diag`, so production callers inherit diagnostic-only options
and report coupling even when those options are rejected.

The cutover should preserve the diagnostic harness while establishing a smaller
runtime-owned API and command boundary that can become the default production
entry point.

## Goals / Non-Goals

**Goals:**

- Define runtime-owned production config and report types.
- Add a thinner production command/binary surface that only accepts production
  RX/TX flow options.
- Keep diagnostics and RF-quality automation working during migration.
- Preserve existing JSON fields needed by automation where practical.

**Non-Goals:**

- Removing `wfb-radio-diag` or legacy diagnostic commands.
- Rewriting every init/calibration phase in this change.
- Promoting experimental IQK/LCK profiles to production defaults.
- Solving long-distance RF acceptance or wide-PPDU proof.

## Decisions

- Introduce runtime-owned production flow types first, then route commands
  through them. This gives tests and future callers a stable target without
  requiring a full bridge-loop rewrite in one step.
- Keep diagnostic bridge internals available as an adapter during the first
  cutover slice. The bridge loop is already hardware-proven; the initial
  production work should narrow the public surface without destabilizing RX/TX.
- Add a separate production command/binary surface instead of mutating
  `bridge-run`. Existing RF-quality automation and diagnostic reproductions use
  `bridge-run`; changing it directly would mix operational and experimental
  concerns.
- Explicitly reject diagnostic-only register experiments at the production
  boundary. Diagnostics remain available through `wfb-radio-diag`.

## Risks / Trade-offs

- Runtime-owned types may initially wrap existing diagnostic execution.
  Mitigation: keep the types report-neutral and add tests that production
  config cannot carry diagnostic-only register experiments.
- Maintaining two command surfaces can drift. Mitigation: make the diagnostic
  compatibility command translate into the runtime config and share report
  structs where possible.
- The production surface may still need macOS-specific flags during bring-up.
  Mitigation: keep backend selection explicit but scoped to transport/open
  policy rather than diagnostic register controls.

## Migration Plan

1. Add runtime production config/report types and tests.
2. Add a thin production command/binary that maps CLI arguments into the runtime
   config.
3. Update `runtime-flow` to use or mirror the runtime-owned types while keeping
   its existing JSON shape compatible.
4. Move more init/calibration execution behind runtime APIs in later changes.
5. When field usage is stable, document the production command as the primary
   operator entry point and leave diagnostic commands for bring-up/debugging.
