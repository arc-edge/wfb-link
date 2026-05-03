# Userspace USB Radio Specification

## Purpose

Define the native macOS userspace RTL8812AU radio backend used to discover, claim, initialize, tune, receive from, transmit through, and inspect supported USB Wi-Fi adapters without requiring a macOS network interface.
## Requirements
### Requirement: Adapter Discovery
The system SHALL discover supported USB Wi-Fi adapters by USB vendor ID, product ID, bus, address, and chipset family without requiring the adapter to appear as a macOS network interface.

#### Scenario: Supported AWUS036ACH is present
- **WHEN** an ALFA AWUS036ACH compatible RTL8812AU adapter is attached
- **THEN** the system reports the adapter as a supported `rtl8812au` radio candidate with its USB identifiers and location

#### Scenario: Unsupported adapter is present
- **WHEN** a USB Wi-Fi adapter with an unknown chipset is attached
- **THEN** the system reports it as unsupported and does not attempt chip initialization

### Requirement: USB Interface Claim
The system SHALL claim the radio's USB interface for exclusive userspace access before issuing register, firmware, RX, or TX operations, using runtime-owned open policy for production runtime transport paths.

#### Scenario: Interface claim succeeds
- **WHEN** the selected adapter is not owned by another active driver or process
- **THEN** the runtime open policy claims interface 0 and records the bulk IN and bulk OUT endpoints

#### Scenario: Interface claim fails
- **WHEN** macOS or another process prevents claiming the USB interface
- **THEN** the runtime open policy fails before chip initialization and reports the owning or failing USB operation when available

### Requirement: macOS Direct-Control Fallback
The system SHALL provide a macOS-only direct-control diagnostic and runtime transport path for RTL8812AU adapters that are visible in IOKit but unavailable through libusb enumeration, with retained-session endpoint validation and opening provided by the runtime library.

#### Scenario: IOKit-visible adapter lacks libusb interfaces
- **WHEN** IOKit reports a matching RTL8812AU `IOUSBHostDevice` without libusb-visible interfaces
- **THEN** the system reports the IOKit registration, matching, configuration, location, speed, and interface-child state without requiring a macOS network interface

#### Scenario: Direct-control register access succeeds
- **WHEN** the operator selects a matching VID/PID through the macOS direct-control path
- **THEN** the system can issue bounded standard descriptor reads, RTL8812AU vendor control reads, guarded EFUSE control sequences, guarded power-on/RF-reset control-write diagnostics, guarded firmware download/readiness polling, guarded LLT programming, guarded queue/DMA setup, guarded MAC/WMAC setup, guarded BB PHY/AGC setup, guarded RF radio table setup, IOUSBHost interface/pipe probes, bounded bulk-IN smoke requests, zero-length bulk-OUT smoke requests, and retained-session control plus bulk pipe smokes while clearly reporting that retained full-init RX/TX remains a separate verification target

#### Scenario: Runtime library owns direct-control transport
- **WHEN** macOS direct-control access is used by diagnostic or production runtime code
- **THEN** the IOUSBHost wrapper, retained session, bulk/register trait implementations, endpoint validation, and retained-session opening are provided by the runtime library

### Requirement: RTL8812AU Initialization
The system SHALL initialize RTL8812AU hardware into a monitor-capable state using userspace USB control and bulk transfers, with stable runtime-facing policy, live USB transport, calibration selection, and initialization phase execution exposed from a reusable runtime library rather than only from diagnostic commands.

#### Scenario: Initialization completes
- **WHEN** the system opens a supported RTL8812AU adapter
- **THEN** it powers the chip, loads firmware according to the runtime phase sequence, initializes LLT and queues, configures MAC/BB/RF state, assigns a local MAC address, applies runtime-selected calibration policy, and enters a raw RX/TX ready state

#### Scenario: Firmware load fails
- **WHEN** firmware download or firmware readiness polling fails
- **THEN** initialization stops and reports the failed phase, register, or USB transfer

#### Scenario: Runtime policy available without diagnostic CLI
- **WHEN** a production runtime caller configures RTL8812AU initialization
- **THEN** it can use the runtime library to evaluate stable calibration policy, initialization phase ordering, and same-session init execution without depending on diagnostic CLI parsing or diagnostic report structures

#### Scenario: Runtime transport available without diagnostic enum
- **WHEN** a production runtime caller owns a libusb claim or macOS USBHost retained session
- **THEN** it can represent that live hardware session with the runtime USB transport type

#### Scenario: Runtime init preserves calibration guardrails
- **WHEN** initialization requests an experimental TX calibration profile
- **THEN** the runtime init path MUST require the profile's authorization and report the calibration class before RF TX is enabled

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

### Requirement: Targeted Calibration Parity Profile
The system SHALL provide an explicit targeted calibration profile for RTL8812AU TX-path parity work that applies known Linux-final register overrides separately from full runtime IQK/LCK.

