## Why

`radio-run` now emits a runtime-owned report and uses runtime-owned helpers for
most production behavior, but the command still executes by adapting through
diagnostic `runtime-flow` / `bridge-run` report machinery. That keeps the
diagnostic binary in the production control path and blocks a future daemon or
service from owning the radio flow directly through `wfb-radio-runtime`.

## What Changes

- Add a runtime-owned production flow execution API that opens the adapter,
  initializes the radio, starts TX ingress, runs the interleaved RX/TX bridge
  loop, drives the heartbeat, writes the ready marker, and returns
  `ProductionRuntimeFlowReport` without diagnostic report structs.
- Move the remaining `radio-run` execution harness out of
  `wfb-radio-diag`; keep the diagnostic crate as a CLI adapter that parses
  operator flags, loads diagnostic-only inputs such as EFUSE files, maps them
  into runtime config, calls the runtime API, and prints/serializes the report.
- Preserve the existing production command surface and report shape, including
  heartbeat reporting, ready-marker semantics, RX forwarding snapshots, RX/TX
  counters, signal summaries, calibration evidence, and error classification.
- Keep PCAP/JSONL side outputs, register experiments, TX status probes, and
  legacy bring-up smokes diagnostic-only.
- Preserve existing smoke automation and receiver-backed validation behavior.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `wfb-radio-runtime`: runtime owns the production flow execution API, not only
  config, loop helpers, handlers, and report structs.
- `production-runtime`: `radio-run` uses the runtime-owned execution API
  directly while retaining the same operator-facing command surface and report
  contract.

## Impact

- Affected crates: `wfb-radio-runtime`, `wfb-radio-diag`.
- Affected command: `radio-run`.
- Affected automation: `scripts/run-production-radio-smoke.sh`,
  `scripts/run-radio-run-duplex-smoke.sh`, profile matrix and RF-quality
  automation should continue to work with the same command/report fields.
- No protocol changes and no intentional CLI/report breaking changes.
