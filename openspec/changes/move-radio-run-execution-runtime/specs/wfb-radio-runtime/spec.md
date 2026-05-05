## ADDED Requirements

### Requirement: Runtime-Owned Production Flow Execution
The runtime library SHALL provide a production flow execution API that owns the
end-to-end `radio-run` hardware lifecycle without depending on diagnostic
command argument structs or diagnostic report structs.

#### Scenario: Runtime execution runs the production flow
- **WHEN** a caller supplies a validated production runtime configuration and
  required runtime-owned inputs
- **THEN** the runtime library opens the adapter, initializes the radio, applies
  selected TX-power and calibration policy, writes the ready marker, starts TX
  ingress, runs the production bridge loop, processes RX/TX work, drives the LED
  heartbeat, and returns a `ProductionRuntimeFlowReport`

#### Scenario: Runtime execution rejects invalid config before USB
- **WHEN** a caller supplies invalid production runtime configuration
- **THEN** the runtime library returns a failed `ProductionRuntimeFlowReport`
  without opening or claiming the USB adapter

#### Scenario: Runtime execution preserves production telemetry
- **WHEN** the runtime-owned production flow exits
- **THEN** the report includes adapter identity, endpoints, channel, bandwidth,
  init readiness, calibration evidence, TX-power evidence when present, RX
  signal and frame-type counters, RX forwarding snapshots, TX counters, USB
  counters, heartbeat counters, stop reason, result, and error state

### Requirement: Runtime Flow Excludes Diagnostic Side Outputs
The runtime-owned production flow SHALL keep diagnostic-only side outputs and
experiments outside the production execution API.

#### Scenario: Diagnostic captures remain outside runtime execution
- **WHEN** a production flow runs through the runtime execution API
- **THEN** the API does not require or emit PCAP paths, frame JSONL paths, trace
  replay artifacts, TX-status probes, or generic register experiment reports

#### Scenario: Diagnostic commands remain available
- **WHEN** an operator needs PCAP/JSONL captures, TX-status probes, generic
  register writes, trace replay, or bring-up reports
- **THEN** those workflows remain available through diagnostic commands rather
  than the production runtime execution API
