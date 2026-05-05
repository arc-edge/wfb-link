## MODIFIED Requirements

### Requirement: Production Runtime Command Surface
The system SHALL provide a production-oriented WFB runtime entry point that
opens, initializes, receives, transmits, and selects guarded RF profile controls
through runtime-owned types rather than diagnostic bridge argument or report
types.

#### Scenario: Production command starts full flow
- **WHEN** an operator starts the production runtime command with adapter,
  channel, bandwidth, WFB UDP, firmware, TX-power mode, calibration profile,
  and required authorization settings
- **THEN** the command translates those settings into runtime-owned production
  configuration and runs the full RX/TX flow without exposing diagnostic-only
  register experiment flags

#### Scenario: Production service accepts RF profile controls
- **WHEN** an operator starts `wfb-radio-service` with TX-power mode or
  calibration profile controls from CLI flags or service config
- **THEN** the service validates those controls before USB open and passes them
  to the runtime-owned production flow

#### Scenario: Production report is runtime-owned
- **WHEN** the production runtime command exits
- **THEN** it emits a runtime-owned report containing adapter identity,
  endpoints, init readiness, calibration classification, RX/TX telemetry, RX
  metadata coverage, RX outcome/frame-type counters, USB counters, stop reason,
  and error state
