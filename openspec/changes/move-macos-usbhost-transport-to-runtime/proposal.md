## Why

The macOS USBHost retained-session transport is production runtime code, but it still lives inside `wfb-radio-diag`. Moving it behind `wfb-radio-runtime` makes the runtime crate responsible for platform radio access while leaving the diagnostic binary as a caller and harness.

## What Changes

- Move the macOS USBHost Rust wrapper module from `wfb-radio-diag` into `wfb-radio-runtime`.
- Move the Objective-C IOUSBHost shim and macOS build/link setup into `wfb-radio-runtime`.
- Re-export the transport module from the runtime crate on macOS.
- Update `wfb-radio-diag` to import the runtime transport instead of owning the module.
- Keep existing CLI behavior, report behavior, trait implementations, and tests unchanged.

## Capabilities

### New Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime crate owns platform transport modules needed by production radio operation.
- `userspace-usb-radio`: macOS direct-control transport is provided by the runtime library and remains usable by diagnostic commands.

## Impact

- Affects crate ownership, build scripts, macOS framework linking, and diagnostic imports.
- Adds `radio-core` and macOS shim build dependencies to `wfb-radio-runtime`.
- Removes the macOS USBHost build responsibility from `wfb-radio-diag`.
