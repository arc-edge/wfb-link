## ADDED Requirements

### Requirement: Radio Link Signal Contract
The system SHALL expose radio-link signal metadata in a downstream-facing
contract that includes both raw debug values and a normalized quality
indicator.

#### Scenario: Valid signal samples are exposed
- **WHEN** the radio runtime observes valid receiver-side signal metadata for
  RX frames
- **THEN** downstream health or report payloads include RSSI dBm, SNR dB,
  noise dBm, sample counts, last values, min values, max values, averages,
  last-sample timestamp, source/validity data, and a normalized quality
  indicator

#### Scenario: Invalid fallback RSSI is not rendered as real signal
- **WHEN** the radio parser only has fallback or invalid RSSI for a frame
- **THEN** the downstream signal contract does not count that RSSI as a valid
  signal sample and does not derive quality from it

### Requirement: Radio Link Signal State
The system SHALL represent signal availability state explicitly so downstream
consumers can distinguish unsupported, unknown, fresh, stale, and disconnected
links.

#### Scenario: No valid samples exist
- **WHEN** a backend supports radio metadata but no valid samples have been
  observed yet
- **THEN** the signal state is reported as unknown with no quality level

#### Scenario: Samples become stale
- **WHEN** valid samples exist but the latest sample is older than the
  documented freshness window while the runtime is still running
- **THEN** the signal state is reported as stale while preserving the last raw
  metrics for debugging

#### Scenario: Backend cannot provide radio metadata
- **WHEN** a backend cannot provide radio-link signal metadata
- **THEN** the signal state is reported as unsupported instead of fabricating
  RSSI, SNR, noise, or quality values

### Requirement: Stream-Level Radio Link Metadata
The system SHALL expose stream-level radio-link metadata when RX frames can be
attributed to a configured WFB stream.

#### Scenario: Video and telemetry use separate radio ports
- **WHEN** video and telemetry RX traffic are configured as separate WFB radio
  ports and frames match those ports
- **THEN** downstream stream health includes separate signal summaries and
  quality indicators for each stream

#### Scenario: Stream cannot be attributed
- **WHEN** an endpoint can carry multiple WFB streams or a frame cannot be
  matched to a configured stream identity
- **THEN** the system preserves aggregate RX signal metadata and omits or marks
  stream-level signal metadata as unknown for that endpoint

### Requirement: Local Receiver Semantics
The system SHALL document that local RSSI/SNR/noise describe inbound frames
observed by the local receiver.

#### Scenario: Uplink RSSI is requested
- **WHEN** a downstream consumer needs remote/uplink RSSI for frames received
  by the peer
- **THEN** the contract identifies that value as unavailable unless peer
  metadata is reported back and leaves room for a future remote-signal field
