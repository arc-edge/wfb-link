## Context

The session boundary is the right place for production I/O because it already owns the selected transport, endpoint layout, channel-facing state supplied by callers, and runtime counters. `radio-core` remains responsible for descriptor construction and RX descriptor parsing; runtime session methods compose those primitives with endpoint selection and counter policy.

## Goals / Non-Goals

**Goals:**

- Add runtime TX submit helper on `RuntimeRadioSession`.
- Add runtime RX bulk read + parse helper on `RuntimeRadioSession`.
- Preserve timeout visibility for RX callers.
- Add hardware-free tests for TX/RX counter updates and endpoint selection.

**Non-Goals:**

- Do not move the full bridge loop yet.
- Do not change TX descriptor encoding or RX descriptor parsing semantics.
- Do not add long-distance RF-quality gates in this slice.

## Decisions

- Keep `TxSubmitCounters` as the detailed TX submission counter source.

  Runtime counters are coarse session counters; `radio-core` still owns detailed submit accounting.

- Parse all complete packets in a received bulk buffer.

  The runtime read helper returns parsed packet outcomes so bridge code can forward frames and count drops without manually walking descriptor alignment.

## Risks / Trade-offs

- RX timeout is represented through `RuntimeRadioError.timeout` rather than a separate enum. This keeps the existing runtime error style while preserving loop control for callers.

## Migration Plan

1. Extend runtime error to preserve timeout classification.
2. Add session TX submit and RX read helpers.
3. Add hardware-free runtime tests.
4. Run formatting, workspace tests, and strict OpenSpec validation.
