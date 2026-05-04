## MODIFIED Requirements

### Requirement: Production Runtime Full Flow
The system SHALL provide a production-facing WFB runtime flow that opens,
initializes, receives, and transmits through runtime APIs, runtime-owned loop
planning, and runtime-owned TX ingress setup.

#### Scenario: Production flow starts
- **WHEN** a caller starts the production runtime flow with a supported adapter
  selector, channel, bandwidth, WFB UDP settings, calibration profile, and
  required authorization
- **THEN** the command opens the adapter through runtime open policy,
  initializes it through runtime same-session init, and performs RX/TX through
  `RuntimeRadioSession`

#### Scenario: Production flow starts TX ingress
- **WHEN** the production runtime flow reaches WFB loop execution
- **THEN** TX UDP socket binding and datagram receiver threads are created
  through `wfb-radio-runtime`

#### Scenario: Production flow rejects diagnostic-only dependencies
- **WHEN** the production runtime flow is built
- **THEN** it MUST NOT depend on diagnostic command argument structs or
  diagnostic report structs for radio initialization, RX/TX loop planning, TX
  ingress setup, or emitted production reports

#### Scenario: Production flow rejects diagnostic register experiments
- **WHEN** a caller starts `runtime-flow` or `radio-run` with diagnostic-only
  register pokes or TX-status probes
- **THEN** the command rejects the request before opening USB

#### Scenario: Production flow validates WFB loop settings
- **WHEN** a caller starts `radio-run` with invalid WFB forwarding settings,
  zero RX timeout, or zero TX burst limit
- **THEN** runtime-owned validation rejects the request before socket binding or
  USB open

#### Scenario: Production flow reports readiness
- **WHEN** initialization completes
- **THEN** the production runtime flow reports adapter identity, channel,
  bandwidth, calibration class, init phase status, runtime-owned RX/TX flow
  counters, RX metadata coverage counters, USB counters, and last error state
  through production-facing telemetry
