## 1. Repository Setup

- [x] 1.1 Create a Rust workspace with crates for `radio-core`, `wfb-bridge`, and diagnostic CLIs
- [x] 1.2 Add baseline dependencies: `rusb`, `tokio`, `bytes`, `clap`, `tracing`, `serde`, and `serde_json`
- [x] 1.3 Add CI-friendly commands for `cargo fmt`, `cargo clippy`, and `cargo test`
- [x] 1.4 Add license and third-party attribution notes for WFB-ng, Linux RTL8812AU references, and wifikit references

## 2. USB Discovery and Claim

- [x] 2.1 Implement supported adapter registry for known RTL8812AU VID/PID values, including generic Realtek and expected ALFA IDs
- [x] 2.2 Implement adapter listing with USB bus, address, speed, descriptors, and endpoint discovery
- [x] 2.3 Implement exclusive USB interface claim/release through `rusb`
- [x] 2.4 Add `wfb-radio-diag usb-probe` command with JSON report output
- [x] 2.5 Test absent, unsupported, and claim-failure cases

## 3. RTL8812AU Bring-Up

- [x] 3.1 Implement low-level register read/write helpers using RTL USB vendor control transfers
- [x] 3.2 Port the minimum power-on and RF reset sequence for RTL8812AU
- [x] 3.3 Add embedded or externally loaded RTL8812A firmware handling
- [x] 3.4 Implement firmware download, checksum polling, and readiness polling
- [x] 3.5 Implement LLT, queue, DMA, MAC, BB, and RF setup needed for monitor RX/TX
  - Progress: `llt-smoke` implements and live-verifies LLT table programming; `queue-dma-smoke` implements and live-verifies queue reserved pages and DMA boundaries; `mac-smoke` implements and live-verifies MAC/WMAC setup; `bb-smoke` parses external Realtek BB tables and live-verifies PHY/AGC programming; `rf-smoke` parses external Realtek RF tables and live-verifies radioA/radioB programming.
- [x] 3.6 Add `wfb-radio-diag init` command with phase-level diagnostics
  - Live result: April 30, 2026 macOS 15.7.4 run with `0x0bda:0x8812` passed power-on, firmware, LLT, queue/DMA, MAC, BB, RF, and channel phases over one USB claim. Reports: `/tmp/wfb-live-init.json`, `/tmp/wfb-live-init-channel.json`.
- [ ] 3.7 Compare init transfer sequence against a known-good Linux USB capture

## 4. Channel and RX Path

- [x] 4.1 Implement supported channel model for 2.4 GHz and 5 GHz channels
- [x] 4.2 Implement 20 MHz channel switch and effective channel reporting
  - Live result: April 30, 2026 macOS 15.7.4 `init --channel 36 --bandwidth 20` reported effective channel 36, 5180 MHz, 20 MHz bandwidth. Report: `/tmp/wfb-live-init-channel.json`.
- [x] 4.3 Implement bulk IN RX loop and RTL8812AU RX descriptor parser
  - Live result: April 30, 2026 macOS 15.7.4 `rx-scan --channel 36 --duration-ms 1500` claimed the initialized adapter and completed 14 bounded bulk-IN read timeouts without USB errors. Report: `/tmp/wfb-live-rx-scan.json`.
- [x] 4.4 Emit raw 802.11 frames with RSSI, channel, band, timestamp, and drop counters
  - Implementation: `rx-scan --frame-jsonl` writes one JSON record per parsed raw 802.11 frame with timestamp, RSSI, channel, frequency, band, frame type, length, and frame hex; fixture coverage verifies emitted records. Live channel 36 run was quiet, so `/tmp/wfb-live-rx-frames.jsonl` was empty.
- [x] 4.5 Add optional PCAP writer for captured frames
- [x] 4.6 Add `wfb-radio-diag rx-scan` bounded capture command
  - Live result: command wrote `/tmp/wfb-live-rx-scan.json`, header-only `/tmp/wfb-live-rx-scan.pcap`, and empty `/tmp/wfb-live-rx-frames.jsonl`; actual RF frame reception still needs traffic on the tuned channel.

## 5. TX Path

