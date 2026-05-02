## Why

Long-distance WFB operation needs RF quality close to the Linux RTL8812AU driver, and the current macOS path still relies on planted IQK constants captured from one adapter. Before porting the full upstream IQK sweep, we need a safe staged path that inventories the same MAC/BB/AFE/RF state Linux backs up and reports enough evidence to validate each calibration step on hardware.

## What Changes

- Add an opt-in RTL8812AU IQK marker profile that records IQK staging state without running the destructive calibration sweep.
- Explicitly defer profile-time IQK hardware reads, RF-serial IQK backup reads, and page-C1 IQK latches from the live pre-TX profile after hardware smoke showed these reads can perturb WFB recovery.
- Label IQK probe output separately from LCK runtime calibration and planted Linux-parity constants so RF-quality reports do not overstate calibration maturity.
- Harden short RF-quality smoke runs so WFB receiver recovery can be interpreted even when the exact emitted datagram count differs slightly from the theoretical FEC-derived count.
- Document the staged path toward full Linux-parity IQK, including which pieces are safe now and which remain blocked on a full port and longer-distance validation.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `userspace-usb-radio`: Adds an explicit read-only RTL8812AU IQK staging profile and structured IQK register/RF-state diagnostics.
- `rf-quality-and-range`: Adds RF-quality report requirements for distinguishing IQK probe evidence, planted IQK constants, and future runtime IQK calibration.

## Impact

- `crates/wfb-radio-diag`: new TX calibration profile enum value, IQK probe report structures, upstream register inventory constants, and guarded hardware readback logic.
- `scripts/run-rf-quality-close-range.sh`: short-run WFB datagram accounting/reporting hardening.
- `docs/`: calibration-state and RF-quality baseline notes for IQK probe semantics and remaining Linux-parity gaps.
- OpenSpec artifacts for the staged IQK calibration implementation.
