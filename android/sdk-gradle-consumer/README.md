# WFB Link Android Gradle Consumer

This sample is a standard Android app layout that consumes the generated local
WFB Link SDK AAR from `app/libs/wfb-link-android-sdk-debug.aar`.

Build the AAR first:

```sh
INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-sdk-aar.sh
mkdir -p android/sdk-gradle-consumer/app/libs
cp target/android-sdk-aar/wfb-link-android-sdk-debug.aar android/sdk-gradle-consumer/app/libs/
```

The source intentionally imports only `com.arcedge.wfblink.sdk`, not the smoke
harness package. It demonstrates USB permission handoff, endpoint selection,
named managed stream config, foreground-service startup, cooperative stop
request, raw control uplink UDP send, raw video downlink UDP receive, and typed
result/error callback handling. `WfbLinkForegroundService` is the production
shape: the service owns the long-running SDK session and raw UDP sockets while
an Activity or product controller binds to it for control payloads and status.

Local compile validation without invoking Gradle:

```sh
scripts/build-android-sdk-gradle-consumer-smoke.sh
```
