## ADDED Requirements

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
