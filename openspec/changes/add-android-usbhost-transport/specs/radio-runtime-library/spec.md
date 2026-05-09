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

#### Scenario: Android fd-backed bridge opened

- **WHEN** a caller opens the Android USBHost backend with a non-negative
  app-owned device file descriptor, supported VID/PID metadata, and valid
  endpoint configuration
- **THEN** the runtime library wraps the fd as a USB transport that supports
  RTL8812AU vendor control transfers and bulk IN/OUT transfers

#### Scenario: Android fd-backed bridge rejected

- **WHEN** a caller opens the Android USBHost backend with a missing fd,
  negative fd, missing VID/PID metadata, unsupported adapter metadata,
  unsupported bus/address selector, or invalid endpoint layout
- **THEN** the runtime library returns a structured runtime error with a stable
  code and human-readable message before attempting live USB transfers
