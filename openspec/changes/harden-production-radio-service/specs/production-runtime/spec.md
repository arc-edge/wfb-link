## ADDED Requirements

### Requirement: Production Runtime Config File
The production `radio-run` command SHALL accept a service-oriented config file
that maps to runtime-owned production flow settings without exposing
diagnostic-only experiments.

#### Scenario: Config-only production run starts
- **WHEN** an operator runs `radio-run --config <path>` with a valid production
  config file containing adapter, channel, bandwidth, firmware, WFB loop,
  heartbeat, TX-power, calibration, and authorization settings
- **THEN** the command loads the file, maps it into runtime-owned production
  configuration and execution inputs, and starts the same runtime execution path
  used by the equivalent CLI-only invocation

#### Scenario: CLI overrides config value
- **WHEN** an operator supplies both `--config <path>` and an explicit CLI flag
  for a setting also present in the file
- **THEN** the explicit CLI flag value takes precedence and the report records
  the effective setting used by the run

#### Scenario: Config rejects diagnostic-only fields
- **WHEN** a production config file includes diagnostic-only register pokes, TX
  status probes, PCAP paths, frame JSONL paths, or trace replay settings
- **THEN** `radio-run` rejects the config before opening USB and returns a
  stable production error code

### Requirement: Production Runtime Service Health Artifact
The production `radio-run` command SHALL support writing a service health
artifact that reports lifecycle state and supervisor-facing health independently
from the one-shot ready marker and final detailed report.

#### Scenario: Health file records startup and ready states
- **WHEN** `radio-run` is started with `--health-file <path>`
- **THEN** it writes a JSON health artifact during validation/startup and
  updates it when radio initialization reaches the existing ready-marker point

#### Scenario: Health file records exit state
- **WHEN** `radio-run` exits after success, failure, or signal stop
- **THEN** the health artifact records the final lifecycle state, stop reason,
  result, last error when present, heartbeat counters, TX drop/failure summary,
  RX forwarding summary, and path to the final report when one was requested

#### Scenario: Health artifact is optional
- **WHEN** `radio-run` is started without `--health-file`
- **THEN** the production flow preserves existing ready-marker and final report
  behavior without requiring a health artifact path

#### Scenario: Health write failure is reported
- **WHEN** `radio-run` cannot serialize or write the configured health artifact
  before RF transmission begins
- **THEN** it fails before starting RX/TX traffic with a stable production error
  code
