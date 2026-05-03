# wfb-mac-radio

Native macOS experiments for driving WFB-ng over USB Wi-Fi radios without relying on macOS monitor-mode packet injection.

The working theory is that the Mac should treat an ALFA AWUS036ACH as a USB radio peripheral. A userspace backend claims the RTL8812AU USB interface, initializes the chip, sends raw 802.11 frames through bulk OUT transfers, receives raw 802.11 frames through bulk IN transfers, and bridges those frames to WFB-ng's existing distributor/aggregator protocols.

## Initial Target

- Host: Apple Silicon Mac, macOS 15 or macOS 26
- Radio: ALFA AWUS036ACH / Realtek RTL8812AU
- Peer: known-good Linux WFB-ng system for baseline comparison
- First success criterion: low-rate WFB payload exchange
- Later success criterion: sustained WFB video and telemetry

## Planning

The first OpenSpec change is:

- `openspec/changes/native-macos-usb-radio-backend/`

Useful entry points:

- `proposal.md`: motivation and capability split
- `design.md`: architecture, decisions, risks, and staged migration plan
- `specs/userspace-usb-radio/spec.md`: USB radio backend requirements
- `specs/wfb-radio-bridge/spec.md`: WFB bridge requirements
- `specs/radio-verification/spec.md`: diagnostic and proof requirements
- `tasks.md`: implementation checklist
- `docs/bench-plan.md`: hardware assumptions, Mac/Linux bench setup, USB capture baseline workflow, safe defaults, and limitations
- `docs/macos-usbhost.md`: macOS 26 IOUSBHost fallback for devices libusb cannot enumerate
- `docs/usb-trace-format.md`: normalized USB trace schema for Linux-vs-macOS transfer comparison
- `docs/register-smoke.md`: first live, read-only RTL8812AU register diagnostic
- `docs/led-smoke.md`: guarded RTL8812AU software LED control diagnostic and latest hardware result
- `docs/efuse-dump.md`: guarded RTL8812AU EFUSE dump, logical map decode, and latest hardware result
- `docs/power-on-smoke.md`: first guarded RTL8812AU register-write diagnostic
- `docs/firmware-smoke.md`: guarded RTL8812A firmware download/checksum/readiness diagnostic
- `docs/llt-smoke.md`: guarded RTL8812A linked-list table programming diagnostic
- `docs/queue-dma-smoke.md`: guarded RTL8812A queue and DMA register programming diagnostic
- `docs/mac-smoke.md`: guarded RTL8812A MAC/WMAC register programming diagnostic
- `docs/bb-smoke.md`: guarded RTL8812A BB PHY/AGC table programming diagnostic
- `docs/rf-smoke.md`: guarded RTL8812A RF radioA/radioB table programming diagnostic
- `docs/init-live.md`: integrated live RTL8812AU init diagnostic and latest hardware result
- `docs/rx-scan.md`: bounded live bulk-IN RX diagnostic and latest hardware result
- `docs/tx-once.md`: guarded live single-frame bulk-OUT TX diagnostic and latest hardware result
- `docs/tx-repeat.md`: guarded live repeated bulk-OUT TX diagnostic and latest hardware result
- `docs/rf-quality-baseline.md`: Linux receiver-backed RF-quality baseline procedure, capture helper, and current close-range 20 MHz baseline fixture
- `docs/rtl8812au-efuse-tx-power.md`: EFUSE-derived RTL8812AU TXAGC calculation, Linux basis, guarded CLI mode, and current channel-36 HT20 scope
- `docs/init-dry-run.md`: hardware-free init transfer planning scaffold and limitations
- `docs/rtl8812au-init-audit.md`: source audit reference points behind the dry-run init skeleton

## Shape of the System

```text
stock WFB-ng distributor
        |
        | fwmark + radiotap + 802.11 frame
        v
wfb-radio-bridge  <---->  userspace RTL8812AU backend  <---->  AWUS036ACH
        |
        | wrxfwd_t + WFB payload
        v
stock WFB-ng aggregator
```

The bridge replaces Linux `PF_PACKET`/`pcap` radio I/O. It does not attempt to create a fake macOS Wi-Fi interface.

## Current Status

Initial implementation has started. The current code can:

