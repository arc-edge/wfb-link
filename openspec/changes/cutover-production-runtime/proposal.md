## Why

The current `runtime-flow` command has production-shaped telemetry, but it is
still implemented as an adapter around diagnostic bridge argument and report
types. This makes the production path harder to stabilize because operational
callers still inherit diagnostic-only CLI surface, report shape, and internal
coupling.

## What Changes

- Add a smaller production runtime entry point for WFB full-flow operation.
- Move production runtime configuration and report types into
  `wfb-radio-runtime` so callers can use them without depending on
  `wfb-radio-diag`.
- Keep the diagnostic `runtime-flow` command as a compatibility harness while it
  translates into runtime-owned config/report types.
- Preserve existing diagnostic bridge commands and RF-quality automation.
- Do not remove diagnostic-only register experiments; keep them outside the
  production command/API.

## Capabilities

### New Capabilities
- `production-runtime`: Production runtime command/API surface for native WFB
  full-flow operation.

### Modified Capabilities
- `wfb-radio-runtime`: Production runtime full-flow behavior now requires
  runtime-owned config/report types rather than diagnostic bridge report types.
- `radio-runtime-library`: Runtime library exposes report-neutral production
  flow types usable by a production binary or diagnostic adapter.

## Impact

- Adds or refactors runtime-owned production config/report types in
  `crates/wfb-radio-runtime`.
- Adds a thinner production command or binary that does not expose
  diagnostic-only register experiments.
- Updates `wfb-radio-diag` to call the runtime-facing types for compatibility.
- Updates docs and OpenSpec specs for the new production boundary.
