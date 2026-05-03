## ADDED Requirements

### Requirement: Production Readiness Evidence
The system SHALL classify RF-quality production readiness using Linux peer preflight status, calibration profile evidence, receiver-backed WFB outcomes, and RX metadata confidence.

#### Scenario: Production evidence is incomplete
- **WHEN** a run lacks channel-state evidence, uses stop-gap calibration, or only has fallback RSSI metadata
- **THEN** the RF-quality report MUST keep the run usable for bench diagnostics but MUST NOT classify it as long-distance production-ready

#### Scenario: Targeted parity is used
- **WHEN** a run uses targeted Linux-parity calibration overrides
- **THEN** the report MUST identify the profile and affected registers and MUST keep full IQK/LCK validation listed as remaining work unless the full routines have been ported and validated
