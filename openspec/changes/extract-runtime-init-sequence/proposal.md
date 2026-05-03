## Why

Runtime transport ownership is now in place, but the RTL8812AU init phase ordering is still hard-coded in `wfb-radio-diag`. The runtime crate should own the reusable init sequence definition so production callers and diagnostics execute the same phase profile.

## What Changes

- Add runtime RTL8812AU init phase and init order types.
- Add runtime phase sequence helpers for same-session initialization.
- Update diagnostic same-session init to use runtime phase ordering for the LLT/firmware ordering decision.
- Keep diagnostic reports and phase details unchanged.

## Capabilities

### New Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime crate defines RTL8812AU init phase sequencing.
- `userspace-usb-radio`: Initialization order is defined by runtime policy rather than diagnostic-only branching.

## Impact

- Affects runtime crate API and same-session init control flow.
- No register write, calibration, TX/RX, CLI, or report schema changes are intended.
