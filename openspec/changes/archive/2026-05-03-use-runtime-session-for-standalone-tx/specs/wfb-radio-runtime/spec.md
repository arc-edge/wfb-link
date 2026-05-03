## MODIFIED Requirements

### Requirement: Standalone Runtime TX
The system SHALL submit standalone live TX diagnostics through the userspace USB radio runtime.

#### Scenario: Single-frame TX uses runtime session
- **WHEN** `tx-once` receives a valid IEEE 802.11 frame and explicit transmit authorization
- **THEN** it submits the frame through the runtime radio session and records TX submit counters

#### Scenario: Repeated TX uses runtime session
- **WHEN** `tx-repeat` receives a valid IEEE 802.11 frame, repeat count, interval, and explicit transmit authorization
- **THEN** it submits each frame through the runtime radio session and records throughput and submit counters
