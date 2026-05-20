## ADDED Requirements

### Requirement: Android SDK Radio Link Metadata
The Android SDK SHALL expose radio-link metadata in app-facing result and
health models when the native runtime provides it.

#### Scenario: Managed result includes signal quality
- **WHEN** an Android managed WFB stream session completes with native runtime
  RX signal metadata
- **THEN** the SDK result exposes raw RSSI/SNR/noise metrics, last/min/max/
  average values, sample counts, signal state, and normalized quality indicator
  through typed Java fields

#### Scenario: Signal metadata is absent
- **WHEN** the native runtime report does not include signal metadata
- **THEN** the SDK exposes an unsupported or unknown signal state without
  throwing a parsing error and without fabricating quality values

### Requirement: Android SDK Radio Metadata Documentation
The Android SDK documentation SHALL explain how product apps should consume
radio-link metadata.

#### Scenario: Integrator reads Android docs
- **WHEN** an Android integrator reads the SDK documentation
- **THEN** the documentation identifies the signal quality field, raw debug
  metrics, local receiver semantics, stale/unknown behavior, and the current
  limitation that remote/uplink RSSI requires peer-reported metadata