- Build a Rust workspace with `radio-core`, `wfb-bridge`, and `wfb-radio-diag`.
- Use `wfb-radio-runtime` for live USB session ownership, endpoint selection, TX submission, raw packet replay, RX packet reads, and shared runtime counters across bridge and standalone RX/TX commands.
- Discover supported RTL8812AU-class USB adapters by VID/PID.
- Walk USB descriptors and endpoint layouts.
- Claim and release interface 0 for a supported adapter.
- Emit human-readable or JSON `usb-probe` reports.
- Inspect macOS IOKit USB state with `macos-usb-state`, including devices libusb cannot enumerate.
- Read USB descriptors, RTL8812AU registers, EFUSE, guarded power-on/RF-reset writes, firmware download/readiness, LLT programming, queue/DMA setup, MAC/WMAC setup, BB tables, and RF tables through macOS IOUSBHost default-control transfers when macOS has not created libusb-visible interfaces.
- Configure IOUSBHost interface matching, open interface 0, copy descriptor-confirmed bulk pipes, submit bounded bulk-IN and zero-length bulk-OUT smokes, hold a retained IOUSBHost session across register reads plus both bulk pipe directions, and run retained-session `init`, `rx-scan`, `tx-once`, and `tx-repeat` on macOS 26.
- Run a read-only RTL8812AU register smoke test after claiming the adapter.
- Run guarded RTL8812AU software LED on/off/blink diagnostics across normal, antenna-diversity, and minicard LED paths.
- Read RTL8812AU physical EFUSE bytes, decode the logical EFUSE map, and summarize identity/RFE/TX-power offsets without EFUSE programming.
- Run a guarded RTL8812AU power-on/RF-reset smoke test with before/after register readback.
- Load, validate, header-skip, and download external RTL8812A firmware images through guarded control transfers.
- Program the RTL8812A LLT page chain through guarded writes and per-entry polling.
- Program RTL8812A queue reserved pages, TX/RX DMA boundaries, TXDMA queue map, and packet-buffer page size through guarded control transfers.
- Program RTL8812A MAC/WMAC driver-info, receive filter, rate/retry, EDCA, HW sequence, BAR, and MAC TX/RX enable registers through guarded control transfers.
- Parse external RTL8812A `PHY_REG` and `AGC_TAB` tables and program BB registers through guarded control transfers.
- Parse external RTL8812A `radioA`/`radioB` tables and program RF registers through the BB 3-wire write registers.
- Run integrated live `init` over one USB claim, covering power-on, firmware download, LLT, queue/DMA, MAC, BB, RF, and 20/40/80 MHz channel setup phases with phase-level JSON diagnostics.
- Model supported 2.4 GHz and 5 GHz channels.
- Program RTL8812A 20/40/80 MHz channel switches and report the effective channel/bandwidth.
- Build RTL8812AU 40-byte TX descriptors for validated IEEE 802.11 frames.
- Encode OFDM, HT MCS, and VHT NSS/MCS rate IDs in RTL8812AU TX descriptors.
- Select explicit diagnostic TX rates with `--tx-rate`, including legacy rates, `mcsN`, and `vhtNss-mcsM`.
- Submit descriptor-prefixed frames through runtime-owned USB bulk OUT transport with TX counters.
- Run guarded live `tx-once` single-frame TX against an already-initialized adapter.
- Run guarded live `tx-repeat` bounded repeated TX with explicit count, interval, frame, and authorization.
- Drive the confirmed visible blue LED during live `tx-once` and `tx-repeat` software TX submissions with `--tx-led`.
- Sample read-only RTL8812AU TX status registers around live `tx-once` and `tx-repeat` submissions with `--tx-status`.
- Expose optional TX descriptor flags for SGI, LDPC, and STBC through visible CLI flags.
- Parse synthetic RTL8812AU RX descriptor buffers into raw frame records.
- Write captured raw IEEE 802.11 frames to classic PCAP files for offline inspection.
- Run live bounded `rx-scan` runtime bulk-IN reads against an already-initialized adapter and write optional PCAP output plus JSONL raw-frame records with RX metadata, including RTL8812AU OFDM PHY-status RSSI/SNR/noise when present.
- Compare normalized USB transfer traces for future Linux baseline regression checks.
- Build a hardware-free, source-audited init transfer skeleton from an RTL8812A firmware image and write it as normalized USB trace JSON.
- Match WFB-ng link/radio-port headers and serialize `wrxfwd_t` forwarding headers.
- Forward matching RX payloads to a WFB-ng aggregator UDP socket with RX counters.
- Parse WFB distributor/injector TX datagrams containing firmware mark, radiotap, and 802.11 frame bytes.
- Parse the HT/VHT radiotap layouts WFB-ng uses for TX metadata and map them into conservative radio TX options.
- Submit stripped TX frames to a trait-backed radio sink with TX bridge counters.
- Submit one WFB distributor-style datagram through the live radio backend with `bridge-tx-once`.
- Listen for bounded UDP WFB distributor-style datagrams and submit them through live radio TX with `bridge-tx-listen`.
- Run bounded RX forwarding and TX injection in one retained runtime radio session with `bridge-run`.
- Receive stock WFB-ng distributor traffic from Linux `wfb_tx -d`, inject it over the Mac-controlled AWUS036ACH, and deliver low-rate payloads to Linux `wfb_rx`.
- Build structured RF-quality envelopes with `rf-quality-report`, including macOS bridge artifacts, EFUSE context, Linux baseline parameters, parameter mismatches, recovery/loss/throughput summaries, receiver artifact paths, and receiver SNR confidence.
- List verification stages with `wfb-radio-diag stages`.
- Emit JSON diagnostics for current commands, including live `init`, `efuse-dump`, `rx-scan`, `tx-once`, `tx-repeat`, `bridge-tx-once`, `bridge-tx-listen`, and `bridge-run` reports; `tx-once --dry-run` also builds descriptor-prefixed bytes without touching USB.

