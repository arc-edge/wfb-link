## Context

The integrated bridge path still calls many local diagnostic helpers. Moving every phase at once would entangle runtime code with large diagnostic report structs. A narrower extraction is to move the register write/read behavior and have `diag` keep translating runtime execution evidence into its current JSON reports.

## Goals / Non-Goals

**Goals:**

- Introduce runtime-level register counters and structured runtime radio errors.
- Move TX scheduler tail execution out of `wfb-radio-diag`.
- Move monitor receive filter/opmode execution out of `wfb-radio-diag`.
- Add runtime tests for the exact migrated register writes.

**Non-Goals:**

- Do not move full same-session init orchestration yet.
- Do not change diagnostic report schemas.
- Do not move EFUSE MAC programming in this slice.

## Decisions

- Runtime helpers return compact execution structs with numeric evidence.

  The diagnostic crate already owns formatting helpers like hex strings and MAC display. Runtime should report raw values and counters so production code can make decisions without inheriting diagnostic formatting.

- Counter conversion remains in `wfb-radio-diag`.

  `DiagnosticCounters` is diagnostic-local. Runtime owns an equivalent stable counter type; the CLI adapter converts at the boundary.

## Risks / Trade-offs

- This temporarily leaves wrapper functions in `diag`. That is intentional so call sites and report schemas remain stable while execution ownership moves.
- The runtime crate gains register constants duplicated from diagnostic code. Later slices can centralize common RTL8812AU register names once more phases have moved.

## Migration Plan

1. Add runtime counters, errors, and register access helpers.
2. Move TX scheduler tail execution and test the write sequence.
3. Move monitor receive filter/opmode execution and test the write sequences.
4. Update diagnostic wrappers to call runtime helpers.
5. Run formatting, runtime tests, workspace tests, and strict OpenSpec validation.
