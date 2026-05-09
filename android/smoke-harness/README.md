# Android Smoke Harness Source

This directory contains source-only Android harness code for the first
AWUS036ACH USBHost smoke. It is intentionally not a complete Gradle project yet
because this checkout does not currently have Gradle or an Android NDK compiler
configured.

Expected packaging flow:

1. Build `wfb-android-smoke` as an Android `cdylib`.
2. Package the resulting native library into an Android app.
3. Use `WfbUsbSmokeActivity` to request USB permission, open the matching
   RTL8812AU device, obtain `UsbDeviceConnection.getFileDescriptor()`, and call
   the Rust JNI smoke entry point.

The first smoke reads one 8-bit RTL8812AU register through the Android fd-backed
transport. Return values `0..255` are register values. Negative return values
are error classes from `wfb-android-smoke`:

- `-1`: invalid JNI/app argument
- `-2`: transport open error
- `-3`: register read error

The app must keep the `UsbDeviceConnection` alive until the Rust call returns.
Do not claim the interface or issue Java-side control/bulk transfers while Rust
owns the smoke call.
