## Context

WFB-ng's radio path assumes a Linux monitor-mode interface that accepts radiotap-prefixed 802.11 frames and emits radiotap-prefixed captures through libpcap. macOS can capture some Wi-Fi metadata through Apple-controlled interfaces, but it does not provide the Linux-style packet injection surface WFB-ng needs for third-party USB Wi-Fi radios.

The proposed system treats the ALFA AWUS036ACH as a USB peripheral, not a macOS network interface. A native process claims the USB interface, initializes the RTL8812AU chipset, injects frames through bulk OUT transfers, and receives frames through bulk IN transfers. A bridge process then maps that radio API onto WFB-ng's existing distributor and aggregator protocols.

The immediate stakeholders are development and bench testing workflows on Apple Silicon Macs. Flight use comes only after the backend survives staged verification against a known-good Linux WFB peer.

## Goals / Non-Goals

**Goals:**

- Run the radio-facing side of WFB-ng natively on Apple Silicon macOS 15 and macOS 26.
- Support ALFA AWUS036ACH / Realtek RTL8812AU as the first hardware target.
- Keep stock WFB-ng usable on the peer side and, where possible, on the Mac side for FEC/encryption/aggregation.
- Prove the radio backend in stages before attempting sustained video.
- Preserve enough telemetry to debug USB, descriptor, channel, rate, and WFB packet-loss behavior.

**Non-Goals:**

- General macOS Wi-Fi monitor-mode support.
- Association with normal Wi-Fi access points.
- Multi-chipset support before RTL8812AU is stable.
- Wi-Fi security testing features unrelated to WFB.
- Regulatory bypasses, DFS bypasses, or operation outside authorized local rules.

## Decisions

### D1: Rust workspace with a small C-compatible boundary if needed

The implementation should start as a Rust workspace. Rust is a good fit for binary parsing, bounded buffers, USB error handling, and long-running async bridge tasks. `rusb` provides the first USB access path and already builds on macOS.

If integration with WFB-ng's C++ internals becomes useful, expose the radio backend through a narrow C ABI or run it as a separate process over UDP/Unix sockets. Avoid linking WFB-ng internals into the first proof unless the licensing and build complexity are worth it.

### D2: libusb/rusb first, USBDriverKit only if macOS forces it

The first implementation should claim the device through libusb/rusb. That gives fast iteration, no system extension signing loop, and parity with the existing userspace proof vector.

USBDriverKit remains the fallback if device ownership, permissions, deployment, or macOS 26 behavior make direct libusb access unreliable. The radio core should keep USB operations behind a trait so the transport can move from rusb to DriverKit without rewriting chip logic.

### D3: Port the minimum RTL8812AU radio path

The RTL8812AU backend only needs the pieces required by WFB:

- USB discovery and interface claim.
- Register control transfers.
- Power-on and firmware load.
- LLT, queue, DMA, MAC, BB, and RF setup.
- Channel and bandwidth control.
- RX descriptor parsing.
- TX descriptor construction for raw 802.11 frames.

Do not port managed-mode MLME, WPA supplicant integration, AP mode, or normal Ethernet networking. Reference behavior should come from three sources: a known-good Linux RTL8812AU WFB driver, USB captures from that driver, and the existing `wifikit` userspace RTL8812AU implementation.

### D4: Bridge WFB-ng protocols instead of pretending to be a network interface

The first WFB integration should be a standalone bridge, not a fake macOS interface.

On TX, the bridge listens for WFB-ng distributor/injector datagrams containing the firmware mark and radiotap-prefixed 802.11 frame. It parses the radiotap fields into radio TX options, strips radiotap, and submits the 802.11 frame to the userspace radio.

On RX, the bridge reads raw 802.11 frames from the radio, filters WFB frames by link ID and radio port, strips the WFB 802.11 header, builds WFB-ng's forwarding header with RSSI/channel metadata, and sends the payload to the stock WFB-ng aggregator.

This preserves WFB-ng's FEC, encryption, telemetry, and stream handling while replacing only the Linux monitor-mode I/O layer.

### D5: Make verification a first-class binary

The repo should include a diagnostic CLI that can run without WFB-ng:

- List candidate adapters.
- Claim and release USB.
- Initialize the chip.
- Set channel.
- Capture frames for a bounded interval.
- Inject a single management/data test frame.
- Run an RX/TX loop against a Linux peer.

Every stage should emit structured logs and counters. This matters because most failures will be "USB write succeeded but firmware ignored the frame" or "RX descriptor parsed but channel/rate setup was wrong."

### D6: Safe defaults for radio operation

Default operation should use conservative channels, bandwidth, rate, and TX power. DFS channels, 40/80 MHz, higher MCS, LDPC, STBC, and power overrides should require explicit flags and should be visible in logs.

The project cannot enforce every regulatory rule, but it can avoid silent high-power or unsupported-channel behavior.

## Risks / Trade-offs

- [macOS blocks or races USB ownership] -> Start with libusb, document permission behavior, and keep a USBDriverKit transport path available.
- [RTL8812AU initialization sequence is incomplete] -> Use Linux USB captures as the source of truth and reduce the sequence until it still works.
- [20 MHz only is too slow for video] -> Prove correctness at low bitrate first, then port 40/80 MHz, VHT, SGI, LDPC, and STBC paths.
- [Bulk OUT succeeds but frames do not radiate] -> Add TX status probes where available and verify with a second monitor receiver.
- [RX metadata is too weak for diversity decisions] -> Forward best-effort RSSI/channel metadata initially; add per-path RSSI and antenna metadata after basic RX works.
- [Copying WFB-ng code pulls GPL obligations into the repo] -> Prefer protocol-compatible Rust serialization. If copying WFB-ng code is necessary, make the license decision explicit before merging.
- [Raw Wi-Fi operation can interfere with other users] -> Keep verification local, authorized, low-power, and channel-controlled.

## Migration Plan

1. Build a standalone `wfb-radio-diag` that can claim and initialize AWUS036ACH.
2. Add RX scan and PCAP export so the Mac can prove raw 802.11 reception.
3. Add single-frame TX and verify with a separate Linux monitor receiver.
4. Add `wfb-radio-bridge` RX path to feed a stock WFB-ng aggregator.
5. Add `wfb-radio-bridge` TX path from a stock WFB-ng distributor/injector.
6. Run low-rate bidirectional WFB tests against a Linux peer.
7. Increase bandwidth/rate only after counters and packet captures line up.

Rollback is simple during development: stop the bridge and return to the Linux radio sled or VM path.

## Open Questions

- Does the arriving AWUS036ACH revision enumerate as `0bda:8812`, `0bda:881a`, or an ALFA-specific VID/PID?
- Can libusb reliably claim the adapter on macOS 26 without a DriverKit system extension?
- Which Linux RTL8812AU fork and driver version should be treated as the "known-good" capture baseline for WFB?
- What minimum bitrate is acceptable for the first useful video test?
- Should the repo be MIT/Apache for clean-room protocol work, or GPLv3 if WFB-ng code reuse becomes practical?
