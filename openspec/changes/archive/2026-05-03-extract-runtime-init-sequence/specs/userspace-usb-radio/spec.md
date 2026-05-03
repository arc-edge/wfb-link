## MODIFIED Requirements

### Requirement: RTL8812AU Initialization
The system SHALL initialize RTL8812AU hardware into a monitor-capable state using userspace USB control and bulk transfers, with stable runtime-facing policy, live USB transport, and initialization phase sequencing exposed from a reusable runtime library rather than only from diagnostic commands.

#### Scenario: Initialization completes
- **WHEN** the system opens a supported RTL8812AU adapter
- **THEN** it powers the chip, loads firmware according to the runtime phase sequence, initializes LLT and queues, configures MAC/BB/RF state, assigns a local MAC address, applies runtime-selected calibration policy, and enters a raw RX/TX ready state

#### Scenario: Firmware load fails
- **WHEN** firmware download or firmware readiness polling fails
- **THEN** initialization stops and reports the failed phase, register, or USB transfer

#### Scenario: Runtime policy available without diagnostic CLI
- **WHEN** a production runtime caller configures RTL8812AU initialization
- **THEN** it can use the runtime library to evaluate stable calibration policy and initialization phase ordering without depending on diagnostic CLI parsing

#### Scenario: Runtime transport available without diagnostic enum
- **WHEN** a production runtime caller owns a libusb claim or macOS USBHost retained session
- **THEN** it can represent that live hardware session with the runtime USB transport type
