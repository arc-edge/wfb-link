# Production Runtime Specification

## Purpose

Define the production-oriented WFB runtime command surface and its diagnostic
compatibility boundary.

## Requirements

### Requirement: Production Runtime Command Surface
The system SHALL provide a production-oriented WFB runtime entry point that
opens, initializes, receives, and transmits through runtime-owned types rather
than diagnostic bridge argument or report types.

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

### Requirement: Production Smoke Automation
The system SHALL provide repeatable production smoke automation for both remote
hardware-Mac deployment and local-adapter execution from the active checkout.

#### Scenario: Local adapter production smoke
- **WHEN** an operator runs the production smoke automation with local hardware
  mode enabled
- **THEN** the automation builds the current checkout, runs the RX-only and
  TX-positive production `radio-run` gates without SSH deployment, and validates
  clean TX submission plus runtime-owned RX outcome/frame-type telemetry

### Requirement: Diagnostic Compatibility Boundary
The system SHALL keep diagnostic commands available while making the production
runtime path independent of diagnostic-only CLI and report structs.

#### Scenario: Diagnostics retain experiments
- **WHEN** an operator needs register pokes, TX status probes, trace replay, or
  other bring-up experiments
- **THEN** those options remain available through diagnostic commands and remain
  absent from the production runtime command surface

#### Scenario: Runtime-flow compatibility preserved
- **WHEN** existing automation calls the diagnostic `runtime-flow` command
- **THEN** the command continues to produce compatible production-shaped
  telemetry while the new production entry point is introduced
