## ADDED Requirements

### Requirement: Runtime-Owned Production Service Health
The runtime library SHALL expose production service health types and
classification helpers that summarize a production radio flow without depending
on diagnostic command argument structs or diagnostic report structs.

#### Scenario: Health model classifies lifecycle state
- **WHEN** a production runtime flow is validating, initializing, ready,
  running, stopping, exited successfully, or exited with failure
- **THEN** the runtime health model represents that state with a stable
  serialized lifecycle value and timestamp

#### Scenario: Health model summarizes runtime telemetry
- **WHEN** production runtime telemetry is available
- **THEN** the health model summarizes init readiness, heartbeat counters, TX
  submitted/failed/dropped counts, RX parsed/forwarded/dropped counts, signal
  sample availability, stop reason, result, and last error without requiring
  diagnostic report structs

#### Scenario: Health model recommends operator action
- **WHEN** a production flow has not started, is healthy, degraded, failed, or
  stopped by signal
- **THEN** the runtime health model exposes a stable operator-action
  classification suitable for service supervisors and automation logs

### Requirement: Runtime Production Health Writer
The runtime library SHALL provide a best-effort JSON health writer for
production callers that need a supervisor-readable file.

#### Scenario: Health writer writes valid JSON
- **WHEN** a production caller supplies a health artifact path and a runtime
  service health value
- **THEN** the runtime writes pretty JSON with a trailing newline and returns a
  structured runtime error on serialization or filesystem failure

#### Scenario: Health writer accepts absent path
- **WHEN** a production caller supplies no health artifact path
- **THEN** the runtime health writer is a no-op and returns success

#### Scenario: Health writer preserves report independence
- **WHEN** a health artifact is written during a production flow
- **THEN** the artifact contains supervisor-facing summary fields and MUST NOT
  contain diagnostic-only register experiment, PCAP, frame JSONL, or trace
  replay fields
