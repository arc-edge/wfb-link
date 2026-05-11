## ADDED Requirements

### Requirement: Android SDK Artifact
The system SHALL build a reusable Android SDK artifact for product apps without
requiring those apps to import the smoke harness package.

#### Scenario: AAR is built
- **WHEN** an integrator runs the Android SDK packaging script with Android SDK
  and NDK tools available
- **THEN** the repository produces an AAR containing SDK Java classes, Android
  manifest requirements, `arm64-v8a` native runtime libraries, and optional
  Android WFB helper executables

#### Scenario: Smoke package remains separate
- **WHEN** the smoke APK is built
- **THEN** it remains a validation app and does not become the package namespace
  that product Android apps must depend on

### Requirement: Android SDK App-Facing API
The Android SDK SHALL expose app-facing Java APIs for USB-backed WFB Link
startup, status, and result reporting.

#### Scenario: Managed stream session starts
- **WHEN** an app supplies an opened `UsbDeviceConnection`, selected bulk
  endpoint objects, adapter metadata, app-private asset paths, key path,
  channel, runtime bounds, and managed stream settings
- **THEN** the SDK starts the userspace Android USBHost runtime through SDK
  package entry points rather than smoke-only JNI entry points

#### Scenario: Invalid config rejected
- **WHEN** required USB objects, endpoint metadata, key path, native helper
  paths, asset paths, channel, or stream settings are missing or invalid
- **THEN** the SDK returns a typed failure code and message before attempting
  live USB runtime execution

#### Scenario: Result is structured
- **WHEN** a managed session exits
- **THEN** the SDK exposes submitted-frame counts, raw TX/RX counts, forwarded
  RX counts, stop reason, error code, and report JSON or report path to the
  caller

### Requirement: Android SDK Packaging Contract
The Android SDK SHALL package native dependencies in a form consumable by a
normal Android app.

#### Scenario: Native library extracted
- **WHEN** an app includes the AAR
- **THEN** Android can load the SDK native library through `System.loadLibrary`
  from the app's normal native library directory

#### Scenario: Helpers are executable
- **WHEN** managed WFB streams are enabled and helper binaries are packaged
- **THEN** the SDK can resolve executable `wfb_tx`, `wfb_rx`, and optional
  `wfb_keygen` helper paths from the app's native library directory

### Requirement: Android SDK Documentation
The system SHALL document enough Android integration detail for another project
to consume the SDK artifact.

#### Scenario: Integrator reads docs
- **WHEN** an Android integrator reads the repository documentation
- **THEN** they can identify the AAR artifact, Gradle/local dependency setup,
  required manifest permissions/features, USB permission handoff, key/asset
  provisioning, start/stop threading expectations, telemetry fields, and current
  limitations

### Requirement: Android SDK Consumer Validation
The system SHALL include a minimal consumer path that verifies the AAR can be
used outside the smoke harness.

#### Scenario: Consumer compiles
- **WHEN** Android SDK and NDK tools are available
- **THEN** the consumer validation builds against the SDK/AAR without importing
  `com.arcedge.wfblink.smoke`

#### Scenario: Hardware smoke remains available
- **WHEN** Android hardware is connected for bench validation
- **THEN** the smoke harness can still run register, init, TX, RX, and managed
  stream smokes using the current checkout