- [x] 5.1 Implement safe IEEE 802.11 frame validation before TX
- [x] 5.2 Implement RTL8812AU 40-byte TX descriptor construction and checksum
- [x] 5.3 Map conservative TX options for queue, rate, retry limit, bandwidth, SGI, LDPC, and STBC
- [x] 5.4 Implement bulk OUT frame submission and TX counters
- [x] 5.5 Add `wfb-radio-diag tx-once` command for single-frame TX verification
  - Live result: April 30, 2026 macOS 15.7.4 `tx-once --channel 36 --bandwidth 20 --frame-hex <fixture> --i-understand-this-transmits` claimed the initialized adapter and wrote one 64-byte descriptor-prefixed packet to bulk OUT endpoint `0x02`. Report: `/tmp/wfb-live-tx-once.json`.
- [x] 5.6 Add repeated-TX diagnostic mode gated by explicit count, interval, channel, and authorization flag
  - Live result: April 30, 2026 macOS 15.7.4 `tx-repeat --channel 36 --bandwidth 20 --count 3 --interval-ms 100 --frame-hex <fixture> --i-understand-this-transmits` submitted 3 descriptor-prefixed packets to bulk OUT endpoint `0x02` with no failed or short writes. Report: `/tmp/wfb-live-tx-repeat.json`.

## 6. WFB Bridge RX

- [x] 6.1 Implement WFB MAC/link-id/radio-port frame filter
- [x] 6.2 Strip IEEE 802.11 header and extract WFB payload from matching RX frames
- [x] 6.3 Implement `wrxfwd_t` serialization compatible with WFB-ng aggregator network mode
- [x] 6.4 Forward RX payloads to a configured WFB-ng aggregator UDP address
- [x] 6.5 Add RX bridge counters for received, matched, forwarded, filtered, malformed, and send-failed packets
- [ ] 6.6 Verify RX bridge with a Linux WFB peer transmitting low-rate test payloads

## 7. WFB Bridge TX

- [x] 7.1 Implement WFB distributor/injector datagram parser for firmware mark plus radiotap-prefixed 802.11 frame
- [x] 7.2 Implement radiotap parser for WFB TX metadata used by HT and VHT modes
- [x] 7.3 Map radiotap metadata into radio TX options with conservative fallbacks
- [x] 7.4 Submit stripped 802.11 frames to the userspace radio backend
- [x] 7.5 Add TX bridge counters for incoming, injected, dropped, malformed, and unsupported-radiotap packets
- [ ] 7.6 Verify TX bridge with stock WFB-ng distributor and a Linux WFB receiver
  - Progress: April 30, 2026 remote macOS 26 `bridge-tx-once --macos-usbhost` parsed one WFB distributor-style datagram with fwmark `0x00000000`, a 13-byte HT radiotap header, and a 24-byte IEEE 802.11 frame, then submitted one 64-byte descriptor-prefixed packet to endpoint `0x02` with bridge counters `incoming=1`, `injected=1`, `dropped=0`. `bridge-tx-listen --macos-usbhost` then bound `127.0.0.1:5611`, received one local UDP datagram with the same shape, and submitted one 64-byte descriptor-prefixed packet with the same clean counters; a follow-up `--max-datagrams 3` run submitted three of three UDP datagrams with `incoming=3`, `injected=3`, `dropped=0`, 192 USB bytes written, and no failed or short writes. Reports: `/tmp/wfb-remote-macos-bridge-tx-once-usbhost.json`, `/tmp/wfb-remote-macos-bridge-tx-listen-usbhost.json`, `/tmp/wfb-remote-macos-bridge-tx-listen-3-usbhost.json`. This proves diagnostic bridge parse-to-radio-submit and UDP-to-radio-submit paths, but stock WFB-ng distributor and Linux receiver verification remains open.
  - Progress: May 1, 2026 `bridge-tx-listen --macos-usbhost --init-before-tx --tx-status` now reports the generated RTL8812AU TX descriptor hex and pre/post Tier 1 TX bring-up registers for live UDP submissions. A 20 MHz HT radiotap probe-control datagram submitted 50/50 frames and the Linux monitor captured 49 `WFBMACRF1` probe requests. The same listener with a 40 MHz radiotap bit submitted 50/50 but produced no RF marker, proving descriptor bandwidth bits matter even when the channel fallback is 20 MHz. Report/capture: `/tmp/wfb-listen-probe20-status.json`, `/tmp/mac-listen-probe20-status.pcap`.
  - Progress: May 1, 2026 WFB-style data frames still do not decode on the Linux monitor even though host submission succeeds and chip-side queues return empty. A 100-frame MGNT-queue OFDM6 WFB data burst submitted 100/100 with descriptor `2500288d001207000000000000050000041f020000000000000000000c8500000080000000000000`, unchanged `REG_Q0_INFO`/`REG_MGQ_INFO`/`REG_TXPKT_EMPTY`, and no `MACDATAWFBBR1` marker in `/tmp/mac-bench-wfbdata20-status.pcap`. A QoS-data variant behaved the same. Reports: `/tmp/wfb-bench-wfbdata20-status.json`, `/tmp/wfb-bench-qosdata-status.json`.

