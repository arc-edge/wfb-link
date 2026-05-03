## Context

`wfb-radio-diag` currently defines `InitUsbTransport`, an enum over `ClaimedUsbDevice` and macOS USBHost sessions. It implements the two traits that nearly all live hardware code needs: `Rtl8812auUsbTransport` for vendor register access and `UsbBulkTransfer` for RX/TX bulk pipes. This is runtime infrastructure, not diagnostic reporting.

## Goals / Non-Goals

**Goals:**

- Move the unified live USB transport enum into `wfb-radio-runtime`.
- Keep existing trait behavior and call sites intact.
- Keep transport opening and diagnostic error mapping in `wfb-radio-diag` for now.

**Non-Goals:**

- Do not move adapter selection or claim/open policy yet.
- Do not change endpoint validation.
- Do not change RX/TX loops or calibration execution.

## Decisions

- Move the enum before moving open policy.

  The enum has a clean dependency shape and is already generic runtime behavior. Open policy still depends on diagnostic arguments and error reports, so it should move later behind a purpose-built runtime config/error type.

- Preserve the existing variant shape.

  `Libusb(Box<ClaimedUsbDevice>)` and macOS USBHost session ownership remain unchanged. That keeps ownership and drop behavior stable.

## Risks / Trade-offs

- This still leaves opening logic in diag. → This is intentional; the next runtime slice can introduce runtime config and error types for open policy.
- Runtime crate grows a direct dependency on libusb-backed `ClaimedUsbDevice` through `radio-core`. → That dependency already exists for the macOS transport and matches runtime ownership.

## Migration Plan

1. Add `RuntimeUsbTransport` and trait implementations to `wfb-radio-runtime`.
2. Replace diagnostic `InitUsbTransport` references.
3. Run full workspace tests and OpenSpec validation.
