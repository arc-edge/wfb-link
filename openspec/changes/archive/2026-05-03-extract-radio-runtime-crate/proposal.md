## Why

Most of the production radio behavior now lives inside `wfb-radio-diag`, which makes the diagnostic binary both the hardware harness and the only implementation of runtime policy. The next step toward a real WFB runtime is to extract stable configuration, calibration, and safety decisions into a library crate that the bridge can own while the diagnostic CLI remains a bring-up and verification tool.

## What Changes

- Add a `wfb-radio-runtime` crate for stable runtime-facing radio policy and configuration types.
- Move TX calibration profile classification and write-safety policy out of `wfb-radio-diag` and into the runtime crate.
- Wire the existing diagnostic commands through the runtime crate without changing command-line behavior.
- Document the runtime boundary so future work can migrate USB session ownership, TX/RX loops, telemetry, and Linux-parity calibration out of the diagnostic binary in controlled slices.
- No breaking CLI or report schema changes are intended in this first extraction.

## Capabilities

### New Capabilities
- `radio-runtime-library`: Defines the production runtime library boundary for radio link configuration, calibration policy, safety gates, and future TX/RX runtime ownership.

### Modified Capabilities
- `userspace-usb-radio`: Clarifies that stable runtime radio policy must be available from a reusable runtime library, not only from diagnostic commands.

## Impact

- Affects workspace crate layout, `wfb-radio-diag` calibration policy calls, runtime-facing documentation, and OpenSpec coverage.
- Adds an internal Rust dependency from `wfb-radio-diag` to `wfb-radio-runtime`.
- Leaves hardware initialization, macOS USBHost transport, IQK/LCK execution, WFB traffic loops, and RF-quality automation in `wfb-radio-diag` for now.
