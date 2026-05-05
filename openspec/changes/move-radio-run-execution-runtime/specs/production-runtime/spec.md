## ADDED Requirements

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