The simplest diagnostic entry point is still:

```sh
cargo run -p wfb-radio-diag -- usb-probe
```

That lists candidate USB radios, attempts to claim/release the selected supported interface, and reports endpoint layout.

Useful variants:

```sh
cargo run -p wfb-radio-diag -- --json usb-probe
cargo run -p wfb-radio-diag -- --json --all usb-probe --no-claim
cargo run -p wfb-radio-diag -- usb-probe --vid 0x0bda --pid 0x8812
cargo run -p wfb-radio-diag -- --json macos-usb-state --vid 0x0bda --pid 0x8812
cargo run -p wfb-radio-diag -- --json macos-descriptor-smoke --vid 0x0bda --pid 0x8812
cargo run -p wfb-radio-diag -- --json macos-interface-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-may-reconfigure-usb
cargo run -p wfb-radio-diag -- --json macos-bulk-in-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-may-reconfigure-usb
cargo run -p wfb-radio-diag -- --json macos-bulk-out-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-may-reconfigure-usb --i-understand-this-submits-bulk-out
cargo run -p wfb-radio-diag -- --json macos-session-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-may-reconfigure-usb --i-understand-this-submits-bulk-out
cargo run -p wfb-radio-diag -- --json macos-reg-smoke --vid 0x0bda --pid 0x8812
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-macos-efuse-dump.json macos-efuse-dump --vid 0x0bda --pid 0x8812 --raw-out /tmp/wfb-macos-efuse-raw.bin --logical-map-out /tmp/wfb-macos-efuse-logical.bin --i-understand-this-writes-control-registers
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-macos-power-on-smoke.json macos-power-on-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json extract-firmware --source /tmp/wfb-ref-rtl8812au/hal/rtl8812a/hal8812a_fw.c --array array_mp_8812a_fw_nic --out /tmp/rtl8812aefw.bin --overwrite
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-macos-firmware-smoke.json macos-firmware-smoke --vid 0x0bda --pid 0x8812 --firmware /tmp/rtl8812aefw.bin --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-macos-llt-smoke.json macos-llt-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-macos-queue-dma-smoke.json macos-queue-dma-smoke --vid 0x0bda --pid 0x8812 --bulk-out-endpoint-count 3 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-macos-mac-smoke.json macos-mac-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json reg-smoke --vid 0x0bda --pid 0x8812
cargo run -p wfb-radio-diag -- --json led-smoke --vid 0x0bda --pid 0x8812 --pin led0 --mode normal --action blink --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json led-smoke --vid 0x0bda --pid 0x8812 --pin led0 --mode antdiv --action blink --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-efuse-dump.json efuse-dump --vid 0x0bda --pid 0x8812 --raw-out /tmp/wfb-live-efuse-raw.bin --logical-map-out /tmp/wfb-live-efuse-logical.bin --i-understand-this-writes-control-registers
cargo run -p wfb-radio-diag -- --json power-on-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json firmware-smoke --vid 0x0bda --pid 0x8812 --firmware /tmp/rtl8812aefw.bin --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json llt-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json queue-dma-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json mac-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json bb-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json rf-smoke --vid 0x0bda --pid 0x8812 --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json init --vid 0x0bda --pid 0x8812 --channel 36 --bandwidth 20 --firmware /tmp/rtl8812aefw.bin --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json init --vid 0x0bda --pid 0x8812 --channel 36 --bandwidth 80 --firmware /tmp/rtl8812aefw.bin --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json init --macos-usbhost --vid 0x0bda --pid 0x8812 --channel 36 --bandwidth 20 --firmware /tmp/rtl8812aefw.bin --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- stages
cargo run -p wfb-radio-diag -- --json stages
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-stages.json stages
cargo run -p wfb-radio-diag -- --json init --dry-run --firmware /path/to/rtl8812a.bin --trace-out /tmp/planned-init.json
cargo run -p wfb-radio-diag -- --json rx-scan --channel 36 --duration-ms 1000
cargo run -p wfb-radio-diag -- --json rx-scan --macos-usbhost --vid 0x0bda --pid 0x8812 --channel 36 --duration-ms 1000
cargo run -p wfb-radio-diag -- --json rx-scan --channel 36 --bandwidth 80 --duration-ms 1000
cargo run -p wfb-radio-diag -- --json rx-scan --channel 36 --pcap /tmp/wfb-rx.pcap --frame-jsonl /tmp/wfb-rx-frames.jsonl
cargo run -p wfb-radio-diag -- --json rx-scan --channel 36 --fixture-bulk-in /path/to/bulk-in.bin --pcap /tmp/wfb-rx-fixture.pcap --frame-jsonl /tmp/wfb-rx-fixture.jsonl
cargo run -p wfb-radio-diag -- --json tx-once --channel 36 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-once --macos-usbhost --vid 0x0bda --pid 0x8812 --channel 36 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-once --channel 36 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --tx-led --tx-led-hold-ms 700 --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-once --channel 36 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --tx-led --tx-status --tx-status-delay-ms 50 --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-once --channel 36 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --short-gi --ldpc --stbc --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-once --channel 36 --bandwidth 80 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --tx-rate vht2ss-mcs9 --short-gi --ldpc --stbc --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-once --channel 36 --dry-run --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)"
cargo run -p wfb-radio-diag -- --json tx-repeat --channel 36 --count 2 --interval-ms 100 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-repeat --macos-usbhost --vid 0x0bda --pid 0x8812 --channel 36 --count 3 --interval-ms 100 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-repeat --channel 36 --count 3 --interval-ms 200 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --tx-led --tx-led-hold-ms 700 --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-repeat --channel 36 --count 3 --interval-ms 200 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --tx-led --tx-status --tx-status-delay-ms 50 --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-repeat --channel 36 --bandwidth 80 --count 3 --interval-ms 200 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --tx-led --tx-status --tx-status-delay-ms 50 --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json tx-repeat --channel 36 --bandwidth 80 --count 3 --interval-ms 200 --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)" --tx-rate vht2ss-mcs9 --short-gi --ldpc --stbc --tx-led --tx-status --tx-status-delay-ms 50 --i-understand-this-transmits
cargo run -p wfb-radio-diag -- --json trace-import --input fixtures/traces/usbmon-sample.txt --output /tmp/usbmon-sample.json
cargo run -p wfb-radio-diag -- --json trace-compare --expected fixtures/traces/init-minimal-expected.json --observed fixtures/traces/init-minimal-observed.json
```

