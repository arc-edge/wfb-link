## ADDED Requirements

### Requirement: Standalone Production Service Binary
The system SHALL provide a standalone production service binary that starts the
runtime-owned WFB radio flow without requiring operators to invoke the
diagnostic binary.

#### Scenario: Service binary starts from reviewed config
- **WHEN** an operator runs the production service binary with `--config <path>`
  and required live-operation acknowledgements
- **THEN** the binary loads the reviewed production config, applies supported
  CLI overrides, and invokes the runtime-owned production flow execution path

#### Scenario: Service binary emits production artifacts
- **WHEN** the production service binary runs with report, ready-marker, and
  health-file paths
- **THEN** it writes the same production report, ready marker, and service
  health artifact schema used by the accepted `radio-run` production path

#### Scenario: Service binary excludes diagnostic experiments
- **WHEN** an operator inspects or invokes the production service binary
- **THEN** diagnostic-only register experiments, TX status probes, PCAP output,
  frame JSONL output, trace replay, and generic bring-up commands are absent
  from its command surface

### Requirement: Shared Production Command Support
The system SHALL share production config parsing, CLI override merging, runtime
input construction, and report writing between the diagnostic compatibility
command and the standalone production service binary.

#### Scenario: Diagnostic and service config merge match
- **WHEN** equivalent config and CLI override inputs are supplied to
  `wfb-radio-diag radio-run` and the production service binary
- **THEN** both command paths resolve the same production runtime configuration,
  runtime execution inputs, authorization policy, and artifact paths

#### Scenario: Diagnostic compatibility remains available
- **WHEN** existing automation invokes `wfb-radio-diag radio-run`
- **THEN** the command continues to run through the shared production command
  support and preserves the existing report fields, ready-marker behavior,
  health behavior, and error classification

### Requirement: Production Smoke Command Selection
Production smoke automation SHALL be able to run the standalone production
service binary while retaining an explicit diagnostic compatibility fallback.

#### Scenario: Smoke records command surface
- **WHEN** production or receiver-backed smoke automation starts the radio flow
- **THEN** the smoke summary records whether the standalone production service
  binary or diagnostic compatibility command was used

#### Scenario: Receiver-backed gate exercises service binary
- **WHEN** the production service binary is selected for receiver-backed smoke
- **THEN** the automation validates M2L/L2M recovery, decrypt gates, TX
  failures/drops, RX forwarding, source timing, signal summaries, ready marker,
  health artifact, and final report using the same robust tuple gates
