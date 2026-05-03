## ADDED Requirements

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
Diagnostic commands SHALL use the runtime library for macOS USBHost retained-session endpoint validation, metadata synthesis, and opening.

#### Scenario: Diagnostic macOS open
- **WHEN** a diagnostic live command uses the macOS USBHost path
- **THEN** it translates CLI flags into runtime macOS USBHost config and maps runtime open results into existing diagnostic reports
