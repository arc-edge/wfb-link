# Production Runtime Specification

## Purpose

Define the production-oriented WFB runtime command surface and its diagnostic
compatibility boundary.
## Requirements
### Requirement: Production Runtime Command Surface
The system SHALL provide a production-oriented WFB runtime entry point that
opens, initializes, receives, and transmits through runtime-owned types rather
than diagnostic bridge argument or report types.

#### Scenario: Production command starts full flow
- **WHEN** an operator starts the production runtime command with adapter,
  channel, bandwidth, WFB UDP, firmware, calibration profile, and required
  authorization settings
- **THEN** the command translates those settings into runtime-owned production
  configuration and runs the full RX/TX flow without exposing diagnostic-only
  register experiment flags

#### Scenario: Production command accepts TX pacing
- **WHEN** an operator starts the production runtime command with a nonzero
  `--tx-min-interval-us`
- **THEN** the command passes that microsecond TX pacing interval into the
  runtime-owned bridge loop and records it in readiness/configuration outputs

#### Scenario: Production report is runtime-owned
- **WHEN** the production runtime command exits
- **THEN** it emits a runtime-owned report containing adapter identity,
  endpoints, init readiness, calibration classification, RX/TX telemetry, RX
  metadata coverage, RX outcome/frame-type counters, USB counters, stop reason,
  and error state

### Requirement: Production Smoke Automation
The system SHALL provide repeatable production smoke automation for both remote
hardware-Mac deployment and local-adapter execution from the active checkout.

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

### Requirement: Production Runtime LED Heartbeat Hook
The production runtime command SHALL invoke the runtime LED heartbeat at each
iteration of the production bridge loop, and SHALL invoke its turn-off method
exactly once when the production runtime flow returns.

#### Scenario: Heartbeat is invoked from the bridge loop
- **WHEN** the production runtime command runs the bridge loop with heartbeat
  enabled by default
- **THEN** the production command calls the heartbeat's `maybe_toggle` on every
  loop iteration with the current `Instant::now()`

#### Scenario: Heartbeat is turned off on flow exit
- **WHEN** the production runtime flow returns on success or failure
- **THEN** the production command calls the heartbeat's `turn_off` exactly once
  before the runtime session is dropped

### Requirement: Production Runtime Heartbeat Configuration Flags
The production runtime command SHALL accept `--no-heartbeat-led` to disable the
heartbeat and `--heartbeat-led-half-period-ms <ms>` to override the toggle
half-period within bounded limits.

#### Scenario: Default flags result in enabled 500 ms half-period
- **WHEN** the operator runs the production runtime command without any
  heartbeat flags
- **THEN** the heartbeat is enabled
- **AND** the half-period is 500 ms

#### Scenario: --no-heartbeat-led disables the heartbeat
- **WHEN** the operator passes `--no-heartbeat-led`
- **THEN** the heartbeat is configured with `enabled = false`

#### Scenario: --heartbeat-led-half-period-ms within range applies
- **WHEN** the operator passes `--heartbeat-led-half-period-ms <ms>` with
  `50 <= ms <= 5000`
- **THEN** the heartbeat half-period is `ms` milliseconds

#### Scenario: --heartbeat-led-half-period-ms out of range is rejected
- **WHEN** the operator passes `--heartbeat-led-half-period-ms <ms>` with
  `ms < 50` or `ms > 5000`
- **THEN** the production runtime command fails at argument validation before
  opening the radio

### Requirement: Production Runtime Report Surfaces Heartbeat Counters
The production runtime report SHALL include a `heartbeat_led` block with toggle
counters, the configured half-period, and the enabled flag, so operators can
verify the heartbeat ran.

#### Scenario: Report includes heartbeat block on a normal run
- **WHEN** the production runtime command exits after a normal run
- **THEN** its report includes a `heartbeat_led` field with at least:
  `enabled`, `half_period_ms`, `toggles_attempted`, `toggles_succeeded`, and
  `toggles_failed`

#### Scenario: Report reflects disabled heartbeat
- **WHEN** the operator ran the command with `--no-heartbeat-led`
- **THEN** the report's `heartbeat_led.enabled` is false
- **AND** the toggle counters are zero

### Requirement: Diagnostic Compatibility Boundary
The system SHALL keep diagnostic commands available while making the production
runtime path independent of diagnostic-only CLI and report structs.

#### Scenario: Diagnostics retain experiments
- **WHEN** an operator needs register pokes, TX status probes, trace replay, or
  other bring-up experiments
- **THEN** those options remain available through diagnostic commands and remain
  absent from the production runtime command surface

#### Scenario: Runtime-flow compatibility preserved
- **WHEN** existing automation calls the diagnostic `runtime-flow` command
- **THEN** the command continues to produce compatible production-shaped
  telemetry while the new production entry point is introduced

### Requirement: Production Command Uses Runtime Execution
The production `radio-run` command SHALL call the runtime-owned production flow
execution API directly and SHALL NOT adapt through diagnostic `runtime-flow` or
`bridge-run` report structs for normal execution.

#### Scenario: radio-run dispatches to runtime execution
- **WHEN** an operator runs `radio-run` with valid production settings
- **THEN** the diagnostic binary maps CLI inputs into runtime-owned
  configuration and invokes the runtime-owned production flow execution API
  directly

#### Scenario: radio-run preserves command contract
- **WHEN** `radio-run` exits after using the runtime execution API
- **THEN** the command preserves the existing operator-facing CLI surface, JSON
  report fields, text output summaries, ready-marker behavior, heartbeat
  behavior, RX/TX counters, calibration evidence, and error classification

#### Scenario: radio-run remains free of diagnostic experiments
- **WHEN** an operator inspects or invokes `radio-run`
- **THEN** diagnostic-only register experiments, TX status probes, PCAP output,
  frame JSONL output, trace replay, and legacy bring-up report flags remain
  absent from the production command surface

### Requirement: Production Smoke Gates Runtime Execution
Production smoke automation SHALL verify that the direct runtime execution path
preserves the accepted production behavior before the migration is considered
complete.

#### Scenario: Production smoke uses direct runtime execution
- **WHEN** production smoke automation runs after this change
- **THEN** RX-only and TX-positive `radio-run` gates execute through the direct
  runtime execution API and validate init readiness, clean TX submission,
  runtime-owned RX telemetry, heartbeat reporting, ready-marker output, and zero
  unexpected TX drops or failures

#### Scenario: Receiver-backed smoke remains compatible
- **WHEN** duplex or RF-quality automation runs `radio-run`
- **THEN** it can continue to parse the same report fields and gate on peer
  recovery, decrypt failures, TX failures/drops, RX forwarding snapshots,
  source timing, and signal summaries without command-specific changes
