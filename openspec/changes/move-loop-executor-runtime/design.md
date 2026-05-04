## Context

The current bridge loop owns three kinds of behavior in one diagnostic block:
runtime scheduling, packet processing, and report mutation. Runtime scheduling
is production policy: signal stop, duration stop, max datagram stop, TX burst
draining, and RX timeout calculation. Packet parsing and report mutation are
still diagnostic-heavy and can remain as callbacks for now.

## Goals / Non-Goals

**Goals:**

- Move loop scheduling and stop-condition behavior into `wfb-radio-runtime`.
- Preserve current semantics, including max-datagram stop only for unbounded
  duration runs.
- Keep diagnostic code responsible for packet-specific errors and report fields.

**Non-Goals:**

- Moving WFB datagram parsing or radio TX submission into the executor.
- Moving RX packet file output, WFB forwarding sockets, or TX status probes.
- Changing run timing defaults or stop reason labels.

## Decisions

- Use a single callback taking a runtime step enum. This avoids borrow conflicts
  from separate TX/RX closures over the same diagnostic report and session.
- Use stable runtime stop-reason enum values and convert them to existing string
  labels at the diagnostic report boundary.
- Let callback errors pass through generically so diagnostic code can preserve
  phase-specific failure reports without runtime depending on diagnostic types.

## Risks / Trade-offs

- The executor still calls diagnostic packet handlers. Mitigation: this slice
  intentionally separates scheduler ownership first; later slices can move TX
  and RX handlers one at a time.
- Timing-sensitive behavior may shift slightly. Mitigation: preserve the
  existing duration/deadline calculations and verify with hardware smoke.
