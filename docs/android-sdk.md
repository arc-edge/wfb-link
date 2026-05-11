# Android SDK Integration

The Android SDK is the product-facing wrapper around the Android USBHost radio
path. It packages Java API classes, the Rust JNI library, and optionally the
Android arm64 WFB-NG helper executables into a local AAR.

## Build

Install the Android SDK/NDK and Rust target:

```sh
rustup target add aarch64-linux-android
```

Build the local AAR:

```sh
scripts/build-android-sdk-aar.sh
```

Include packaged `wfb_tx`, `wfb_rx`, and `wfb_keygen` helpers when the app wants
SDK-supervised managed raw streams:

```sh
INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-sdk-aar.sh
```

The artifact is written to:

```text
target/android-sdk-aar/wfb-link-android-sdk-debug.aar
```

The AAR currently contains:

- `classes.jar` with `com.arcedge.wfblink.sdk`.
- `AndroidManifest.xml` declaring USB host and `INTERNET`.
- `res/xml/wfb_usb_filter.xml` for RTL8812AU USB attach filtering.
- `jni/arm64-v8a/libwfb_android.so`.
- Optional `libwfb_tx_exec.so`, `libwfb_rx_exec.so`, and
  `libwfb_keygen_exec.so` helper executables.

## Local Gradle Consumption

For a local app module:

```gradle
dependencies {
    implementation files("libs/wfb-link-android-sdk-debug.aar")
}
```

The app should copy or generate the paired `gs.key` and Realtek init assets into
app-readable paths before starting a session. The SDK does not manage app
storage, foreground service policy, or USB permission UX.

## API Shape

The host app owns Android USB permission and passes the live USB objects into
the SDK:

```java
WfbUsbHandoff usb =
        new WfbUsbHandoff(
                connection,
                bulkInEndpoint,
                bulkOutEndpoint,
                connection.getFileDescriptor(),
                device.getVendorId(),
                device.getProductId(),
                0,
                0x81,
                0x02,
                3);

WfbManagedStreamsConfig config =
        WfbManagedStreamsConfig.builder(context, usb)
                .keyPath(context.getFilesDir() + "/gs.key")
                .initAssets(
                        context.getFilesDir() + "/rtl8812aefw.bin",
                        context.getFilesDir() + "/halhwimg8812a_mac.c",
                        context.getFilesDir() + "/halhwimg8812a_bb.c",
                        context.getFilesDir() + "/halhwimg8812a_rf.c")
                .channelNumber(161)
                .durationMs(15000)
                .payloadCount(20)
                .build();

WfbManagedStreamsResult result =
        new WfbLinkManager().runManagedStreamsBlocking(config);
```

`runManagedStreamsBlocking` is a long-running native call. Run it from an
app-owned worker thread or foreground service. Keep the `UsbDeviceConnection`
and selected `UsbEndpoint` objects alive until it returns, and do not issue
unrelated Java-side transfers on the same interface while the SDK owns the
session.

## Validation

Compile an external consumer against the AAR:

```sh
scripts/build-android-sdk-consumer-smoke.sh
```

Build the smoke APK, which now exercises the same SDK facade for managed
streams while keeping register/RX/init/TX diagnostic JNI entry points:

```sh
scripts/build-android-smoke-apk.sh
```

Run the managed hardware smoke when Android has enumerated the RTL8812AU
adapter and a matching Linux WFB peer is on the selected channel:

```sh
adb shell am start \
  -n com.arcedge.wfblink.smoke/.WfbUsbSmokeActivity \
  --ei channelNumber 161 \
  --ez runManagedStreams true \
  --ei managedDurationMs 15000 \
  --ei managedPayloadCount 20
```

If `dumpsys usb` reports `connected=false`, Android has not electrically
enumerated the adapter yet; check OTG direction, hub power, cable orientation,
and phone unlock state before debugging SDK code.

## Current Limitations

- Local AAR only. Maven/registry publishing is intentionally deferred.
- Android arm64 only.
- Caller-owned foreground service, lifecycle, assets, keys, and USB permission.
- Managed stream config currently exposes one uplink raw producer and one
  downlink raw receiver through the SDK smoke path. Product-specific stream
  multiplexing should use the generic `wfb-link` stream contract as it is
  lifted into Android.
- RF-quality and long-range Android validation must be rerun whenever the
  antenna/phone/hub setup changes.