`rx-scan` is live: it assumes `init` already completed on the requested channel, claims the adapter, reads the bulk-IN endpoint for a bounded duration, parses RTL8812AU RX descriptors, and can write captured frames to PCAP plus JSONL metadata records. It does not issue control writes, run init, submit bulk OUT, or transmit frames.

`tx-once` is live: it assumes `init` already completed on the requested channel, claims the adapter, validates the supplied IEEE 802.11 frame hex, builds the RTL8812AU descriptor-prefixed packet, and writes exactly one packet to the selected bulk-OUT endpoint. It requires `--frame-hex` and `--i-understand-this-transmits`. The live April 30, 2026 run on macOS 15.7.4 wrote one 64-byte packet to endpoint `0x02` with `attempted=1`, `submitted=1`, and `failed=0`; independent over-the-air confirmation still needs a second monitor receiver.

`tx-once` and `tx-repeat` accept visible optional descriptor flags: `--short-gi`, `--ldpc`, and `--stbc`. These are explicit opt-ins and are reported back in JSON under `tx_options`. The April 30, 2026 flagged `tx-once` run confirmed those options reached the live descriptor path and still completed one bulk-OUT submission.

`tx-once` and `tx-repeat` accept `--tx-rate` for direct descriptor-rate diagnostics. Supported forms include legacy rates such as `ofdm6m`, HT forms such as `mcs7`, and VHT forms such as `vht2ss-mcs9`; the selected rate is echoed in JSON under `tx_options.rate`. April 30, 2026 VHT live reports: `/tmp/wfb-live-tx-once-vht-rate.json`, `/tmp/wfb-live-tx-repeat-vht-rate.json`.