## 8. End-to-End Verification

- [x] 8.1 Add `wfb-radio-diag stages` command describing every verification stage and prerequisite
- [x] 8.2 Add machine-readable JSON reports for every diagnostic command
- [x] 8.3 Run USB probe and init verification on macOS 15
  - Live result: April 30, 2026 macOS 15.7.4 `usb-probe` and `init` passed, including channel 36/20 MHz effective reporting. Reports: `/tmp/wfb-pre-init-usb-probe.json`, `/tmp/wfb-live-init.json`, `/tmp/wfb-live-init-channel.json`, `/tmp/wfb-post-channel-reg-smoke.json`.
- [x] 8.4 Run USB probe and init verification on macOS 26
  - Live result: April 30, 2026 remote macOS 26 run on `rownd@rownds-macbook-pro` showed `usb-probe --all` could not enumerate the `0x0bda:0x8812` radio through libusb, while `macos-usb-state` found it in IOKit as `!registered, !matched`, active, 480 Mbps, and without interface children before direct configuration. The IOUSBHost retained-session path now passes descriptor/interface/pipe smokes, register reads, EFUSE, guarded power-on, firmware, LLT, queue/DMA, MAC, BB, RF, integrated `init --macos-usbhost`, bounded `rx-scan --macos-usbhost`, `tx-once --macos-usbhost`, `tx-repeat --macos-usbhost`, `bridge-tx-once --macos-usbhost`, `bridge-tx-listen --macos-usbhost`, and combined `tx-once --macos-usbhost --tx-led --tx-status`. Full init programmed channel 36/20 MHz with 491 control reads and 1,396 control writes; RX accepted ten bounded bulk-IN requests with clean timeouts; TX wrote one 64-byte descriptor-prefixed packet; repeated TX wrote three 64-byte packets with no failures or short writes; bridge TX parsed and injected both an operator-supplied and a UDP-received WFB distributor-style datagram through the live radio backend. Reports: `/tmp/wfb-remote-macos-usb-state.json`, `/tmp/wfb-remote-macos-descriptor-smoke.json`, `/tmp/wfb-remote-macos-interface-smoke.json`, `/tmp/wfb-remote-macos-bulk-in-smoke.json`, `/tmp/wfb-remote-macos-bulk-out-smoke.json`, `/tmp/wfb-remote-macos-session-smoke.json`, `/tmp/wfb-remote-macos-reg-smoke.json`, `/tmp/wfb-remote-macos-efuse-dump.json`, `/tmp/wfb-remote-macos-power-on-smoke.json`, `/tmp/wfb-remote-macos-firmware-smoke.json`, `/tmp/wfb-remote-macos-llt-smoke.json`, `/tmp/wfb-remote-macos-queue-dma-smoke.json`, `/tmp/wfb-remote-macos-mac-smoke.json`, `/tmp/wfb-remote-macos-bb-smoke.json`, `/tmp/wfb-remote-macos-rf-smoke.json`, `/tmp/wfb-remote-macos-init-usbhost.json`, `/tmp/wfb-remote-macos-rx-scan-usbhost.json`, `/tmp/wfb-remote-macos-tx-once-usbhost.json`, `/tmp/wfb-remote-macos-tx-repeat-usbhost.json`, `/tmp/wfb-remote-macos-tx-once-led-status-usbhost.json`, `/tmp/wfb-remote-macos-bridge-tx-once-usbhost.json`, `/tmp/wfb-remote-macos-bridge-tx-listen-usbhost.json`.
