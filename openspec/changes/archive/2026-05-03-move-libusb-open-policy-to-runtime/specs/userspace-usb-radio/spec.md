## MODIFIED Requirements

### Requirement: USB Interface Claim
The system SHALL claim the radio's USB interface for exclusive userspace access before issuing register, firmware, RX, or TX operations, using runtime-owned open policy for production runtime transport paths.

#### Scenario: Interface claim succeeds
- **WHEN** the selected adapter is not owned by another active driver or process
- **THEN** the runtime open policy claims interface 0 and records the bulk IN and bulk OUT endpoints

#### Scenario: Interface claim fails
- **WHEN** macOS or another process prevents claiming the USB interface
- **THEN** the runtime open policy fails before chip initialization and reports the owning or failing USB operation when available
