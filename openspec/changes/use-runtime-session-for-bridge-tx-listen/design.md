## Context

The listener path has additional pre-TX diagnostics and status sampling, so this migration keeps those report-heavy register helpers in diagnostics while moving only the actual radio TX submission onto `RuntimeRadioSession`.

## Goals / Non-Goals

**Goals:**

- Use runtime session TX submission in `bridge-tx-listen`.
- Preserve pre-TX init/calibration/status behavior.
- Preserve report shape and counters.

**Non-Goals:**

- Do not migrate combined `bridge-run` yet.
- Do not move diagnostic pre-TX register mutation helpers.

## Migration Plan

1. Convert listener open path to a runtime session.
2. Route listener submit loop through `BridgeTxSessionRadio`.
3. Run formatting, workspace tests, and strict OpenSpec validation.
