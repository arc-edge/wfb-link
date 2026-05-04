## MODIFIED Requirements

### Requirement: Runtime Bridge Loop Plan
The runtime library SHALL expose a production WFB bridge-loop plan, TX ingress
lifecycle, and loop executor that expand TX ingress sockets, RX forwarding
targets, runtime bounds, WFB metadata, and scheduling policy without depending
on diagnostic command argument structs.

#### Scenario: Valid loop plan is built
- **WHEN** a production runtime flow supplies a TX bind address, additional TX
  bind addresses, RX timeout, TX burst limit, max datagrams, and WFB forwarding
  targets
- **THEN** the runtime library returns a loop plan containing ordered TX bind
  addresses, validated RX forwarding configs, WFB metadata, and runtime bounds

#### Scenario: TX ingress starts from plan
- **WHEN** the production loop plan is ready and execution begins
- **THEN** the runtime library binds TX ingress sockets and starts receiver
  threads using the ordered TX bind addresses from the plan

#### Scenario: Runtime executor drives loop cadence
- **WHEN** the production bridge loop runs
- **THEN** the runtime library controls signal stop checks, duration stop
  checks, max-datagram stop checks, TX burst draining, and RX timeout
  calculation

#### Scenario: Invalid loop bounds are rejected
- **WHEN** a production runtime flow supplies zero RX timeout or zero TX burst
  limit
- **THEN** the runtime library rejects the loop plan before socket binding or
  USB open with a stable runtime error code

### Requirement: Runtime Loop Telemetry
The runtime library SHALL own production bridge-loop telemetry types for RX/TX
counts and loop execution outcomes used by production reports.

#### Scenario: Executor reports stop reason
- **WHEN** a runtime-owned bridge loop exits normally
- **THEN** the runtime library reports a stable stop reason for signal,
  duration elapsed, or TX datagram limit

#### Scenario: Telemetry is report-neutral
- **WHEN** a production loop finishes or is adapted from a diagnostic execution
  harness
- **THEN** runtime telemetry records RX metadata coverage, TX datagrams,
  submitted frames, failures, drops, byte counts, loop stop reason, and USB
  counters without depending on diagnostic report structs
