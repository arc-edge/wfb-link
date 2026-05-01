# macOS IOUSBHost Fallback

`wfb-radio-diag` has macOS-only IOUSBHost diagnostics for adapters that IOKit can see but libusb cannot enumerate.

This matters on macOS 26 because the attached RTL8812AU appeared in the IOUSB plane as an `IOUSBHostDevice`, but not as a registered, matched, configured interface tree that libusb could list or claim.

## Commands

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-usb-state.json macos-usb-state \
  --vid 0x0bda \
  --pid 0x8812

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-descriptor-smoke.json macos-descriptor-smoke \
  --vid 0x0bda \
  --pid 0x8812

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-interface-smoke.json macos-interface-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-may-reconfigure-usb

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-bulk-in-smoke.json macos-bulk-in-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-may-reconfigure-usb

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-bulk-out-smoke.json macos-bulk-out-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-may-reconfigure-usb \
  --i-understand-this-submits-bulk-out

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-session-smoke.json macos-session-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-may-reconfigure-usb \
  --i-understand-this-submits-bulk-out

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-reg-smoke.json macos-reg-smoke \
  --vid 0x0bda \
  --pid 0x8812

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-efuse-dump.json macos-efuse-dump \
  --vid 0x0bda \
  --pid 0x8812 \
  --raw-out /tmp/wfb-remote-macos-efuse-raw.bin \
  --logical-map-out /tmp/wfb-remote-macos-efuse-logical.bin \
  --i-understand-this-writes-control-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-power-on-smoke.json macos-power-on-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-firmware-smoke.json macos-firmware-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --firmware /tmp/rtl8812aefw.bin \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-llt-smoke.json macos-llt-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-queue-dma-smoke.json macos-queue-dma-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --bulk-out-endpoint-count 3 \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-mac-smoke.json macos-mac-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-bb-smoke.json macos-bb-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --bb-source /tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-rf-smoke.json macos-rf-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --rf-source /tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-init-usbhost.json init \
  --macos-usbhost \
  --vid 0x0bda \
  --pid 0x8812 \
  --channel 36 \
  --bandwidth 20 \
  --firmware /tmp/rtl8812aefw.bin \
  --bb-source /tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c \
  --rf-source /tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-rx-scan-usbhost.json rx-scan \
  --macos-usbhost \
  --vid 0x0bda \
  --pid 0x8812 \
  --channel 36 \
  --duration-ms 1000 \
  --pcap /tmp/wfb-remote-macos-rx-scan-usbhost.pcap \
  --frame-jsonl /tmp/wfb-remote-macos-rx-scan-usbhost.jsonl

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-tx-once-usbhost.json tx-once \
  --macos-usbhost \
  --vid 0x0bda \
  --pid 0x8812 \
  --channel 36 \
  --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" \
  --i-understand-this-transmits

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-tx-repeat-usbhost.json tx-repeat \
  --macos-usbhost \
  --vid 0x0bda \
  --pid 0x8812 \
  --channel 36 \
  --count 3 \
  --interval-ms 100 \
  --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" \
  --i-understand-this-transmits
