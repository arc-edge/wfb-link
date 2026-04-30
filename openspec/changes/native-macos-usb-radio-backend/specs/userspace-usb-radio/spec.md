## ADDED Requirements

### Requirement: Adapter Discovery
The system SHALL discover supported USB Wi-Fi adapters by USB vendor ID, product ID, bus, address, and chipset family without requiring the adapter to appear as a macOS network interface.

#### Scenario: Supported AWUS036ACH is present
- **WHEN** an ALFA AWUS036ACH compatible RTL8812AU adapter is attached
- **THEN** the system reports the adapter as a supported `rtl8812au` radio candidate with its USB identifiers and location

#### Scenario: Unsupported adapter is present
- **WHEN** a USB Wi-Fi adapter with an unknown chipset is attached
- **THEN** the system reports it as unsupported and does not attempt chip initialization

### Requirement: USB Interface Claim
The system SHALL claim the radio's USB interface for exclusive userspace access before issuing register, firmware, RX, or TX operations.

#### Scenario: Interface claim succeeds
- **WHEN** the selected adapter is not owned by another active driver or process
- **THEN** the system claims interface 0 and records the bulk IN and bulk OUT endpoints

#### Scenario: Interface claim fails
- **WHEN** macOS or another process prevents claiming the USB interface
- **THEN** the system fails before chip initialization and reports the owning or failing USB operation when available

### Requirement: macOS Direct-Control Fallback
The system SHALL provide a macOS-only direct-control diagnostic path for RTL8812AU adapters that are visible in IOKit but unavailable through libusb enumeration.

#### Scenario: IOKit-visible adapter lacks libusb interfaces
- **WHEN** IOKit reports a matching RTL8812AU `IOUSBHostDevice` without libusb-visible interfaces
- **THEN** the system reports the IOKit registration, matching, configuration, location, speed, and interface-child state without requiring a macOS network interface

#### Scenario: Direct-control register access succeeds
- **WHEN** the operator selects a matching VID/PID through the macOS direct-control path
- **THEN** the system can issue bounded RTL8812AU vendor control reads, guarded EFUSE control sequences, guarded power-on/RF-reset control-write diagnostics, guarded firmware download/readiness polling, guarded LLT programming, guarded queue/DMA setup, and guarded MAC/WMAC setup while clearly reporting that bulk RX/TX endpoint access is not proven

### Requirement: RTL8812AU Initialization
The system SHALL initialize RTL8812AU hardware into a monitor-capable state using userspace USB control and bulk transfers.

#### Scenario: Initialization completes
- **WHEN** the system opens a supported RTL8812AU adapter
- **THEN** it powers the chip, loads firmware, initializes LLT and queues, configures MAC/BB/RF state, assigns a local MAC address, and enters a raw RX/TX ready state

#### Scenario: Firmware load fails
- **WHEN** firmware download or firmware readiness polling fails
- **THEN** initialization stops and reports the failed phase, register, or USB transfer

### Requirement: Channel Control
The system SHALL tune the radio to an explicitly selected 2.4 GHz or 5 GHz Wi-Fi channel before RX or TX operation.

#### Scenario: Valid channel selected
- **WHEN** the operator selects a supported channel
- **THEN** the radio applies the channel and reports the effective frequency, band, and bandwidth

#### Scenario: Unsupported channel selected
- **WHEN** the operator selects a channel outside the supported set
- **THEN** the system rejects the configuration before changing radio state

### Requirement: Raw Frame Reception
The system SHALL receive raw IEEE 802.11 frames from the USB bulk IN endpoint and expose frame bytes with metadata needed by WFB consumers.

#### Scenario: Valid frame received
- **WHEN** the radio receives an 802.11 frame without a bad-FCS indication
- **THEN** the system emits the frame bytes without the chipset RX descriptor and includes best-effort RSSI, channel, band, and timestamp metadata

#### Scenario: Corrupt frame received
- **WHEN** the RX descriptor indicates CRC/FCS failure
- **THEN** the system drops the frame and increments a corrupt-frame counter

### Requirement: Raw Frame Transmission
The system SHALL transmit caller-provided IEEE 802.11 frames by prepending the correct RTL8812AU TX descriptor and writing to the USB bulk OUT endpoint.

#### Scenario: Management frame transmitted
- **WHEN** a valid management frame and TX options are submitted
- **THEN** the system builds a TX descriptor with queue, rate, retry, bandwidth, and checksum fields and writes the descriptor plus frame to bulk OUT

#### Scenario: Invalid frame rejected
- **WHEN** a frame is too short to contain a valid IEEE 802.11 header
- **THEN** the system rejects the TX request without writing to USB

### Requirement: Radio Telemetry
The system SHALL expose counters and structured diagnostics for USB transfers, initialization phases, RX frames, dropped frames, TX attempts, and TX failures.

#### Scenario: Diagnostics requested
- **WHEN** the operator requests radio status
- **THEN** the system reports current adapter identity, channel, initialization state, RX counters, TX counters, and last error
