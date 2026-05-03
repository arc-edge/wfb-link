## ADDED Requirements

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

### Requirement: Diagnostic Uses Runtime Open Policy
Diagnostic live commands SHALL use runtime-owned open policy when constructing `RuntimeUsbTransport`.

#### Scenario: bridge or init path opens adapter
- **WHEN** a diagnostic bridge, init, TX, or RX live path opens a runtime USB transport
- **THEN** it calls the runtime open API and maps the runtime result into existing diagnostic report fields
