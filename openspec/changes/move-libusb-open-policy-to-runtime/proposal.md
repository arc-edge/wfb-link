## Why

The runtime crate owns macOS open policy and the unified USB transport, but libusb adapter selection and claiming are still implemented in `wfb-radio-diag`. Runtime callers need a single non-diagnostic path for opening supported adapters through either libusb or macOS USBHost.

## What Changes

- Add runtime libusb adapter selection and claim/open helpers.
- Add a backend enum and unified runtime open config for libusb or macOS USBHost.
- Update diagnostic live paths to construct runtime open configs instead of selecting/claiming libusb directly.
- Preserve existing CLI flags, diagnostic error codes, and report schemas.

## Capabilities

### New Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime crate provides cross-backend USB open policy.
- `userspace-usb-radio`: Live RTL8812AU USB access can be opened through runtime-owned libusb or macOS USBHost policy.

## Impact

- Affects runtime crate API and diagnostic open paths.
- No RF init, calibration, TX/RX descriptor, or report schema changes are intended.
