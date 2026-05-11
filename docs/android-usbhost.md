# Android USBHost Backend

Android support uses the same direct-radio runtime as macOS. The Android app
layer owns USB permission and device open; Rust owns RTL8812AU init, register
access, bulk RX/TX, calibration, WFB forwarding, health, and reports.

## Transport Decision

Use app-owned `UsbDeviceConnection` calls through JNI:

```text
Android app
  -> UsbManager permission
  -> UsbDeviceConnection open
  -> claimInterface()
  -> resolve selected UsbEndpoint objects
  -> Rust JNI smoke transport
  -> UsbDeviceConnection.controlTransfer/bulkTransfer
  -> shared RTL8812AU init/RX/TX/runtime code
```

The app must keep the owning `UsbDeviceConnection` alive until the Rust radio
session exits. It must also keep the selected `UsbEndpoint` Java objects alive
while Rust is using them. The app should not issue unrelated Java-side
bulk/control transfers for the same interface while Rust owns a smoke/runtime
call.

The original file-descriptor/libusb wrapping path was tested first, but Pixel 7
Pro returned `Input/Output Error` from `libusb_wrap_sys_device` while Java
control transfers against the same open device succeeded. The Android runtime
config still carries fd metadata for validation/report compatibility, but the
active hardware path is direct JNI.

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

The harness opens the first attached AWUS036ACH (`0x0bda:0x8812`), passes the
live `UsbDeviceConnection` plus selected bulk endpoint objects into Rust, and
reads one RTL8812AU register through the Android JNI transport. It then runs
bounded bulk-IN reads through the runtime RX descriptor parser, full
RTL8812AU production init on the selected HT20 channel, monitor opmode receive
filter setup, and descriptor-prefixed TX submission through the production
bridge TX path. Register return values `0..255` are register values; RX return
values `0..N` are parsed frame counts; negative values are smoke error classes
documented in
`android/smoke-harness/README.md`.

When built with `INCLUDE_ANDROID_WFB_HELPERS=1`, the debug APK also packages
Android arm64 `wfb_tx`, `wfb_rx`, and `wfb_keygen` helper executables. Launching
the Activity with `--ez runManagedStreams true` runs an intent-gated managed
raw-application stream smoke: Rust starts the packaged WFB helpers, runs the
production bridge loop on the live Android USBHost session, sends raw UDP into
the local helper, forwards RF RX frames into the local aggregator helper, and
logs raw payload recovery counts.

Product Android packaging should own the app shell, foreground-service policy,
USB permission UX, key/asset provisioning, and session threading. For app
integration, use the local SDK AAR documented in
[Android SDK integration](android-sdk.md). For bench work,
`scripts/build-android-smoke-apk.sh` builds and signs a direct debug APK at
`target/android-smoke-apk/wfb-link-android-smoke-debug.apk`, and
`scripts/install-android-smoke-apk.sh` installs and launches it over `adb`.

### Bench Runbook

1. Enable Developer Options, USB debugging, and Wireless debugging on the phone.
2. Connect `adb` over Wi-Fi before occupying the phone's USB-C port with the
   radio adapter:

   ```sh
   adb mdns services
   adb connect PHONE_IP:5555
   ```

   If the phone only advertises `_adb-tls-connect._tcp`, use the pairing code
   shown by Android's Wireless debugging UI before `adb connect`.

3. Build, install, and launch the smoke APK:

   ```sh
   scripts/build-android-smoke-apk.sh
   scripts/install-android-smoke-apk.sh
   adb logcat -c
  adb shell am start -n com.arcedge.wfblink.smoke/.WfbUsbSmokeActivity
  ```

   To match a peer on another channel, pass `--ei channelNumber <channel>`.
   To run the managed helper-supervised smoke after the basic TX/RX smoke, use:

   ```sh
   INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-smoke-apk.sh
   adb install -r target/android-smoke-apk/wfb-link-android-smoke-debug.apk
   adb shell am start \
     -n com.arcedge.wfblink.smoke/.WfbUsbSmokeActivity \
     --ei channelNumber 161 \
     --ez runManagedStreams true \
     --ez managedOnly true \
     --ei managedDurationMs 15000 \
     --ei managedPayloadCount 20 \
     --ei managedPayloadIntervalMs 20
   ```

   For longer validation, `scripts/run-android-managed-soak.sh` launches the
   same Activity with a configurable duration and Android TX payload interval,
   then writes request metadata, filtered logcat, completion lines, and crash
   scans into `/tmp/wfb-link-android-managed-soak-*`.

   The managed smoke expects a GS key at `/data/local/tmp/wfb-link/gs.key` and
   a Linux peer using the matching `drone.key`. The smoke APK declares
   `INTERNET` permission because Android requires it for localhost UDP sockets.

4. Unlock the phone. Attach the AWUS036ACH through USBHost/OTG, preferably via
   a powered USB-C hub, and accept the Android USB permission prompt.
5. Read the smoke result:

   ```sh
   adb logcat -d -s WfbUsbSmoke
   adb shell dumpsys usb
   ```

`dumpsys usb` should show `connected=true`, a host connection count above zero,
and the smoke app's device filter for `vendor_id=3034`, `product_id=34834`.
If it still reports `connected=false`, Android has not electrically enumerated
the adapter yet; check OTG direction, hub power, and cable orientation before
debugging Rust.

## Current Status

Implemented:

- Android USBHost runtime and production service config.
- Endpoint validation for the AWUS036ACH bulk layout.
- Runtime open-plan validation for fd, VID/PID, endpoint shape, and unsupported
  selectors.
- Direct JNI control and bulk transfers through app-owned `UsbDeviceConnection`
  and `UsbEndpoint` objects.
- Source-only Android USB permission, register/RX/init/TX smoke harness.
- Direct SDK/NDK debug APK build script for the smoke harness.
- Local Android SDK AAR build with Java USB handoff/config/session/result
  classes, named stream config, product-facing JNI symbol names, native library
  packaging, and direct plus Gradle-style consumer compile smokes.
- Android arm64 WFB-NG codec helper build and debug APK packaging path for
  managed-stream smoke validation.
- Pixel 7 Pro short-range RF smoke against `drone-2f389` on channel 161 HT20:
  Linux monitor captures saw Android-origin synthetic WFB headers, and Android
  post-init RX parsed Linux `wfb_tx` data frames including WFB-like MCS1 frames
  after applying the production monitor opmode receive filter.
- Pixel 7 Pro managed-stream smoke against `drone-2f389` on channel 161 HT20
  after the SDK facade cutover: Android submitted 41 WFB datagrams from 20 raw
  uplink payloads, forwarded 42 matching downlink WFB frames into packaged
  `wfb_rx`, decoded 20 raw downlink payloads, and the Linux peer decoded 19/20
  Android raw uplink payloads. A stale phone-side `gs.key` caused symmetric
  decrypt failures before refreshing `/data/local/tmp/wfb-link/gs.key` from the
  current paired key.

Pending:

- Product Gradle app or instrumentation target around the SDK AAR.
- Android target CI with NDK toolchain configured.
- Long-range Android managed-stream profile comparison against the macOS/Linux
  bench.
