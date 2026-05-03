## Why

Full Linux-parity IQK is the largest remaining RF-quality gap, but live
pre-TX IQK deep reads have already perturbed WFB receiver recovery. We need a
standalone, non-WFB diagnostic that can collect the unsafe IQK backup and
page-latch surfaces on hardware before porting the destructive calibration
sweep.

## What Changes

- Add a guarded RTL8812AU standalone IQK diagnostic command that performs init,
  collects IQK MAC/BB, AFE, RF serial backup, page-C1 latch, and final IQK
  register evidence, then exits without starting WFB TX/RX.
- Restore page-select, HSSI/RF readback selectors, and RF serial state after
  the diagnostic so follow-up bridge runs start from a known state.
- Emit structured JSON and human-readable output that labels the diagnostic as
  evidence-only, not runtime IQK calibration.
- Document how to run the diagnostic on the hardware Mac and how to interpret
  its output before the full `phy_iq_calibrate_8812a` port.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `userspace-usb-radio`: Adds a standalone RTL8812AU IQK evidence diagnostic
  that can read deep IQK state outside the live WFB TX path.
- `rf-quality-and-range`: Clarifies that standalone IQK evidence may be
  attached to range-readiness analysis, but does not itself satisfy runtime IQK
  calibration.

## Impact

- `crates/wfb-radio-diag`: new diagnostic CLI path, report structures, guarded
  RF serial/page-C1 read helpers, tests, and human output.
- `docs/`: updates to calibration-state and diagnostic instructions.
- Hardware validation: one standalone diagnostic run on the attached
  AWUS036ACH, followed by a short WFB smoke to prove cleanup did not leave the
  adapter in a bad state.