```

## April 30, 2026 Remote Result

On `rownd@rownds-macbook-pro`, `usb-probe --all` did not list the RTL8812AU through libusb. IOKit did list it:

- Product: `802.11n NIC`
- VID/PID: `0x0bda:0x8812`
- Serial: `123456`
- Location ID: `0x01100000`
- USB link speed: 480 Mbps
- IOKit flags: `!registered`, `!matched`, `active`
- Configurations: `1`
- Current configuration: absent before manual configuration, `1` after a direct IOUSBHost configure attempt
- Interface children: absent

The read-only IOUSBHost descriptor smoke test passed even though the IORegistry tree still lacked interface children:

- Report: `/tmp/wfb-remote-macos-descriptor-smoke.json`
- Control reads: 3
- Device descriptor VID/PID: `0x0bda:0x8812`
- Configurations: 1
- Configuration value: 1
- Interfaces in descriptor: 1
- Total configuration descriptor length: 53 bytes
- Bulk IN endpoints: `0x81`
- Bulk OUT endpoints: `0x02`, `0x03`, `0x04`
- Interrupt IN endpoint: `0x85`
- Max packet size: 512 bytes for all bulk endpoints, 64 bytes for interrupt IN

The IOUSBHost interface/pipe smoke test then passed after issuing `configureWithValue:1 matchInterfaces:YES`:

- Report: `/tmp/wfb-remote-macos-interface-smoke.json`
- Configuration: 1
- Interface: 0
- Interface polls observed: 2
- Matched interfaces: 1
- Copied pipes: bulk IN `0x81`, bulk OUT `0x02`, `0x03`, `0x04`
- Bulk max packet size: 512 bytes
- Bulk IO submitted: none

The bounded IOUSBHost bulk-IN smoke test passed against endpoint `0x81`:

- Report: `/tmp/wfb-remote-macos-bulk-in-smoke.json`
- Configuration: 1
- Interface polls observed: 1
- Matched interfaces: 1
- Pipe copied: `0x81`
- Request length: 512 bytes
- Result: timed out after 100 ms with `IOUSBHostErrorDomain` code `-536870186`
- Interpretation: acceptable for this smoke because no RF traffic was present; it proves the pipe accepted a synchronous bulk-IN request
- Bulk OUT writes: 0

The zero-length IOUSBHost bulk-OUT smoke test also passed against endpoint `0x02`:

- Report: `/tmp/wfb-remote-macos-bulk-out-smoke.json`
- Configuration: 1
- Interface polls observed: 1
- Matched interfaces: 1
- Pipe copied: `0x02`
- Request length: 0 bytes
- Result: synchronous write completed
- Bulk IN reads: 0
- Bulk OUT writes: 1

The retained IOUSBHost session smoke test passed using one configured interface session:

- Report: `/tmp/wfb-remote-macos-session-smoke.json`
- Configuration: 1
- Retained pipes: bulk IN `0x81`, bulk OUT `0x02`
- Control reads through the same process/session: 6
- `REG_SYS_FUNC_EN`: `0x1f`
- `REG_APS_FSMCO`: `0x20020002`
- `REG_SYS_CLKR`: `0xfc23`
- `REG_RF_CTRL`: `0x07`
- `REG_MCUFWDL`: `0xc6`
- `REG_CR`: `0x06ff`
- Retained bulk IN request: endpoint `0x81`, 512 bytes requested, timed out after 100 ms with no RF traffic
- Retained bulk OUT request: endpoint `0x02`, zero bytes requested, completed

The integrated IOUSBHost transport then passed direct default-control register reads:

```text
REG_SYS_FUNC_EN 0x0002 u8  = 0x1c
REG_APS_FSMCO   0x0004 u32 = 0x20020002
REG_SYS_CLKR    0x0008 u16 = 0xfc23
REG_RF_CTRL     0x001f u8  = 0x00
REG_MCUFWDL     0x0080 u8  = 0x05
REG_CR          0x0100 u16 = 0xeaea
```

The same IOUSBHost transport also passed the guarded EFUSE dump:

- Report: `/tmp/wfb-remote-macos-efuse-dump.json`
- Raw physical dump: `/tmp/wfb-remote-macos-efuse-raw.bin`
- Decoded logical map: `/tmp/wfb-remote-macos-efuse-logical.bin`
- Decoded packets: 49
- Physical EFUSE used before terminator: 378 bytes
- EFUSE USB identity: `0x0bda:0x8812`
- EFUSE MAC address: `00:c0:ca:ba:bd:9f`
- RFE option byte: `0x03`
- TX power region: 84 bytes, 66 non-`0xff` bytes

The guarded IOUSBHost power-on smoke test also passed:

- Report: `/tmp/wfb-remote-macos-power-on-smoke.json`
- Steps: 14 passed
- Control reads: 25
- Control writes: 11
- Bulk IN reads: 0
- Bulk OUT writes: 0
- Covered phases: card-emulation-to-active, command-register enable, RF path A/B reset

After a fresh `macos-power-on-smoke`, the guarded IOUSBHost firmware smoke test also passed with a temporary Linux-firmware `rtlwifi/rtl8812aefw.bin` copy:

- Firmware SHA-256: `d40396544ee56c9dab43a458344b8936aa3d878c1582e96a62e9346bdfbdf50f`
- Report: `/tmp/wfb-remote-macos-firmware-smoke.json`
- Firmware payload written: 27,484 bytes
- Firmware control writes: 290
- Checksum poll attempts: 1
- Ready poll attempts: 18
- Final `REG_MCUFWDL`: `0x000607c6`
- Bulk IN reads: 0
- Bulk OUT writes: 0

After a fresh `macos-power-on-smoke`, the guarded IOUSBHost LLT smoke test also passed:

- Report: `/tmp/wfb-remote-macos-llt-smoke.json`
- Entries written: 256
- Max poll attempts observed: 1
- Control reads: 257
- Control writes: 256
- Bulk IN reads: 0
- Bulk OUT writes: 0

After fresh power-on, firmware, and LLT smoke stages, the guarded IOUSBHost queue/DMA smoke test passed with the AWUS036ACH's known three-bulk-OUT queue layout supplied explicitly:

- Report: `/tmp/wfb-remote-macos-queue-dma-smoke.json`
- Bulk OUT endpoint count: 3
- Queue select mask: `0x07`
- HPQ pages: `0x10`
- LPQ pages: `0x10`
- NPQ pages: `0x00`
- Public queue pages: `0xd8`
- `REG_RQPN`: `0x80d81010`
- Control reads: 22
- Control writes: 10
- Bulk IN reads: 0
- Bulk OUT writes: 0

After fresh power-on, firmware, LLT, and queue/DMA smoke stages, the guarded IOUSBHost MAC/WMAC smoke test also passed:

- Report: `/tmp/wfb-remote-macos-mac-smoke.json`
- Receive configuration: `0x740060ce`
- Retry limit: `0x3030`
- Control reads: 50
- Control writes: 24
- Bulk IN reads: 0
- Bulk OUT writes: 0

After fresh power-on, firmware, LLT, queue/DMA, and MAC/WMAC smoke stages, the guarded IOUSBHost BB smoke test also passed using `aircrack-ng/rtl8812au` reference source commit `7344855`:

- Report: `/tmp/wfb-remote-macos-bb-smoke.json`
- `PHY_REG` writes applied: 215
- `AGC_TAB` writes applied: 132
- Delays applied: 0
- Control reads: 12
- Control writes: 352
- Bulk IN reads: 0
- Bulk OUT writes: 0

After BB smoke, the guarded IOUSBHost RF smoke test also passed:

- Report: `/tmp/wfb-remote-macos-rf-smoke.json`
- `radioA` writes applied: 206
- `radioB` writes applied: 193
- Delays applied: 4
- Control reads: 3
- Control writes: 399
- Bulk IN reads: 0
- Bulk OUT writes: 0

The integrated retained-session radio path then passed full init, RX, and TX diagnostics:

- `init --macos-usbhost`: `/tmp/wfb-remote-macos-init-usbhost.json`, result `pass`, channel 36/20 MHz, 491 control reads, 1,396 control writes, all power, firmware, LLT, queue/DMA, MAC, BB, RF, and channel phases completed.
- `rx-scan --macos-usbhost`: `/tmp/wfb-remote-macos-rx-scan-usbhost.json`, result `pass`, 10 bounded bulk-IN timeouts on endpoint `0x81`, 0 USB errors, header-only PCAP, empty frame JSONL because no RF traffic was present.
- `tx-once --macos-usbhost`: `/tmp/wfb-remote-macos-tx-once-usbhost.json`, result `pass`, one 64-byte descriptor-prefixed packet written to endpoint `0x02`.
- `tx-repeat --macos-usbhost`: `/tmp/wfb-remote-macos-tx-repeat-usbhost.json`, result `pass`, three 64-byte descriptor-prefixed packets written to endpoint `0x02` with no failed or short writes.
- `tx-once --macos-usbhost --tx-led --tx-status`: `/tmp/wfb-remote-macos-tx-once-led-status-usbhost.json`, result `pass`, LED on/off register readback passed, TX status pre/post reads passed, and one 64-byte bulk-OUT packet was submitted.
- `init --macos-usbhost --bandwidth 80`: `/tmp/wfb-remote-macos-init-80mhz-usbhost.json`, result `pass`, channel 36/80 MHz completed in 19 channel steps.
- `tx-once --macos-usbhost --bandwidth 80 --tx-rate vht2ss-mcs9 --short-gi --ldpc --stbc`: `/tmp/wfb-remote-macos-tx-once-vht-usbhost.json`, result `pass`, one 64-byte descriptor-prefixed VHT packet was submitted.
- `bridge-tx-once --macos-usbhost`: `/tmp/wfb-remote-macos-bridge-tx-once-usbhost.json`, result `pass`, parsed one 41-byte WFB distributor-style datagram with fwmark `0x00000000`, a 13-byte HT radiotap header, and a 24-byte IEEE 802.11 frame, then submitted one 64-byte descriptor-prefixed packet to endpoint `0x02` with bridge counters `incoming=1`, `injected=1`, `dropped=0`.
- `bridge-tx-listen --macos-usbhost`: `/tmp/wfb-remote-macos-bridge-tx-listen-usbhost.json`, result `pass`, bound `127.0.0.1:5611`, received one local UDP WFB distributor-style datagram, and submitted one 64-byte descriptor-prefixed packet to endpoint `0x02` with bridge counters `incoming=1`, `injected=1`, `dropped=0`.
- `bridge-tx-listen --macos-usbhost --max-datagrams 3`: `/tmp/wfb-remote-macos-bridge-tx-listen-3-usbhost.json`, result `pass`, received three local UDP datagrams in one retained session and submitted all three packets with bridge counters `incoming=3`, `injected=3`, `dropped=0`, 192 USB bytes written, and no failed or short writes.
- `bridge-tx-listen --macos-usbhost --init-before-tx`: `/tmp/wfb-agent-listen-linuxorder.json`, result `pass`, used same-session init order `linux_llt_before_firmware`, received 40 UDP WFB distributor-style datagrams, submitted 40/40 MGNT-queue HT MCS1 packets with no drops or short writes, and the Linux monitor pcap `/tmp/mac-listen-linuxorder-rf.pcap` contained 39 `LISTENORD` WFB payload markers on channel 36/HT20.
- `bridge-tx-listen --macos-usbhost --init-before-tx` with stock WFB-ng distributor input: `/tmp/wfb-agent-stock-controlled-listen.json`, result `pass`, received 300 datagrams from Linux `wfb_tx -d`, submitted 300/300 packets with no drops or short writes, and dedicated Linux `wfb_rx` recovered 99 decrypted `STOCKCTRL` payloads from the Mac RF path. RF and receiver captures: `/tmp/mac-stock-controlled-rf.pcap`, `/tmp/mac-stock-controlled-rx-lo.pcap`.
- `bridge-tx-listen --macos-usbhost --init-before-tx` now reports runtime throughput and CPU metrics. A 20 MHz baseline with stock `wfb_tx -d -k 8 -n 12` and 512-ish source payloads received and injected 1,200/1,200 distributor datagrams, wrote 723,735 USB bytes in 14.896 s, reported 80.56 submitted datagrams/s, 48.58 KB/s USB write rate, and 1.43% of one CPU core. The Linux monitor captured 1,169 WFB MAC frames, and Linux `wfb_rx` forwarded 796 decrypted `SIZE512` payloads. Reports/captures: `/tmp/wfb-agent-size512-listen.json`, `/tmp/mac-size512-rf.pcap`, `/tmp/mac-size512-rx-lo.pcap`.
- Payload-size bracketing showed that 768, 900, and exactly 1,000 byte source payloads all recover cleanly with stock `wfb_tx/wfb_rx -l 1000`: each short run injected 600/600 distributor datagrams and recovered 400/400 decrypted payloads. A sustained 1,000-byte run injected 3,000/3,000 datagrams, wrote 3,271,300 USB bytes in 32.525 s, captured 2,906 WFB MAC frames, and recovered 1,999/2,000 decrypted `SUST1000` payloads at 92.24 submitted datagrams/s, 100.58 KB/s USB write rate, and 1.32% CPU. Reports/captures: `/tmp/wfb-agent-size768c-listen.json`, `/tmp/wfb-agent-size900-listen.json`, `/tmp/wfb-agent-size1000-listen.json`, `/tmp/wfb-agent-sust1000-listen.json`, `/tmp/mac-sust1000-rf.pcap`, `/tmp/mac-sust1000-rx-lo.pcap`.
- The earlier 1024-ish source-payload failure submitted cleanly on the Mac but exceeded the `-l 1000` source-payload setting used by the stock WFB tools. It is no longer evidence of a Mac-side large-MPDU TX gate inside the configured MTU.
- `bridge-run --macos-usbhost --init-before-tx` is the first bounded full bridge command. It keeps one retained IOUSBHost session, runs Linux-order init once, preserves station/MSR state for TX, opens RX filter maps, interleaves UDP TX input with bulk-IN RX reads, and forwards matching WFB RX frames to an aggregator socket. The first bidirectional run forwarded 44 Linux-to-Mac WFB frames to a Mac UDP aggregator while injecting 121/121 Mac-to-Linux distributor datagrams; Linux `wfb_rx` recovered 80/80 `MAC2LIN` payloads and the Linux monitor captured 120 WFB MAC frames. Reports/captures: `/tmp/wfb-agent-bridge-run-duplex3.json`, `/tmp/wfb-agent-bridge-run-duplex3-agg.json`, `/tmp/mac-bridgerun-duplex3-rf.pcap`, `/tmp/mac-bridgerun-duplex3-rx-lo.pcap`.
- Multi-port `bridge-run --macos-usbhost` now works in one retained IOUSBHost session. A four-stream run used two TX bind sockets (`0.0.0.0:5612`, `0.0.0.0:5614`) and two RX forward targets (`radio_port=0` to `127.0.0.1:5700`, `radio_port=1` to `127.0.0.1:5701`). With `--tx-burst-limit 8`, the Mac injected 423/423 distributor datagrams, per-bind counters reported 242 datagrams on port 5612 and 181 on port 5614, Linux `wfb_rx` recovered 120/120 `M2LVID` markers and 79/80 `M2LTEL` markers, and the Mac forwarded 120 port-0 frames plus 69 port-1 frames. Reports/captures: `/tmp/wfb-agent-bridge-run-multibridge3.json`, `/tmp/wfb-agent-bridge-run-multibridge3-agg.json`, `/tmp/mac-multibridge3-rf.pcap`, `/tmp/mac-multibridge3-rx-lo.pcap`.
- RX forwarding from the Mac bridge into a real stock WFB-ng aggregator also passes. Linux `wfb_tx` sent 120 source UDP payloads on radio port 0; Mac `bridge-run` received the RF frames and forwarded 138 matching WFB frames as `wrxfwd_t` UDP datagrams to Linux `wfb_rx -a 5800`; the aggregator recovered 120/120 decoded `L2MRXAG` payloads. Report/capture: `/tmp/wfb-agent-bridge-run-rxagg1.json`, `/tmp/mac-rxagg1-agg-lo.pcap`.

## Interpretation

The macOS 26 blocker is not raw USB device visibility, descriptor access, default-control access, interface matching, one-shot pipe IO, retaining interface and pipe objects, full RTL8812AU init, bounded RX reads, bulk-OUT TX submission, or low-rate WFB-shaped RF TX. The default control endpoint is reachable through IOUSBHost even when libusb cannot enumerate the radio, standard USB descriptors can be read, guarded register-write sequences can execute through channel setup, interface 0 can be opened after `configureWithValue:matchInterfaces:`, and descriptor-confirmed bulk pipes can serve `rx-scan`, `tx-once`, `tx-repeat`, and UDP-fed bridge TX.

The remaining macOS 26 proof target is no longer basic RF visibility, low-rate WFB interoperability, sustained packet flow at `-l 1000`, bounded full-duplex bridge operation, multi-port bridge plumbing, or stock WFB-ng aggregator compatibility; it is real encoded video/telemetry workload stability.

## SDK Notes

The macOS 26.4 Command Line Tools IOUSBHost headers confirm the public object split:

- `IOUSBHostDevice` exposes default-control requests, `configureWithValue:matchInterfaces:error:`, the current `configurationDescriptor`, and reset.
- `IOUSBHostInterface` exposes `copyPipeWithAddress:error:` for endpoint pipes.
- `IOUSBHostPipe` exposes synchronous `sendIORequestWithData:bytesTransferred:completionTimeout:error:` for bulk/interrupt transfers and async enqueue APIs.

Endpoint discovery, one-shot pipe IO, retained control access, full init, bounded RX, descriptor-prefixed TX, low-rate UDP-fed bridge RF TX, and stock WFB-ng distributor-to-Linux-receiver payload delivery are now proven. The remaining implementation target is sustained bidirectional load.
