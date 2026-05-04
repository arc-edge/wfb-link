## ADDED Requirements

### Requirement: Runtime Bridge TX Handler
The runtime library SHALL process one queued production WFB TX datagram without
depending on diagnostic command argument structs or diagnostic report structs.

#### Scenario: Valid datagram is submitted
- **WHEN** a queued production TX datagram contains supported WFB radiotap and a
  supported IEEE 802.11 frame
- **THEN** the runtime handler submits the frame through `RuntimeRadioSession`
  and returns report-neutral TX metadata and counters

#### Scenario: Malformed datagram is counted
- **WHEN** a queued production TX datagram is malformed or uses unsupported
  radiotap
- **THEN** the runtime handler returns a non-fatal handled outcome with dropped
  and malformed bridge counters

#### Scenario: Radio submit failure is reported
- **WHEN** descriptor construction succeeds but radio submission fails
- **THEN** the runtime handler returns a stable TX submission error and updated
  bridge/submit counters for the caller to report

#### Scenario: Metadata supports diagnostic adaptation
- **WHEN** a valid datagram is handled
- **THEN** the runtime handler returns source length, peer, fwmark, radiotap
  length, frame length, packet length, TX options, and descriptor preview
  without depending on diagnostic report types
