## Context

Android support currently exists as a source-only smoke harness plus a Rust JNI
crate named `wfb-android-smoke`. That was the right shape for hardware bring-up:
it kept USB permission UI, direct `UsbDeviceConnection` transfer experiments,
init/RX/TX smoke tests, and managed-stream validation close together. It is not
the right shape for product integration because another Android app would have
to import smoke package names, fixed `/data/local/tmp` key paths, fixed port
defaults, and ADB-oriented build/install scripts.

The product-facing Rust `wfb-link` interface already models lifecycle,
endpoints, managed streams, tunnel endpoints, readiness, health, and final
reports across platform backends. Android should consume that same model, while
the Android app layer owns the Android-specific duties: USB permission, opened
`UsbDeviceConnection`, endpoint objects, app-private key/config/assets, and
foreground-service lifecycle.

## Goals / Non-Goals

**Goals:**

- Provide a reusable Android SDK/AAR artifact with a stable Java API.
- Package the native runtime library and WFB helper executables in a form a
  normal Android app can consume.
- Move reusable Android USBHost/runtime entry points out of smoke-only naming.
- Let apps provide keys, firmware/table assets, stream config, channel/profile
  settings, and lifecycle hooks through explicit config.
- Keep the smoke harness as a hardware regression app by making it call the same
  SDK surface.
- Add docs and a consumer sample that are sufficient for another Android project
  to integrate without reading the smoke implementation.

**Non-Goals:**

- Publishing to Maven Central or another public registry in this change.
- Replacing the existing macOS/Linux backend contracts.
- Implementing a full Android foreground service policy for every product app.
  The SDK should expose clean start/stop primitives and document the service
  requirement; product apps can choose their foreground-service UX.
- Solving long-range RF profile tuning or IQK/LCK parity; those remain RF
  quality work.

## Decisions

### Package AAR Without Requiring Gradle Internals

The repository will add a direct SDK-tools AAR build script first, matching the
existing APK packaging style. The AAR will contain Java classes, manifest
requirements, resources, the native `libwfb_android.so`, and packaged helper
executables named as native libraries so Android extracts them with executable
permissions.

Alternative considered: introduce a full Gradle multi-project build now. Gradle
will be useful later for publishing and instrumentation tests, but the current
repo already has deterministic SDK/NDK scripts and direct packaging gets an
integrable artifact faster.

### Add SDK-Named JNI Entry Points Before Large Native Refactor

The first SDK artifact will expose SDK-named Java/JNI entry points that own
production managed-stream startup and return structured JSON/status data. The
existing smoke JNI functions can stay for regression coverage while new app code
depends on `com.arcedge.wfblink.sdk`.

Alternative considered: fully split every reusable Rust function out of
`wfb-android-smoke` before shipping an AAR. That is cleaner, but it delays
integration. The SDK boundary can be introduced first, then the internal Rust
module/crate split can happen behind the same Java API.

### Java API Uses Explicit Config And App-Owned USB Objects

The Android API will require the app to pass an already-open
`UsbDeviceConnection`, selected bulk endpoints, VID/PID/interface metadata,
asset paths, key path, channel/bandwidth, stream definitions, runtime duration
or stop mode, and helper/native directory paths. It will not open USB permission
dialogs itself.

Alternative considered: the SDK discovers devices and requests permission
internally. Android permission flows are Activity/receiver specific, and product
apps need control over UX, so the SDK should provide helpers but not own the
permission UI.

### Long-Running Sessions Are Background-Thread Native Calls Initially

The initial SDK will run a managed runtime call synchronously from a caller-owned
background thread and support cooperative stop through an SDK handle as the next
slice. This matches current runtime behavior and avoids pretending we have a
complete service abstraction before product integration exercises it.

Alternative considered: immediately add a bound foreground service. That is
likely the long-term app pattern, but service policy belongs in the consuming
app unless we decide to ship an opinionated Android component.

## Risks / Trade-offs

- Android may reject helper execution from packaged native libraries on some
  devices or target SDK combinations. Mitigation: keep helpers under extracted
  `lib/arm64-v8a`, validate executable bits in the sample, and preserve smoke
  coverage on Pixel hardware.
- A synchronous native runtime call can be misused from the UI thread.
  Mitigation: Java API docs and sample always run it from an executor and fail
  fast if required config is absent.
- First AAR packaging is local-file based, not registry-based. Mitigation:
  document `flatDir`/local file integration now and keep artifact naming stable
  for later Maven publishing.
- Duplicating some smoke/native code during the transition may create drift.
  Mitigation: smoke harness should call SDK Java/native entry points for the
  managed path once they exist, and tests should cover SDK packaging.

## Migration Plan

1. Add SDK Java package, manifest/resource files, and AAR packaging script.
2. Add SDK-named native library build output and package helpers into the AAR.
3. Add SDK config/status classes and JNI declaration for managed-stream runs.
4. Refactor smoke harness managed-stream path to call SDK helpers where
   practical while preserving existing smoke-only tests.
5. Add docs and a minimal consumer sample.
6. Run host tests, Android target build, AAR packaging, smoke APK packaging, and
   at least one hardware managed-stream smoke.

Rollback is straightforward: product apps continue using the current smoke APK
only for validation, and the new AAR/script can be removed without affecting
macOS/Linux runtime paths.