- [ ] 8.5 Run RX scan and single-frame TX verification against an independent Linux monitor receiver
  - Progress: May 1, 2026 independent Linux monitor capture on `drone-2f389.local:wfb0` confirmed RF TX for management/probe frames from both `bridge-tx-bench` and `bridge-tx-listen`. `bridge-tx-bench` sent 200/200 probe frames and the monitor captured 199 `WFBMACRF1` frames at 5180 MHz / 6 Mbps. `bridge-tx-listen` sent 50/50 20 MHz probe datagrams and the monitor captured 49. Reports/captures: `/tmp/wfb-probe-status.json`, `/tmp/mac-probe-status.pcap`, `/tmp/wfb-listen-probe20-status.json`, `/tmp/mac-listen-probe20-status.pcap`.
- [ ] 8.6 Run low-rate bidirectional WFB payload test against a Linux peer
- [ ] 8.7 Run sustained WFB video/telemetry test only after low-rate tests pass

## 9. Performance and Radio Features

- [ ] 9.1 Measure 20 MHz baseline throughput, packet loss, and CPU usage
  - Progress: USB-side 20 MHz TX burst on April 30, 2026 submitted 50/50 descriptor-prefixed packets with no failed or short writes, 3,200 USB bytes in 65 ms, about 769 submitted frames/s, and about 2.48 ms process CPU time. Report: `/tmp/wfb-live-tx-repeat-20mhz-burst-cpu.json`. Packet loss still needs an independent receiver.
  - Progress: remote macOS 26 IOUSBHost `bridge-tx-bench --init-before-tx` saturation runs on May 1, 2026 show host-to-chip queue fill rather than RF throughput. With 1024-byte WFB payloads the radio accepts 75 descriptor-prefixed packets, 81,600 USB bytes, then packet 76 times out after the RTL8812AU queue stops accepting bulk OUT writes. Pacing the same payload at 1 ms, 5 ms, and 20 ms does not move the failure index, so the chip queue is not draining between submissions. Smaller payloads fail later: 16, 64, and 256 byte payloads each submit 227 packets before packet 228 times out, matching TX page/queue capacity more than airtime. Reports: `/tmp/wfb-throughput-payload-16.json`, `/tmp/wfb-throughput-payload-64.json`, `/tmp/wfb-throughput-payload-256.json`, `/tmp/wfb-throughput-payload-1024.json`, `/tmp/wfb-throughput-pace-1000.json`, `/tmp/wfb-throughput-pace-5000.json`, `/tmp/wfb-throughput-pace-20000.json`.
  - Progress: descriptor and scheduler experiments did not restore queue drain. `REG_CR=0x0fff`, `REG_TRXDMA_CTRL=0xf5b4`, `REG_TXDMA_OFFSET_CHK=0x00fd0200`, management-queue descriptors without `AGG_BREAK`, MACID 1, station MSR plus H2C media-status, and alternate OUT endpoints still fail at the same queue-fill boundary. The cleaner management descriptor removes the `REG_TXDMA_STATUS=0x00000400` checksum-like flag on endpoint `0x02`, but `REG_MGQ_INFO` and `REG_TXPKT_EMPTY` still show queued, undrained packets. Reports: `/tmp/wfb-throughput-cr.json`, `/tmp/wfb-throughput-trxdma.json`, `/tmp/wfb-throughput-both.json`, `/tmp/wfb-throughput-noagg-mgnt.json`, `/tmp/wfb-throughput-linuxish-mgnt.json`, `/tmp/wfb-throughput-linuxish-mgnt-macid1.json`, `/tmp/wfb-throughput-linuxish-sta-h2c.json`, `/tmp/wfb-throughput-mgnt-ep-04.json`, `/tmp/wfb-throughput-mgnt-ep-03.json`.
  - Progress: May 1, 2026 macOS 26 bisection isolated the TX saturation gate to `REG_OFDMCCKEN_JAGUAR` (`0x0808`) bits `0x30000000`. The BB table left the register at `0x0e028233`; explicitly ensuring the Linux value shape `0x3e028233` after band selection moved channel 36/20 MHz from the old queue-fill timeout to `1000/1000` submitted 64-byte WFB packets and `500/500` submitted 1024-byte WFB packets with no failed or short bulk writes. Reports: `/tmp/wfb-throughput-channel36-ofdmccken-initfix-1000x64.json`, `/tmp/wfb-tx-status-channel36-ofdmccken-initfix.json`, `/tmp/wfb-throughput-channel36-initfix-500x1024.json`. Receiver-side packet loss and RF decode still need the Linux peer.
  - Progress: May 1, 2026 after descriptor/status instrumentation, MGNT-queue probe traffic is RF-visible while MGNT-queue data traffic drains without peer decode. BE-queue WFB data is still not usable: a 100-frame BE burst submitted 100/100 but left `REG_Q0_INFO` changed from `0x007f80ff` to `0x017f80ff`, set `REG_TXDMA_STATUS=0x00000401`, and produced no monitor marker. Report/capture: `/tmp/wfb-bench-wfbdata-be-status.json`, `/tmp/mac-bench-wfbdata-be-status.pcap`.
