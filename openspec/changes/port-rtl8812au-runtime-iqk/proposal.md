## Why

The macOS RTL8812AU path still depends on captured IQK constants, which is not
good enough for long-distance RF quality across adapters, temperature, antenna
loading, and channel changes. We now have a standalone IQK evidence command and
receiver telemetry, so the next step is to port the guarded runtime
`_phy_iq_calibrate_8812a` path instead of treating planted IQC values as final.

## What Changes

- Add a guarded runtime IQK calibration profile for RTL8812AU that follows the
  Linux/aircrack-ng 8812A sequence in bounded stages.
- Port the upstream IQK MAC/BB/RF/AFE backup and restore flow, TX/RX one-shot
  loops, IQC candidate selection, and TX/RX IQC fill helpers.
- Report per-path TX/RX IQK readiness, failures, retries, selected IQC values,
  restore status, affected registers, and USB counters.
- Keep the existing `current-default` and `rtl8812a-iqk-probe` profiles intact
  until receiver-backed evidence shows runtime IQK is safe and beneficial.
- Require explicit hardware-write authorization and bounded retries before any
  runtime IQK sequence can run.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `userspace-usb-radio`: Adds a guarded RTL8812AU runtime IQK calibration path
  that can execute the Linux 8812A IQK sequence in a retained userspace USB
  session.
- `rf-quality-and-range`: Promotes IQK from stop-gap evidence to reportable
  runtime calibration when the guarded sequence runs and cleanup succeeds.

## Impact

- `crates/wfb-radio-diag`: TX calibration profile enum, IQK runtime state
  machine, BB/RF masked write helpers, report structs, tests, and human output.
- `scripts/run-rf-quality-close-range.sh`: optional profile selection remains
  the validation harness for close-range A/B runs.
- `docs/`: calibration-state and RF-quality guidance for when runtime IQK is
  considered usable.
- Hardware: the first implementation stages will run on the attached
  AWUS036ACH only with explicit write authorization, short retry bounds, and a
  post-run WFB smoke.
