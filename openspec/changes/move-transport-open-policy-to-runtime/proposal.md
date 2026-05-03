## Why

The runtime crate owns the macOS USBHost transport and the unified USB transport enum, but diagnostic code still owns macOS endpoint validation, synthetic adapter metadata, and retained-session opening policy. Moving those decisions into runtime is the next step toward a production runtime API that can open hardware without depending on diagnostic internals.

## What Changes

- Add runtime macOS USBHost configuration and open-result types.
- Move macOS endpoint validation and derived endpoint layout into `wfb-radio-runtime`.
- Move IOUSBHost synthetic adapter metadata construction into `wfb-radio-runtime`.
- Move macOS retained-session opening policy into `wfb-radio-runtime`.
- Update diagnostic commands to translate CLI args into runtime config and map runtime errors into diagnostic reports.

## Capabilities

### New Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime crate provides transport open/config policy for macOS USBHost sessions.
- `userspace-usb-radio`: macOS direct-control runtime access validates endpoints and opens retained sessions through the runtime library.

## Impact

- Affects runtime crate API and diagnostic live hardware open paths.
- No CLI flag or JSON schema changes are intended.
- No RF init, calibration, TX, or RX behavior changes are intended.
