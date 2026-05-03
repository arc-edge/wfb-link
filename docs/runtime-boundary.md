# Runtime Boundary

`wfb-radio-runtime` is the production-facing runtime layer for native WFB radio operation. It starts with stable policy that multiple binaries must share, then can absorb hardware session orchestration in later slices.

## Runtime-Owned Now

- TX calibration profile identity.
- Calibration profile safety policy, including whether a profile requires live register write authorization.
- Calibration class policy used by RF-quality reports and future runtime state.

## Still Diagnostic-Owned

- macOS IOUSBHost retained-session implementation.
- RTL8812AU init sequencing and table loading.
- Runtime IQK/LCK register execution.
- WFB TX/RX traffic loops and RF-quality automation.
- CLI parsing and human-facing diagnostic reports.

## Migration Order

1. Keep moving stable policy and configuration into `wfb-radio-runtime`.
2. Move a reusable adapter/session abstraction once the macOS USBHost path and libusb fallback API shape is settled.
3. Move TX/RX loop orchestration behind runtime APIs while keeping `wfb-radio-diag` as a harness that calls those APIs.
4. Move calibration execution once IQK/LCK parity is stable enough to expose as runtime behavior rather than diagnostic experiment.
5. Expose production telemetry types for RSSI/SNR/MCS, calibration state, USB transfer counters, queue state, and WFB flow counters.

The diagnostic binary should continue to be able to run every bring-up probe, but it should stop being the only place where production behavior exists.
