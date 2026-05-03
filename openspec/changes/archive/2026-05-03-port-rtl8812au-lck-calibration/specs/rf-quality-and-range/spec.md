## MODIFIED Requirements

### Requirement: Calibration State Tracking
The system SHALL track and report IQK, LCK, thermal, RFE pinmux, and RF path calibration state for RF-quality runs, even when the implementation is still using a stop-gap or captured value.

#### Scenario: Stop-gap calibration is labeled
- **WHEN** a run uses planted, captured, or static calibration values
- **THEN** the report MUST label those values as stop-gap calibration and MUST include the register values that were applied

#### Scenario: Runtime calibration is verified
- **WHEN** a runtime calibration routine or approximation is enabled
- **THEN** the report MUST include the calibration routine name, affected registers, success/failure status, and before/after values sufficient to compare against Linux behavior

#### Scenario: Runtime LCK is enabled
- **WHEN** a run uses the RTL8812A LCK calibration profile
- **THEN** the report MUST include RF readback and restore evidence for RF LCK and RF CHNLBW while continuing to mark IQK as remaining work until IQK is ported and validated
