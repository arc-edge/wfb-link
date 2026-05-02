## ADDED Requirements

### Requirement: Hardware Mac Deploy Sync
The system SHALL support an opt-in deploy mode that copies the local checkout to a separate hardware-Mac run directory before starting RF-quality automation.

#### Scenario: Deploy sync is requested
- **WHEN** the operator enables deploy sync with a deploy path distinct from the hardware-Mac working checkout path
- **THEN** the command copies the local checkout to the deploy path, excludes repository metadata and build outputs, runs the bridge from the deploy path, and records the deploy path in run configuration

#### Scenario: Deploy sync would overwrite the working checkout
- **WHEN** the operator enables deploy sync with a deploy path equal to the configured hardware-Mac working checkout path
- **THEN** the command fails before syncing and reports that deploy sync requires a separate destination unless explicitly overridden

#### Scenario: Deploy sync is not requested
- **WHEN** the operator leaves deploy sync disabled
- **THEN** the command preserves the existing hardware-Mac checkout behavior and does not copy local files to the hardware Mac
