## ADDED Requirements

### Requirement: Targeted Calibration Parity Profile
The system SHALL provide an explicit targeted calibration profile for RTL8812AU TX-path parity work that applies known Linux-final register overrides separately from full runtime IQK/LCK.

#### Scenario: Targeted parity profile is enabled
- **WHEN** the operator enables the targeted Linux-parity calibration profile for a supported channel and bandwidth
- **THEN** the radio command MUST apply the profile's guarded register writes, record before/write/after evidence, and label the calibration mode as targeted parity rather than full Linux-ported calibration

#### Scenario: Targeted parity profile is unsupported
- **WHEN** the operator enables the targeted parity profile for an unsupported channel, bandwidth, or chipset state
- **THEN** the command MUST fail before RF transmission or leave the profile unapplied with an explicit reportable reason

### Requirement: Receiver Metadata Confidence
The system SHALL expose RX metadata with enough source and confidence information for RF-quality reports to distinguish measured signal evidence from fallbacks.

#### Scenario: PHY status supplies RSSI evidence
- **WHEN** an RX descriptor includes PHY status bytes sufficient for RSSI extraction
- **THEN** the emitted frame metadata MUST mark RSSI as measured and include descriptor/PHY-status evidence fields

#### Scenario: RSSI is a fallback
- **WHEN** an RX descriptor does not include usable PHY status bytes
- **THEN** the emitted frame metadata MUST preserve a fallback RSSI for compatibility but MUST mark it as fallback/invalid for RF-quality decisions
