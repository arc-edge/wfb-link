## ADDED Requirements

### Requirement: TX Bridge Input
The system SHALL accept WFB-ng distributor or injector datagrams containing a firmware mark and radiotap-prefixed IEEE 802.11 frame.

#### Scenario: TX datagram received
- **WHEN** the bridge receives a valid TX datagram from a configured UDP or Unix socket
- **THEN** it extracts the firmware mark, radiotap header, IEEE 802.11 frame, and WFB radio payload

#### Scenario: TX datagram malformed
- **WHEN** a TX datagram is shorter than the required WFB distributor format
- **THEN** the bridge drops the datagram and increments a bad-TX-input counter

### Requirement: Radiotap TX Translation
The system SHALL translate WFB-ng radiotap TX metadata into userspace radio TX options before injection.

#### Scenario: HT radiotap metadata received
- **WHEN** the radiotap header includes HT MCS, bandwidth, guard interval, STBC, or LDPC fields
- **THEN** the bridge maps those fields into the radio TX options used to build the chipset TX descriptor

#### Scenario: Unsupported radiotap field received
- **WHEN** the radiotap header contains a field the radio backend does not support
- **THEN** the bridge ignores the unsupported field, records a warning counter, and preserves safe default TX behavior

### Requirement: WFB Frame Injection
The system SHALL inject WFB-ng IEEE 802.11 frames through the userspace USB radio without requiring a Linux monitor interface.

#### Scenario: Frame injection succeeds
- **WHEN** the TX bridge receives a valid WFB 802.11 frame for the active radio
- **THEN** it submits the frame to the radio backend and increments the injected-packet and injected-byte counters

#### Scenario: Frame injection fails
- **WHEN** the radio backend rejects or fails a TX request
- **THEN** the bridge increments the dropped-packet counter and logs the radio error with the bridge packet context

### Requirement: RX WFB Frame Filtering
The system SHALL filter received raw IEEE 802.11 frames so that only WFB-ng frames for the configured link ID and radio port are forwarded to the aggregator.

#### Scenario: Matching WFB frame received
- **WHEN** the radio backend emits a frame with the configured WFB link ID and radio port in the WFB MAC encoding
- **THEN** the bridge strips the IEEE 802.11 header and treats the remaining bytes as WFB payload

#### Scenario: Non-matching frame received
- **WHEN** the radio backend emits a non-WFB frame or a WFB frame for a different link ID
- **THEN** the bridge drops the frame without forwarding it to the aggregator

### Requirement: RX Aggregator Forwarding
The system SHALL forward received WFB payloads to a stock WFB-ng aggregator using WFB-ng's forwarding header format.

#### Scenario: Payload forwarded
- **WHEN** a matching WFB payload is received from the radio
- **THEN** the bridge sends a datagram containing `wrxfwd_t` metadata followed by the WFB payload to the configured aggregator address

#### Scenario: Aggregator socket unavailable
- **WHEN** the bridge cannot send to the configured aggregator address
- **THEN** it increments an RX-forward-failure counter and keeps the radio receive loop running

### Requirement: Bridge Configuration
The system SHALL allow operators to configure adapter selection, channel, link ID, radio port, TX input socket, RX aggregator address, and conservative TX defaults.

#### Scenario: Complete configuration supplied
- **WHEN** all required bridge configuration is supplied by CLI flags or config file
- **THEN** the bridge initializes the selected radio and starts the configured TX and RX loops

#### Scenario: Required configuration missing
- **WHEN** required bridge configuration is missing
- **THEN** the bridge exits before radio initialization and reports the missing field