- [ ] 9.2 Port 40 MHz channel support and verify with captures
  - Progress: 40 MHz channel programming is implemented and live USB-verified on April 30, 2026 with `init --channel 36 --bandwidth 40` reporting effective 40 MHz bandwidth, `rx-scan --bandwidth 40` completing bounded bulk-IN reads, and `tx-once --bandwidth 40` submitting one descriptor-prefixed packet. Reports: `/tmp/wfb-live-init-channel40.json`, `/tmp/wfb-live-rx-scan-40mhz.json`, `/tmp/wfb-live-tx-once-40mhz.json`. Independent RF capture/peer decode remains open because only one RTL8812AU is attached.
- [x] 9.3 Port 80 MHz and VHT MCS support if 40 MHz is stable
  - Implementation: 80 MHz channel setup now programs the RTL8812AU WMAC 80 MHz bit, primary 40/20 subchannel mapping, BB RF mode, ADC buffer clock, CCA-on-secondary, L1 peak threshold, and RF bandwidth bits for supported 5 GHz 80 MHz groups. TX descriptor rate mapping now encodes HT MCS0-31 and VHT 1SS-4SS MCS0-9 using Realtek descriptor rate IDs instead of falling back to OFDM 6 Mbps.
  - Live result: April 30, 2026 macOS 15.7.4 `init --channel 36 --bandwidth 80` reported effective 80 MHz bandwidth with the channel phase completing in 19 steps. `rx-scan --bandwidth 80` completed 10 bounded bulk-IN read timeouts, `tx-once --bandwidth 80 --tx-led --tx-status` submitted one descriptor-prefixed packet and reported `REG_TXPKT_EMPTY` changing from `0x0fff` to `0x0ffe`, and `tx-repeat --bandwidth 80 --count 3 --tx-led --tx-status` submitted 3 descriptor-prefixed packets. Reports: `/tmp/wfb-live-init-channel80.json`, `/tmp/wfb-live-rx-scan-80mhz.json`, `/tmp/wfb-live-tx-once-80mhz.json`, `/tmp/wfb-live-tx-repeat-80mhz.json`.
  - Live result: April 30, 2026 macOS 15.7.4 `tx-once --bandwidth 80 --tx-rate vht2ss-mcs9 --short-gi --ldpc --stbc --tx-led --tx-status` submitted one descriptor-prefixed packet and echoed VHT 2SS MCS9 in JSON. The matching `tx-repeat --count 3 --interval-ms 200` submitted 3 descriptor-prefixed packets with the same rate/options. Reports: `/tmp/wfb-live-tx-once-vht-rate.json`, `/tmp/wfb-live-tx-repeat-vht-rate.json`.
