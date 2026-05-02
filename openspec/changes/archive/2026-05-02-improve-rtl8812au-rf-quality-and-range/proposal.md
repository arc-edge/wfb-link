## Why

The native macOS RTL8812AU path can now run WFB-ng over the AWUS036ACH, but long-distance operation depends on RF quality, calibrated power behavior, and repeatable range evidence rather than basic packet flow. The current implementation still relies on captured or planted RF/TX calibration values in places, so the next milestone is to make the transmit chain measurable, tunable, and comparable to the Linux baseline before treating it as field-ready.

## What Changes

- Add RF-quality diagnostics that compare macOS and Linux RTL8812AU behavior using the same adapters, antennas, channels, bandwidths, keys, rates, and payloads.
- Replace ad hoc/planted TX power behavior with EFUSE-derived per-path/per-rate TX power programming where the Linux driver provides a clear source of truth.
- Add calibration evidence and controls for IQK/LCK/thermal-sensitive TX behavior, starting with instrumentation and safe approximations before porting heavier runtime calibration.
- Add repeatable range-test reports for close-range, attenuated/stepped, and outdoor/long-distance runs.
- Add WFB-focused acceptance metrics for payload recovery, FEC loss, receiver RSSI/noise metadata when available, throughput, CPU, TX descriptor shape, power settings, channel state, and calibration state.
- Keep wide-bandwidth and VHT claims behind explicit evidence; verify 20 MHz long-range quality first, then use separate evidence before claiming actual wide-PPDU range gains.

## Capabilities

### New Capabilities

- `rf-quality-and-range`: Defines calibrated RTL8812AU RF behavior, Linux-baseline comparison, RF-quality reporting, and range-test acceptance criteria for WFB-ng operation on macOS.

### Modified Capabilities

- None.

## Impact

- Affected crates: `radio-core`, `wfb-radio-diag`, and potentially `wfb-bridge` report surfaces.
- Affected commands: `efuse-dump`, `init`, `bridge-tx-bench`, `bridge-tx-listen`, `bridge-run`, `rx-scan`, and TX status/reporting helpers.
- New or expanded documentation for bench setup, Linux-baseline captures, attenuation/range procedure, and interpreting RF-quality reports.
- No intentional breaking changes to existing bounded diagnostics; new controls remain explicit, guarded, and report-backed.
