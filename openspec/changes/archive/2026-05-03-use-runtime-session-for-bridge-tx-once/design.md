## Context

`bridge-tx-once` already performs local datagram parsing and a bounded USB submit. Moving this path first verifies the runtime session TX API without entangling the full `bridge-run` event loop, RX forwarding, or timeout handling.

## Goals / Non-Goals

**Goals:**

- Use runtime session TX submission in `bridge-tx-once`.
- Preserve existing TX override semantics.
- Keep reports stable.

**Non-Goals:**

- Do not migrate `bridge-tx-listen` or `bridge-run` in this slice.
- Do not change TX descriptor construction.

## Migration Plan

1. Add session-backed `RadioTx` adapter.
2. Convert `bridge-tx-once` open and submit path to a runtime session.
3. Run formatting, workspace tests, and strict OpenSpec validation.
