## MODIFIED Requirements

### Requirement: WFB TX Benchmark Runtime
The system SHALL submit bounded WFB TX benchmark traffic through the userspace USB radio runtime without requiring a Linux monitor interface.

#### Scenario: Generated benchmark submissions use runtime TX
- **WHEN** `bridge-tx-bench` generates valid WFB datagrams for the active channel
- **THEN** it submits each generated frame through the runtime radio session and records runtime-aligned TX counters

#### Scenario: Exact packet replay uses runtime TX
- **WHEN** `bridge-tx-bench` is supplied an exact descriptor-prefixed packet override
- **THEN** it submits the packet through the runtime radio session bulk-OUT path and records short-write or USB errors in the submit counters
