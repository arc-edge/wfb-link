## Context

The production bridge loop now gets its cadence from `wfb-radio-runtime`, and TX
UDP ingress is runtime-owned. The callback body for one queued TX datagram still
lives in `wfb-radio-diag`: it updates diagnostic report fields, parses WFB
radiotap, previews the RTL8812AU descriptor, submits through a diagnostic
`RadioTx` adapter, and mutates bridge/submit counters.

## Goals / Non-Goals

**Goals:**

- Move one queued TX datagram processing step into `wfb-radio-runtime`.
- Keep runtime results report-neutral and reusable by production commands.
- Preserve existing bridge-run report fields by adapting runtime metadata in
  diagnostics.
- Keep TX status probes and diagnostic error report formatting outside runtime.

**Non-Goals:**

- Moving RX packet processing, PCAP/JSONL output, or WFB RX forwarding.
- Changing `wfb-bridge` parsing rules or RTL8812AU TX descriptor construction.
- Changing TX rates, overrides, calibration behavior, or radio-run CLI shape.

## Decisions

- The runtime handler will take a queued runtime datagram, channel, bandwidth,
  and TX override options, then submit through `RuntimeRadioSession`.
- The runtime handler will return a structured outcome with datagram metadata,
  byte counters, bridge counters, submit counters, and an optional descriptor
  preview so diagnostics can preserve `last_datagram`.
- Malformed datagrams and descriptor-build failures remain non-fatal TX step
  outcomes. Radio submission failures become stable runtime TX errors that the
  diagnostic adapter maps back to `bridge_tx_submit_failed`.
- Diagnostic code will stop using a local `RadioTx` bridge adapter for
  bridge-run TX once the runtime handler exists; other diagnostic commands can
  keep their existing adapters until they are migrated.

## Risks / Trade-offs

- Runtime metadata may initially mirror diagnostic report needs. Mitigation:
  name the types around production TX datagram facts rather than diagnostic
  report fields.
- The handler may duplicate parsing/building that `wfb-bridge::submit_tx_datagram`
  already performs. Mitigation: keep the behavior identical first; later slices
  can collapse duplication inside `wfb-bridge` or `radio-core`.
- Submit counter ownership crosses the runtime/diagnostic boundary. Mitigation:
  return updated runtime counters explicitly and let diagnostics assign them to
  their legacy report shape.
