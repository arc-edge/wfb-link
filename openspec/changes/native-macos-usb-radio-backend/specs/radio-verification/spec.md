## ADDED Requirements

### Requirement: Verification Stages
The system SHALL provide staged verification commands that prove adapter access, chip initialization, RX, TX, WFB RX forwarding, and WFB TX injection independently.

#### Scenario: Stage list requested
- **WHEN** the operator requests available verification stages
- **THEN** the system lists each stage, its prerequisites, required hardware, and expected pass/fail signal

### Requirement: USB Probe Verification
The system SHALL verify USB discovery and interface claim without changing RF state.

#### Scenario: USB probe succeeds
- **WHEN** a supported adapter is attached and available
- **THEN** the probe reports adapter identifiers, endpoint layout, USB speed, and claim/release success

#### Scenario: USB probe fails
- **WHEN** the adapter is absent or cannot be claimed
- **THEN** the probe reports a failing result without attempting firmware or RF initialization

### Requirement: macOS Descriptor Verification
The system SHALL verify USB device and configuration descriptors through macOS IOUSBHost default-control reads when libusb cannot enumerate interfaces.

#### Scenario: macOS descriptor smoke succeeds
- **WHEN** a matching RTL8812AU `IOUSBHostDevice` is visible
- **THEN** the verification command reports the device descriptor, configuration descriptor, interfaces, endpoint addresses, transfer types, max packet sizes, and derived bulk IN/OUT endpoint lists without claiming interfaces, issuing vendor register writes, or using bulk traffic

#### Scenario: macOS descriptor smoke fails
- **WHEN** IOUSBHost cannot open the device or descriptor reads are short, malformed, or rejected
- **THEN** the verification command reports the failed descriptor phase and preserves any partial descriptor data read before failure

### Requirement: Firmware Smoke Verification
The system SHALL verify RTL8812A firmware download independently from channel tuning, RX, and TX.

#### Scenario: Firmware smoke succeeds
- **WHEN** power-on smoke has completed and an RTL8812A firmware image is supplied
- **THEN** the verification command writes the firmware payload, polls checksum and firmware-readiness bits, and reports the final firmware state without issuing bulk traffic

#### Scenario: Firmware smoke fails
- **WHEN** firmware download, checksum polling, or readiness polling fails
- **THEN** the verification command reports the failing register, phase, transfer length, and last observed firmware status when available

### Requirement: LLT Smoke Verification
The system SHALL verify RTL8812A linked-list table programming independently from queue/DMA setup, channel tuning, RX, and TX.

#### Scenario: LLT smoke succeeds
- **WHEN** power-on smoke has completed
- **THEN** the verification command writes the LLT page chain, polls each LLT operation idle, and reports the page boundary and entry count without issuing bulk traffic

#### Scenario: LLT smoke fails
- **WHEN** an LLT write or LLT idle poll fails
- **THEN** the verification command reports the failing LLT address, data byte, register value, and poll attempt count

### Requirement: Queue/DMA Smoke Verification
The system SHALL verify RTL8812A queue and DMA register programming independently from MAC receive enable, BB/RF setup, channel tuning, RX, and TX.

#### Scenario: Queue/DMA smoke succeeds
- **WHEN** firmware smoke and LLT smoke have completed
- **THEN** the verification command derives the queue layout from the USB endpoint count, writes reserved-page and DMA boundary registers, verifies readback, and reports the queue page layout without issuing bulk traffic

#### Scenario: Queue/DMA smoke fails
- **WHEN** preflight readiness, endpoint layout, register write, or register readback fails
- **THEN** the verification command reports the failing phase, register, expected value, observed value, and USB counters

### Requirement: MAC Smoke Verification
The system SHALL verify RTL8812A MAC/WMAC register programming independently from BB/RF setup, channel tuning, RX, and TX.

#### Scenario: MAC smoke succeeds
- **WHEN** firmware smoke, LLT smoke, and queue/DMA smoke have completed
- **THEN** the verification command writes driver-info, network type, WMAC receive filter, rate/retry, EDCA, HW sequence, BAR, and MAC TX/RX enable registers and verifies readback without issuing bulk traffic

#### Scenario: MAC smoke fails
- **WHEN** preflight readiness, register write, or register readback fails
- **THEN** the verification command reports the failing phase, register, expected value, observed value, and USB counters

### Requirement: BB Smoke Verification
The system SHALL verify RTL8812A BB PHY/AGC table programming independently from RF radio table setup, channel tuning, RX, and TX.

#### Scenario: BB smoke succeeds
- **WHEN** firmware, LLT, queue/DMA, and MAC smoke setup have completed and an RTL8812A BB table source file is supplied
- **THEN** the verification command parses PHY_REG and AGC_TAB tables, evaluates Realtek condition markers, writes selected BB registers, applies the crystal-cap update, and reports table counts without issuing bulk traffic

#### Scenario: BB smoke fails
- **WHEN** table parsing, condition planning, preflight readiness, setup register writes, or BB table writes fail
- **THEN** the verification command reports the failing phase, table pair, register, source file, condition inputs, and USB counters

### Requirement: RF Smoke Verification
The system SHALL verify RTL8812A RF radio table programming independently from channel tuning, RX, and TX.

