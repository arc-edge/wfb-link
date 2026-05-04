## Context

The retained-session bridge loop now delegates scheduling and queued TX work to
`wfb-radio-runtime`. The RX callback still mutates diagnostic counters directly
and owns WFB forwarding socket state. Optional PCAP and frame JSONL output are
diagnostic concerns and should remain in `wfb-radio-diag`.

## Goals / Non-Goals

**Goals:**

- Move parsed RX packet outcome accounting into runtime.
- Move WFB RX forwarding socket lifecycle and send counters into runtime.
- Return report-neutral telemetry and forward snapshots that diagnostics can
  adapt into existing report fields.
- Preserve existing bridge-run behavior and radio-run smoke behavior.

**Non-Goals:**

- Moving PCAP file creation/writes/flushes into runtime.
- Moving frame JSONL serialization into runtime.
- Changing WFB frame filtering, aggregator payload format, or RX parser logic.

## Decisions

- Runtime RX processing will accept `radio_core::ParsedRxPacket` values already
  returned by `RuntimeRadioSession::read_rx_packets`.
- Runtime will count frame/drop/need-more-data outcomes, PHY metadata coverage,
  and frame type buckets.
- Runtime will own `ProductionRuntimeRxForwardRuntime` values created from the
  existing runtime WFB loop plan. A snapshot API will expose config, aggregator,
  counters, and forwarded bytes without exposing sockets.
- Diagnostics will call runtime RX processing first, then iterate frame outcomes
  only for optional PCAP and JSONL side effects.

## Risks / Trade-offs

- Diagnostics will temporarily iterate RX frames twice: once through runtime for
  counters/forwarding and once for file output. The second pass is side-effect
  only and avoids mixing file output into runtime.
- Runtime RX telemetry mirrors fields currently expected by diagnostic reports.
  Keeping the names factual makes it suitable for production reports too.
- Forwarding send failures cross the runtime/diagnostic boundary. Runtime will
  return stable `RuntimeRadioError` codes that diagnostics map into their
  existing error report shape.