`tx-once` and `tx-repeat` also accept `--tx-led` for software TX activity indication using the confirmed visible blue LED path by default: normal `led0` on `REG_LEDCFG0`. The JSON report includes `tx_activity_led` with LEDCFG on/off steps and counters. This indicates USB/software TX submission activity only; it is not RF proof. Live reports: `/tmp/wfb-live-tx-once-led.json`, `/tmp/wfb-live-tx-repeat-led.json`.

`tx-once` and `tx-repeat` accept `--tx-status` to read selected RTL8812AU interrupt, TXDMA, queue, TX pause, scheduler, and C2H event registers before and after live bulk-OUT submissions. The JSON report includes `tx_status.pre`, `tx_status.post`, `tx_status.changed`, counters, and probe errors. This is read-only chip-side telemetry around USB submission, not RF confirmation. Live reports: `/tmp/wfb-live-tx-once-status.json`, `/tmp/wfb-live-tx-repeat-status.json`.

`tx-repeat` is live with stronger gating: it requires an explicit frame, count, interval, channel, and `--i-understand-this-transmits`. The live April 30, 2026 run on macOS 15.7.4 sent three 64-byte descriptor-prefixed packets to endpoint `0x02` at 100 ms spacing with `attempted=3`, `submitted=3`, and `failed=0`. This is a USB submission and pacing diagnostic until an independent receiver confirms RF packet reception.

`bridge-tx-once` is live and write-gated: it reads one WFB distributor/injector datagram from `--datagram-hex` or `--datagram-file`, parses the firmware mark plus radiotap-prefixed IEEE 802.11 frame, maps HT/VHT radiotap metadata into radio TX options, and submits the stripped 802.11 frame through the live radio TX backend. The first macOS 26 IOUSBHost run on April 30, 2026 parsed a 41-byte datagram and submitted one 64-byte descriptor-prefixed HT MCS0 packet to endpoint `0x02` with bridge counters `incoming=1`, `injected=1`, `dropped=0`. Report: `/tmp/wfb-remote-macos-bridge-tx-once-usbhost.json`. This still needs a stock WFB-ng distributor plus Linux receiver before the TX bridge is considered end-to-end verified.

`bridge-tx-listen` is the bounded socket-facing version: it binds UDP with `--bind`, waits for up to `--max-datagrams`, and submits each received WFB distributor/injector datagram through the same bridge parser and live radio backend. The report includes bridge counters plus elapsed time, datagrams/s, byte rates, and process CPU usage. On May 1, 2026, the Linux-order same-session init path received 300 stock Linux `wfb_tx -d` distributor datagrams, injected 300/300, and a dedicated Linux `wfb_rx` recovered 99 decrypted `STOCKCTRL` payloads from the Mac RF path. The measured 20 MHz `-l 1000` path now recovers 768, 900, and exactly 1,000 byte source payloads; the sustained 1,000-byte run injected 3,000/3,000 datagrams, captured 2,906 WFB MAC frames, and recovered 1,999/2,000 decrypted payloads at 92.24 submitted datagrams/s, 100.58 KB/s USB write rate, and 1.32% CPU. Reports/captures: `/tmp/wfb-agent-stock-controlled-listen.json`, `/tmp/wfb-agent-sust1000-listen.json`, `/tmp/mac-sust1000-rf.pcap`, `/tmp/mac-sust1000-rx-lo.pcap`.

