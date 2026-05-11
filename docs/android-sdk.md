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

For a local app module, copy the generated AAR to the app's `libs/` directory
and depend on it directly:

```gradle
dependencies {
    implementation files("libs/wfb-link-android-sdk-debug.aar")
}
```

The repository includes a Gradle-style consumer sample at
`android/sdk-gradle-consumer`. Compile-check it without requiring Gradle:

```sh
scripts/build-android-sdk-gradle-consumer-smoke.sh
```

The app should copy or generate the paired `gs.key` and Realtek init assets into
app-readable paths before starting a session. The `gs.key` must match the
peer's current `drone.key`; a stale phone-side key presents as symmetric
`Unable to decrypt session key` errors even when RF TX/RX counters look healthy.
The SDK does not manage app storage, foreground service policy, or USB
permission UX.

## API Shape

The host app owns Android USB permission and passes the live USB objects into
the SDK. Product code should use named managed streams and the lifecycle
session helper:

```java
ExecutorService executor = Executors.newSingleThreadExecutor();

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
                .addStream(
                        WfbManagedStream.tx("control-up", 6, 15606)
                                .txProfile(WfbManagedTxProfile.of(20, 0, 2, 4))
                                .build())
                .addStream(WfbManagedStream.rx("video-down", 4, 15904).build())
                .build();

WfbManagedStreamsSession session =
        new WfbLinkManager()
                .startManagedStreams(
                        config,
                        executor,
                        new WfbManagedStreamsCallback() {
                            @Override
                            public void onStatusChanged(WfbManagedStreamsStatus status) {}

                            @Override
                            public void onCompleted(WfbManagedStreamsResult result) {}

                            @Override
                            public void onFailed(WfbLinkException error) {}
                        });
```

`startManagedStreams` validates the config, runs the existing blocking native
runtime on the caller-provided `ExecutorService`, and returns a
`WfbManagedStreamsSession`. `session.status()` returns immutable snapshots,
`session.await()` blocks for the final result, and `session.requestStop()`
records a cooperative stop request. The current native USB runtime cannot be
force-interrupted from Java; use bounded `durationMs` values and treat stop as
best-effort until the runtime reaches its next normal exit.

`runManagedStreamsBlocking` remains available for tests and callers that
already own their worker thread. In either mode, keep the
`UsbDeviceConnection` and selected `UsbEndpoint` objects alive until the
session finishes, and do not issue unrelated Java-side transfers on the same
interface while the SDK owns it.

## Named Streams

The Android managed path currently supports exactly one raw TX stream and one
raw RX stream. The Java config accepts the product-shaped stream model now and
maps the supported shape onto the proven native runtime ports:

- TX stream: raw app UDP into Android, then `wfb_tx`, then radio.
- RX stream: radio, then `wfb_rx`, then raw app UDP out of Android.

Startup rejects duplicate stream names, duplicate local UDP ports, unsupported
payload kinds, multiple TX/RX streams, missing TX/RX streams, and mismatched
link IDs with typed `WfbLinkException` codes before live USB execution.
`WFB_DISTRIBUTOR_DATAGRAM` payloads and N-stream Android multiplexing are
reserved for a later native-runtime expansion.

## Validation

Compile an external consumer against the AAR:

```sh
scripts/build-android-sdk-consumer-smoke.sh
scripts/build-android-sdk-gradle-consumer-smoke.sh
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
  --ez managedOnly true \
  --ei managedDurationMs 15000 \
  --ei managedPayloadCount 20 \
  --ei managedPayloadIntervalMs 20
```

For longer hardware validation, `scripts/run-android-managed-soak.sh` runs the
same managed path with configurable `DURATION_MS`, `PAYLOAD_COUNT`, and
`PAYLOAD_INTERVAL_MS`, then captures filtered logcat and completion/crash
evidence into a timestamped directory.

If `dumpsys usb` reports `connected=false`, Android has not electrically
enumerated the adapter yet; check OTG direction, hub power, cable orientation,
and phone unlock state before debugging SDK code.

## Current Limitations

- Local AAR only. Maven/registry publishing is intentionally deferred.
- Android arm64 only.
- Caller-owned foreground service, lifecycle, assets, keys, and USB permission.
- Managed stream config currently maps one named uplink raw producer and one
  named downlink raw receiver into the Android native path. Additional named
  stream pairs are modeled in Java but rejected until native Android
  multiplexing is added.
- Stop requests are cooperative. They update Java session state immediately but
  native USB execution exits at its bounded runtime stop point.
- RF-quality and long-range Android validation must be rerun whenever the
  antenna/phone/hub setup changes.
