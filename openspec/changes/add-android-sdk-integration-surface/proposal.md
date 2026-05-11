## Why

Android hardware bring-up now validates direct USBHost register/init/RX/TX and
managed raw WFB streams, but the reusable pieces still live behind the smoke
harness. Product Android apps need an integration surface they can consume as an
SDK artifact without depending on smoke-only package names, fixed file paths, or
ADB-driven launch scripts.

## What Changes

- Add a reusable Android SDK packaging surface for WFB Link with app-facing Java
  APIs, native library packaging, helper executable packaging, and manifest
  requirements.
- Separate reusable Android native entry points and USBHost/session contracts
  from the smoke harness entry points.
- Add a minimal Android sample/consumer path that demonstrates USB permission,
  key/config provisioning, start/stop, endpoint discovery, and telemetry/error
  handling.
- Keep the existing smoke harness as hardware validation and regression tooling,
  updated to consume the reusable SDK surface where practical.
- Document Android integration steps, supported assets, lifecycle expectations,
  and current limitations.

## Capabilities

### New Capabilities

- `android-sdk-integration`: reusable Android SDK/AAR contract, app-facing API,
  packaging, lifecycle, telemetry, and consumer validation for Android projects.

### Modified Capabilities

- `radio-runtime-library`: expose Android USBHost/native runtime pieces through
  reusable SDK-owned entry points instead of smoke-only JNI helpers.
- `production-runtime`: preserve production stream, readiness, health, and
  managed-helper semantics when the runtime is started from the Android SDK.

## Impact

- Affected crates: new Android SDK native crate, `wfb-android-smoke`,
  `wfb-radio-runtime`, and possibly `wfb-link` if shared product-facing config
  needs small adapter helpers.
- Affected Android source: new SDK Android source tree, existing smoke harness,
  manifest/resource packaging, and build scripts.
- Affected docs: Android USBHost docs, product integration docs, cross-platform
  interface docs, smoke harness docs, and release/readiness notes.
- Affected validation: host Rust tests, Android target build, AAR packaging,
  smoke APK packaging, and hardware managed-stream smoke.
