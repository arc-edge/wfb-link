## MODIFIED Requirements
### Requirement: Production Runtime Command Surface
The system SHALL provide a production-oriented WFB runtime entry point that
opens, initializes, receives, and transmits through runtime-owned types rather
than diagnostic bridge argument or report types. The production runtime MAY be
composed with external WFB-NG UDP codec processes for operator recovery flows
that need WFB-NG encryption/FEC compatibility.

#### Scenario: Production command starts full flow
- **WHEN** an operator starts the production runtime command with adapter,
  channel, bandwidth, WFB UDP, firmware, calibration profile, and required
  authorization settings
- **THEN** the command translates those settings into runtime-owned production
  configuration and runs the full RX/TX flow without exposing diagnostic-only
  register experiment flags

#### Scenario: Production report is runtime-owned
- **WHEN** the production runtime command exits
- **THEN** it emits a runtime-owned report containing adapter identity,
  endpoints, init readiness, calibration classification, RX/TX telemetry, RX
  metadata coverage, RX outcome/frame-type counters, USB counters, stop reason,
  and error state

#### Scenario: Recovery tunnel composes stock WFB-NG codec processes
- **WHEN** an operator starts the macOS WFB-NG tunnel recovery runner with a
  readable matching WFB-NG keypair
- **THEN** the runner starts the production radio service plus WFB-NG
  distributor/aggregator codec processes and a macOS `utun` bridge using the
  configured GS tunnel ports and tunnel IPs

#### Scenario: Recovery reports observed WFB channel IDs
- **WHEN** the production runtime receives 802.11 frames during a recovery run
- **THEN** the RX telemetry reports any WFB-prefixed source/destination channel
  IDs it observed, including raw IDs, decoded link IDs, decoded radio ports,
  and counts
