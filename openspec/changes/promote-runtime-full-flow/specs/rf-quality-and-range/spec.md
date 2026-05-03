## MODIFIED Requirements

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
