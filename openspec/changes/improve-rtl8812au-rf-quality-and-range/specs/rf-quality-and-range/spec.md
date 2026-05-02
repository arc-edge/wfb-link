## ADDED Requirements

### Requirement: Linux Baseline Comparison
The system SHALL support RF-quality comparison runs that use a Linux RTL8812AU/WFB-ng baseline captured with the same adapter class, antenna setup, channel, bandwidth, WFB key, radio port, FEC settings, payload size, and fixed TX rate/profile as the macOS run.

#### Scenario: Baseline metadata is recorded
- **WHEN** an operator runs a Linux-baseline comparison workflow
- **THEN** the report MUST include the Linux command parameters, adapter identity when available, channel, bandwidth, WFB settings, payload size, duration or packet count, and artifact paths for receiver logs or captures

#### Scenario: Mac and Linux runs are comparable
- **WHEN** a macOS RF-quality run references a Linux baseline
- **THEN** the report MUST identify any mismatched test parameters that make the comparison invalid or degraded

### Requirement: RF Quality Report
The system SHALL emit structured RF-quality reports for macOS WFB runs that capture the RF, descriptor, calibration, WFB, and receiver evidence needed to evaluate range readiness.

#### Scenario: RF state is reportable
- **WHEN** a macOS RF-quality run completes
- **THEN** the report MUST include channel, bandwidth, TX rate/MCS, TX descriptor profile, TX queue, MACID, rate ID, retry/fallback settings, RFE type, EFUSE summary, TX power mode, TX power register evidence, and calibration state

#### Scenario: WFB outcome is reportable
- **WHEN** a macOS RF-quality run forwards or injects WFB traffic
- **THEN** the report MUST include submitted datagrams, recovered payloads when available, malformed/dropped counters, FEC/source payload settings, throughput, CPU usage, and receiver artifact paths

### Requirement: EFUSE-Derived TX Power Programming
The system SHALL provide an explicit TX power mode that computes RTL8812AU per-path/per-rate TXAGC register values from decoded EFUSE power data and records the calculation inputs and writes.

#### Scenario: TX power calculation is visible
- **WHEN** an operator enables EFUSE-derived TX power programming
- **THEN** the report MUST include decoded EFUSE source values, selected channel group, selected RF path, per-rate power indexes, regulatory or safety clamps, and before/write/after register evidence

#### Scenario: TX power override remains guarded
- **WHEN** an operator attempts to change TX power behavior
- **THEN** the command MUST require explicit RF-transmit authorization and MUST reject values outside the guarded RTL8812AU TX power index range

### Requirement: Calibration State Tracking
The system SHALL track and report IQK, LCK, thermal, RFE pinmux, and RF path calibration state for RF-quality runs, even when the implementation is still using a stop-gap or captured value.

#### Scenario: Stop-gap calibration is labeled
- **WHEN** a run uses planted, captured, or static calibration values
- **THEN** the report MUST label those values as stop-gap calibration and MUST include the register values that were applied

#### Scenario: Runtime calibration is verified
- **WHEN** a runtime calibration routine or approximation is enabled
- **THEN** the report MUST include the calibration routine name, affected registers, success/failure status, and before/after values sufficient to compare against Linux behavior

### Requirement: Range Test Profiles
The system SHALL define repeatable range-test profiles for close-range sanity, stepped/attenuated comparison, and outdoor/long-distance validation.

#### Scenario: Close-range profile passes before field range
- **WHEN** an operator selects a long-distance validation profile
- **THEN** the documented procedure MUST require a passing close-range baseline for the same channel, rate, bandwidth, power mode, and payload settings

#### Scenario: Field run records geometry
- **WHEN** an outdoor or long-distance range test is recorded
- **THEN** the report or companion notes MUST include distance or geometry estimate, antenna orientation, adapter placement, channel/bandwidth, environment notes, WFB settings, and all produced artifacts

### Requirement: Acceptance Criteria for RF Quality
The system SHALL define RF-quality acceptance criteria in terms of receiver-backed WFB outcomes and Linux-baseline comparison rather than USB submission success alone.

#### Scenario: Bench success is not enough
- **WHEN** a run only proves USB bulk submission, descriptor construction, or close-range packet visibility
- **THEN** the system MUST NOT classify the result as long-distance RF-quality success

#### Scenario: Linux baseline margin is evaluated
- **WHEN** macOS and Linux comparison runs are available for the same profile
- **THEN** the report MUST summarize macOS payload recovery, loss, throughput, and receiver metadata against the Linux baseline and identify whether the run is within the configured acceptance margin

### Requirement: Wide Bandwidth Evidence Gate
The system SHALL require separate evidence before claiming HT40 or VHT80 wide-PPDU range benefit.

#### Scenario: HT40 context is not wide-PPDU proof
- **WHEN** WFB traffic succeeds while both radios are tuned to an HT40 channel context but RX metadata reports 20 MHz frames
- **THEN** the documentation and reports MUST classify the result as HT40 channel-context WFB flow, not proven 40 MHz PPDU occupancy

#### Scenario: Wide-mode promotion requires evidence
- **WHEN** an operator marks a 40 MHz or 80 MHz profile as range-ready
- **THEN** the profile MUST reference receiver metadata, SDR/spectrum evidence, or equivalent proof that the intended wide-PPDU mode is actually being transmitted and decoded
