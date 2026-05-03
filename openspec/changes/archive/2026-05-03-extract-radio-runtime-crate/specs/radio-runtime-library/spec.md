## ADDED Requirements

### Requirement: Runtime Calibration Policy
The system SHALL expose calibration profile policy from a reusable runtime library rather than requiring callers to depend on the diagnostic binary.

#### Scenario: Calibration profile classified
- **WHEN** a caller selects a supported TX calibration profile
- **THEN** the runtime library reports whether it is the default profile, which calibration class it represents, and whether it requires live register write authorization

#### Scenario: Runtime IQK protected
- **WHEN** a caller selects the runtime RTL8812A IQK profile
- **THEN** the runtime library marks the profile as requiring live register write authorization

### Requirement: Diagnostic Commands Use Runtime Policy
Diagnostic commands SHALL call the runtime library for stable calibration policy while retaining diagnostic-only CLI parsing and hardware execution code.

#### Scenario: Diagnostic authorization check
- **WHEN** a diagnostic command validates a selected calibration profile
- **THEN** it uses the runtime library decision to accept or reject the command before opening the adapter

#### Scenario: Diagnostic report classification
- **WHEN** a diagnostic RF-quality report classifies calibration state before TX
- **THEN** it uses the runtime library calibration class and preserves the existing report enum values

### Requirement: Runtime Boundary Documentation
The system SHALL document which radio responsibilities are owned by the runtime crate and which remain diagnostic-only pending later migration.

#### Scenario: Developer reviews runtime boundary
- **WHEN** a developer needs to move another radio path out of the diagnostic binary
- **THEN** the repository documentation identifies the current runtime-owned responsibilities and the next migration targets
