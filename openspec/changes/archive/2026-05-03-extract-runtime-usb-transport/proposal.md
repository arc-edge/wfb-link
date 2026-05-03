## Why

The project now has runtime-owned platform transport code, but `wfb-radio-diag` still owns the enum that unifies libusb and macOS USBHost sessions for register and bulk transfers. Moving that transport abstraction into `wfb-radio-runtime` lets future runtime APIs accept one production transport type instead of depending on diagnostic internals.

## What Changes

- Add a `RuntimeUsbTransport` enum to `wfb-radio-runtime`.
- Implement RTL8812AU register transport and USB bulk transfer traits for the runtime transport.
- Update diagnostic live paths to use the runtime transport type.
- Preserve existing libusb and macOS USBHost behavior.

## Capabilities

### New Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime crate provides the unified live USB transport abstraction used by register, TX, and RX code.
- `userspace-usb-radio`: Live USB access is represented by the runtime transport type rather than a diagnostic-only enum.

## Impact

- Affects runtime crate API and diagnostic transport construction sites.
- No CLI or report schema changes.
- No hardware behavior changes.
