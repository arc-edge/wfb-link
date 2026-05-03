## ADDED Requirements

### Requirement: RTL8812AU Init Phase Sequence
The runtime library SHALL define the RTL8812AU same-session initialization phase sequence.

#### Scenario: Default phase order requested
- **WHEN** a caller requests the default same-session init sequence
- **THEN** the runtime library returns firmware before LLT and preserves the remaining MAC, queue, BB, RF, channel, calibration, and TX scheduler phases

#### Scenario: Linux phase order requested
- **WHEN** a caller requests Linux-order same-session init
- **THEN** the runtime library returns LLT before firmware and preserves the remaining phase order

### Requirement: Diagnostic Uses Runtime Init Sequence
Diagnostic same-session init SHALL use runtime-owned phase ordering policy.

#### Scenario: Diagnostic executes LLT and firmware
- **WHEN** diagnostic same-session init decides whether LLT or firmware runs first
- **THEN** it uses the runtime init sequence rather than diagnostic-only branching
