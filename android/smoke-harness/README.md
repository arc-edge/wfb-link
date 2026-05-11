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
HT20 channel, the same monitor opmode receive filter used by production, and a
bounded RX descriptor read loop. An RX timeout means the adapter was idle and no
packet arrived before the bounded read deadline. The final smoke reruns init,
applies monitor opmode, submits three descriptor-prefixed null-data frames
through bulk OUT, and submits three synthetic WFB distributor datagrams through
the production bridge TX path.

To include Android WFB-NG codec helpers in the debug APK, build with:

```bash
INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-smoke-apk.sh
```

This cross-compiles Android arm64 `wfb_tx`, `wfb_rx`, and `wfb_keygen` into
`target/wfb-ng-android/bin` and packages them as extracted native executables.
With a paired GS key at `/data/local/tmp/wfb-link/gs.key`, the Activity can run
the managed raw-application stream smoke:

```bash
adb shell am start \
  -n com.arcedge.wfblink.smoke/.WfbUsbSmokeActivity \
  --ei channelNumber 161 \
  --ez runManagedStreams true \
  --ez managedOnly true \
  --ei managedDurationMs 15000 \
  --ei managedPayloadCount 20 \
  --ei managedPayloadIntervalMs 20
```

The managed smoke uses the reusable `com.arcedge.wfblink.sdk` Java facade. It
starts packaged `wfb_tx`/`wfb_rx` helpers inside the app, runs the production
bridge loop over Android USBHost, sends raw UDP into the TX helper, forwards RF
RX frames into the RX helper, and logs raw payload counters. It explicitly
enables SDK validation traffic; product SDK sessions leave that disabled so the
app can own the raw UDP sockets.

For soak runs, use the wrapper so the request, filtered logcat, completion
line, and crash scan land in one evidence directory:

```bash
DURATION_MS=1200000 PAYLOAD_INTERVAL_MS=100 \
  scripts/run-android-managed-soak.sh
```

`managedPayloadIntervalMs` controls the Android raw TX producer interval. A
larger value, such as `100`, better represents sparse control uplink while a
Linux peer sends steadier downlink traffic.
`managedOnly=true` skips the destructive diagnostic pre-smokes and runs the
production-shaped managed path once, matching SDK/integrator usage.

`scripts/install-android-smoke-apk.sh` pushes the current bench firmware and
Realtek table sources to `/data/local/tmp/wfb-link` before launch. Override
`ANDROID_SMOKE_FIRMWARE`, `ANDROID_SMOKE_MAC_SOURCE`, `ANDROID_SMOKE_BB_SOURCE`,
or `ANDROID_SMOKE_RF_SOURCE` when testing another checkout.

The earlier libusb `getFileDescriptor()` wrapping path failed on Pixel 7 Pro
with `libusb_wrap_sys_device` returning `Input/Output Error`; direct JNI
`UsbDeviceConnection` control/bulk calls are now the active smoke path.

Register return values `0..255` are register values. RX return values `0..N`
are parsed frame counts. Negative return values are error
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
