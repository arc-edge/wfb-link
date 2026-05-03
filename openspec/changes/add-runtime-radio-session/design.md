## Context

`RuntimeUsbTransportOpen` is enough for low-level opening, but it is not the right production boundary: runtime consumers need the transport plus adapter identity, endpoints, and counters to move together. Diagnostic code currently wraps that shape in `LiveUsbTransportOpen`; this slice introduces the runtime version while leaving large call-site conversions for later.

## Goals / Non-Goals

**Goals:**

- Add a runtime-owned live session type.
- Preserve existing transport open APIs for low-level callers.
- Route diagnostic opening through the session type.
- Add focused tests for session metadata/counter behavior where hardware is not required.

**Non-Goals:**

- Do not move full bridge TX/RX loops in this slice.
- Do not force every diagnostic call site to hold `RuntimeRadioSession` yet.
- Do not change USB claim, macOS IOUSBHost, or libusb open behavior.

## Decisions

- Keep fields public initially.

  The diagnostic crate still needs to destructure the session while migration continues. Methods provide the intended production path, but public fields avoid a large call-site rewrite.

- Keep `RuntimeUsbTransportOpen` as the low-level open result.

  Session construction wraps it and initializes counters; code that only needs a transport can keep using the lower-level function.

## Risks / Trade-offs

- This is a boundary step, not a complete runtime migration. It deliberately creates a stable object that later init/TX/RX extraction can target.
- Public fields can be tightened after diagnostic call sites stop destructuring session state directly.

## Migration Plan

1. Add runtime session type and constructors.
2. Add session register/bulk endpoint/counter helpers.
3. Update diagnostic open helper to construct a runtime session.
4. Run formatting, tests, and strict OpenSpec validation.
