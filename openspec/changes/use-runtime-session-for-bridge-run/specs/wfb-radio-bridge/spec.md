## MODIFIED Requirements

### Requirement: Combined WFB Bridge Runtime
The system SHALL run a combined WFB RX/TX bridge loop over the userspace USB radio without requiring a Linux monitor interface.

#### Scenario: Bridge-run frame injection succeeds
- **WHEN** `bridge-run` receives a valid WFB 802.11 frame from a configured UDP TX listener
- **THEN** it submits the frame through the runtime radio session and increments the injected-packet and injected-byte counters

#### Scenario: Bridge-run RX forwarding remains available
- **WHEN** the runtime-owned USB transport returns a bulk-IN buffer containing supported receive metadata and an IEEE 802.11 payload
- **THEN** `bridge-run` parses the buffer and forwards matching WFB payloads to the configured UDP peers