#### Scenario: RF smoke succeeds
- **WHEN** BB smoke setup has completed and an RTL8812A RF table source file is supplied
- **THEN** the verification command parses radioA and radioB tables, evaluates Realtek condition markers, writes selected RF entries through the path-specific 3-wire BB registers, honors table delay markers, and reports table counts without issuing bulk traffic

#### Scenario: RF smoke fails
- **WHEN** table parsing, condition planning, preflight readiness, RF serial encoding, delay handling, or RF table writes fail
- **THEN** the verification command reports the failing phase, table pair, RF path, RF offset, encoded register write, source file, condition inputs, and USB counters

### Requirement: Radio Initialization Verification
The system SHALL verify that the RTL8812AU backend can initialize the chip and enter raw RX/TX ready state.

#### Scenario: Init verification succeeds
- **WHEN** initialization completes on a selected adapter
- **THEN** the verification command reports each completed phase and the resulting MAC, channel, and firmware state

#### Scenario: Init verification fails
- **WHEN** any initialization phase fails
- **THEN** the verification command reports the failed phase and preserves enough detail to compare against Linux USB captures

### Requirement: RX Verification
The system SHALL verify raw frame reception by capturing frames on a selected channel for a bounded interval.

#### Scenario: RX captures frames
- **WHEN** the radio is initialized on an active Wi-Fi channel
- **THEN** the verification command reports frame count, RSSI range, management/data/control counts, and optional PCAP output

#### Scenario: RX captures no frames
- **WHEN** no frames are received during the capture interval
- **THEN** the verification command reports a timeout result and includes USB read counters

### Requirement: TX Verification
The system SHALL verify raw frame transmission using bounded, operator-selected test frames and conservative TX defaults.

#### Scenario: Single test frame sent
- **WHEN** the operator requests a single-frame TX test on an initialized radio
- **THEN** the system sends one valid test frame, reports the USB write result, and records TX counters

#### Scenario: Explicit TX rate selected
- **WHEN** the operator selects a supported legacy, HT MCS, or VHT NSS/MCS diagnostic TX rate
- **THEN** the system builds the TX descriptor with that rate and reports the selected rate in the TX options

#### Scenario: TX requires confirmation for repeated frames
- **WHEN** the operator requests repeated TX
- **THEN** the system requires an explicit repeat count, interval, channel, and acknowledgement that the test is authorized

#### Scenario: TX status sampling enabled
- **WHEN** the operator enables TX status sampling on a live TX diagnostic
- **THEN** the system reads selected RTL8812AU status registers before and after USB TX submission, reports pre/post values, reports changed registers, and labels the data as chip-side telemetry rather than RF confirmation

### Requirement: EFUSE Verification
The system SHALL provide a guarded RTL8812AU EFUSE diagnostic that reads physical EFUSE bytes, decodes the logical map, and reports power-table source bytes without programming EFUSE.

#### Scenario: EFUSE dump succeeds
- **WHEN** the operator authorizes EFUSE control-register writes on a supported adapter
- **THEN** the verification command reads bounded physical EFUSE bytes, decodes packets into the logical map, reports the terminator offset, reports selected identity/RFE/TX-power bytes, and writes optional raw and logical-map artifacts

#### Scenario: EFUSE dump remains read-only with respect to EFUSE contents
- **WHEN** the EFUSE dump command runs
- **THEN** it does not issue EFUSE programming operations, bulk traffic, channel retunes, or RF TX operations, and it labels TX-power data as audit input rather than enabled power control

### Requirement: LED Verification
The system SHALL verify RTL8812AU software LED control independently from RF TX and RX.

#### Scenario: LED smoke succeeds
- **WHEN** the operator selects a supported LED pin, LED mode, and guarded action
- **THEN** the verification command writes the selected LEDCFG path, verifies readback, reports each on/off step, and does not issue bulk traffic or RF operations

#### Scenario: LED smoke requires confirmation
- **WHEN** the operator requests LED register writes
- **THEN** the system requires an acknowledgement that hardware registers will be written before claiming USB or changing LED state

#### Scenario: TX activity LED enabled
- **WHEN** the operator enables TX activity LED indication on a live TX diagnostic
- **THEN** the system toggles the selected software LED around USB TX submissions, reports LEDCFG steps and counters, and labels the indication as software submission activity rather than RF confirmation

### Requirement: WFB Link Verification
The system SHALL verify bridge compatibility with a stock WFB-ng peer before video workloads are attempted.

#### Scenario: WFB RX path verified
- **WHEN** a Linux WFB peer transmits test WFB packets on the configured link
- **THEN** the Mac bridge forwards payloads to the aggregator and reports received, forwarded, dropped, and decrypted packet counters when available

#### Scenario: WFB TX path verified
- **WHEN** a stock WFB-ng distributor sends test packets to the Mac bridge
- **THEN** the Mac bridge injects them through the radio and a Linux peer receives payloads for the configured link

### Requirement: Verification Reports
The system SHALL write machine-readable verification reports for reproducibility.

#### Scenario: Verification run completes
- **WHEN** any verification stage completes
- **THEN** the system writes a JSON report containing timestamp, platform, adapter identity, command arguments, counters, result, and error details when applicable
