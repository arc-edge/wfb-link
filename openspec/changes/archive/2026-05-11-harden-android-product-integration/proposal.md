## Why

The Android radio path is now proven at the RF and AAR level, but another app
still has to infer too much from the smoke harness to integrate safely. We need
the SDK surface and validation path to look like a production Android
dependency rather than a bench-only JNI call.

## What Changes

- Add a Gradle-style Android consumer sample that imports the local AAR and
  demonstrates USB permission handoff, asset/key paths, worker-thread startup,
  and result handling.
- Extend the Java SDK facade with a production-shaped managed session API:
  explicit session object, background execution helper, cooperative stop
  request, status snapshots, and callback/result delivery.
- Add named managed stream configuration classes so product code can describe
  video, telemetry, control, and future stream shapes without relying on
  smoke-specific one-up/one-down terminology.
- Add CI/build automation that validates host tests, Android target checks, AAR
  packaging, external consumer compile, and the Gradle consumer sample when the
  Android SDK is available.
- Update Android integration docs to center the SDK sample and lifecycle API.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `android-sdk-integration`: add production Android consumer sample, lifecycle
  API, named stream config, and CI/build validation requirements.

## Impact

- Android SDK Java API under `android/sdk/src/main/java/com/arcedge/wfblink/sdk`.
- Android sample/consumer source and build scripts.
- Native JNI config marshalling where needed for named stream settings.
- GitHub Actions or local CI scripts for Android SDK validation.
- Documentation in `docs/android-sdk.md`, `docs/android-usbhost.md`, and the
  README.