#### Scenario: Targeted parity profile is enabled
- **WHEN** the operator enables the targeted Linux-parity calibration profile for a supported channel and bandwidth
- **THEN** the radio command MUST apply the profile's guarded register writes, record before/write/after evidence, and label the calibration mode as targeted parity rather than full Linux-ported calibration

#### Scenario: Targeted parity profile is unsupported
- **WHEN** the operator enables the targeted parity profile for an unsupported channel, bandwidth, or chipset state
- **THEN** the command MUST fail before RF transmission or leave the profile unapplied with an explicit reportable reason

### Requirement: Receiver Metadata Confidence
The system SHALL expose RX metadata with enough source and confidence information for RF-quality reports to distinguish measured signal evidence from fallbacks.

#### Scenario: PHY status supplies RSSI evidence
- **WHEN** an RX descriptor includes PHY status bytes sufficient for RSSI extraction
- **THEN** the emitted frame metadata MUST mark RSSI as measured and include descriptor/PHY-status evidence fields

#### Scenario: RSSI is a fallback
- **WHEN** an RX descriptor does not include usable PHY status bytes
- **THEN** the emitted frame metadata MUST preserve a fallback RSSI for compatibility but MUST mark it as fallback/invalid for RF-quality decisions

#### Scenario: PHY status supplies SNR evidence
- **WHEN** an RTL8812AU OFDM/HT/VHT RX descriptor includes a known PHY-status layout
- **THEN** the emitted frame metadata MUST include SNR source and derived noise fields and MUST count those frames separately in RX telemetry

### Requirement: RTL8812AU RF Readback
The system SHALL support RTL8812AU RF serial readback for RF path A and RF path B using the Linux 8812A `phy_RFSerialRead` register sequence.

#### Scenario: RF register is read
- **WHEN** a diagnostic or calibration routine reads an RTL8812AU RF register
- **THEN** the system MUST select the RF offset through the HSSI read-address register, read the 20-bit value from the path's PI or SI readback register, and record the readback source in structured output

### Requirement: Guarded RTL8812A LCK Calibration
The system SHALL provide an explicit RTL8812A LCK calibration profile that runs the upstream local-oscillator calibration sequence after init and before TX.

#### Scenario: LCK profile is enabled
- **WHEN** the operator enables the LCK calibration profile
- **THEN** the command MUST pause packet TX when appropriate, read and preserve RF channel state, enter LCK mode, trigger the RF CHNLBW calibration bit, wait for the calibration window, exit LCK mode, restore state, and record structured evidence

#### Scenario: LCK profile is not enabled
- **WHEN** the operator uses the default calibration profile
- **THEN** the command MUST NOT run LCK and MUST preserve the existing default TX behavior

### Requirement: RTL8812AU IQK Probe
The system SHALL provide an opt-in RTL8812AU IQK profile that labels IQK staging evidence without running the IQK calibration sweep, switching BB pages, using RF serial backup reads, adding profile-time hardware reads, or modifying RF transmit power.

#### Scenario: IQK profile labels deferred probe state
- **WHEN** an operator selects the RTL8812AU IQK probe profile on an initialized adapter
- **THEN** the system reports that live IQK hardware probing is deferred and identifies the existing RF calibration IQK readback as the safe source of pre-TX IQK register evidence

#### Scenario: IQK probe remains read-only
- **WHEN** the IQK probe profile completes successfully
- **THEN** the system labels the profile as read-only deferred probe evidence and MUST NOT report runtime IQK calibration as completed

#### Scenario: IQK deep probe is unsafe for live TX
- **WHEN** deeper IQK evidence requires profile-time hardware reads, RF serial backup reads, or page-C1 latch access
- **THEN** the live pre-TX profile MUST skip that evidence and label it as deferred to a standalone diagnostic or full IQK port

### Requirement: RTL8812AU Standalone IQK Diagnostic
The system SHALL provide a guarded standalone RTL8812AU IQK diagnostic that
initializes the adapter and collects deep IQK evidence without running WFB TX,
WFB RX, synthetic TX, or the IQK calibration sweep.

#### Scenario: Standalone IQK diagnostic collects evidence
- **WHEN** an operator runs the standalone IQK diagnostic on an initialized or
  initializable RTL8812AU adapter with the required hardware-write
  acknowledgement
- **THEN** the system reports MAC/BB backup registers, AFE backup registers, RF
  backup offsets for path A and path B, page-C1 latch registers, normal-page
  IQK result registers, USB counters, and cleanup status

#### Scenario: Standalone IQK diagnostic avoids live traffic
- **WHEN** the standalone IQK diagnostic runs
- **THEN** the system MUST NOT submit WFB datagrams, synthetic TX frames, or
  bulk-IN receive loops as part of the diagnostic

