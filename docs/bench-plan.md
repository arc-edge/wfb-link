# Bench Plan

This project assumes the Mac owns the AWUS036ACH as a USB peripheral. The adapter does not need to appear as a macOS Wi-Fi interface, and the first proof path is a native userspace process using `rusb`. On macOS 26, IOUSBHost direct-control diagnostics are the current fallback when IOKit sees the device but libusb cannot enumerate it.

## Hardware Assumptions

- Host: Apple Silicon Mac running macOS 15 or macOS 26.
- Primary radio: ALFA AWUS036ACH or another RTL8812AU adapter with a known VID/PID.
- Supported IDs currently include Realtek `0x0bda:0x8812`, `0x0bda:0x881a`, `0x0bda:0x881b`, `0x0bda:0x881c`, and several common RTL8812AU rebrands.
- Peer: a known-good Linux WFB-ng system with a working RTL8812AU/RTL88xxAU radio path.
- Monitor: an independent receiver or Linux monitor-mode adapter for confirming that TX frames actually radiate.

## Mac And Linux Setup

Use the Mac for the experimental userspace backend and the Linux peer as the reference WFB endpoint.

1. On the Mac, run `wfb-radio-diag usb-probe` first and record the VID/PID, bus, address, speed, interfaces, and endpoint layout.
2. If `usb-probe` cannot see the adapter on macOS, run `wfb-radio-diag macos-usb-state --vid <vid> --pid <pid>` and record the IOKit registration, matching, configuration, and interface-child state.
3. Run `wfb-radio-diag reg-smoke` to confirm userspace register reads work before any init writes are attempted. On macOS 26, use `macos-reg-smoke` if the libusb claim path is unavailable.
4. Run `wfb-radio-diag efuse-dump --i-understand-this-writes-control-registers` to capture physical EFUSE bytes, decode the logical map, and archive RFE/TX-power source bytes before tuning power behavior. On macOS 26, use `macos-efuse-dump` if the libusb claim path is unavailable.
5. Run `wfb-radio-diag led-smoke --pin led0 --mode normal --action blink --i-understand-this-writes-registers` while watching the adapter, then sweep other pin/mode pairs if the visible LED does not move.
6. Run `wfb-radio-diag power-on-smoke --i-understand-this-writes-registers` to verify the first guarded power-on/RF-reset write sequence. On macOS 26, use `macos-power-on-smoke` if the libusb claim path is unavailable.
7. Run `wfb-radio-diag llt-smoke --i-understand-this-writes-registers` to verify LLT page-chain programming without bulk traffic. On macOS 26, use `macos-llt-smoke` after `macos-power-on-smoke` if the libusb claim path is unavailable.
8. Run `wfb-radio-diag firmware-smoke --firmware <rtl8812aefw.bin> --i-understand-this-writes-registers` to verify firmware download, checksum, and readiness without bulk traffic. On macOS 26, use `macos-firmware-smoke` after `macos-power-on-smoke` if the libusb claim path is unavailable.
9. Run `wfb-radio-diag queue-dma-smoke --i-understand-this-writes-registers` to verify queue reserved pages and DMA boundaries without bulk traffic.
10. Run `wfb-radio-diag mac-smoke --i-understand-this-writes-registers` to verify MAC/WMAC setup without BB/RF setup or bulk traffic.
11. Run `wfb-radio-diag bb-smoke --i-understand-this-writes-registers` to verify BB PHY/AGC table programming without RF radio table setup, channel tuning, or bulk traffic.
12. Run `wfb-radio-diag rf-smoke --i-understand-this-writes-registers` to verify RF radioA/radioB table programming without channel tuning or bulk traffic.
13. Run `wfb-radio-diag init --channel 36 --bandwidth 20 --firmware <rtl8812aefw.bin> --i-understand-this-writes-registers` to verify the integrated power, firmware, LLT, queue/DMA, MAC, BB, RF, and 20 MHz channel path over one USB claim.
14. On Linux, confirm the same adapter model works with the chosen WFB-ng driver and channel.
15. Run `wfb-radio-diag rx-scan --channel 36 --duration-ms 1000 --pcap <path> --frame-jsonl <path>` after `init` to prove bounded bulk-IN reads, descriptor parsing, PCAP output, and raw-frame metadata emission on the selected channel.
16. Run `wfb-radio-diag tx-once --channel 36 --frame-hex <hex> --tx-status --i-understand-this-transmits` after `init` to prove one guarded descriptor-prefixed bulk-OUT submission and capture read-only TX status register deltas.
17. Run `wfb-radio-diag tx-repeat --channel 36 --count <n> --interval-ms <ms> --frame-hex <hex> --tx-status --i-understand-this-transmits` only after `tx-once` passes, starting with small counts.
18. Keep the first link one-way and low-rate: Linux transmits test WFB payloads, Mac receives and forwards to an aggregator.
19. After RX forwarding is stable, reverse the path: stock WFB-ng distributor traffic feeds the Mac bridge, and Linux verifies received payloads.
20. Only attempt sustained video after low-rate bidirectional payload counters line up.

