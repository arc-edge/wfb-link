## 1. SDK Artifact Structure

- [x] 1.1 Add Android SDK source tree with manifest, resources, Java package,
      and README distinct from the smoke harness.
- [x] 1.2 Add a native SDK Rust crate or SDK-named native build target that
      exports product-facing JNI symbols without `smoke` package names.
- [x] 1.3 Add an AAR packaging script that builds Java classes, resources,
      native library, and optional WFB helper executables.
- [x] 1.4 Add a lightweight consumer/sample build path that verifies the AAR is
      usable outside `com.arcedge.wfblink.smoke`.

## 2. App-Facing Android API

- [x] 2.1 Add Java config/status/result classes for USB handoff, assets, keys,
      channel/profile, runtime bounds, and managed stream settings.
- [x] 2.2 Add `WfbLinkManager` or equivalent API for loading native code,
      validating config, starting a managed session on a caller-owned thread,
      and returning structured results.
- [x] 2.3 Add SDK-native validation and typed error/result JSON so callers can
      distinguish invalid config, USB failure, helper failure, and runtime
      failure.
- [x] 2.4 Make key paths, firmware/table paths, helper paths, ports, channel,
      payload counts, and durations app-provided rather than smoke-fixed in the
      SDK path.

## 3. Smoke Harness Cutover

- [x] 3.1 Keep register/RX/init/TX smoke entry points available for diagnostics.
- [x] 3.2 Refactor managed-stream smoke to use the reusable SDK Java/native
      surface where practical.
- [x] 3.3 Preserve the current ADB-driven smoke APK workflow and hardware
      managed-stream validation commands.

## 4. Documentation

- [x] 4.1 Document AAR build and local Gradle dependency setup.
- [x] 4.2 Document required Android manifest entries, USB permission handoff,
      native helper packaging, key/asset provisioning, threading, and lifecycle.
- [x] 4.3 Update Android USBHost, product integration, and cross-platform docs
      to distinguish SDK integration from smoke validation.
- [x] 4.4 Document current limitations: local AAR publishing only, caller-owned
      foreground service, arm64-only packaging, and RF-quality validation status.

## 5. Validation

- [x] 5.1 Run host Rust tests covering runtime and Android smoke/native helper
      behavior touched by the refactor.
- [x] 5.2 Build Android native SDK/smoke targets for `aarch64-linux-android`.
- [x] 5.3 Build the SDK AAR and verify its contents include classes, manifest,
      native library, and helper executables when requested.
- [x] 5.4 Build and install the smoke APK after the SDK cutover.
- [x] 5.5 Run an Android managed-stream hardware smoke against the Linux peer
      when hardware is reachable.
      - 2026-05-11: phone key refreshed from current paired `gs.key`; managed
        smoke on channel 161 submitted 41 frames, Android decoded 20 raw
        downlink payloads, and Linux decoded 19/20 Android uplink payloads.
