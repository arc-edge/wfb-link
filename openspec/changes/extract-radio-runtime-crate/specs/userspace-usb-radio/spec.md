## MODIFIED Requirements

### Requirement: RTL8812AU Initialization
The system SHALL initialize RTL8812AU hardware into a monitor-capable state using userspace USB control and bulk transfers, with stable runtime-facing policy exposed from a reusable runtime library rather than only from diagnostic commands.

#### Scenario: Initialization completes
- **WHEN** the system opens a supported RTL8812AU adapter
- **THEN** it powers the chip, loads firmware, initializes LLT and queues, configures MAC/BB/RF state, assigns a local MAC address, applies runtime-selected calibration policy, and enters a raw RX/TX ready state

#### Scenario: Firmware load fails
- **WHEN** firmware download or firmware readiness polling fails
- **THEN** initialization stops and reports the failed phase, register, or USB transfer

#### Scenario: Runtime policy available without diagnostic CLI
- **WHEN** a production runtime caller configures RTL8812AU initialization
- **THEN** it can use the runtime library to evaluate stable calibration policy without depending on diagnostic CLI parsing
