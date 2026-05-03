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
