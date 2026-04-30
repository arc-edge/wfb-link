# Project Context

## Purpose

Build a native macOS userspace radio path that can drive WFB-ng over ALFA AWUS036ACH / RTL8812AU USB Wi-Fi adapters.

## Constraints

- Primary host target is Apple Silicon macOS 15 and macOS 26.
- First hardware target is RTL8812AU, specifically ALFA AWUS036ACH.
- The implementation should avoid kernel extensions and DriverKit until direct libusb/rusb access is proven insufficient.
- WFB-ng compatibility matters more than general Wi-Fi functionality.
- Raw radio transmission must default to conservative, explicit, locally authorized operation.

## Technical Direction

- Prefer Rust for new implementation work.
- Keep the USB transport abstract enough to swap `rusb` for USBDriverKit later.
- Keep WFB protocol bridging separate from RTL8812AU chip mechanics.
- Use Linux WFB-ng and Linux RTL8812AU USB captures as the behavioral baseline.
- Treat existing userspace RTL8812AU work as a proof vector, not as an unquestioned dependency.

## Validation Philosophy

Every hardware-facing step should be independently testable:

- USB discovery and claim.
- Chip initialization.
- RX capture.
- Single-frame TX.
- WFB RX forwarding.
- WFB TX injection.
- Sustained video/telemetry.
