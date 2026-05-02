## Why

Close-range macOS-to-Linux WFB flow is working, but the current path is still bench-grade. Production use, especially long-distance WFB, needs stronger Linux peer automation, explicit RF calibration behavior, and receiver metadata that distinguishes real signal evidence from fallbacks.

The highest-risk RF gaps are no longer basic USB ownership or descriptor submission. They are calibration parity with the Linux RTL8812AU driver, repeatability of the Linux peer harness, and evidence quality: RSSI/SNR/rate/bandwidth/MCS should be reported with source/confidence so field results can be trusted.

## What Changes

- Harden the Linux peer automation so missing tools such as `iw`, `tcpdump`, or `docker` are detected, reported, and either treated as optional or fail-fast depending on the run policy.
- Add explicit command/report support for a targeted Linux-parity calibration profile that applies known Linux-final RF/TX calibration register overrides and labels them separately from full runtime IQK/LCK.
- Add an LCK/IQK porting path that starts with safe, reportable register/RF operations and leaves distance-sensitive validation gates visible.
- Enrich RX frame metadata with PHY-status confidence, descriptor/driver-info evidence, rate/bandwidth fields, and nullable signal/noise/SNR fields instead of only a fallback RSSI.
- Update RF-quality reports and documentation so production-readiness status is based on Linux-comparable receiver outcomes and calibration evidence, not USB submission success.

## Capabilities

### Modified Capabilities

- `rf-quality-run-automation`: Linux peer preflight, command discovery, channel-state evidence, and artifact collection become explicit and reportable.
- `userspace-usb-radio`: RX metadata and calibration profile behavior become richer and source-labeled.
- `rf-quality-and-range`: RF-quality reports gain production-readiness evidence for calibration parity and receiver metadata confidence.

## Impact

- Affected scripts: `scripts/run-rf-quality-close-range.sh`.
- Affected crates: `radio-core`, `wfb-radio-diag`, and `wfb-bridge`.
- Affected commands: `bridge-tx-listen`, `bridge-run`, `bridge-tx-bench`, `rx-scan`, and `rf-quality-report`.
- Live RF validation can be run close-range now; longer-distance/profile acceptance remains blocked until the hardware is placed in a real range or attenuation geometry.
