# Android Consumer Quickstart

This is the handoff guide for another Android project, such as arc-uas, that
wants to consume WFB Link Android support.

## What To Consume

Use the Android SDK AAR. Do not depend on the smoke harness package.

Build the AAR from this repo:

```sh
INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-sdk-aar.sh
```

The build writes:

```text
target/android-sdk-aar/wfb-link-android-sdk-debug.aar
```

For first integration, copy that AAR into the product app, for example:

```text
arc-uas/android/app/libs/wfb-link-android-sdk-debug.aar
```

Then add:

```gradle
dependencies {
    implementation files("libs/wfb-link-android-sdk-debug.aar")
}
```

Release automation should eventually attach the AAR to each GitHub release, or
publish it to a Maven-compatible package repository. Until that exists, pin the
wfb-link Git commit used to build the local AAR and keep the binary under the
product branch/release artifact being tested.

## App Manifest

The product app needs USB host, network, and foreground-service declarations:

```xml
<uses-feature android:name="android.hardware.usb.host" android:required="true" />
<uses-permission android:name="android.permission.INTERNET" />
<uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE" />

<service
    android:name=".YourWfbLinkService"
    android:exported="false"
    android:foregroundServiceType="connectedDevice" />
```

Use `android/sdk-gradle-consumer` as the compile-checked reference. In
particular, mirror `WfbLinkForegroundService` rather than putting the session
inside an Activity.

## Required Assets

Before starting a session, the app must place these files in app-readable
storage and pass their paths to `WfbManagedStreamsConfig`:

- `gs.key`
- `rtl8812aefw.bin`
- `halhwimg8812a_mac.c`
- `halhwimg8812a_bb.c`
- `halhwimg8812a_rf.c`

The phone-side `gs.key` must match the Linux peer's `drone.key`. A stale key
usually appears as decrypt failures even when RF counters look active.

## Session Shape

The product app owns:

- Android USB permission UX.
- Opening and claiming the RTL8812AU USB interface.
- Keeping `UsbDeviceConnection` and endpoint objects alive until the session
  finishes.
- Foreground service lifecycle.
- Raw application UDP sockets.
- Pairing/key refresh and asset provisioning.

WFB Link owns:

- RTL8812AU init/RX/TX over Android USBHost.
- `wfb_tx` and `wfb_rx` helper supervision when the AAR includes helpers.
- WFB helper-to-radio UDP wiring.
- Runtime health/result reporting.

## Minimal Config

```java
WfbUsbHandoff usb =
        new WfbUsbHandoff(
                connection,
                bulkInEndpoint,
                bulkOutEndpoint,
                connection.getFileDescriptor(),
                device.getVendorId(),
                device.getProductId(),
                dataInterface.getId(),
                bulkInEndpoint.getAddress(),
                bulkOutEndpoint.getAddress(),
                bulkOutCount);

WfbManagedStreamsConfig config =
        WfbManagedStreamsConfig.builder(context, usb)
                .keyPath(files + "/gs.key")
                .initAssets(
                        files + "/rtl8812aefw.bin",
                        files + "/halhwimg8812a_mac.c",
                        files + "/halhwimg8812a_bb.c",
                        files + "/halhwimg8812a_rf.c")
                .channelNumber(161)
                .durationMs(15000)
                .validationTrafficEnabled(false)
                .addStream(
                        WfbManagedStream.tx("control-up", 6, 15606)
                                .txProfile(WfbManagedTxProfile.of(20, 0, 2, 4))
                                .build())
                .addStream(WfbManagedStream.rx("video-down", 4, 15904).build())
                .build();

WfbManagedStreamsSession session =
        new WfbLinkManager().startManagedStreams(config, executor, callback);
```

Product mode is the default. Keep `validationTrafficEnabled(false)` so the app
owns raw UDP:

- Send control/uplink payload bytes to `127.0.0.1:15606`.
- Bind/read video/downlink payload bytes from `127.0.0.1:15904`.

## Status And Health

Use these product-facing helpers:

```java
WfbManagedStreamsStatus status = session.status();
boolean running = session.isRunning();
String uiLabel = status.summaryLabel();

WfbManagedStreamsResult result = session.await();
boolean healthy = result.isProductionHealthy();
String finalLabel = result.health.summaryLabel();
boolean txDrops = result.health.hasTxDrops();
Long avgRssi = result.rxSignal.rssiDbm.average;
```

For UI and logs, surface:

- `status.summaryLabel()`
- `WfbLinkException.code`
- `result.health.summaryLabel()`
- `result.helperStatus`
- `result.health.hasTxDrops()`
- `result.rxSignal`
- `result.stopReason`

## Android Policy Requirement

Run the link from a foreground service or equivalent foreground execution
context. Android can block loopback UDP for an app UID in doze/background
policy, producing `SocketException: Operation not permitted`.

The smoke wrapper has debug policy modes for validation:

```sh
ANDROID_NETWORK_POLICY_MODE=preauthorize VALIDATION_TRAFFIC=false scripts/run-android-managed-soak.sh
ANDROID_NETWORK_POLICY_MODE=strict VALIDATION_TRAFFIC=false scripts/run-android-managed-soak.sh
ANDROID_NETWORK_POLICY_MODE=unchanged VALIDATION_TRAFFIC=false scripts/run-android-managed-soak.sh
```

The product app should not rely on adb allowlisting. It should use a foreground
service.

## Current Limits

- Android arm64 only.
- One managed raw TX stream and one managed raw RX stream are supported in the
  Android native path today.
- Additional Java stream objects are modeled, but Android native N-stream
  multiplexing is not implemented yet.
- Stop requests are cooperative; use bounded `durationMs` until force-stop is
  implemented at the native runtime layer.
- AAR publishing is not automated yet. Build/copy locally for first
  integration, then move to release-attached AAR or Maven publication.

## Validation Before Merge

From this repo:

```sh
INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-sdk-aar.sh
scripts/build-android-sdk-consumer-smoke.sh
scripts/build-android-sdk-gradle-consumer-smoke.sh
```

From the product app:

- Confirm the APK contains `libwfb_android.so`, `libwfb_tx_exec.so`, and
  `libwfb_rx_exec.so`.
- Start the foreground service with the adapter attached.
- Verify app-owned UDP TX/RX works with `validationTrafficEnabled(false)`.
- Verify no adb network allowlist is required for the product app.
