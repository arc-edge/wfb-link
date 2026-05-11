# WFB Link Android SDK

This source tree builds the reusable Android integration artifact for WFB Link.
It is separate from `android/smoke-harness`, which remains the hardware
validation app.

The SDK expects the host app to own Android USB permission and pass an opened
`UsbDeviceConnection` plus selected `UsbEndpoint` objects into
`WfbLinkManager`. Product apps should call `startManagedStreams(...)` with a
caller-owned `ExecutorService`; the returned `WfbManagedStreamsSession` exposes
status snapshots, cooperative stop request, typed health counters, RX signal
summary, and result/error delivery.
On Android, run the session from a foreground service or equivalent foreground
execution context so app-owned loopback UDP is not blocked by doze/background
network policy.

SDK sessions default to product mode: the app owns raw application UDP sockets.
Send uplink payloads to the configured TX stream local UDP port and bind the
configured RX stream local UDP port for downlink payloads. The smoke harness
explicitly enables validation traffic when it wants generated packet counts.

Build the local AAR with:

```bash
INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-sdk-aar.sh
```

The output is written to `target/android-sdk-aar/wfb-link-android-sdk-debug.aar`.

Run the external consumer compile smoke with:

```bash
scripts/build-android-sdk-consumer-smoke.sh
scripts/build-android-sdk-gradle-consumer-smoke.sh
```

See `docs/android-sdk.md` for the complete integration contract and
`docs/android-production-preflight.md` for the hardware and run checklist.
The Gradle consumer sample includes a compile-checked foreground service that
owns the SDK session and product raw UDP sockets.