- [ ] 9.4 Add explicit TX power control after EFUSE/power table behavior is understood
  - Progress: guarded `efuse-dump` reads RTL8812AU physical EFUSE through control-register reads, decodes the 512-byte logical map, and summarizes USB identity, MAC, RFE, thermal, and TX-power offsets without EFUSE programming, bulk traffic, channel retune, or RF TX. Live April 30, 2026 macOS 15.7.4 result on `0x0bda:0x8812` decoded 49 packets, found the EFUSE terminator at physical byte 378, reported EFUSE USB ID `0x0bda:0x8812`, MAC `00:c0:ca:ba:bd:9f`, RFE option `0x03`, and 66 non-`0xff` bytes in the 84-byte TX-power region. Report: `/tmp/wfb-live-efuse-dump.json`. The remote macOS 26 IOUSBHost fallback produced the same EFUSE summary at `/tmp/wfb-remote-macos-efuse-dump.json`. Explicit TX power control remains unchecked until these bytes are mapped to final Linux driver RF power indexes.
- [x] 9.5 Add optional LDPC, STBC, and SGI support behind visible CLI flags
  - Live result: April 30, 2026 macOS 15.7.4 `tx-once --short-gi --ldpc --stbc` submitted one descriptor-prefixed packet to bulk OUT endpoint `0x02` and reported all three options in JSON. Report: `/tmp/wfb-live-tx-once-flags.json`.

## 10. Documentation

- [x] 10.1 Document hardware assumptions, supported adapter IDs, and macOS versions
- [x] 10.2 Document bench setup with one Mac radio and one Linux WFB peer
- [x] 10.3 Document how to capture Linux USB baselines for regression comparison
- [x] 10.4 Document safe/default channel, bandwidth, and TX power behavior
- [x] 10.5 Document known limitations and when USBDriverKit becomes necessary

## 11. LED Diagnostics

- [x] 11.1 Add guarded RTL8812AU LED on/off/blink diagnostic
  - Live result: April 30, 2026 macOS 15.7.4 `led-smoke` passed register readback for normal LED0/1/2 and alternate antdiv/minicard LEDCFG paths on `0x0bda:0x8812`. Reports: `/tmp/wfb-live-led-smoke-led0.json`, `/tmp/wfb-live-led-smoke-led1.json`, `/tmp/wfb-live-led-smoke-led2.json`, `/tmp/wfb-live-led-smoke-antdiv-led0.json`, `/tmp/wfb-live-led-smoke-minicard-led0.json`, `/tmp/wfb-live-led-smoke-minicard-led1.json`.
  - Confirmed mapping: the visible blue enclosure LED is normal `led0` on `REG_LEDCFG0`; confirmation report: `/tmp/wfb-live-led-confirm-normal-led0.json`.
- [x] 11.2 Add opt-in TX activity LED hook after a visible LED pin/mode pair is confirmed
  - Live result: April 30, 2026 macOS 15.7.4 `tx-once --tx-led --tx-led-hold-ms 700` submitted one descriptor-prefixed packet to bulk OUT endpoint `0x02` and toggled normal `led0` / `REG_LEDCFG0` on/off with readback pass. Report: `/tmp/wfb-live-tx-once-led.json`.
  - Live result: April 30, 2026 macOS 15.7.4 `tx-repeat --count 3 --interval-ms 200 --tx-led --tx-led-hold-ms 700` submitted 3 descriptor-prefixed packets and held normal `led0` / `REG_LEDCFG0` on across the software TX burst before turning it off. Report: `/tmp/wfb-live-tx-repeat-led.json`.

## 12. TX Status Diagnostics

- [x] 12.1 Add read-only RTL8812AU TX status snapshots around live TX diagnostics
  - Live result: April 30, 2026 macOS 15.7.4 `tx-once --tx-led --tx-status --tx-status-delay-ms 50` submitted one descriptor-prefixed packet, toggled normal `led0` / `REG_LEDCFG0`, sampled 15 TX status registers before/after TX with no probe error, and reported `REG_TXPKT_EMPTY` changing from `0x0fff` to `0x0ffe`. Report: `/tmp/wfb-live-tx-once-status.json`.
  - Live result: April 30, 2026 macOS 15.7.4 `tx-repeat --count 3 --interval-ms 200 --tx-led --tx-status --tx-status-delay-ms 50` submitted 3 descriptor-prefixed packets, toggled normal `led0` / `REG_LEDCFG0`, sampled 15 TX status registers before/after the burst with no probe error, and reported no changed status registers in that window. Report: `/tmp/wfb-live-tx-repeat-status.json`.
- [ ] 12.2 Promote stable TX status evidence into RF-aware reporting after independent peer verification
