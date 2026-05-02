## MODIFIED Requirements

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

### Requirement: RF Quality Report
The system SHALL emit structured RF-quality reports for macOS WFB runs that capture the RF, descriptor, calibration, WFB, and receiver evidence needed to evaluate range readiness.

#### Scenario: RF state is reportable
- **WHEN** a macOS RF-quality run completes
- **THEN** the report MUST include channel, bandwidth, TX rate/MCS, TX descriptor profile, TX queue, MACID, rate ID, retry/fallback settings, RFE type, EFUSE summary, TX power mode, TX power register evidence, calibration state, and any selected TX calibration profile evidence

#### Scenario: WFB outcome is reportable
- **WHEN** a macOS RF-quality run forwards or injects WFB traffic
- **THEN** the report MUST include submitted datagrams, recovered payloads when available, malformed/dropped counters, FEC/source payload settings, throughput, CPU usage, receiver artifact paths, expected-versus-observed datagram evidence for short FEC runs, and receiver session/decrypt health when supplied by the run automation
