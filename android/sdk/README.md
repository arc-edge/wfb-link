# WFB Link Android SDK

This source tree builds the reusable Android integration artifact for WFB Link.
It is separate from `android/smoke-harness`, which remains the hardware
validation app.

The SDK expects the host app to own Android USB permission and pass an opened
`UsbDeviceConnection` plus selected `UsbEndpoint` objects into
`WfbLinkManager`. Product apps should call `startManagedStreams(...)` with a
caller-owned `ExecutorService`; the returned `WfbManagedStreamsSession` exposes
status snapshots, cooperative stop request, and result/error delivery.

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

See `docs/android-sdk.md` for the complete integration contract, manifest
requirements, USB handoff shape, named stream config, asset/key provisioning,
validation commands, and current limitations.
