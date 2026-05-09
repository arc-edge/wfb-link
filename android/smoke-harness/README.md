# Android Smoke Harness Source

This directory contains the minimal Android harness source for the first
AWUS036ACH USBHost smoke. It is intentionally not a complete Gradle project;
`scripts/build-android-smoke-apk.sh` packages it directly with SDK build tools.

Packaging flow:

1. Run `scripts/build-android-smoke-apk.sh`.
2. Pair wireless `adb` with the phone.
3. Run `scripts/install-android-smoke-apk.sh` to install and launch the smoke
   Activity.
4. Attach the AWUS036ACH through USBHost/OTG, preferably with a powered hub.
5. Accept the Android USB permission prompt.
6. Use `WfbUsbSmokeActivity` to request USB permission, open the matching
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
