## ADDED Requirements

### Requirement: RTL8812AU Runtime IQK Calibration
The system SHALL provide a guarded RTL8812AU runtime IQK calibration profile
that can execute the Linux 8812A IQK sequence after retained init and channel
setup without making runtime IQK the default path.

#### Scenario: Runtime IQK requires authorization
- **WHEN** an operator selects the runtime IQK profile without explicit
  hardware-write authorization
- **THEN** the system MUST reject the run before claiming USB or changing RF
  state

#### Scenario: Runtime IQK records per-path result evidence
- **WHEN** the runtime IQK profile runs on an initialized RTL8812AU adapter
- **THEN** the system reports per-path TX IQK status, RX IQK status, retry
  counts, selected TX/RX IQC values, fallback use, affected registers, USB
  counters, and cleanup status

#### Scenario: Runtime IQK restores saved state
- **WHEN** runtime IQK exits after success or failure
- **THEN** the system attempts to restore saved MAC/BB, AFE, RF, BB page-select,
  and HSSI selector state and reports any restore failure separately from the
  calibration result

#### Scenario: Runtime IQK remains opt-in
- **WHEN** an operator uses the default calibration profile
- **THEN** the system MUST NOT run the runtime IQK sequence or classify IQK as
  completed
