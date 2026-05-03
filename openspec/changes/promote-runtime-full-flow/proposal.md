## Why

The live TX/RX paths now use `RuntimeRadioSession`, but retained same-session init and calibration selection still depend on diagnostic command plumbing. Production WFB flow needs a runtime-owned init boundary so diagnostics become wrappers instead of the implementation owner.

## What Changes

- Add a runtime-facing RTL8812AU init API that executes same-session initialization through a `RuntimeRadioSession` and returns report-neutral phase/counter evidence.
- Move retained same-session init execution out of diagnostic-only command code while preserving existing diagnostic JSON/report outputs as wrappers.
- Add a thin production runtime command for initialized TX/RX flow that uses the runtime init API and runtime session I/O.
- Make runtime calibration profile selection explicit for production callers, including default, targeted parity, captured IQK/LCK, and runtime IQK policy labels.
- Keep long-distance validation profiles and receiver placement-dependent tests deferred until hardware geometry can be controlled.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `radio-runtime-library`: Runtime library owns same-session init execution and production calibration policy.
- `userspace-usb-radio`: RTL8812AU initialization is callable by production runtime code without depending on diagnostic command structures.
- `wfb-radio-runtime`: Production-facing TX/RX flow can initialize and use the radio through runtime APIs.
- `wfb-radio-bridge`: Bridge commands can use the runtime init API while preserving existing behavior.
- `rf-quality-and-range`: Calibration policy labels distinguish production-safe defaults from experimental IQK/LCK profiles.

## Impact

- Affected crates: `wfb-radio-runtime`, `wfb-radio-diag`, and any new production CLI crate or command module added for WFB runtime flow.
- Affected APIs: runtime init config/result structs, runtime calibration profile policy, and diagnostic adapters around init reports.
- Affected systems: local macOS RTL8812AU bring-up, Linux receiver interop, and future long-distance RF quality runs.
