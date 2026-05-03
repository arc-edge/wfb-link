## ADDED Requirements

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
