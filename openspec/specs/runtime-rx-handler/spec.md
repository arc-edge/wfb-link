# Runtime RX Handler Specification

## Purpose

Define runtime-owned processing for parsed production RX packet outcomes and WFB
RX forwarding, while keeping diagnostic PCAP/JSONL formatting outside the
runtime layer.

## Requirements

### Requirement: Runtime Bridge RX Handler
The runtime library SHALL process parsed production RX packet outcomes and WFB
RX forwarding without depending on diagnostic command argument structs or
diagnostic report structs.

#### Scenario: Frame outcomes are counted
- **WHEN** parsed RX packets contain supported frame outcomes
- **THEN** the runtime handler records parsed frame counts, PHY status coverage,
  valid RSSI coverage, SNR coverage, noise coverage, and frame type buckets

#### Scenario: Drop and tail outcomes are counted
- **WHEN** parsed RX packets contain dropped packets or incomplete tails
- **THEN** the runtime handler records dropped packet and need-more-data counts

#### Scenario: Matching WFB frames are forwarded
- **WHEN** an RX frame matches a configured WFB forward target with an
  aggregator address
- **THEN** the runtime handler sends the WFB forward datagram to the aggregator
  and records forwarded payload counters and bytes

#### Scenario: Forwarding without aggregator still counts filtering
- **WHEN** an RX forward target has no aggregator address
- **THEN** the runtime handler applies WFB filtering and records match/filter
  counters without sending UDP output

#### Scenario: Diagnostic file output remains external
- **WHEN** diagnostics request PCAP or frame JSONL output
- **THEN** file output is performed outside the runtime RX handler using the
  frame outcomes returned by the radio session