## Linux USB Baselines

Capture Linux USB behavior before porting more initialization code. The useful traces are init, channel switch, bounded RX, and single-frame TX.

Recommended capture flow:

1. Boot the Linux reference system with the known-good driver and adapter.
2. Start a USB capture for the adapter bus with Wireshark usbmon, `tcpdump` on usbmon, or a hardware analyzer if available.
3. Record one trace for each stage: driver attach/init, set channel 36 at 20 MHz, receive a short burst, transmit one known frame.
4. Save captures with metadata: adapter VID/PID, driver fork and commit, kernel version, channel, bandwidth, rate/MCS, and WFB-ng command line.
5. Compare macOS control transfer ordering, register values, firmware download chunks, TX descriptor bytes, and bulk endpoint behavior against those traces.

## Safe Defaults

Default operation should stay conservative until captures prove otherwise.

- Channel: start on non-DFS 5 GHz channel 36 or a locally authorized 2.4 GHz channel.
- Bandwidth: 20 MHz.
- TX rate: OFDM 6 Mbps by default. `tx-once` and `tx-repeat` can use `--tx-rate` for explicit descriptor diagnostics with legacy, HT MCS, or VHT NSS/MCS rates; WFB bridge TX still follows radiotap metadata with conservative fallbacks.
- TX power: no override until the captured EFUSE power-table bytes are mapped to the Linux driver's final per-rate RF power indexes.
- Repeated TX: require explicit count, interval, channel, and authorization flag.
- SGI, LDPC, and STBC: available only behind visible `--short-gi`, `--ldpc`, and `--stbc` TX flags and echoed in JSON counters.
- LED control: the attached unit's visible blue LED is normal `led0` on `REG_LEDCFG0`; `tx-once` and `tx-repeat` can use `--tx-led` to show software TX submissions, but this is not RF proof.
- TX status sampling: `tx-once` and `tx-repeat` can use `--tx-status` to read selected RTL8812AU status registers before and after bulk-OUT submissions; this is chip-side telemetry, not RF proof.
- Wider bandwidth: keep behind explicit bandwidth selection and do not treat as verified until captures prove it.

## Known Limitations

