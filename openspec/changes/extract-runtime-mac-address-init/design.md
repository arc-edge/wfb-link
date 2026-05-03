## Context

REG_MACID programming is used by same-session init and manual bridge TX setup. The simple register write behavior can move directly. EFUSE dump reports are much larger and include decoded packet tables, so this slice moves only the runtime behavior needed by production init: read physical EFUSE bytes, decode the logical map, extract a valid MAC, and program REG_MACID.

## Goals / Non-Goals

**Goals:**

- Provide runtime EFUSE MAC read helpers with runtime counters/errors.
- Provide runtime REG_MACID programming helpers with before/written/after evidence.
- Keep existing diagnostic JSON reports unchanged.
- Add runtime tests for EFUSE MAC decode and REG_MACID writes.

**Non-Goals:**

- Do not move the full EFUSE dump report schema.
- Do not move TX-power EFUSE interpretation in this slice.
- Do not change the same-session init order.

## Decisions

- Runtime EFUSE decoding returns only the logical map and MAC, not packet report rows.

  The diagnostic EFUSE dump still owns packet-level reporting. Runtime only needs the adapter identity for init.

- Diagnostic wrappers preserve formatting.

  Runtime returns raw byte arrays and counter deltas; `wfb-radio-diag` keeps converting them to the current `BridgeTxBenchLocalMacReport`.

## Risks / Trade-offs

- EFUSE physical reads still write EFUSE selector/control registers. This is existing behavior and remains gated by the caller's live-init authorization path.
- Runtime duplicates a small EFUSE logical decode path while diagnostic packet reporting remains local. This can be unified later when EFUSE dump reporting is moved.

## Migration Plan

1. Add runtime EFUSE MAC helpers and tests.
2. Add runtime REG_MACID programming helper and tests.
3. Replace diagnostic MACID helper bodies with runtime calls.
4. Run formatting, workspace tests, and strict OpenSpec validation.
