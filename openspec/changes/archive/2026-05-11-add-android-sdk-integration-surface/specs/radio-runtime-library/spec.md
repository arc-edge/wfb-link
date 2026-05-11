## ADDED Requirements

### Requirement: Android Runtime SDK Entry Points
The runtime library SHALL provide Android USBHost runtime behavior through
reusable SDK-owned entry points rather than requiring callers to use smoke-only
JNI helpers.

#### Scenario: SDK entry point runs runtime flow
- **WHEN** the Android SDK passes app-owned USB connection objects, endpoint
  objects, adapter metadata, init assets, keys, channel, stream config, and
  runtime bounds into native code
- **THEN** the runtime executes the same Android USBHost production flow used by
  hardware smoke validation and returns report-neutral runtime telemetry

#### Scenario: Smoke entry points remain diagnostic
- **WHEN** the smoke harness runs register, RX, init, TX, or managed-stream
  smokes
- **THEN** those smokes may call reusable SDK/runtime internals but remain
  diagnostic validation entry points rather than product integration APIs

### Requirement: Android SDK Native Failure Contract
The runtime library SHALL expose stable Android native failure codes/messages
for product SDK callers.

#### Scenario: Native startup fails
- **WHEN** Android SDK native startup fails due to invalid configuration,
  missing assets, USB transport failure, runtime init failure, helper startup
  failure, or runtime flow failure
- **THEN** the SDK receives a stable error code, human-readable message, and
  any partial runtime report evidence available at the failure point