`bridge-run` is the first full bridge loop. It owns one retained radio session, runs Linux-order init once, preserves station/MSR state for TX, opens RX filter maps, drains UDP TX input with socket receiver thread(s), interleaves queued TX with bulk-IN RX reads, and forwards matching WFB RX frames to aggregator sockets. It supports multiple TX UDP inputs with repeated `--tx-bind` and multiple RX forwarding targets with repeated `--rx-forward`; `--tx-burst-limit` defaults to 8 so multi-port WFB traffic is paced between bulk-IN reads instead of dumping a large burst into the radio. The default run is bounded for diagnostics, while `--duration-ms 0` removes the time bound and `--max-datagrams 0` removes the TX datagram cap for longer bridge runs; `--ready-file` writes a JSON marker after init/calibration/monitor setup and immediately before RX/TX loops, so external traffic generators can avoid racing radio bring-up. SIGINT/SIGTERM exits through the normal report path with `stop_reason="signal"`, verified at `/tmp/wfb-agent-bridge-run-signal2.json`. The startup-burst regression is fixed: `/tmp/wfb-agent-bridgerun-drain2.json` drained and submitted 90/90 distributor datagrams sent during radio init, and Linux `wfb_rx` recovered 30/30 `DRAIN2` payloads. The first bidirectional macOS 26 run forwarded 44 Linux-to-Mac WFB frames to a Mac UDP aggregator while injecting 121/121 Mac-to-Linux distributor datagrams; Linux `wfb_rx` recovered 80/80 `MAC2LIN` payloads. A later four-stream video/telemetry-shaped run forwarded Linux-to-Mac WFB ports 0 and 1 to separate Mac aggregator sockets while injecting Mac-to-Linux ports 2 and 4 from separate TX bind sockets; Linux `wfb_rx` recovered 120/120 `M2LVID` markers and 79/80 `M2LTEL` markers, and the Mac forwarded 120 port-0 frames plus 69 port-1 frames. The RX forwarding path also works with a real stock WFB-ng network aggregator: Linux RF TX -> Mac bridge RX -> UDP `wrxfwd_t` to Linux `wfb_rx -a` recovered 120/120 decoded `L2MRXAG` payloads. Reports/captures: `/tmp/wfb-agent-bridge-run-duplex3.json`, `/tmp/wfb-agent-bridge-run-multibridge3.json`, `/tmp/wfb-agent-bridge-run-rxagg1.json`, `/tmp/wfb-agent-bridge-run-signal2.json`, `/tmp/wfb-agent-bridgerun-drain2.json`, `/tmp/wfb-agent-bridge-run-multibridge3-agg.json`, `/tmp/mac-bridgerun-duplex3-rf.pcap`, `/tmp/mac-multibridge3-rf.pcap`, `/tmp/mac-multibridge3-rx-lo.pcap`, `/tmp/mac-rxagg1-agg-lo.pcap`.

`runtime-flow` is the production-shaped wrapper around the retained full bridge path. It reports adapter identity, endpoints, init readiness, calibration class/evidence source, runtime-owned RX/TX flow telemetry, RX metadata coverage counters for PHY status/RSSI/SNR/noise, USB counters, stop reason, and last error without exposing the full diagnostic bridge report shape. It accepts the bridge runtime controls needed for normal RX/TX, including `--ready-file` for external orchestration, but rejects diagnostic-only register pokes and TX-status probes before USB open; the remaining migration step is a smaller production binary/API that no longer reuses diagnostic CLI structs.

`reg-smoke` is live but read-only: it claims the adapter, reads a small set of RTL8812AU registers through vendor control requests, reports the values, and then releases the interface. It does not issue control writes, bulk transfers, RF changes, or TX operations.

