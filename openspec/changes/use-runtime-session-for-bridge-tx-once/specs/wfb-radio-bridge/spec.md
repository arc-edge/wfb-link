## MODIFIED Requirements

### Requirement: WFB Frame Injection
The system SHALL inject WFB-ng IEEE 802.11 frames through the userspace USB radio without requiring a Linux monitor interface.

#### Scenario: Frame injection succeeds
- **WHEN** the TX bridge receives a valid WFB 802.11 frame for the active radio
- **THEN** it submits the frame through the runtime radio session and increments the injected-packet and injected-byte counters

#### Scenario: Frame injection fails
- **WHEN** the runtime radio session rejects or fails a TX request
- **THEN** the bridge increments the dropped-packet counter and logs the radio error with the bridge packet context
