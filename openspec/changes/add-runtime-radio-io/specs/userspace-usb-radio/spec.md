## ADDED Requirements

### Requirement: Runtime Session Radio I/O
The runtime session SHALL expose TX submission and RX read helpers that use selected USB bulk endpoints and maintain runtime counters.

#### Scenario: Session submits TX frame
- **WHEN** a caller submits an IEEE 802.11 frame through a runtime session
- **THEN** the session uses the selected bulk OUT endpoint, delegates descriptor construction and USB write to the RTL8812AU core, and updates runtime TX and bulk OUT counters from the submit outcome

#### Scenario: Session reads RX buffer
- **WHEN** a caller reads RX packets through a runtime session
- **THEN** the session uses the selected bulk IN endpoint, parses complete RTL8812AU RX packets from the received buffer, returns parsed packet outcomes, and updates runtime RX, drop, and bulk IN counters
