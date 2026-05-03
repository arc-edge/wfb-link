## ADDED Requirements

### Requirement: Runtime Register Phase Execution
The runtime library SHALL own reusable RTL8812AU register phase execution helpers for production callers and diagnostics.

#### Scenario: TX scheduler tail executed by runtime
- **WHEN** a caller provides RTL8812AU register access and runtime counters
- **THEN** the runtime executes the TX scheduler tail register reads and writes, returns the phase identifier, register write count, and counter delta, and leaves diagnostic-specific report formatting to the caller

#### Scenario: Monitor receive setup executed by runtime
- **WHEN** a caller requests monitor receive filter or monitor opmode programming
- **THEN** the runtime programs the monitor receive registers, returns raw MSR/RCR/RXFLTMAP2 evidence and counter deltas, and does not require the diagnostic crate to own those register writes
