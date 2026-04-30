# Third-Party References

This repository is starting as clean-room implementation work under Apache-2.0.

The following projects are reference material for behavior, protocol shape, and hardware bring-up. They are not vendored into this repository.

## WFB-ng

- Project: `svpcom/wfb-ng`
- License: GPL-3.0
- Use here: protocol behavior, distributor/aggregator boundary, packet forwarding formats, operational expectations.
- Constraint: do not copy WFB-ng source code into this repository without making a deliberate license decision.

## Linux RTL8812AU Drivers

- Projects include `aircrack-ng/rtl8812au` and WFB-specific RTL88xxAU driver forks.
- Licenses vary by fork, commonly GPL-family.
- Use here: hardware behavior, initialization ordering, TX/RX descriptor semantics, and USB capture comparison.
- Constraint: prefer behavior observed from USB traces and independent Rust code over direct source copying.
- Current dry-run init audit reference: `aircrack-ng/rtl8812au` commit `734485506a30d6237c2deaad666a19f8ca5379f2`.
- Current reuse boundary: register addresses, transfer lengths, and init phase ordering are documented as behavior references; source code is not vendored or copied.

## wifikit

- Project: `RLabs-Inc/wifikit`
- Use here: proof that userspace USB radio control for RTL8812AU-class devices is plausible on macOS, plus high-level implementation comparison.
- Constraint: treat as a reference/proof vector unless its license and reuse terms are reviewed for the specific code being reused.

## libusb / rusb

- `rusb` is the initial USB transport crate.
- It provides cross-platform access to USB descriptors, interface claim/release, control transfers, and bulk transfers.
