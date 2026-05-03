## Why

`wfb-radio-runtime` now owns transport opening and RTL8812AU init phase ordering, but the phase register execution still lives in `wfb-radio-diag`. Production bridge/runtime code should be able to execute reusable init steps without depending on diagnostic command/report code.

## What Changes

- Add runtime register-execution counters and errors.
- Move the TX scheduler tail register sequence into runtime.
- Move monitor receive filter and monitor opmode register programming into runtime.
- Keep diagnostic report schemas and command behavior unchanged by converting runtime evidence back into existing report structs.

## Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime crate can execute initial RTL8812AU register phases, not just describe their order.
- `userspace-usb-radio`: Diagnostic commands use runtime-owned execution for migrated phases.

## Impact

- Affects runtime crate API and diagnostic helper wiring.
- No intended USB wire change for migrated phases.
- No intended CLI or JSON schema changes.
