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
   RTL8812AU device, claim interface 0, and call the Rust JNI smoke entry
   point with the live `UsbDeviceConnection` plus selected bulk endpoint
   objects.

The smoke defaults to channel 36 HT20. To match a running peer without changing
the peer channel, launch it with an Intent extra, for example:

```bash
adb shell am start \
  -n com.arcedge.wfblink.smoke/.WfbUsbSmokeActivity \
  --ei channelNumber 161
```

The smoke first reads one 8-bit RTL8812AU register through Java
`controlTransfer`, then through Rust's JNI-backed transport. It then runs one
bounded bulk-IN read, followed by full RTL8812AU production init on the selected
HT20 channel and a second bounded RX descriptor read. An RX timeout means the
adapter was idle and no packet arrived before the bounded read deadline. The
final smoke reruns init, submits three descriptor-prefixed null-data frames
through bulk OUT, and submits three synthetic WFB distributor datagrams through
the production bridge TX path.

`scripts/install-android-smoke-apk.sh` pushes the current bench firmware and
Realtek table sources to `/data/local/tmp/wfb-link` before launch. Override
`ANDROID_SMOKE_FIRMWARE`, `ANDROID_SMOKE_MAC_SOURCE`, `ANDROID_SMOKE_BB_SOURCE`,
or `ANDROID_SMOKE_RF_SOURCE` when testing another checkout.

The earlier libusb `getFileDescriptor()` wrapping path failed on Pixel 7 Pro
with `libusb_wrap_sys_device` returning `Input/Output Error`; direct JNI
`UsbDeviceConnection` control/bulk calls are now the active smoke path.

Register return values `0..255` are register values. RX return values `0..N`
are parsed frame counts for that single read. Negative return values are error
classes from `wfb-android-smoke`:

- `-1`: invalid JNI/app argument
- `-2`: transport open error
- `-3`: register read error
- `-4`: RX bulk read timeout
- `-5`: RX bulk read error
- `-6`: native panic caught at JNI boundary

The app must keep the `UsbDeviceConnection` and endpoint objects alive until the
Rust call returns. Do not issue unrelated Java-side transfers while Rust owns
the smoke call.
