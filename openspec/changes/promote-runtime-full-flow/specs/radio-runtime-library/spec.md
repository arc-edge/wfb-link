## ADDED Requirements

### Requirement: Runtime Same-Session Init API
The runtime library SHALL expose a report-neutral RTL8812AU same-session initialization API for production callers and diagnostics.

#### Scenario: Runtime init executes on a runtime session
- **WHEN** a caller provides a `RuntimeRadioSession`, validated init configuration, normalized init assets, and required hardware-write authorization
- **THEN** the runtime library performs the same-session init phases and returns phase identifiers, counter deltas, calibration decisions, and final readiness state without using diagnostic command argument or report types

#### Scenario: Runtime init rejects unsafe calibration
- **WHEN** a caller selects a calibration profile that requires live register write authorization without providing that authorization
- **THEN** the runtime library rejects initialization before applying the profile and returns a structured runtime error

### Requirement: Runtime Calibration Selection
The runtime library SHALL provide production-facing calibration selection for RTL8812AU init.

#### Scenario: Default production calibration selected
- **WHEN** a caller selects the default calibration profile
- **THEN** the runtime library classifies the profile as production-safe default behavior and does not run targeted parity, captured IQK/LCK, or runtime IQK sequences

#### Scenario: Experimental calibration selected
- **WHEN** a caller selects targeted parity, captured IQK/LCK, or runtime IQK calibration
- **THEN** the runtime library classifies the profile as experimental, records the evidence source, and reports whether receiver-backed validation is still required

## MODIFIED Requirements

### Requirement: Diagnostic Uses Runtime Init Sequence
Diagnostic same-session init SHALL use runtime-owned phase ordering and runtime-owned same-session init execution policy.

#### Scenario: Diagnostic executes LLT and firmware
- **WHEN** diagnostic same-session init decides whether LLT or firmware runs first
- **THEN** it uses the runtime init sequence rather than diagnostic-only branching

#### Scenario: Diagnostic wraps runtime init result
- **WHEN** a diagnostic command runs retained same-session init
- **THEN** it calls the runtime same-session init API and maps runtime phase, counter, and calibration evidence into existing diagnostic report fields
