## MODIFIED Requirements

### Requirement: Runtime Ownership Documentation
The system SHALL document the current boundary between production-facing runtime code and diagnostic harness code.

#### Scenario: Runtime session ownership is documented
- **WHEN** live RX/TX commands use runtime session APIs for frame movement
- **THEN** the runtime-boundary documentation lists session I/O and counters as runtime-owned while leaving CLI/report orchestration diagnostic-owned
