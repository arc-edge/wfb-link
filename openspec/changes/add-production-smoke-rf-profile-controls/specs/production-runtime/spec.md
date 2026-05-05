## MODIFIED Requirements

### Requirement: Production Smoke Automation
The system SHALL provide repeatable production smoke automation for both remote
hardware-Mac deployment and local-adapter execution from the active checkout,
including guarded RF profile controls for production TX power and calibration
selection.

#### Scenario: Local adapter production smoke
- **WHEN** an operator runs the production smoke automation with local hardware
  mode enabled
- **THEN** the automation builds the current checkout, runs the RX-only and
  TX-positive production `radio-run` gates without SSH deployment, and validates
  clean TX submission plus runtime-owned RX outcome/frame-type telemetry

#### Scenario: Runtime-owned ready marker
- **WHEN** a production bridge loop reaches the point immediately before RX/TX
  processing begins
- **THEN** the runtime writes a JSON ready marker containing the source, channel,
  bandwidth, loop bounds, init/calibration flags, and runtime timestamp for
  automation that needs to start traffic only after radio initialization

#### Scenario: RF profile smoke selection
- **WHEN** an operator runs the production smoke automation with a non-default
  TX power mode or TX calibration profile selected
- **THEN** the automation passes the selected RF profile to the production
  `radio-run` command, applies the required guarded write authorization, and
  records the selected RF profile plus TX power evidence in the generated smoke
  summary
