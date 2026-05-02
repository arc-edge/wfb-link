## ADDED Requirements

### Requirement: RTL8812AU IQK Probe
The system SHALL provide an opt-in RTL8812AU IQK profile that labels IQK staging evidence without running the IQK calibration sweep, switching BB pages, using RF serial backup reads, adding profile-time hardware reads, or modifying RF transmit power.

#### Scenario: IQK profile labels deferred probe state
- **WHEN** an operator selects the RTL8812AU IQK probe profile on an initialized adapter
- **THEN** the system reports that live IQK hardware probing is deferred and identifies the existing RF calibration IQK readback as the safe source of pre-TX IQK register evidence

#### Scenario: IQK probe remains read-only
- **WHEN** the IQK probe profile completes successfully
- **THEN** the system labels the profile as read-only deferred probe evidence and MUST NOT report runtime IQK calibration as completed

#### Scenario: IQK deep probe is unsafe for live TX
- **WHEN** deeper IQK evidence requires profile-time hardware reads, RF serial backup reads, or page-C1 latch access
- **THEN** the live pre-TX profile MUST skip that evidence and label it as deferred to a standalone diagnostic or full IQK port
