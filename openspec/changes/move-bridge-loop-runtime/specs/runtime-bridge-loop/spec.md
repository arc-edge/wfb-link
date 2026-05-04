## ADDED Requirements

### Requirement: Runtime Bridge Loop Plan
The runtime library SHALL expose a production WFB bridge-loop plan that expands
TX ingress sockets, RX forwarding targets, runtime bounds, and WFB metadata
without depending on diagnostic command argument structs.

#### Scenario: Valid loop plan is built
- **WHEN** a production runtime flow supplies a TX bind address, additional TX
  bind addresses, RX timeout, TX burst limit, max datagrams, and WFB forwarding
  targets
- **THEN** the runtime library returns a loop plan containing ordered TX bind
  addresses, validated RX forwarding configs, WFB metadata, and runtime bounds

#### Scenario: Invalid loop bounds are rejected
- **WHEN** a production runtime flow supplies zero RX timeout or zero TX burst
  limit
- **THEN** the runtime library rejects the loop plan before socket binding or
  USB open with a stable runtime error code

### Requirement: Runtime WFB Forward Validation
The runtime library SHALL validate WFB RX forwarding targets using WFB channel
ID rules before production execution begins.

#### Scenario: Aggregator requires a complete channel filter
- **WHEN** a production runtime flow supplies an RX aggregator without both WFB
  link ID and radio port
- **THEN** runtime validation fails before socket binding or USB open

#### Scenario: Repeated forwarding target can be self-contained
- **WHEN** a production runtime flow supplies an RX forwarding target with its
  own link ID, radio port, and aggregator address
- **THEN** runtime validation accepts the target without requiring global WFB
  link settings

#### Scenario: Defaulted forwarding target requires global link ID
- **WHEN** a production runtime flow supplies a repeated RX forwarding target
  with radio port and aggregator address but no target link ID
- **THEN** runtime validation requires a global WFB link ID and fails without it

### Requirement: Runtime Loop Telemetry
The runtime library SHALL own production bridge-loop telemetry types for RX/TX
counts used by production reports.

#### Scenario: Telemetry is report-neutral
- **WHEN** a production loop finishes or is adapted from a diagnostic execution
  harness
- **THEN** runtime telemetry records RX metadata coverage, TX datagrams,
  submitted frames, failures, drops, byte counts, and USB counters without
  depending on diagnostic report structs
