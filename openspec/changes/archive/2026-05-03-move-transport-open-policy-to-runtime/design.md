## Context

`wfb-radio-runtime` now owns `RuntimeUsbTransport` and the macOS USBHost FFI wrapper. The remaining macOS open policy in `wfb-radio-diag` validates endpoint direction/count, synthesizes a `UsbDeviceInfo`, checks supported VID/PID, and opens retained IOUSBHost sessions. That logic is production transport policy and should move behind runtime types.

## Goals / Non-Goals

**Goals:**

- Define a clap-free runtime config for macOS USBHost retained sessions.
- Return runtime-owned endpoint, adapter, transport, and initial counter evidence.
- Keep diagnostic error codes stable by mapping runtime errors back to diagnostic reports.
- Use the runtime open API from all diagnostic paths that open retained IOUSBHost sessions.

**Non-Goals:**

- Do not move libusb adapter selection/claim policy yet.
- Do not move diagnostic counters or phase-report types into runtime.
- Do not change queue/DMA page layout calculations.

## Decisions

- Runtime errors use stable `code` plus `message`.

  Diagnostic reports already expose that shape. Returning the same shape from runtime keeps mapping direct while avoiding a dependency on diagnostic report structs.

- Runtime open results include `initial_usb_control_writes`.

  The existing diagnostics count the IOUSBHost configure attempt as an initial control write. Runtime should expose that evidence without owning diagnostic counter structs.

- Endpoint validation stays focused on macOS transport layout.

  Full queue page layout remains in diagnostic/init code for now. Runtime validates endpoint direction, count, and selected bulk OUT membership because those are transport-open prerequisites.

## Risks / Trade-offs

- Error text in the live init phase can become less specific. → Preserve runtime error codes/messages and include them in the phase description.
- Runtime open policy is macOS-only while libusb open policy stays in diag. → This is deliberate; libusb claim policy can move later with a cross-platform open config.

## Migration Plan

1. Add runtime config, error, endpoint, adapter, and open helpers.
2. Replace diagnostic endpoint/adapter/open helpers with runtime calls.
3. Route live init's inline macOS open path through the runtime helper.
4. Run workspace tests and OpenSpec validation.
