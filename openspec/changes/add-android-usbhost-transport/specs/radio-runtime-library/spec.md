## ADDED Requirements

### Requirement: Android USBHost Runtime Transport Contract

The runtime library SHALL expose Android USBHost as a selectable runtime USB
backend without changing the shared RTL8812AU runtime configuration shape.

#### Scenario: Android endpoint config validated

- **WHEN** a caller supplies Android USBHost endpoint configuration
- **THEN** the runtime library validates endpoint direction, supported bulk OUT
  endpoint count, and selected bulk OUT endpoint membership

#### Scenario: Android backend selected

- **WHEN** a caller builds production runtime USB config with the Android
  USBHost backend
- **THEN** the runtime snapshot records `android_usbhost` config and maps it to
  a live Android runtime open request

#### Scenario: Android native bridge pending

- **WHEN** a caller opens the Android USBHost backend before the native Android
  transfer bridge is implemented
- **THEN** the runtime library returns a structured fail-closed error with a
  stable code and human-readable message