#### Scenario: Standalone IQK diagnostic restores selectors
- **WHEN** the diagnostic reads page-C1 or RF serial IQK evidence
- **THEN** the system attempts to restore BB page selection, HSSI/RF readback
  selectors, and RF serial state before exiting and reports any cleanup failure

#### Scenario: Standalone IQK diagnostic does not claim calibration
- **WHEN** the diagnostic completes successfully
- **THEN** the report MUST label the output as evidence-only and MUST NOT
  report runtime IQK calibration as completed

### Requirement: RTL8812AU Runtime IQK Calibration
The system SHALL provide a guarded RTL8812AU runtime IQK calibration profile
that can execute the Linux 8812A IQK sequence after retained init and channel
setup without making runtime IQK the default path.

#### Scenario: Runtime IQK requires authorization
- **WHEN** an operator selects the runtime IQK profile without explicit
  hardware-write authorization
- **THEN** the system MUST reject the run before claiming USB or changing RF
  state

#### Scenario: Runtime IQK records per-path result evidence
- **WHEN** the runtime IQK profile runs on an initialized RTL8812AU adapter
- **THEN** the system reports per-path TX IQK status, RX IQK status, retry
  counts, selected TX/RX IQC values, fallback use, affected registers, USB
  counters, and cleanup status

#### Scenario: Runtime IQK restores saved state
- **WHEN** runtime IQK exits after success or failure
- **THEN** the system attempts to restore saved MAC/BB, AFE, RF, BB page-select,
  and HSSI selector state and reports any restore failure separately from the
  calibration result

#### Scenario: Runtime IQK remains opt-in
- **WHEN** an operator uses the default calibration profile
- **THEN** the system MUST NOT run the runtime IQK sequence or classify IQK as
  completed

### Requirement: Runtime Register Phase Execution
The runtime library SHALL own reusable RTL8812AU register phase execution helpers for production callers and diagnostics.

#### Scenario: TX scheduler tail executed by runtime
- **WHEN** a caller provides RTL8812AU register access and runtime counters
- **THEN** the runtime executes the TX scheduler tail register reads and writes, returns the phase identifier, register write count, and counter delta, and leaves diagnostic-specific report formatting to the caller

#### Scenario: Monitor receive setup executed by runtime
- **WHEN** a caller requests monitor receive filter or monitor opmode programming
- **THEN** the runtime programs the monitor receive registers, returns raw MSR/RCR/RXFLTMAP2 evidence and counter deltas, and does not require the diagnostic crate to own those register writes

### Requirement: Runtime MAC Address Initialization
The runtime library SHALL own RTL8812AU EFUSE MAC extraction and REG_MACID programming for initialization callers.

#### Scenario: EFUSE MAC extracted by runtime
- **WHEN** runtime initialization needs the adapter MAC address
- **THEN** the runtime reads physical EFUSE bytes through guarded EFUSE control-register operations, decodes the logical map, and returns a programmed non-blank MAC address when present

#### Scenario: REG_MACID programmed by runtime
- **WHEN** a caller provides a local adapter MAC address
- **THEN** the runtime reads the current REG_MACID bytes, writes the supplied six MAC bytes, reads them back, and returns before/written/after evidence plus counter deltas

### Requirement: Runtime Radio Session
The runtime library SHALL expose a live radio session object that owns the selected transport, adapter metadata, endpoint layout, and runtime counters.

#### Scenario: Session created from opened transport
- **WHEN** a runtime USB transport open succeeds
- **THEN** the runtime can wrap it in a session carrying the transport, adapter metadata, endpoints, and initial counter state

#### Scenario: Session exposes operational handles
- **WHEN** runtime callers need register access or bulk endpoint selection
- **THEN** the session provides helpers for register access and selected bulk IN/OUT endpoint lookup without requiring diagnostic-only wrapper types

### Requirement: Runtime Session Radio I/O
The runtime session SHALL expose TX submission and RX read helpers that use selected USB bulk endpoints and maintain runtime counters.

#### Scenario: Session submits TX frame
- **WHEN** a caller submits an IEEE 802.11 frame through a runtime session
- **THEN** the session uses the selected bulk OUT endpoint, delegates descriptor construction and USB write to the RTL8812AU core, and updates runtime TX and bulk OUT counters from the submit outcome

#### Scenario: Session reads RX buffer
- **WHEN** a caller reads RX packets through a runtime session
- **THEN** the session uses the selected bulk IN endpoint, parses complete RTL8812AU RX packets from the received buffer, returns parsed packet outcomes, and updates runtime RX, drop, and bulk IN counters

### Requirement: Runtime Ownership Documentation
The system SHALL document the current boundary between production-facing runtime code and diagnostic harness code.

#### Scenario: Runtime session ownership is documented
- **WHEN** live RX/TX commands use runtime session APIs for frame movement
- **THEN** the runtime-boundary documentation lists session I/O and counters as runtime-owned while leaving CLI/report orchestration diagnostic-owned
