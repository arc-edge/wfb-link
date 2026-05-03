## ADDED Requirements

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
