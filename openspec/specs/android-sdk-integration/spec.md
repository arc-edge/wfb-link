# android-sdk-integration Specification

## Purpose

Define the Android app-facing SDK artifact, packaging, API, validation, and
documentation requirements for using WFB Link from another Android project.

## Requirements

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

### Requirement: Android SDK Lifecycle Session API
The Android SDK SHALL expose a lifecycle-safe managed session API above the
blocking native runtime call.

#### Scenario: Session starts on caller executor
- **WHEN** an app creates a managed session with valid config and an executor
- **THEN** the SDK starts runtime execution off the UI thread and exposes a
  session handle with status and completion accessors

#### Scenario: Session completion is observable
- **WHEN** the native managed runtime returns or fails
- **THEN** the session records a terminal status and delivers either a
  `WfbManagedStreamsResult` or a typed `WfbLinkException` to the caller

#### Scenario: Stop is cooperative
- **WHEN** an app requests stop on a running managed session
- **THEN** the SDK records stop-requested status and prevents duplicate starts,
  even if native USB execution can only finish at the next bounded runtime exit

### Requirement: Android SDK Named Managed Streams
The Android SDK SHALL let apps describe managed WFB streams with stable names
and stream metadata instead of only smoke-specific fixed ports.

#### Scenario: Named stream config accepted
- **WHEN** an app configures one supported TX raw stream and one supported RX
  raw stream with names, radio ports, link IDs, local UDP ports, and TX profile
- **THEN** the SDK maps those streams into the native Android managed runtime
  config and preserves the stream names in Java-visible configuration

#### Scenario: Unsupported shape rejected
- **WHEN** an app configures duplicate stream names, duplicate local UDP ports,
  unsupported payload kind, or more streams than the current Android native path
  supports
- **THEN** the SDK rejects startup with a typed validation error before live USB
  runtime execution

### Requirement: Android Gradle Consumer Sample
The repository SHALL include a Gradle-style Android sample that consumes the
local SDK AAR without depending on the smoke harness package.

#### Scenario: Sample imports local AAR
- **WHEN** the sample project is configured with the generated local AAR
- **THEN** it compiles against `com.arcedge.wfblink.sdk` and does not import
  `com.arcedge.wfblink.smoke`

#### Scenario: Sample demonstrates integration contract
- **WHEN** an integrator reads the sample source
- **THEN** they can see USB permission handoff, SDK config construction,
  worker-thread/session startup, stop request, and result/error handling

### Requirement: Android SDK CI Validation
The repository SHALL provide automated validation for Android SDK packaging and
consumer compilation.

#### Scenario: CI validates Android SDK artifacts
- **WHEN** CI runs with Android SDK and NDK tooling available
- **THEN** it builds the Android native target, packages the SDK AAR, compiles
  the direct consumer smoke, and compiles the Gradle-style consumer sample

#### Scenario: Hardware remains manual
- **WHEN** CI runs without attached RTL8812AU hardware and a Linux peer
- **THEN** it skips RF hardware smoke while preserving documented manual bench
  commands for Android managed-stream validation
