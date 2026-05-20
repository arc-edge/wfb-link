## ADDED Requirements

### Requirement: Runtime RX Signal Quality Summary
The runtime library SHALL derive a stable RX signal quality summary from valid
runtime-parsed RX metadata while preserving raw signal statistics.

#### Scenario: Runtime summarizes valid signal metadata
- **WHEN** a runtime-parsed RX frame includes valid RSSI and optional SNR/noise
  metadata
- **THEN** runtime RX telemetry records raw RSSI, SNR, and noise metrics with
  sample counts, last values, min values, max values, averages, source/coverage
  data, last-sample timestamp, and derived quality information

#### Scenario: Runtime ignores invalid RSSI for quality
- **WHEN** a runtime-parsed RX frame has fallback or invalid RSSI
- **THEN** runtime RX telemetry preserves metadata coverage counters but MUST
  NOT count that RSSI toward raw valid-signal metrics or quality

### Requirement: Runtime RX Forward Signal Attribution
The runtime library SHALL attach signal summaries to RX forward snapshots for
frames that match each configured WFB forward target.

#### Scenario: Matching forward target observes signal
- **WHEN** a parsed RX frame matches a configured RX forward link ID and radio
  port
- **THEN** the corresponding RX forward snapshot records that frame's valid
  signal metadata, quality information, last RX timestamp, and RF debug context

#### Scenario: Non-matching frames do not affect stream signal
- **WHEN** a parsed RX frame does not match a configured RX forward target
- **THEN** that target's stream-level signal summary is unchanged while the
  aggregate RX signal summary may still observe the frame

### Requirement: Runtime Running Health Snapshots
The runtime library SHALL support bounded running health snapshots that expose
current radio-link metadata without per-packet streaming.

#### Scenario: Health file is configured during bridge loop
- **WHEN** a production runtime flow is running with a configured health file
- **THEN** the runtime writes service-health snapshots at the documented
  cadence with current aggregate and RX-forward signal metadata

#### Scenario: Health file is absent
- **WHEN** a production runtime flow is running without a configured health
  file
- **THEN** the runtime does not attempt filesystem health writes and preserves
  existing radio loop behavior
