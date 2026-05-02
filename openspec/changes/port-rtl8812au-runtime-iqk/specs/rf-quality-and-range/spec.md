## ADDED Requirements

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
