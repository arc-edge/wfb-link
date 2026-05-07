## Why

The product binary needs one production link contract that can run on macOS
through the native userspace RTL8812AU bridge and on Linux through the native
aircrack/rtl88xxau monitor-mode WFB stack. The integration must be usable on
the 8-hour path without rewriting the Linux bridge or moving the whole WFB-NG
codec into Rust immediately.

## What Changes

- Define a cross-platform Rust link interface with a shared control plane:
  start, wait-ready, endpoint discovery, health, stop, and final report.
- Keep WFB stream/tunnel semantics as the product-facing data boundary, while
  allowing platform backends to use different radio implementations.
- Define a macOS backend that embeds or supervises the existing production
  radio runtime plus WFB-NG UDP codec helpers.
- Define a Linux backend contract that uses native `wfb0` monitor mode,
  stock WFB-NG tools, and the aircrack/rtl88xxau driver path.
- Document the short-term 8-hour implementation slice and the longer-term
  upgrade path toward an in-process Rust WFB codec.

## Capabilities

### New Capabilities

- `cross-platform-link-interface`: product-facing link lifecycle, data-plane
  endpoint semantics, backend responsibilities, and health/report contract for
  macOS and Linux WFB operation.

### Modified Capabilities

- `production-runtime`: expose the macOS production radio runtime in an
  embedded/supervisable form suitable for a Rust product binary backend.

## Impact

- Affected docs: new cross-platform link interface design and runtime boundary
  notes.
- Affected APIs: a future small Rust interface crate/facade plus a macOS
  backend wrapper around `wfb-radio-runtime` / `wfb-radio-service`.
- Affected systems: product binary integration, macOS userspace USB radio
  backend, and Linux native WFB backend orchestration.
