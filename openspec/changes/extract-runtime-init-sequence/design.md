## Context

`run_bridge_tx_bench_same_session_init()` executes the integrated RTL8812AU init path used by bridge, RX, RF-quality, and benchmark runs. Its phase code is report-heavy, so moving the whole function in one step would mix runtime logic with diagnostic report types. A safer first extraction is for runtime to own the phase sequence/profile while diagnostic code continues to execute and report each phase.

## Goals / Non-Goals

**Goals:**

- Define reusable RTL8812AU init phase identifiers in runtime.
- Define the standard same-session phase sequence and the Linux-order LLT/firmware variant.
- Use runtime sequence policy from diagnostic init execution.

**Non-Goals:**

- Do not move all init register writes yet.
- Do not move diagnostic phase reports.
- Do not change current phase order.

## Decisions

- Start with phase order, not phase execution.

  Phase execution depends on many diagnostic report structs. Owning the sequence in runtime gives production callers a stable contract without forcing a large report refactor.

- Keep runtime phase IDs as string-compatible values.

  Diagnostic reports already expose phase IDs. Runtime phase types provide those IDs so future migrations can preserve report compatibility.

## Risks / Trade-offs

- This is a partial init migration. → It is deliberately narrow and prepares the next move: phase execution helpers that return runtime evidence rather than diagnostic reports.
- Only the LLT/firmware order branch is immediately loop-driven by runtime. → The full phase list is tested in runtime and can be consumed by later init extraction work.

## Migration Plan

1. Add runtime init phase/order types and sequence helpers.
2. Use runtime LLT/firmware sequence in same-session init execution.
3. Validate behavior with existing tests and new runtime unit tests.
