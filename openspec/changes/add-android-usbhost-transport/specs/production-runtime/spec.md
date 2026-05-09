## ADDED Requirements

### Requirement: Production Service Android USBHost Selection

The production service config SHALL allow Android USBHost transport selection
without changing stream, WFB, tunnel, calibration, or report semantics.

#### Scenario: Android USBHost config resolved

- **WHEN** service config enables `[android_usbhost]`
- **THEN** the service maps Android USBHost fields into
  `ProductionRuntimeUsbConfig` with the Android backend selected

#### Scenario: Multiple USB backends rejected

- **WHEN** service config enables both `[macos_usbhost]` and
  `[android_usbhost]`
- **THEN** the service rejects the runtime config before opening hardware
