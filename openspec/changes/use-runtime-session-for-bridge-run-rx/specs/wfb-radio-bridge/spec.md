## MODIFIED Requirements

### Requirement: Combined WFB Bridge Runtime
The system SHALL run a combined WFB RX/TX bridge loop over the userspace USB radio without requiring a Linux monitor interface.

#### Scenario: Bridge-run frame injection succeeds
- **WHEN** `bridge-run` receives a valid WFB 802.11 frame from a configured UDP TX listener
- **THEN** it submits the frame through the runtime radio session and increments the injected-packet and injected-byte counters

#### Scenario: Bridge-run RX forwarding remains available
- **WHEN** the runtime radio session returns a bulk-IN read with supported receive metadata and an IEEE 802.11 payload
- **THEN** `bridge-run` processes the runtime-parsed packet and forwards matching WFB payloads to the configured UDP peers

#### Scenario: Bridge-run RX incomplete tail remains visible
- **WHEN** the runtime radio session returns a bulk-IN read whose trailing bytes do not contain a complete RX packet
- **THEN** `bridge-run` increments the RX need-more-data counter without treating the read as a fatal bridge error
