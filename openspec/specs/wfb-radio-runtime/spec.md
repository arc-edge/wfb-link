# WFB Radio Runtime Specification

## Purpose

Define runtime-owned live radio behavior that standalone diagnostics and future production commands share.
## Requirements
### Requirement: Standalone Runtime RX Capture
The system SHALL capture standalone live RX traffic through the userspace USB radio runtime.

#### Scenario: RX scan captures runtime-parsed frames
- **WHEN** `rx-scan` receives a bulk-IN read containing supported RTL8812AU RX packet metadata
- **THEN** it processes the runtime-parsed packet outcomes and records frame, drop, and incomplete-tail counters

#### Scenario: RX scan forwards matching WFB payloads
- **WHEN** a runtime-parsed RX frame matches the configured WFB channel filter
- **THEN** `rx-scan` forwards the WFB payload to the configured UDP aggregator and records forwarding counters

### Requirement: Standalone Runtime TX
The system SHALL submit standalone live TX diagnostics through the userspace USB radio runtime.

#### Scenario: Single-frame TX uses runtime session
- **WHEN** `tx-once` receives a valid IEEE 802.11 frame and explicit transmit authorization
- **THEN** it submits the frame through the runtime radio session and records TX submit counters

#### Scenario: Repeated TX uses runtime session
- **WHEN** `tx-repeat` receives a valid IEEE 802.11 frame, repeat count, interval, and explicit transmit authorization
- **THEN** it submits each frame through the runtime radio session and records throughput and submit counters

### Requirement: Production Runtime Full Flow
The system SHALL provide a production-facing WFB runtime flow that opens, initializes, receives, and transmits through runtime APIs.

#### Scenario: Production flow starts
- **WHEN** a caller starts the production runtime flow with a supported adapter selector, channel, bandwidth, WFB UDP settings, calibration profile, and required authorization
- **THEN** the command opens the adapter through runtime open policy, initializes it through runtime same-session init, and performs RX/TX through `RuntimeRadioSession`

#### Scenario: Production flow rejects diagnostic-only dependencies
- **WHEN** the production runtime flow is built
- **THEN** it MUST NOT depend on diagnostic command argument structs or diagnostic report structs for radio initialization, RX, or TX execution

#### Scenario: Production flow reports readiness
- **WHEN** initialization completes
- **THEN** the production runtime flow reports adapter identity, channel, bandwidth, calibration class, init phase status, RX counters, TX counters, and last error state through production-facing telemetry
