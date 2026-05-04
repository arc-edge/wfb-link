## ADDED Requirements

### Requirement: Production Flow Types
The runtime library SHALL expose report-neutral production flow configuration
and report types for full WFB RX/TX operation.

#### Scenario: Production config is diagnostic-free
- **WHEN** a caller builds production flow configuration
- **THEN** the type includes transport selection, adapter selector, channel,
  bandwidth, WFB socket settings, firmware path, calibration profile, duration,
  ready-marker, and required authorization state without including
  diagnostic-only register experiment fields

#### Scenario: Production report is diagnostic-free
- **WHEN** a production flow finishes or fails before opening hardware
- **THEN** the runtime-owned report records stable production telemetry fields
  without depending on diagnostic report structs

### Requirement: Production Flow Validation
The runtime library SHALL validate production flow configuration before hardware
open when a failure can be determined without touching USB.

#### Scenario: Missing authorization rejected
- **WHEN** production flow configuration requests RF transmit or live register
  calibration without the required explicit authorization
- **THEN** validation fails before opening USB with a stable runtime error code

#### Scenario: Invalid runtime bounds rejected
- **WHEN** production flow configuration includes invalid runtime bounds such as
  an unusable channel, unsupported bandwidth, or zero TX burst limit
- **THEN** validation fails before opening USB with a stable runtime error code