- RTL8812AU live init now completes power-on, firmware download, LLT, queue/DMA, MAC, BB, RF, and 20/40/80 MHz channel setup. `rx-scan` can issue bounded bulk-IN reads after init, and `tx-once` can submit one descriptor-prefixed frame over bulk OUT after init.
- The first live `rx-scan` on channel 36 completed without USB errors but captured no frames: 14 read timeouts over 1.5 seconds, 0 bulk bytes, a header-only PCAP, and an empty frame JSONL. A Linux peer or known-active channel is still needed to prove actual frame reception.
- The first live `tx-once` on channel 36 completed one USB bulk-OUT write without USB errors: 24-byte frame, 64-byte descriptor-prefixed packet, endpoint `0x02`, one submitted TX frame. An independent monitor receiver is still needed to prove over-the-air radiation.
- The first live `tx-repeat` on channel 36 completed three paced USB bulk-OUT writes without USB errors: 3 submitted frames, 192 bytes written, no failed or short writes. A Linux peer or independent monitor receiver is still needed before treating this as RF throughput.
- The first live `tx-once --tx-led` and `tx-repeat --tx-led` runs toggled the visible `REG_LEDCFG0` LED around software TX submissions while preserving successful bulk-OUT counters. Reports: `/tmp/wfb-live-tx-once-led.json`, `/tmp/wfb-live-tx-repeat-led.json`.
- The first live `tx-once --tx-status` run submitted one packet and reported one read-only status delta, `REG_TXPKT_EMPTY` from `0x0fff` to `0x0ffe`. The first live `tx-repeat --tx-status` run submitted three packets and reported no changed status registers in its post-burst window. Reports: `/tmp/wfb-live-tx-once-status.json`, `/tmp/wfb-live-tx-repeat-status.json`.
- A 20 MHz USB-side burst submitted 50 of 50 frames at 1 ms requested spacing, wrote 3,200 descriptor-prefixed USB bytes in 65 ms, reported about 769 submitted frames/s, and used about 2.48 ms process CPU time. This is not packet-loss or RF throughput without a receiver.
- A flagged live `tx-once` run with `--short-gi --ldpc --stbc` completed one bulk-OUT write and reported all three descriptor flags in JSON. Peer decode of those options is not yet verified.
- LEDCFG software-control writes pass across normal LED0/1/2 plus antdiv/minicard alternatives on the attached `0x0bda:0x8812` adapter. Operator observation confirmed the visible blue enclosure LED is normal LED0 on `REG_LEDCFG0`.
- 40 MHz channel setup now passes live USB-level init, RX-scan, and single-frame TX checks on channel 36, but RF capture/peer decode is not verified because only one RTL8812AU is attached.
- 80 MHz channel setup now passes live USB-level init, RX-scan, single-frame TX, and repeated-TX checks on channel 36. Reports: `/tmp/wfb-live-init-channel80.json`, `/tmp/wfb-live-rx-scan-80mhz.json`, `/tmp/wfb-live-tx-once-80mhz.json`, `/tmp/wfb-live-tx-repeat-80mhz.json`.
- Explicit VHT diagnostic rate selection now passes USB-level live TX checks: `tx-once --bandwidth 80 --tx-rate vht2ss-mcs9 --short-gi --ldpc --stbc` and matching `tx-repeat` submitted descriptor-prefixed packets and echoed the VHT rate in JSON. Reports: `/tmp/wfb-live-tx-once-vht-rate.json`, `/tmp/wfb-live-tx-repeat-vht-rate.json`.
- EFUSE dump now passes on the attached `0x0bda:0x8812` adapter and captured a decoded logical map with EFUSE USB ID `0x0bda:0x8812`, MAC `00:c0:ca:ba:bd:9f`, RFE option `0x03`, and 66 non-`0xff` bytes in the 84-byte TX-power region. Report: `/tmp/wfb-live-efuse-dump.json`.
- On macOS 26, the remote hardware Mac saw the RTL8812AU in IOKit as `!registered, !matched` with no interface children, so libusb `usb-probe` could not enumerate or claim it. `macos-usb-state`, `macos-reg-smoke`, `macos-efuse-dump`, `macos-power-on-smoke`, `macos-firmware-smoke`, and `macos-llt-smoke` passed through IOUSBHost default-control transfers, proving register reads, EFUSE access, guarded power-on/RF-reset writes, firmware download/readiness, and LLT programming but not bulk endpoints. Reports: `/tmp/wfb-remote-macos-usb-state.json`, `/tmp/wfb-remote-macos-reg-smoke.json`, `/tmp/wfb-remote-macos-efuse-dump.json`, `/tmp/wfb-remote-macos-power-on-smoke.json`, `/tmp/wfb-remote-macos-firmware-smoke.json`, `/tmp/wfb-remote-macos-llt-smoke.json`.
- EFUSE-derived final per-rate RF power indexes, IQK, and explicit TX power controls remain pending.
- Direct `rusb` ownership may be blocked if macOS attaches another driver or changes USB ownership behavior.
- The first bridge path is not a fake macOS network interface; it speaks WFB-ng distributor and aggregator wire formats directly.
- PCAP output is raw IEEE 802.11 linktype, not radiotap, until RX metadata is rich enough to justify radiotap capture output.
- IOUSBHost can reach default-control transfers on macOS 26 even when libusb cannot see the adapter, but bulk RX/TX still needs interface/pipe access. USBDriverKit becomes necessary if libusb and IOUSBHost cannot provide reliable bulk endpoints, deployment requires a signed system extension, or macOS 26 changes make userspace interface ownership unstable.
