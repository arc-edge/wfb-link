## ADDED Requirements

### Requirement: Runtime Bridge Loop Executor
The runtime library SHALL provide a callback-driven bridge loop executor that
owns production loop cadence without depending on diagnostic report types.

#### Scenario: Executor drains bounded TX bursts
- **WHEN** queued TX datagrams are available and TX burst limit is configured
- **THEN** the executor invokes TX work no more than the configured burst limit
  before running RX work

#### Scenario: Executor stops on unbounded max datagrams
- **WHEN** duration is unbounded and the configured max datagram count is
  reached
- **THEN** the executor stops with the TX datagram limit stop reason

#### Scenario: Executor keeps duration-bounded runs alive
- **WHEN** duration is bounded and the configured max datagram count is reached
- **THEN** the executor limits further TX work but keeps running RX work until
  duration expires or signal stop is requested

#### Scenario: Executor computes bounded RX timeout
- **WHEN** a duration-bounded run is close to its deadline
- **THEN** the executor provides an RX timeout no greater than the remaining
  duration
