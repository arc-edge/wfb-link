## ADDED Requirements

### Requirement: Production Runtime Radio Link Metadata
The production runtime SHALL expose radio-link metadata through production
health and report payloads for downstream consumers.

#### Scenario: Production report includes aggregate metadata
- **WHEN** a production runtime flow exits after receiving frames with valid
  signal metadata
- **THEN** the final report includes aggregate RX radio-link metadata with raw
  metrics, quality indicator, source/validity semantics, and RF debug context

#### Scenario: Production health includes current metadata
- **WHEN** a production runtime flow is running and health snapshots are
  enabled
- **THEN** the health payload includes the current aggregate RX signal summary
  and any available per-forward stream signal summaries

### Requirement: Production Stream Radio Link Metadata
The production runtime SHALL expose radio-link metadata for configured RX
forward targets when frames can be attributed to those targets.

#### Scenario: RX forward target receives matching frames
- **WHEN** a configured RX forward target receives frames matching its WFB link
  ID and radio port
- **THEN** the target's production snapshot includes forwarded counters,
  last-RX timestamp, raw signal metrics, quality indicator, and available
  channel/rate/bandwidth metadata

#### Scenario: RX forward target has no matching signal
- **WHEN** a configured RX forward target has not observed valid matching
  signal samples
- **THEN** the target's production snapshot reports unknown signal state
  without fabricating quality or RSSI values

### Requirement: Product Link Health Radio Metadata
The product-facing link health model SHALL expose production radio-link
metadata without requiring consumers to parse backend-specific JSON.

#### Scenario: Link health maps aggregate signal
- **WHEN** `LinkHandle::health()` reads a runtime health or report payload with
  aggregate RX signal metadata
- **THEN** `LinkHealth.rx` includes a structured signal summary and preserves
  the existing aggregate average fields for compatibility

#### Scenario: Link health maps stream signal
- **WHEN** a named RX stream maps to a production RX forward snapshot with
  signal metadata
- **THEN** that stream's `LinkStreamRxHealth` includes the structured signal
  summary for downstream UI and logging
