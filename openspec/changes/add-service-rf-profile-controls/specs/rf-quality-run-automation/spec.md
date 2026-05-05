## MODIFIED Requirements

### Requirement: Close-Range Run Orchestration
The system SHALL provide a single operator-facing command that orchestrates the accepted close-range RF-quality workflow across the hardware Mac and Linux WFB peer.

#### Scenario: Close-range automation run starts
- **WHEN** the operator invokes the automation command with the required host, channel, payload, and report settings
- **THEN** the command starts the Mac-side relay and bridge listener, waits for the bridge ready marker, prepares the Linux WFB peer, runs the Linux sender and receiver, and records the produced artifact paths

#### Scenario: Production radio command is selected
- **WHEN** the operator selects a production Mac command for an automated
  close-range run
- **THEN** the command MUST start the selected production command with runtime
  WFB TX ingress settings and the selected TX-power mode/source, wait for the
  same ready marker before Linux traffic, and generate datagram evidence from
  the production report's nested TX counters

#### Scenario: Production service command is selected
- **WHEN** the operator selects `MAC_RADIO_COMMAND=radio-service` for an
  automated close-range run with a supported TX-power mode and calibration
  profile
- **THEN** the command MUST start `wfb-radio-service` with equivalent production
  RF profile controls and MUST record `radio-service` as the Mac radio command
  in collected evidence

#### Scenario: Close-range automation run rejects missing settings
- **WHEN** required host, repository, firmware, key, or network settings are missing
- **THEN** the command fails before starting RF transmission and reports the missing setting
