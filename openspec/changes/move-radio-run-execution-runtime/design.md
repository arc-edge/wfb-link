## Context

The production `radio-run` command has been progressively cut over to
`wfb-radio-runtime`: config validation, loop planning, TX ingress sockets,
bridge-loop scheduling, queued TX datagram handling, parsed RX outcome
accounting, RX forwarding, TX-power execution, calibration profile execution,
ready-marker writing, telemetry structs, and heartbeat reporting are already
runtime-owned.

The remaining gap is orchestration. `wfb-radio-diag` still calls
`runtime_flow_report`, which calls `bridge_run_report`, then adapts that
diagnostic report into `ProductionRuntimeFlowReport`. That path is proven, but
it keeps diagnostic report mutation and command-specific bridge plumbing in the
middle of the production flow. A future production daemon should be able to
start a runtime flow without linking to diagnostic command/report machinery.

## Goals / Non-Goals

**Goals:**

- Add a runtime-owned production execution API that returns
  `ProductionRuntimeFlowReport` directly.
- Preserve the existing `radio-run` CLI, JSON shape, text output, ready marker,
  heartbeat behavior, and smoke automation.
- Keep diagnostic-only features out of `radio-run`: PCAP/JSONL, TX status,
  register pokes, trace replay, and legacy bring-up report details.
- Make `wfb-radio-diag` a thin adapter around CLI parsing, file loading, and
  human/report output for the production path.
- Keep the migration incremental so every slice remains hardware-smokeable.

**Non-Goals:**

- Removing `bridge-run`, `runtime-flow`, or legacy diagnostic commands.
- Rewriting RF calibration algorithms or changing production calibration
  defaults.
- Introducing a long-running daemon in this change.
- Changing WFB packet format, channel/rate defaults, or field acceptance
  criteria.

## Decisions

### 1. Runtime API owns orchestration, diagnostic crate owns adapters

Add a runtime API such as `run_production_runtime_flow(config, inputs)` that
opens the radio, performs same-session init, writes the ready marker, starts TX
ingress, runs the bridge-loop executor, drives heartbeat, handles RX/TX work,
and returns `ProductionRuntimeFlowReport`.

`wfb-radio-diag` should keep parsing CLI flags and loading diagnostic-origin
inputs that the runtime should not know how to read directly, such as EFUSE JSON
files or Realtek table source paths. Those inputs are converted into
runtime-owned config/value structs before the runtime execution call.

Alternative considered: keep the current bridge adapter and only rename helper
functions. That would not remove diagnostic report structs from the production
control path and would leave future daemon work blocked.

### 2. Report shape stays stable

The runtime execution API returns the existing `ProductionRuntimeFlowReport`.
Fields currently supplied by bridge adaptation remain present: init readiness,
calibration class/evidence, TX power/calibration reports, RX signal and frame
type counters, RX forward snapshots, TX counters, USB counters, heartbeat
counters, stop reason, and error report.

Alternative considered: add a v2 report shape while moving execution. That
would force automation changes unrelated to the ownership migration. Report
schema churn should wait until there is a real product requirement.

### 3. Keep side outputs diagnostic-only

PCAP and JSONL frame output remain in `bridge-run`/diagnostic paths. Production
flow reports carry counters and snapshots needed for automation gates, not
diagnostic sidecar captures.

Alternative considered: add optional PCAP/JSONL sinks to the runtime API. That
would expand the runtime boundary before the basic production runner has been
made independent. It can be revisited later if a production workflow needs
bounded captures.

### 4. Implement in small extraction slices

The current `bridge_run_report` body is large. The migration should avoid a
single risky rewrite by extracting report-neutral pieces into runtime-owned
helpers first, then replacing the `runtime_flow_report` adapter once the runtime
API can produce equivalent reports.

Recommended slice order:

1. Add runtime-owned production execution input/output scaffolding and tests for
   failure-before-USB validation.
2. Move same-session init and ready-marker orchestration into the runtime
   execution function.
3. Move heartbeat lifecycle and bridge-loop invocation into the runtime
   execution function.
4. Replace `radio_run_report`'s diagnostic `runtime_flow_report` adapter with a
   direct runtime call.
5. Keep `runtime-flow` and `bridge-run` diagnostic paths intact.

## Risks / Trade-offs

- **[Risk]** Behavioral regression in a path that currently has good hardware
  evidence. **Mitigation:** keep the command/report shape stable and rerun
  `run-production-radio-smoke`, duplex smoke, and a short receiver-backed
  matrix after each substantial slice.
- **[Risk]** Runtime crate grows diagnostic file-loading concerns. **Mitigation:**
  pass already-parsed runtime-owned values into the runtime API; leave CLI/file
  parsing in `wfb-radio-diag`.
- **[Risk]** Borrowing/lifetime complexity around TX ingress threads, heartbeat,
  and USB transport. **Mitigation:** preserve the existing runtime session and
  loop executor types, then move ownership boundaries around them rather than
  changing their concurrency model.
- **[Risk]** Duplicated code during migration. **Mitigation:** tolerate short
  duplication only while a slice is under test, then delete the adapter path once
  direct `radio-run` execution passes the smoke gates.

## Migration Plan

1. Add runtime execution API and unit tests without changing `radio-run`.
2. Switch `radio-run` to the runtime execution API behind the same CLI and report
   output.
3. Run `cargo test -p wfb-radio-runtime -p wfb-radio-diag`,
   `openspec validate --specs --strict`, and production smoke automation.
4. Run a short receiver-backed duplex smoke to prove WFB flow still works.
5. Archive the change only after local or remote hardware smoke confirms parity
   with the current production command.

Rollback is a normal revert: the existing diagnostic `runtime-flow` /
`bridge-run` execution path remains available until the direct runtime path is
accepted.

## Open Questions

- Should `runtime-flow` remain as a diagnostic compatibility command forever, or
  eventually become an alias around the runtime execution API?
- Should a future daemon call the same runtime execution API directly or use a
  lower-level long-running session object with explicit lifecycle control?
