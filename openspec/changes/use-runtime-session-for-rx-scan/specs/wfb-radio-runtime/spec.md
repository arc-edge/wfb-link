## MODIFIED Requirements

### Requirement: Standalone Runtime RX Capture
The system SHALL capture standalone live RX traffic through the userspace USB radio runtime.

#### Scenario: RX scan captures runtime-parsed frames
- **WHEN** `rx-scan` receives a bulk-IN read containing supported RTL8812AU RX packet metadata
- **THEN** it processes the runtime-parsed packet outcomes and records frame, drop, and incomplete-tail counters

#### Scenario: RX scan forwards matching WFB payloads
- **WHEN** a runtime-parsed RX frame matches the configured WFB channel filter
- **THEN** `rx-scan` forwards the WFB payload to the configured UDP aggregator and records forwarding counters
