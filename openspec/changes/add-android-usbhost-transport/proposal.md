## Why

The first macOS flight proved the userspace RTL8812AU runtime and `wfb-link`
facade are viable in the field. Android should reuse the same product-facing
link contract instead of creating a separate app-specific radio API.

Android cannot use the macOS IOUSBHost transport or the Linux monitor-mode
driver path. It needs an Android USBHost transport that can run the same
runtime init, RX, TX, calibration, and WFB forwarding code after the app layer
obtains USB permission.

## What Changes

- Add an Android USBHost runtime config and backend selector beside libusb and
  macOS IOUSBHost.
- Add Android endpoint validation and synthetic RTL8812AU adapter metadata so
  runtime reports keep the same shape before native transfer code exists.
- Add service TOML and CLI mapping for `[android_usbhost]`.
- Implement the native bridge as an app-owned file descriptor handoff wrapped
  with libusb, so the existing RTL8812AU control and bulk transfer traits are
  reused.
- Document the Android support boundary and the follow-up native bridge work.

## Capabilities

### Modified Capabilities

- `radio-runtime-library`: add Android USBHost transport selection, endpoint
  validation, config serialization, fd handoff validation, and fd-backed
  control/bulk transfer behavior.
- `production-runtime`: allow the production service config to select the
  Android USBHost backend without changing stream, WFB, tunnel, or health
  contracts.

## Impact

- Affected crates: `wfb-radio-runtime`, `wfb-radio-service`.
- Affected docs: README, product integration, cross-platform interface,
  service config reference, runtime boundary.
- Follow-up implementation: Android app/NDK smoke harness, USB permission and
  fd handoff wiring, and hardware validation against the same WFB peer flows
  used by macOS.
