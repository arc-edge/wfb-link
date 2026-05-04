## Context

`bridge-run` and `radio-run` currently bind one or more UDP sockets for WFB
distributor datagrams, configure a large receive buffer, spawn one receive
thread per socket, and feed queued datagrams into the retained radio RX/TX loop.
This is production behavior, but it lives in the diagnostic binary.

The runtime crate already owns loop planning and report-neutral telemetry. TX
ingress socket lifecycle is the next boundary to move because it is independent
of RTL8812AU register programming and can keep the existing loop unchanged.

## Goals / Non-Goals

**Goals:**

- Move TX ingress socket/receiver lifecycle into `wfb-radio-runtime`.
- Preserve bind ordering and per-socket report index behavior.
- Preserve UDP receive buffer configuration and timeout behavior.
- Keep diagnostic bridge-loop execution stable.

**Non-Goals:**

- Moving RX forwarding sockets, ready-marker writing, or PCAP/JSONL artifacts.
- Moving USB RX/TX loop execution.
- Changing WFB datagram parsing or TX descriptor behavior.

## Decisions

- Runtime ingress helpers use `std::net::UdpSocket` and `std::sync::mpsc` to
  match the current synchronous bridge loop.
- Runtime exposes concrete structs with public fields for queued datagrams and
  receiver handles so the diagnostic loop can migrate without a large rewrite.
- Unix receive-buffer setup moves with ingress; non-Unix remains a no-op.
- Errors are reported as `RuntimeRadioError` with stable codes and converted to
  diagnostic errors at the adapter boundary.

## Risks / Trade-offs

- Runtime now depends on `libc` for Unix socket buffer sizing. Mitigation:
  constrain use to one small helper and keep non-Unix behavior unchanged.
- Public receiver structs expose implementation detail. Mitigation: this is a
  transitional API for moving execution; later slices can hide it behind a
  runtime loop executor.
