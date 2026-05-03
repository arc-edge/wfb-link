## MODIFIED Requirements

### Requirement: Combined WFB Bridge Runtime
The system SHALL run combined WFB bridge RX/TX traffic through the userspace USB radio runtime session and runtime same-session init execution.

#### Scenario: Bridge run transmits UDP datagrams
- **WHEN** `bridge-run` receives WFB UDP datagrams and explicit transmit authorization
- **THEN** it initializes through the runtime same-session init API, submits outgoing IEEE 802.11 frames through the runtime radio session, and records TX counters

#### Scenario: Bridge run receives WFB frames
- **WHEN** `bridge-run` receives runtime-parsed bulk-IN packets that match the configured WFB channel filter
- **THEN** it forwards payloads to the configured UDP aggregator and records RX and forwarding counters

#### Scenario: Bridge run shares one initialized session
- **WHEN** `bridge-run` enables both RX and TX
- **THEN** both directions share one runtime-owned initialized USB radio session rather than opening or initializing the adapter separately per direction
