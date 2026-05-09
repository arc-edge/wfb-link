# Android USBHost Backend

Android support uses the same direct-radio runtime as macOS. The Android app
layer owns USB permission and device open; Rust owns RTL8812AU init, register
access, bulk RX/TX, calibration, WFB forwarding, health, and reports.

## Transport Decision

Use file-descriptor handoff rather than per-transfer JNI calls:

```text
Android app
  -> UsbManager permission
  -> UsbDeviceConnection open
  -> getFileDescriptor()
  -> wfb-radio-runtime AndroidUsbHostConfig { device_fd, endpoints, VID/PID }
  -> rusb/libusb_wrap_sys_device
  -> existing RTL8812AU control + bulk traits
```

The app must keep the owning `UsbDeviceConnection` alive until the Rust radio
session exits. The app should not also run Java-side bulk/control transfers for
the same interface while Rust owns the session.

Rust/libusb claims the configured interface after wrapping the fd. The initial
Android harness should therefore open the device and hand off the fd before any
long-lived Java-side interface ownership.

## Runtime Config

The service/runtime Android transport section is:

```toml
[adapter]
vid = 3034
pid = 34834

[android_usbhost]
enabled = true
device_fd = 42
interface_number = 0
bulk_in_endpoint = 129
bulk_out_endpoint = 2
bulk_out_endpoint_count = 3
```

The `device_fd` normally comes from the app at runtime, not from a checked-in
profile. VID/PID metadata is still required so runtime reports and supported
adapter checks remain stable even before descriptor querying is added to the
Android path.

## Build Requirement

The Rust target can be installed with:

```sh
rustup target add aarch64-linux-android
```

`libusb1-sys` also needs an Android NDK compiler on `PATH`, such as
`aarch64-linux-android-clang`, or equivalent Cargo target linker settings.
Without the NDK compiler, `cargo check --target aarch64-linux-android` stops in
the vendored libusb build before the Android Rust code can be fully checked.

## Smoke Harness

The first USB permission and fd-handoff smoke is split between:

- `crates/wfb-android-smoke`: JNI-facing Rust `cdylib` entry point.
- `android/smoke-harness`: source-only Android Activity, manifest, and USB
  device filter.

The harness opens the first attached AWUS036ACH (`0x0bda:0x8812`), passes
`UsbDeviceConnection.getFileDescriptor()` into Rust, and reads one RTL8812AU
register through the Android fd-backed transport. Return values `0..255` are
register values; negative values are smoke error classes documented in
`android/smoke-harness/README.md`.

This is intentionally not a complete Gradle project yet. Product Android
packaging should own the app shell, USB permission UX, and native library
loading policy, then reuse the Rust smoke entry point during bring-up.

## Current Status

Implemented:

- Android USBHost runtime and production service config.
- Endpoint validation for the AWUS036ACH bulk layout.
- Runtime open-plan validation for fd, VID/PID, endpoint shape, and unsupported
  selectors.
- fd-backed libusb wrapping for Android control and bulk transfers.
- Source-only Android USB permission and register-read smoke harness.

Pending:

- Packaged Android app or instrumentation target around the smoke harness.
- Android target CI with NDK toolchain configured.
- Hardware smoke: descriptor/register read, RX-only parsing, single TX, then
  bounded bidirectional WFB distributor datagrams against the Linux peer.
