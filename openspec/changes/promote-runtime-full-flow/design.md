## Context

`wfb-radio-runtime` now owns live transport/session I/O and several low-level RTL8812AU helpers. The remaining full-flow gap is that retained same-session init is still orchestrated from `wfb-radio-diag` command-specific structures, so production callers must either depend on diagnostic code or duplicate init and calibration policy.

The current codebase already has reusable pieces to build on: `RuntimeRadioSession`, runtime USB transport open policy, init phase ordering, TX scheduler tail execution, monitor/opmode execution, MAC address initialization, calibration policy classification, RX parsing, and TX submission. The next step is to make the init orchestration itself report-neutral and callable from production paths.

## Goals / Non-Goals

**Goals:**

- Provide a runtime-owned same-session init API with stable config and result structs.
- Keep existing diagnostic JSON/report commands working by adapting runtime init results into current report formats.
- Add a thin production command that opens, initializes, and runs TX/RX through `RuntimeRadioSession`.
- Require explicit calibration profile policy for runtime callers, with clear labels for production-safe and experimental profiles.
- Preserve hardware-write acknowledgements and existing guardrails for live RF operations.

**Non-Goals:**

- Full Linux-parity IQK/LCK validation at distance.
- Replacing diagnostic-only evidence reports with production telemetry in one step.
- Reworking WFB packet format, UDP aggregation semantics, or Linux peer setup.
- Making runtime IQK/LCK the default calibration path.

## Decisions

1. **Runtime init config is report-neutral.**
   Runtime code will accept structs describing backend/session, init order, channel, bandwidth, MAC address, calibration profile, and authorization. It will not accept diagnostic command arg structs or emit diagnostic report structs. This keeps production callers independent from `wfb-radio-diag`.

2. **Diagnostics adapt runtime evidence instead of owning execution.**
   Existing diagnostic commands will preserve their JSON shape by converting runtime phase summaries, counters, and calibration decisions into report fields. This limits blast radius while moving ownership.

3. **Production command is thin.**
   The first production-facing command will do open-init-run and delegate all USB/RF work to runtime APIs. It will not gain extra diagnostic knobs unless those knobs are part of stable runtime config.

4. **Calibration policy is explicit and conservative.**
   Default init remains the production-safe path. Targeted parity, captured IQK/LCK, and runtime IQK are named opt-in profiles with authorization and reportable classification. Long-distance readiness remains blocked on receiver-backed validation.

5. **Move orchestration incrementally.**
   Large table parsing and report formatting can remain in diagnostic code temporarily if the runtime boundary receives already-normalized init assets. Each iteration should reduce diagnostic ownership without breaking live commands.

## Risks / Trade-offs

- Runtime init may initially wrap normalized inputs while some parsing still lives in diagnostics -> make API boundaries explicit and avoid exposing diagnostic structs.
- Diagnostic report compatibility can slow extraction -> keep adapters small and test output with existing commands.
- Production command could drift into another diagnostic CLI -> restrict it to stable runtime config and flow controls.
- Calibration defaults might be misread as long-distance ready -> label profile class, evidence source, and validation status in runtime results.

## Migration Plan

1. Add runtime init config/result types and an executor function that operates on `RuntimeRadioSession`.
2. Move same-session phase orchestration into the runtime crate, initially accepting normalized assets/config produced by existing loaders.
3. Update diagnostic same-session init callers to invoke the runtime executor and translate results.
4. Add a production command for open-init-run TX/RX flow.
5. Run OpenSpec validation, Rust tests, and hardware smoke where available; commit and push in coherent slices.

## Open Questions

- Which diagnostic-only table parsing helpers should move into runtime first after the executor boundary lands?
- Should the production command live in `wfb-radio-diag` as a hidden/stable subcommand initially, or in a separate binary crate once the API is ready?
- Which calibration profiles should be exposed in the first production command while long-distance validation remains deferred?
