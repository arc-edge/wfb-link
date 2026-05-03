## Context

`wfb-radio-runtime` owns `RuntimeUsbTransport`, macOS transport configuration, and macOS retained-session open policy. Diagnostic code still owns `select_supported_adapter()` and direct `radio_core::usb::claim_usb_device()` calls for libusb. The next runtime boundary is an open API that can return `RuntimeUsbTransportOpen` for either backend.

## Goals / Non-Goals

**Goals:**

- Move libusb adapter discovery/selection and claim policy into runtime.
- Add a unified runtime open config with backend selection.
- Convert bridge/init/TX/RX live paths that already use `RuntimeUsbTransport`.
- Keep diagnostic-specific reports and phase handling in diag.

**Non-Goals:**

- Do not remove every legacy smoke command's direct `ClaimedUsbDevice` use in this slice if it would force unrelated report churn.
- Do not change queue/DMA, firmware, calibration, or WFB data paths.

## Decisions

- Runtime errors continue to use `{ code, message }`.

  This preserves the diagnostic mapping and lets production callers branch on stable codes without depending on diagnostic types.

- Keep `RuntimeUsbTransportOpen` as the common result type.

  Both libusb and macOS open policy produce adapter metadata, endpoints, transport, and initial counter evidence.

- Convert high-level runtime transport paths first.

  Bridge/init/RX/TX paths already accept `RuntimeUsbTransport`; older smoke commands can be migrated later as part of init sequencing extraction or command cleanup.

## Risks / Trade-offs

- Some diagnostic smoke commands may still call `claim_usb_device` directly. → Track as remaining diagnostic-only legacy paths and keep runtime-facing open policy clean.
- Error wording can shift. → Preserve existing error codes where possible.

## Migration Plan

1. Add runtime libusb selection/open helpers and backend config.
2. Replace diagnostic high-level runtime transport open paths.
3. Update docs/tasks and validate.
