## Why

The production WFB loop plan is runtime-owned, but TX UDP socket binding and
receiver threads are still implemented in `wfb-radio-diag`. Moving that ingress
machinery into `wfb-radio-runtime` is the next low-risk execution cutover
because it is socket-only and does not alter the USB/RF loop.

## What Changes

- Add runtime-owned TX ingress socket binding, receive-buffer configuration,
  receiver thread spawning, queued datagram, and shutdown types.
- Route the existing bridge execution path through the runtime TX ingress
  helpers.
- Preserve bridge-loop behavior, bind ordering, report indexing, and bounded
  receiver timeouts.
- Keep signal handling, ready-marker writing, RX/TX USB execution, and
  diagnostic reports in `wfb-radio-diag` for this slice.

## Capabilities

### New Capabilities

- `runtime-tx-ingress`: Runtime-owned UDP TX ingress socket and receiver-thread
  lifecycle for production WFB radio flow.

### Modified Capabilities

- `runtime-bridge-loop`: Runtime bridge-loop ownership expands from pure loop
  planning to TX ingress socket setup and queued datagram delivery.
- `wfb-radio-runtime`: Production full-flow behavior uses runtime-owned TX
  ingress setup before the remaining diagnostic execution adapter runs.

## Impact

- Extends `crates/wfb-radio-runtime` with standard-library UDP socket/thread
  helpers and platform receive-buffer configuration.
- Adds `libc` to `wfb-radio-runtime` for Unix UDP receive-buffer sizing.
- Refactors `wfb-radio-diag` bridge-loop setup to use runtime ingress types.
- Updates tests, OpenSpec specs, and runtime boundary documentation.
