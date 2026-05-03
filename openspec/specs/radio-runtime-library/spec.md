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
Diagnostic same-session init SHALL use runtime-owned phase ordering and runtime-owned same-session init execution policy.

#### Scenario: Diagnostic executes LLT and firmware
- **WHEN** diagnostic same-session init decides whether LLT or firmware runs first
- **THEN** it uses the runtime init sequence rather than diagnostic-only branching

#### Scenario: Diagnostic wraps runtime init result
- **WHEN** a diagnostic command runs retained same-session init
- **THEN** it calls the runtime same-session init API and maps runtime phase, counter, and calibration evidence into existing diagnostic report fields

### Requirement: Runtime Same-Session Init API
The runtime library SHALL expose a report-neutral RTL8812AU same-session initialization API for production callers and diagnostics.

#### Scenario: Runtime init executes on a runtime session
- **WHEN** a caller provides a `RuntimeRadioSession`, validated init configuration, normalized init assets, and required hardware-write authorization
- **THEN** the runtime library performs the same-session init phases and returns phase identifiers, counter deltas, calibration decisions, and final readiness state without using diagnostic command argument or report types

#### Scenario: Runtime init rejects unsafe calibration
- **WHEN** a caller selects a calibration profile that requires live register write authorization without providing that authorization
- **THEN** the runtime library rejects initialization before applying the profile and returns a structured runtime error

### Requirement: Runtime Calibration Selection
The runtime library SHALL provide production-facing calibration selection for RTL8812AU init.

#### Scenario: Default production calibration selected
- **WHEN** a caller selects the default calibration profile
- **THEN** the runtime library classifies the profile as production-safe default behavior and does not run targeted parity, captured IQK/LCK, or runtime IQK sequences

#### Scenario: Experimental calibration selected
- **WHEN** a caller selects targeted parity, captured IQK/LCK, or runtime IQK calibration
- **THEN** the runtime library classifies the profile as experimental, records the evidence source, and reports whether receiver-backed validation is still required

### Requirement: Runtime RX Metadata
The runtime library SHALL expose parsed RTL8812AU RX descriptor metadata needed by production bridge and RF-quality callers.

#### Scenario: RTL8812AU PHY status parsed
- **WHEN** a runtime RX packet includes RTL8812AU OFDM/HT/VHT PHY status bytes
- **THEN** the parsed frame includes best-path RSSI, SNR, SNR source, and a derived noise estimate from the documented PHYDM 1st-type layout

#### Scenario: PHY status lacks SNR
- **WHEN** a runtime RX packet has no PHY status or only a short/non-OFDM PHY status block
- **THEN** the parsed frame preserves fallback RSSI behavior and leaves SNR/noise metadata absent rather than fabricating values
