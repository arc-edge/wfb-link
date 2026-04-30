## Why

WFB-ng can already run over RTL8812AU radios on Linux, but macOS cannot expose the monitor-mode injection interface WFB-ng expects. The opportunity is to bypass macOS Wi-Fi entirely and treat the ALFA AWUS036ACH as a USB radio peripheral controlled from a native userspace process.

## What Changes

- Add a native macOS userspace USB radio backend for RTL8812AU-class adapters, starting with ALFA AWUS036ACH.
- Add a WFB-ng bridge process that accepts WFB-ng distributor traffic, injects raw 802.11 frames through the USB radio, receives raw 802.11 frames from the radio, and forwards WFB payloads to the stock WFB-ng aggregator.
- Add a verification harness that can prove each layer independently: USB claim, chip init, RX scan, single-frame TX, loopback with a Linux peer, and sustained WFB video/telemetry flow.
- Keep Linux compatibility for diagnostics where practical, but the primary target is Apple Silicon macOS 15 and macOS 26.
- Avoid kernel extensions for the first implementation. Use libusb/rusb first; reserve USBDriverKit for cases where macOS USB ownership or deployment requires it.

## Capabilities

### New Capabilities

- `userspace-usb-radio`: Native userspace access to supported USB Wi-Fi chipsets, including discovery, claim, initialization, channel control, raw 802.11 RX, and raw 802.11 TX.
- `wfb-radio-bridge`: Translation between WFB-ng's distributor/aggregator wire formats and the userspace USB radio backend.
- `radio-verification`: Diagnostics and test flows that prove the radio backend and WFB bridge work before attempting flight-video workloads.

### Modified Capabilities

None.

## Impact

- New repository under `~/projects/arc/wfb-mac-radio`.
- New Rust crate or workspace for the radio backend and bridge CLI.
- Dependencies likely include `rusb`/libusb, `tokio`, `bytes`, `clap`, packet parsing helpers, and WFB-compatible serialization.
- Reference code and behavior will be mined from WFB-ng, the Linux RTL8812AU driver, and the existing `wifikit` userspace RTL8812AU implementation.
- The first supported hardware target is ALFA AWUS036ACH / Realtek RTL8812AU. Additional chipsets are explicitly out of scope until the WFB bridge is proven.