`macos-usb-state`, `macos-descriptor-smoke`, `macos-interface-smoke`, `macos-bulk-in-smoke`, `macos-bulk-out-smoke`, `macos-session-smoke`, `macos-reg-smoke`, `macos-efuse-dump`, `macos-power-on-smoke`, `macos-firmware-smoke`, `macos-llt-smoke`, `macos-queue-dma-smoke`, and `macos-mac-smoke` are macOS IOUSBHost fallback diagnostics. They are useful on macOS 26 when `usb-probe` cannot see the radio through libusb because IOKit shows the `0x0bda:0x8812` device as `!registered, !matched`, with no interface children until direct configuration. On April 30, 2026, the remote macOS 26 machine at `rownd@rownds-macbook-pro` could open the IOUSBHost device, read descriptors, configure interface matching, copy bulk pipes, complete a zero-length bulk OUT request, issue bounded bulk IN requests, retain one interface session across control and bulk operations, run full `init --macos-usbhost`, run `rx-scan --macos-usbhost`, submit single and repeated descriptor-prefixed TX frames with `tx-once --macos-usbhost` and `tx-repeat --macos-usbhost`, submit a parsed WFB distributor-style datagram with `bridge-tx-once --macos-usbhost`, and submit a UDP-received WFB distributor-style datagram with `bridge-tx-listen --macos-usbhost`. Reports include `/tmp/wfb-remote-macos-init-usbhost.json`, `/tmp/wfb-remote-macos-rx-scan-usbhost.json`, `/tmp/wfb-remote-macos-tx-once-usbhost.json`, `/tmp/wfb-remote-macos-tx-repeat-usbhost.json`, `/tmp/wfb-remote-macos-bridge-tx-once-usbhost.json`, and `/tmp/wfb-remote-macos-bridge-tx-listen-usbhost.json`. This proves the retained macOS USB transport; RF confirmation still needs an independent receiver. See `docs/macos-usbhost.md`.

`led-smoke` is live and write-gated: it claims the adapter, drives selected RTL8812AU LEDCFG software-control bits, verifies register readback, and releases the interface. It supports `--pin led0|led1|led2`, `--mode normal|antdiv|minicard`, and `--action on|off|blink`. The April 30, 2026 macOS 15.7.4 runs passed for normal LED0/1/2 plus antdiv/minicard alternatives on `0x0bda:0x8812`; the visible blue enclosure LED was operator-confirmed as normal `led0` on `REG_LEDCFG0`. See `docs/led-smoke.md`.

`efuse-dump` is live and write-gated: it claims the adapter, reads physical EFUSE bytes through `REG_EFUSE_CTRL`, decodes the logical 512-byte EFUSE map, and summarizes USB identity, MAC, board/RFE bytes, and the TX-power region. It requires `--i-understand-this-writes-control-registers` because EFUSE reads write selector/control registers, but it does not program EFUSE, tune a channel, issue bulk traffic, or transmit frames. The April 30, 2026 run on `0x0bda:0x8812` decoded 49 EFUSE packets, found the terminator at physical byte 378, reported EFUSE USB ID `0x0bda:0x8812`, MAC `00:c0:ca:ba:bd:9f`, RFE option `0x03`, and 66 non-`0xff` bytes in the 84-byte TX-power region. See `docs/efuse-dump.md`.

`power-on-smoke` is the first guarded write diagnostic: it claims the adapter, runs the RTL8812AU card-emulation-to-active power flow, enables the command-register DMA/protocol/scheduler blocks, performs RF A/B reset writes, and records before/after readback for every write. It requires `--i-understand-this-writes-registers` and still does not download firmware, tune a channel, start RX, write bulk OUT, or transmit frames.

`firmware-smoke` is the guarded firmware diagnostic: after `power-on-smoke`, it claims the adapter, skips the 32-byte Realtek firmware header when present, writes the RTL8812A firmware payload through vendor control transfers, polls checksum/readiness bits, and records final `REG_MCUFWDL`. It requires `--i-understand-this-writes-registers` and still does not tune a channel, start RX, write bulk OUT, or transmit frames.

`llt-smoke` is the guarded linked-list-table diagnostic: after `power-on-smoke`, it writes all 256 RTL8812A LLT entries through `REG_LLT_INIT`, polls every operation idle, and records the TX page boundary and poll counters. It requires `--i-understand-this-writes-registers` and still does not program queue/DMA registers, tune a channel, start RX, write bulk OUT, or transmit frames.

`queue-dma-smoke` is the guarded queue/DMA diagnostic: after firmware and LLT are ready, it derives the USB endpoint queue layout, writes `REG_RQPN_NPQ`, `REG_RQPN`, TX buffer boundaries, `REG_TRXDMA_CTRL`, RX DMA boundary, and `REG_PBP`, then verifies readback. It requires `--i-understand-this-writes-registers` and still does not enable MAC receive, program BB/RF tables, tune a channel, start RX, write bulk OUT, or transmit frames.

