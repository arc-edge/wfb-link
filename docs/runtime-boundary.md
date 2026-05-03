# Runtime Boundary

`wfb-radio-runtime` is the production-facing runtime layer for native WFB radio operation. It starts with stable policy that multiple binaries must share, then can absorb hardware session orchestration in later slices.

## Runtime-Owned Now

- TX calibration profile identity.
- Calibration profile safety policy, including whether a profile requires live register write authorization.
- Calibration class policy used by RF-quality reports and future runtime state.
- Unified runtime USB transport over libusb claims and macOS USBHost retained sessions.
- macOS IOUSBHost direct-control and retained-session transport wrappers.
- macOS USBHost endpoint validation, synthetic adapter metadata, and retained-session open policy.
- Runtime libusb adapter selection/claim policy for bridge, init, TX, RX, and IQK runtime transport paths.
- macOS RTL8812AU register and bulk-transfer trait implementations.
- RTL8812AU same-session init phase identities and default/Linux-order phase sequencing policy.

## Still Diagnostic-Owned

- RTL8812AU init phase execution, table loading, and diagnostic phase reporting.
- Runtime IQK/LCK register execution.
- WFB TX/RX traffic loops and RF-quality automation.
- CLI parsing and human-facing diagnostic reports.
- Legacy standalone smoke commands that still claim `ClaimedUsbDevice` directly while their report shapes remain diagnostic-only.

## Migration Order

1. Keep moving stable policy and configuration into `wfb-radio-runtime`.
2. Move libusb transport open policy and adapter/session configuration once its runtime error model is settled.
3. Move RTL8812AU init phase execution behind runtime APIs while keeping `wfb-radio-diag` as a harness that calls those APIs.
4. Move TX/RX loop orchestration behind runtime APIs.
5. Move calibration execution once IQK/LCK parity is stable enough to expose as runtime behavior rather than diagnostic experiment.
6. Expose production telemetry types for RSSI/SNR/MCS, calibration state, USB transfer counters, queue state, and WFB flow counters.

The diagnostic binary should continue to be able to run every bring-up probe, but it should stop being the only place where production behavior exists.
