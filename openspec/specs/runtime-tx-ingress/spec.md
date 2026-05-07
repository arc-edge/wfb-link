# Runtime TX Ingress Specification

## Purpose

Define runtime-owned UDP ingress socket setup and receiver lifecycle for
production WFB TX datagrams.

## Requirements

### Requirement: Runtime TX Ingress Binding
The runtime library SHALL bind production WFB TX ingress UDP sockets from an
ordered list of bind addresses and preserve each socket's report index.

#### Scenario: Multiple bind addresses are bound
- **WHEN** a production loop plan supplies a primary TX bind address and
  additional TX bind addresses
- **THEN** the runtime library binds each address in order and assigns report
  indexes matching that order

#### Scenario: Bind failure is reported
- **WHEN** a TX ingress UDP socket cannot be bound
- **THEN** the runtime library fails with a stable runtime error code before
  opening or using radio hardware

### Requirement: Runtime TX Ingress Receiver
The runtime library SHALL spawn bounded receiver threads for TX ingress sockets
and deliver queued datagrams with socket index, peer address, and payload bytes.

#### Scenario: Datagram is queued
- **WHEN** a datagram arrives on any bound TX ingress socket
- **THEN** the runtime receiver queues the datagram with the socket report
  index, peer address, and exact datagram bytes

#### Scenario: Receiver drops cleanly
- **WHEN** the TX ingress receiver is dropped
- **THEN** it signals all receiver threads to stop and joins them without
  requiring diagnostic code to manage thread shutdown

### Requirement: Runtime TX Ingress Socket Policy
The runtime library SHALL configure TX ingress sockets with production receive
buffer and read-timeout policy before receiver threads are started.

#### Scenario: Receive buffer configuration fails
- **WHEN** the operating system rejects the requested UDP receive buffer size
- **THEN** the runtime library fails socket setup with a stable runtime error
  code

#### Scenario: Read timeout configuration fails
- **WHEN** the operating system rejects the requested receiver read timeout
- **THEN** the runtime library fails receiver setup with a stable runtime error
  code

### Requirement: Runtime TX Ingress Telemetry
The runtime library SHALL expose production TX ingress telemetry that separates
UDP socket ingress from bridge-loop TX processing and radio submission.

#### Scenario: Datagram ingress is counted
- **WHEN** a datagram arrives on a production TX ingress socket
- **THEN** the runtime receiver increments datagram and byte ingress counters
  before queueing the datagram for bridge-loop processing

#### Scenario: Receiver queue send fails
- **WHEN** an ingress receiver cannot send a datagram into the bridge-loop queue
- **THEN** the runtime receiver increments a stable queue-send-failure counter
  before stopping that receiver thread

#### Scenario: Final report separates ingress from processing
- **WHEN** a production runtime flow emits final TX telemetry
- **THEN** the report includes ingress datagram count, ingress byte count,
  queue-send failures, pending ingress datagrams, processed datagrams,
  submitted frames, failed submissions, drops, submitted bytes, and observed
  WFB TX channel IDs
