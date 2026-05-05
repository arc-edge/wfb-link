# RF Quality and Range Specification

## Purpose

Define the evidence, reports, guarded power behavior, calibration tracking, and acceptance gates required before macOS RTL8812AU WFB operation can be considered range-ready.
## Requirements
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
- **THEN** the report MUST include channel, bandwidth, TX rate/MCS, TX descriptor profile, TX queue, MACID, rate ID, retry/fallback settings, RFE type, EFUSE summary, TX power mode, TX power register evidence, calibration state, and any selected TX calibration profile evidence

#### Scenario: WFB outcome is reportable
- **WHEN** a macOS RF-quality run forwards or injects WFB traffic
- **THEN** the report MUST include submitted datagrams, recovered payloads when available, malformed/dropped counters, FEC/source payload settings, throughput, CPU usage, receiver artifact paths, expected-versus-observed datagram evidence for short FEC runs, receiver session/decrypt health when supplied by the run automation, and receiver SNR confidence that distinguishes nonzero telemetry from zero-only placeholders

### Requirement: EFUSE-Derived TX Power Programming
The system SHALL provide an explicit TX power mode that computes RTL8812AU per-path/per-rate TXAGC register values from decoded EFUSE power data and records the calculation inputs and writes.

#### Scenario: TX power calculation is visible
- **WHEN** an operator enables EFUSE-derived TX power programming
- **THEN** the report MUST include decoded EFUSE source values, selected channel group, selected RF path, per-rate power indexes, regulatory or safety clamps, and before/write/after register evidence

#### Scenario: TX power override remains guarded
- **WHEN** an operator attempts to change TX power behavior
- **THEN** the command MUST require explicit RF-transmit authorization and MUST reject values outside the guarded RTL8812AU TX power index range

### Requirement: Calibration State Tracking
The system SHALL track and report IQK, LCK, thermal, RFE pinmux, and RF path calibration state for RF-quality runs, even when the implementation is still using a stop-gap, captured value, read-only probe, or runtime approximation.

#### Scenario: Stop-gap calibration is labeled
- **WHEN** a run uses planted, captured, or static calibration values
- **THEN** the report MUST label those values as stop-gap calibration and MUST include the register values that were applied

#### Scenario: Runtime calibration is verified
- **WHEN** a runtime calibration routine or approximation is enabled
- **THEN** the report MUST include the calibration routine name, affected registers, success/failure status, and before/after values sufficient to compare against Linux behavior

#### Scenario: Read-only IQK probe is labeled
- **WHEN** a run uses a read-only IQK probe profile
- **THEN** the report MUST include safe final-state IQK register readback, label profile-time RF-serial/page-C1/deep IQK evidence as deferred, and clearly state that runtime IQK calibration was not performed

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

### Requirement: Production Readiness Evidence
The system SHALL classify RF-quality production readiness using Linux peer preflight status, calibration profile evidence, receiver-backed WFB outcomes, and RX metadata confidence.

#### Scenario: Production evidence is incomplete
- **WHEN** a run lacks channel-state evidence, uses stop-gap calibration, or only has fallback RSSI metadata
- **THEN** the RF-quality report MUST keep the run usable for bench diagnostics but MUST NOT classify it as long-distance production-ready

#### Scenario: Linux peer isolation is not clean
- **WHEN** a receiver-backed run records required Linux peer isolation and the peer-isolation status is not `ok`
- **THEN** the RF-quality report MUST mark the outcome outside the production acceptance margin so decrypt errors or competing WFB traffic cannot be mistaken for RF loss

#### Scenario: Receiver decrypt errors are present
- **WHEN** a receiver-backed run records one or more Linux `wfb_rx` unable-decrypt events
- **THEN** the RF-quality report MUST mark the outcome outside the production acceptance margin even if marked payload recovery is otherwise near the close-range baseline

#### Scenario: Targeted parity is used
- **WHEN** a run uses targeted Linux-parity calibration overrides
- **THEN** the report MUST identify the profile and affected registers and MUST keep full IQK/LCK validation listed as remaining work unless the full routines have been ported and validated

### Requirement: Standalone IQK Evidence Handling
The system SHALL distinguish standalone IQK evidence from runtime IQK
calibration when evaluating RF-quality and range-readiness reports.

#### Scenario: Standalone IQK evidence is attachable
- **WHEN** a standalone IQK diagnostic artifact is supplied to an RF-quality or
  range-readiness review
- **THEN** the review material MUST identify the artifact path, diagnostic
  mode, cleanup status, and captured IQK evidence groups

#### Scenario: Standalone IQK evidence is not runtime calibration
- **WHEN** a report includes standalone IQK diagnostic evidence but no runtime
  IQK calibration routine was executed
- **THEN** the report MUST NOT classify the run as Linux-parity runtime IQK
  calibration

### Requirement: Runtime IQK RF-Quality Classification
The system SHALL distinguish runtime IQK calibration from captured or
standalone IQK evidence when evaluating RF-quality and range-readiness reports.

#### Scenario: Runtime IQK is classified as calibration
- **WHEN** an RF-quality run uses the guarded runtime IQK profile and cleanup
  succeeds
- **THEN** the report MAY classify IQK as runtime calibration and MUST include
  the runtime IQK artifact fields needed to compare the run with Linux behavior

#### Scenario: Runtime IQK failure is not hidden
- **WHEN** runtime IQK fails, times out, falls back, or cannot restore a saved
  selector/register state
- **THEN** the RF-quality report MUST preserve the failure evidence and MUST NOT
  classify the profile as Linux-parity runtime IQK

#### Scenario: Runtime IQK needs receiver-backed validation
- **WHEN** runtime IQK completes successfully on hardware
- **THEN** the profile MUST remain experimental until a receiver-backed
  close-range A/B run compares default, captured IQK, LCK, and runtime IQK under
  the same channel, bandwidth, rate, power mode, payload, and antenna geometry

### Requirement: Calibration Profile Comparison
The system SHALL support repeatable RF-quality comparisons across default, targeted parity, captured IQK/LCK, and runtime IQK calibration profiles without treating unvalidated experimental profiles as long-distance-ready.

#### Scenario: Profile labels preserved
- **WHEN** an RF-quality run uses any supported calibration profile
- **THEN** the report MUST include the runtime calibration class, evidence source, authorization state, and receiver-backed validation status

#### Scenario: Long-distance profile deferred
- **WHEN** receiver placement, antenna geometry, or Linux peer state cannot be controlled for long-distance validation
- **THEN** the system MUST keep the profile marked as requiring receiver-backed validation and continue supporting close-range or bench evidence collection

#### Scenario: Runtime IQK needs receiver-backed validation
- **WHEN** runtime IQK completes successfully on hardware
- **THEN** the profile MUST remain experimental until a receiver-backed close-range and long-distance A/B run compares default, captured IQK, LCK, and runtime IQK under the same channel, bandwidth, rate, power mode, payload, and antenna geometry

#### Scenario: Experimental calibration is quarantined on post-session failures
- **WHEN** a sustained receiver-backed A/B run for an experimental TX power or
  calibration mode logs post-session WFB decrypt failures or fails measured
  payload recovery
- **THEN** the system MUST keep that mode out of production defaults and provide
  direction-isolated regression evidence before the mode can be reconsidered for
  range work, while reporting pre-session decrypt failures separately as
  acquisition evidence
