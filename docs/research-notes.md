# Research Notes

## Core Hypothesis

macOS does not need to expose Wi-Fi monitor mode if the application owns the USB radio directly. The backend can speak the RTL8812AU USB protocol from userspace and provide the raw 802.11 RX/TX primitives WFB-ng expects.

## Evidence So Far

- WFB-ng's radio I/O boundary is narrow: TX injects radiotap plus 802.11 frames; RX parses radiotap plus 802.11 frames and forwards WFB payloads.
- WFB-ng already has distributor/aggregator modes that move radio packets over UDP, which gives a clean bridge point.
- A Rust project, `RLabs-Inc/wifikit`, contains a userspace RTL8812AU driver using `rusb`, USB control transfers, bulk RX/TX, RTL8812A firmware handling, and Realtek TX/RX descriptors.
- The wifikit repo builds locally with `cargo test --no-run`, so its codebase can be inspected and used as a proof reference.

## Important Unknowns

- Whether the arriving AWUS036ACH units enumerate with a VID/PID already covered by known RTL8812AU registries.
- Whether libusb can reliably claim the radio on macOS 26 without a DriverKit transport.
- Whether the current userspace RTL8812AU approach can sustain WFB video rates without porting more of the Linux driver's VHT/rate/bandwidth handling.
- Whether 40/80 MHz operation is stable enough for practical video on this chipset from userspace.

## Baseline Strategy

Use a known-good Linux WFB-ng setup as a reference:

1. Run WFB-ng with AWUS036ACH on Linux.
2. Capture USB control and bulk transfers during init, channel switch, RX, and TX.
3. Reproduce the minimum working sequence on macOS.
4. Compare emitted TX descriptors and observed on-air frames with an independent monitor receiver.

## Safety Defaults

- Start with low-rate, low-power, 20 MHz tests.
- Avoid DFS channels until channel/regulatory behavior is explicit.
- Require explicit flags for repeated TX, power overrides, and wider bandwidth.