`mac-smoke` is the guarded MAC/WMAC diagnostic: after queue/DMA setup, it writes driver-info size, network type, receive filter, multicast mask, response rate, retry limit, EDCA timing, HW sequence, BAR mode, and MAC TX/RX enable registers. It requires `--i-understand-this-writes-registers` and still does not program BB/RF tables, tune a channel, start RX, write bulk OUT, or transmit frames.

`bb-smoke` is the guarded BB diagnostic: after MAC/WMAC setup, it parses `array_mp_8812a_phy_reg` and `array_mp_8812a_agc_tab` from an external Realtek `halhwimg8812a_bb.c` source file, evaluates the driver's condition markers with visible CLI parameters, powers the BB/RF gates, writes the selected PHY/AGC table entries, and applies the RTL8812A crystal-cap update. It requires `--i-understand-this-writes-registers` and still does not program RF radio tables, tune a channel, start RX, write bulk OUT, or transmit frames.

`rf-smoke` is the guarded RF diagnostic: after BB setup, it parses `array_mp_8812a_radioa` and `array_mp_8812a_radiob` from an external Realtek `halhwimg8812a_rf.c` source file, evaluates condition markers, encodes each RF offset/data pair, and writes path A through `0x0c90` and path B through `0x0e90`. It requires `--i-understand-this-writes-registers` and still does not tune a channel, start RX, write bulk OUT, or transmit frames.

The macOS IOUSBHost fallback now has matching `macos-descriptor-smoke`, `macos-interface-smoke`, `macos-bulk-in-smoke`, `macos-bulk-out-smoke`, `macos-session-smoke`, `macos-bb-smoke`, and `macos-rf-smoke` stages for macOS 26 cases where libusb cannot enumerate the adapter. The descriptor stage is read-only and reports the AWUS036ACH layout as bulk IN `0x81`, bulk OUT `0x02/0x03/0x04`, and interrupt IN `0x85`; the interface, bulk, and session stages prove retained IOUSBHost pipe access; BB/RF stages run the same guarded table programming through default-control transfers; `--macos-usbhost` on `init`, `rx-scan`, `tx-once`, and `tx-repeat` uses the retained IOUSBHost transport directly.

`init` is the integrated live bring-up diagnostic: it claims the adapter once, parses BB/RF table sources, runs power-on, firmware, LLT, queue/DMA, MAC, BB, RF, and selected channel setup, and emits phase-level counters plus `effective_channel`/`effective_bandwidth`. It requires `--firmware` and `--i-understand-this-writes-registers`. The live April 30, 2026 runs passed with `0x0bda:0x8812` on macOS 15.7.4 through libusb and on macOS 26 through `--macos-usbhost`. It still does not start RX, write bulk OUT, or transmit frames.

`extract-firmware` is hardware-free: it parses a Realtek driver C firmware array such as `array_mp_8812a_fw_nic` from `hal8812a_fw.c` and writes a standalone image for the existing firmware loader. The output preserves the Realtek firmware header when present, so `firmware-smoke`, `init`, `bridge-run`, and `runtime-flow` can continue using `--firmware /tmp/rtl8812aefw.bin` without storing firmware blobs in this repo.

`init --dry-run` is hardware-free: it loads the supplied firmware, skips the 32-byte Realtek firmware header when present, chunks the download payload using the planned firmware download size, and emits a source-audited skeleton of normalized control-transfer events. The current sequence is derived from `aircrack-ng/rtl8812au` commit `734485506a30d6237c2deaad666a19f8ca5379f2` and covers power-on, RF reset, LLT programming, firmware block writes, checksum/readiness polls, queue/DMA, WMAC, BB/RF, and initial channel phases. It is useful for report plumbing, trace comparison tooling, and future Linux-capture regression tests, and it does not issue USB transfers.

`tx-once --dry-run` is the exception: it builds the RTL8812AU descriptor-prefixed packet and can write it with `--packet-out`, but still does not touch USB.

`rx-scan --fixture-bulk-in` is also hardware-free: it parses raw RTL8812AU bulk-IN fixture bytes, reports parser counters, and can write parsed 802.11 frames to PCAP.

## Development Commands

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Equivalent `make` and `just` targets are available: `fmt`, `clippy`, `test`, `check`, and `verify`.
