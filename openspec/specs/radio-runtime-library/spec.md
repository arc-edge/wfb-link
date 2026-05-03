# radio-runtime-library Specification

## Purpose

Define reusable production-facing radio runtime responsibilities that should not depend on diagnostic CLI parsing or report formatting.
## Requirements
### Requirement: Runtime Calibration Policy
The system SHALL expose calibration profile policy from a reusable runtime library rather than requiring callers to depend on the diagnostic binary.

#### Scenario: Calibration profile classified
- **WHEN** a caller selects a supported TX calibration profile
- **THEN** the runtime library reports whether it is the default profile, which calibration class it represents, and whether it requires live register write authorization

#### Scenario: Runtime IQK protected
- **WHEN** a caller selects the runtime RTL8812A IQK profile
- **THEN** the runtime library marks the profile as requiring live register write authorization

### Requirement: Diagnostic Commands Use Runtime Policy
Diagnostic commands SHALL call the runtime library for stable calibration policy while retaining diagnostic-only CLI parsing and hardware execution code.

#### Scenario: Diagnostic authorization check
- **WHEN** a diagnostic command validates a selected calibration profile
- **THEN** it uses the runtime library decision to accept or reject the command before opening the adapter

#### Scenario: Diagnostic report classification
- **WHEN** a diagnostic RF-quality report classifies calibration state before TX
- **THEN** it uses the runtime library calibration class and preserves the existing report enum values

### Requirement: Runtime Boundary Documentation
The system SHALL document which radio responsibilities are owned by the runtime crate and which remain diagnostic-only pending later migration.

#### Scenario: Developer reviews runtime boundary
- **WHEN** a developer needs to move another radio path out of the diagnostic binary
- **THEN** the repository documentation identifies the current runtime-owned responsibilities and the next migration targets

### Requirement: macOS USBHost Runtime Transport
The runtime library SHALL provide the macOS IOUSBHost retained-session transport used for RTL8812AU register access and bulk pipe transfers.

#### Scenario: Runtime transport imported by diagnostic caller
- **WHEN** the diagnostic binary builds on macOS
- **THEN** it imports the macOS USBHost device and session types from the runtime library rather than from a local diagnostic module

#### Scenario: Transport traits preserved
- **WHEN** a caller opens a macOS USBHost device or retained session through the runtime library
- **THEN** the transport still implements the RTL8812AU register transport and USB bulk transfer traits used by existing init, TX, and RX code

### Requirement: Unified Runtime USB Transport
The runtime library SHALL provide a single live USB transport type that can carry either a libusb-claimed adapter or a macOS USBHost retained session.

#### Scenario: Register access through runtime transport
- **WHEN** live RTL8812AU register code receives the runtime USB transport
- **THEN** it can issue vendor register reads and writes through the runtime transport trait implementation

#### Scenario: Bulk transfer through runtime transport
- **WHEN** live RX or TX code receives the runtime USB transport
- **THEN** it can issue USB bulk reads and writes through the runtime transport trait implementation

#### Scenario: Diagnostic caller uses runtime transport
- **WHEN** diagnostic live commands claim or open an adapter
- **THEN** they construct the runtime USB transport type rather than a diagnostic-only transport enum

### Requirement: macOS Transport Open Policy
The runtime library SHALL own macOS USBHost retained-session open policy for supported RTL8812AU adapters.

#### Scenario: macOS endpoint config validated
- **WHEN** a caller supplies macOS USBHost endpoint configuration
- **THEN** the runtime library validates endpoint direction, supported bulk OUT endpoint count, and selected bulk OUT endpoint membership

#### Scenario: macOS retained session opened
- **WHEN** a caller supplies a supported VID/PID selector and valid macOS USBHost configuration
- **THEN** the runtime library opens a retained IOUSBHost session and returns the runtime USB transport, adapter metadata, endpoint layout, and initial USB control-write evidence

#### Scenario: macOS open rejected
- **WHEN** a caller supplies an unsupported selector, missing VID/PID, unsupported bus/address selector, invalid endpoints, or an IOUSBHost open failure
- **THEN** the runtime library returns a structured runtime error with a stable code and human-readable message

### Requirement: Diagnostic Uses Runtime Open Policy
Diagnostic live commands SHALL use runtime-owned open policy when constructing `RuntimeUsbTransport`.

#### Scenario: Diagnostic opens adapter
- **WHEN** a diagnostic bridge, init, TX, or RX live path opens a runtime USB transport
- **THEN** it calls the runtime open API and maps the runtime result into existing diagnostic report fields

### Requirement: Cross-Backend Runtime Open Policy
The runtime library SHALL provide a cross-backend open API for supported RTL8812AU USB adapters.

#### Scenario: libusb adapter opened
- **WHEN** a caller selects the libusb backend with a supported adapter selector
- **THEN** the runtime library discovers the matching supported adapter, claims it, and returns runtime transport, adapter metadata, and endpoint layout

#### Scenario: macOS USBHost adapter opened
- **WHEN** a caller selects the macOS USBHost backend with valid macOS USBHost configuration
- **THEN** the runtime library opens the retained IOUSBHost session through the existing runtime macOS open policy

#### Scenario: open failure reported
- **WHEN** discovery, selection, claiming, endpoint validation, or backend support fails
- **THEN** the runtime library returns a structured runtime error with a stable code and human-readable message

### Requirement: RTL8812AU Init Phase Sequence
The runtime library SHALL define the RTL8812AU same-session initialization phase sequence.

#### Scenario: Default phase order requested
- **WHEN** a caller requests the default same-session init sequence
- **THEN** the runtime library returns firmware before LLT and preserves the remaining MAC, queue, BB, RF, channel, calibration, and TX scheduler phases

#### Scenario: Linux phase order requested
- **WHEN** a caller requests Linux-order same-session init
- **THEN** the runtime library returns LLT before firmware and preserves the remaining phase order

### Requirement: Diagnostic Uses Runtime Init Sequence
Diagnostic same-session init SHALL use runtime-owned phase ordering policy.

#### Scenario: Diagnostic executes LLT and firmware
- **WHEN** diagnostic same-session init decides whether LLT or firmware runs first
- **THEN** it uses the runtime init sequence rather than diagnostic-only branching
