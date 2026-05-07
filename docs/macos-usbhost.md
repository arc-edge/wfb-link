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
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-tx-repeat-usbhost.json tx-repeat \
  --macos-usbhost \
  --vid 0x0bda \
  --pid 0x8812 \
  --channel 36 \
  --count 3 \
  --interval-ms 100 \
  --frame-hex "$(cat fixtures/frames/wfb-data-frame.hex)"
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
- Bridge TX now applies the working Linux monitor-injection shape by default for HT/VHT WFB distributor datagrams. HT MCS traffic uses management queue, MACID 1, rate-ID 7, fallback enabled, fallback limit 0, and no aggregate break unless the operator explicitly overrides those descriptor fields. The CLI reports this as `tx_profile=linux_monitor`; pass `--tx-profile radiotap-direct` to preserve the radiotap-derived descriptor shape for controlled experiments. Live profile smoke `/tmp/wfb-agent-profile20f-listen.json` injected 90/90 datagrams and Linux `wfb_rx` recovered 29/30 `PROF20F` payloads after routing Linux distributor UDP to the hardware Mac LAN address with a local UDP relay.
- `bridge-tx-listen --macos-usbhost --init-before-tx` with stock WFB-ng distributor input: `/tmp/wfb-agent-stock-controlled-listen.json`, result `pass`, received 300 datagrams from Linux `wfb_tx -d`, submitted 300/300 packets with no drops or short writes, and dedicated Linux `wfb_rx` recovered 99 decrypted `STOCKCTRL` payloads from the Mac RF path. RF and receiver captures: `/tmp/mac-stock-controlled-rf.pcap`, `/tmp/mac-stock-controlled-rx-lo.pcap`.
- `bridge-tx-listen --macos-usbhost --init-before-tx` now reports runtime throughput and CPU metrics. A 20 MHz baseline with stock `wfb_tx -d -k 8 -n 12` and 512-ish source payloads received and injected 1,200/1,200 distributor datagrams, wrote 723,735 USB bytes in 14.896 s, reported 80.56 submitted datagrams/s, 48.58 KB/s USB write rate, and 1.43% of one CPU core. The Linux monitor captured 1,169 WFB MAC frames, and Linux `wfb_rx` forwarded 796 decrypted `SIZE512` payloads. Reports/captures: `/tmp/wfb-agent-size512-listen.json`, `/tmp/mac-size512-rf.pcap`, `/tmp/mac-size512-rx-lo.pcap`.
- Payload-size bracketing showed that 768, 900, and exactly 1,000 byte source payloads all recover cleanly with stock `wfb_tx/wfb_rx -l 1000`: each short run injected 600/600 distributor datagrams and recovered 400/400 decrypted payloads. A sustained 1,000-byte run injected 3,000/3,000 datagrams, wrote 3,271,300 USB bytes in 32.525 s, captured 2,906 WFB MAC frames, and recovered 1,999/2,000 decrypted `SUST1000` payloads at 92.24 submitted datagrams/s, 100.58 KB/s USB write rate, and 1.32% CPU. Reports/captures: `/tmp/wfb-agent-size768c-listen.json`, `/tmp/wfb-agent-size900-listen.json`, `/tmp/wfb-agent-size1000-listen.json`, `/tmp/wfb-agent-sust1000-listen.json`, `/tmp/mac-sust1000-rf.pcap`, `/tmp/mac-sust1000-rx-lo.pcap`.
- The earlier 1024-ish source-payload failure submitted cleanly on the Mac but exceeded the `-l 1000` source-payload setting used by the stock WFB tools. It is no longer evidence of a Mac-side large-MPDU TX gate inside the configured MTU.
- `bridge-run --macos-usbhost --init-before-tx` is the first full bridge command. It binds TX input UDP sockets and starts receiver thread(s) before long radio init, requests a 4 MiB UDP receive buffer, keeps one retained IOUSBHost session, runs Linux-order init once, preserves station/MSR state for TX, opens RX filter maps, interleaves queued UDP TX input with bulk-IN RX reads, and forwards matching WFB RX frames to an aggregator socket. It defaults to bounded diagnostics, but `--duration-ms 0` removes the time bound and `--max-datagrams 0` removes the TX datagram cap for longer bridge runs; SIGINT/SIGTERM exits through the normal report path with `stop_reason="signal"` (`/tmp/wfb-agent-bridge-run-signal2.json`). The first bidirectional run forwarded 44 Linux-to-Mac WFB frames to a Mac UDP aggregator while injecting 121/121 Mac-to-Linux distributor datagrams; Linux `wfb_rx` recovered 80/80 `MAC2LIN` payloads and the Linux monitor captured 120 WFB MAC frames. Follow-up `/tmp/wfb-agent-bridgerun-drain2.json` drained and submitted 90/90 distributor datagrams sent during radio init; Linux `wfb_rx` recovered 30/30 `DRAIN2` payloads. Reports/captures: `/tmp/wfb-agent-bridge-run-duplex3.json`, `/tmp/wfb-agent-bridge-run-signal2.json`, `/tmp/wfb-agent-bridgerun-drain2.json`, `/tmp/wfb-agent-bridge-run-duplex3-agg.json`, `/tmp/mac-bridgerun-duplex3-rf.pcap`, `/tmp/mac-bridgerun-duplex3-rx-lo.pcap`.
- Local production `radio-run --macos-usbhost` full-duplex smoke now passes with the adapter attached to this Mac. `/tmp/wfb-radio-run-duplex-local-a3/radio-run.json` used decimal WFB link ID `1` for the Linux WFB-ng CLI and report link `0x000001`: Mac-to-Linux recovered `80/80`, Linux-to-Mac through Mac RX forwarding recovered `80/80`, `radio-run` forwarded `242` WFB frames, and TX submitted `149/149` frames with zero drops or failed submissions. The reusable runner `scripts/run-radio-run-duplex-smoke.sh` was then validated at `/tmp/wfb-radio-run-duplex-script-a1`: both direction counters recovered `80/80`, `radio-run` forwarded `256` frames, and TX again submitted `149/149` with no drops or failed submissions.
- Production `radio-run --macos-usbhost` now exposes the runtime LED heartbeat at top level in JSON and text output. `/tmp/wfb-radio-run-led-heartbeat-20260505-125727.json` ran for 5 s with `heartbeat_led.enabled=true`, `half_period_ms=500`, `toggles_succeeded=10`, and `toggles_failed=0`, and the operator visually confirmed the enclosure LED blinked; a text-output smoke at `/tmp/wfb-radio-run-led-heartbeat-text-20260505-125757.txt` printed `heartbeat-led: enabled, 500 ms, 3 toggles (3 ok, 0 failed)`.
- The production smoke runner also supports a local-adapter mode for cases where the AWUS036ACH is plugged into the active checkout Mac rather than the remote hardware Mac. `LOCAL_HW=1 scripts/run-production-radio-smoke.sh --mode both` builds the current checkout and runs the same RX-only and TX-positive gates without SSH deployment. Local validation at `/tmp/wfb-prod-radio-smoke-local-hw-20260505-095112` passed: RX-only parsed 68 frames with RX outcome telemetry intact, and TX-positive submitted `64/64` synthetic distributor datagrams with zero failed submissions or drops.
- Ready-file generation for production bridge loops is now runtime-owned. The local production smoke rerun at `/tmp/wfb-prod-radio-smoke-local-ready-runtime-20260505-095955` confirmed the runtime marker writer still unblocks the script and emits `source=bridge-run` plus `ready_at_unix_ms` before traffic injection.
- After `radio-run` started emitting detailed `rx.rx_forwards[]` snapshots, local validation at `/tmp/wfb-radio-run-duplex-forward-detail-20260504-135348` confirmed the production report shape against the attached adapter: result `pass`, TX `149/149`, `rx.forwarded_payloads=131`, `rx.rx_forwards[0].counters.forwarded=131`, and zero RX forward send failures. The adapter was about 6 ft from the peer for this run, and both peer counters recovered `76/80`; keep using the earlier 1 ft `80/80` artifacts as the clean close-range baseline until the bench placement is reset.
- The reusable duplex runner now gates on peer payload recovery and the detailed RX-forward snapshots, not just `radio-run.result`. Strict run `/tmp/wfb-radio-run-duplex-strict-20260504-140119` exited with `smoke_result=fail` on the current 6 ft placement (`74/80` Mac-to-Linux and `77/80` Linux-to-Mac), while the production loop stayed healthy: TX `149/149`, zero TX drops/failures, `radio_result=pass`, and `rx.rx_forwards[0].counters.forwarded=123`.
- The runner also supports calibration A/B and stronger FEC overrides. Runtime IQK now avoids final IQC fill unless all TX/RX paths complete; `/tmp/wfb-radio-run-duplex-iqk-allornothing-20260504-141407` recovered `80/80` Mac-to-Linux with `runtime_iqk.status=completed`, but failed Linux-to-Mac at `69/80`. A current-default `FEC_K=8 FEC_N=16` run at `/tmp/wfb-radio-run-duplex-fec16-strict-20260504-141743` also recovered `80/80` Mac-to-Linux but failed Linux-to-Mac at `70/80`, reinforcing that the current placement's remaining loss is on the Mac RX side.
- Runtime IQK live reports now include raw candidate and pre-sweep state evidence. `/tmp/wfb-radio-run-duplex-iqk-evidence-20260504-143429` completed runtime IQK in sweep 2 with cleanup restored and no selected-sweep fallback stages, but still failed the strict receiver gate (`72/80` Mac-to-Linux, `69/80` Linux-to-Mac). The paired Linux driver-reload usbmon trace at `/tmp/wfb-linux-iqk-driver-reload-20260504-143841` captured static IQK final-state writes and RFE pinmux transitions, but not the same dynamic IQC fill sequence, so runtime IQK remains an experimental diagnostic profile rather than a production default.
- The duplex runner now hardens the Linux peer before traffic by verifying monitor/radiotap link state, forcing tcpdump to radiotap, collecting partial peer artifacts on startup failure, and adding `wfb_rx` decrypt-failure counts to `summary.json`. The first hardened `TX_POWER_MODE=efuse-derived` run exposed a state-sensitive Mac-to-Linux corruption artifact (`0/80`, 125 decrypt failures at `/tmp/wfb-radio-run-duplex-default-hardened-20260504-145458`), but a repeat recovered `80/80` with zero decrypt failures (`/tmp/wfb-radio-run-duplex-efuse-full-20260504-150316`). `TX_POWER_MODE=current-default` also recovered Mac-to-Linux cleanly (`80/80`, zero decrypt failures at `/tmp/wfb-radio-run-duplex-no-txpower-20260504-145725`). The runner default is now `current-default`; EFUSE-derived TX power stays explicit and receiver-gated.
- With the adapter back on the local Mac and the receiver reachable as `pi@drone-2f389`, the duplex runner now preflights the Linux peer before claiming the Mac radio and supports `SESSION_ACQUIRE_SETTLE_SECONDS` after observed WFB sessions. The default fast `8/12 MCS1` profile failed L2M at `/tmp/wfb-radio-run-duplex-local-reachable-20260505-100617`, and M2L `5/12` plus L2M `3/12` missed `5/200` M2L payloads at `/tmp/wfb-radio-run-duplex-local-prod-candidate-20260505-100754`. M2L `4/12 MCS1` plus L2M `3/12 MCS2` passed the 200-payload settled gate at `/tmp/wfb-radio-run-duplex-local-m2l4-l2m3-settle-20260505-101147`, but missed `4/1000` L2M payloads in the 1000-payload rerun. Lowering only L2M to MCS1 then missed `2/1000` M2L payloads. The accepted sustained poor-SNR tuple is symmetric `3/12 MCS1`, 20 ms pacing, 100 warmup payloads, and 1 s settle: `/tmp/wfb-radio-run-duplex-local-sym3-mcs1-settle-1000-20260505-101900` recovered `1000/1000` both directions, zero decrypt failures, TX `4417/4417`, zero TX drops/failures, and average Mac RX SNR around 13 dB.
- A later post-move short-range placement on May 5, 2026 was not accepted as a clean production baseline. The previously accepted short-range M2L `5/12` MCS1 plus L2M `3/12` MCS2 tuple failed at `/tmp/wfb-radio-run-postmove-close-m2l5-l2m3-1000-20260505-115903` (`987/1000` M2L, `995/1000` L2M), and the earlier robust symmetric `3/12` MCS1 tuple failed at `/tmp/wfb-radio-run-postmove-close-sym3-mcs1-1000-20260505-120110` (`996/1000` M2L, `997/1000` L2M). The conservative symmetric MCS0 `1/12` tuple with a 100 ms source interval and 50 ms L2M phase offset got closest at `/tmp/wfb-radio-run-postmove-close-sym-mcs0-k1n12-1000-slow100-phase50-20260505-120318` (`1000/1000` M2L, `998/1000` L2M), but isolated L2M still missed at `/tmp/wfb-radio-run-postmove-close-l2m-only-mcs0-k1n12-1000-slow100-20260505-121136` (`999/1000`) and `/tmp/wfb-radio-run-postmove-close-l2m-only-mcs0-k1n16-1000-slow100-20260505-121535` (`991/1000`). Treat this as placement/noise evidence rather than a runtime regression: every run kept `radio_result=pass`, zero TX failures/drops, and zero post-session decrypt failures, while Mac RX reported high malformed/control/filter counts and average SNR around 13-17 dB.
- After moving back to a closer short-range placement, the same symmetric `3/12 MCS1` tuple passed a strict 200-payload smoke at `/tmp/wfb-radio-run-short-close-sym3-mcs1-200-20260505-123010`: M2L `200/200`, L2M `200/200`, source events `200` per direction with max lateness near 3 ms, zero decrypt failures, TX `1189/1189`, zero TX drops/failures, and average Mac RX SNR around 9 dB. Treat this as a current short-range health check, not a sustained or long-distance acceptance artifact.
- Sustained reruns in the same current short-range placement still did not reach strict acceptance. `/tmp/wfb-radio-run-short-close-sym3-mcs1-1000-20260505-123249` recovered M2L `1000/1000` and L2M `998/1000`, with the missing L2M sequences at the tail (`998-999`). The runner now supports unmeasured source-tail padding to flush final partial FEC blocks, but `/tmp/wfb-radio-run-short-close-sym3-mcs1-1000-tail-20260505-123655` still missed mid-run payloads (M2L `999/1000`, L2M `994/1000`). Both runs sent all marked source events on time, had zero decrypt failures and zero TX drops/failures, and therefore classify as current-placement RF/receiver loss rather than source cadence, TX corruption, or USB instability.
- Runtime IQK cleanup now restores the RF, normal-BB, and page-C1 state it mutates. The pre-fix current-TX IQK run corrupted Mac-to-Linux (`0/80`, 123 decrypt failures at `/tmp/wfb-radio-run-duplex-iqk-currenttx-20260504-150531`). After expanding the restore set, `/tmp/wfb-radio-run-duplex-iqk-restorefull-20260504-151048` completed IQK in sweep 2, restored cleanup state, logged zero decrypt failures, and recovered `75/80` Mac-to-Linux plus `77/80` Linux-to-Mac.
- Runtime IQK selected IQC fill now runs after cleanup, and `radio-run` refuses live TX when runtime IQK falls back, cleanup fails, or the selected fill did not apply. The fallback negative `/tmp/wfb-radio-run-duplex-iqk-fill-after-restore-20260504-152042` produced `0/80` Mac-to-Linux with 135 decrypt failures, so fallback is a pre-TX failure state. The completed-fill pass `/tmp/wfb-radio-run-duplex-iqk-fill-passcheck-20260504-152638` completed IQK in sweep 1, applied 20 IQC fill writes, logged zero decrypt failures, and recovered `77/80` Mac-to-Linux plus `80/80` Linux-to-Mac. This is a live IQK correctness fix, not a distance-quality acceptance.
- The same runtime-IQK fill path passed a larger local close-range `radio-run` gate at `/tmp/wfb-rfq-runtime-iqk-fill-2000-20260504-153130/rf-quality-report.json`: 20 post-cleanup fill writes, `1999/2000` recovered payloads, zero decrypt failures, and `within_margin` against the Linux baseline's `1999/2000` recovery. Keep it classified as close-range evidence until controlled distance/attenuation geometry and non-placeholder SNR evidence are available.
- The paired current-default local comparator at `/tmp/wfb-rfq-current-default-2000-20260504-153545/rf-quality-report.json` recovered `2000/2000` payloads with zero decrypt failures and stayed `within_margin` against the same Linux baseline. Current-default remains the production default; runtime IQK is corrected enough for controlled A/B but has not outperformed captured/default calibration on the 6 ft bench.
- The reusable duplex runner now preserves a startup `summary.json` when `radio-run` exits before the ready marker, including runtime IQK cleanup and selected-IQC-fill fields plus the log tail. This keeps calibration fail-closed events actionable in unattended production smokes.
- Live summary validation at `/tmp/wfb-radio-run-duplex-iqk-summary-gate-20260504-154808` passed with runtime IQK status `completed`, cleanup `restored`, selected fill `true`, fill count `20`, zero decrypt failures, and peer recovery `80/80` Mac-to-Linux plus `76/80` Linux-to-Mac.
- Runtime IQK stability is now tracked by the session-gated calibration matrix. Before the RX candidate-cap fix, `/tmp/wfb-radio-calibration-active-sessiongate-duplex-repeat3-20260505-004506` quarantined runtime IQK because it passed 2/3 repeats and one repeat failed pre-TX with `runtime_iqk_not_completed` after path-A RX fallback. After allowing up to five RX IQK candidates while keeping the strict signed tolerance, `/tmp/wfb-radio-calibration-runtime-iqk-rx5-duplex-repeat3-20260505-010136` passed current-default, runtime-IQK, and EFUSE-derived duplex variants 3/3. Runtime IQK completed in sweep 1 for all three repeats, restored cleanup, applied 20 selected-IQC fill writes, logged zero post-session decrypt failures, and recovered worst-case `993/1000` Mac-to-Linux plus `987/1000` Linux-to-Mac. It remains opt-in and receiver-gated pending longer soak and distance evidence.
- HT40 channel-context operation is now verified against the Linux peer. With both radios tuned channel 36/HT40+, `bridge-tx-listen --bandwidth 40` accepted stock `wfb_tx -d -B 40` distributor traffic, injected 300/300 datagrams, and Linux `wfb_rx` recovered 99/100 `STK40STA` payloads. A bounded `bridge-run --bandwidth 40` pass then forwarded 98 Linux-to-Mac WFB frames and injected 300/300 Mac-to-Linux datagrams in the same retained IOUSBHost session; Linux recovered 98/100 `MAC2L40A` payloads. After the bridge default changed, `bridge-tx-listen --bandwidth 40` with no manual TX-shape overrides injected 120/120 and recovered 30 decrypted `DEF40A` source payloads. Linux captures and a Mac-side `rx-scan --frame-jsonl` descriptor run both report the WFB MCS1 frames as `20 MHz`, so this is proof of WFB flow while tuned HT40+, not proof of actual 40 MHz PPDU occupancy. Reports/captures: `/tmp/wfb-agent-stock40stablea-listen.json`, `/tmp/wfb-agent-run40stablea-bridge-run.json`, `/tmp/wfb-agent-default40a-listen.json`, `/tmp/wfb-agent-rxmeta40a.json`, `/tmp/wfb-agent-rxmeta40a.jsonl`, `/tmp/mac-stock40stablea-rf.pcap`, `/tmp/mac-run40stablea-rf.pcap`.
- Multi-port `bridge-run --macos-usbhost` now works in one retained IOUSBHost session. A four-stream run used two TX bind sockets (`0.0.0.0:5612`, `0.0.0.0:5614`) and two RX forward targets (`radio_port=0` to `127.0.0.1:5700`, `radio_port=1` to `127.0.0.1:5701`). With `--tx-burst-limit 8`, the Mac injected 423/423 distributor datagrams, per-bind counters reported 242 datagrams on port 5612 and 181 on port 5614, Linux `wfb_rx` recovered 120/120 `M2LVID` markers and 79/80 `M2LTEL` markers, and the Mac forwarded 120 port-0 frames plus 69 port-1 frames. Reports/captures: `/tmp/wfb-agent-bridge-run-multibridge3.json`, `/tmp/wfb-agent-bridge-run-multibridge3-agg.json`, `/tmp/mac-multibridge3-rf.pcap`, `/tmp/mac-multibridge3-rx-lo.pcap`.
- RX forwarding from the Mac bridge into a real stock WFB-ng aggregator also passes. Linux `wfb_tx` sent 120 source UDP payloads on radio port 0; Mac `bridge-run` received the RF frames and forwarded 138 matching WFB frames as `wrxfwd_t` UDP datagrams to Linux `wfb_rx -a 5800`; the aggregator recovered 120/120 decoded `L2MRXAG` payloads. Report/capture: `/tmp/wfb-agent-bridge-run-rxagg1.json`, `/tmp/mac-rxagg1-agg-lo.pcap`.
- Real encoded-video packet flow has now been verified on the same RX path. Local `ffmpeg` generated a 10 s, 320x180@15 fps H.264 RTP stream at about 271 kb/s and sent it to Linux `wfb_tx -K /var/lib/arc/wfb/drone.key -p 0 -B 20 -k 8 -n 12 -u 5620 wfb0`. Mac `bridge-run --macos-usbhost --init-before-tx` matched and forwarded 446 WFB frames to Linux `wfb_rx -a 5800`; Linux emitted 318 RTP packets toward the hardware Mac, and a hardware-Mac UDP counter received 289 RTP packets / 184,585 bytes with payload type 96. Direct Linux-to-local-Mac UDP did not arrive at `100.112.15.116`, so further playback-facing tests should target the hardware Mac Tailscale address `100.104.12.123` or introduce an explicit UDP relay. Reports/captures: `/tmp/wfb-agent-bridge-run-video4.json`, `/tmp/wfb-video4-udp-recv.json`, `/tmp/mac-video4-agg-to-hwmac.pcap`, `/tmp/wfb-video4-ffmpeg-send.log`.

## Interpretation

The macOS 26 blocker is not raw USB device visibility, descriptor access, default-control access, interface matching, one-shot pipe IO, retaining interface and pipe objects, full RTL8812AU init, bounded RX reads, bulk-OUT TX submission, or low-rate WFB-shaped RF TX. The default control endpoint is reachable through IOUSBHost even when libusb cannot enumerate the radio, standard USB descriptors can be read, guarded register-write sequences can execute through channel setup, interface 0 can be opened after `configureWithValue:matchInterfaces:`, and descriptor-confirmed bulk pipes can serve `rx-scan`, `tx-once`, `tx-repeat`, and UDP-fed bridge TX.

The remaining macOS 26 proof target is no longer basic RF visibility, low-rate WFB interoperability, sustained packet flow at `-l 1000`, bounded full-duplex bridge operation, multi-port bridge plumbing, stock WFB-ng aggregator compatibility, encoded H.264/RTP packet delivery, or WFB flow while tuned to channel 36/HT40+. The remaining work is proving or enabling actual wide-PPDU occupancy for wider RF modes and replacing planted/calibrated TX power behavior with explicit EFUSE-derived controls. BE-queue full-width HT40 data is not the answer yet: clearing global `REG_DATA_SC_8812` avoids the BE queue timeout in a bounded run, but the Linux receiver still decodes zero payloads.

## SDK Notes

The macOS 26.4 Command Line Tools IOUSBHost headers confirm the public object split:

- `IOUSBHostDevice` exposes default-control requests, `configureWithValue:matchInterfaces:error:`, the current `configurationDescriptor`, and reset.
- `IOUSBHostInterface` exposes `copyPipeWithAddress:error:` for endpoint pipes.
- `IOUSBHostPipe` exposes synchronous `sendIORequestWithData:bytesTransferred:completionTimeout:error:` for bulk/interrupt transfers and async enqueue APIs.

Endpoint discovery, one-shot pipe IO, retained control access, full init, bounded RX, descriptor-prefixed TX, low-rate UDP-fed bridge RF TX, stock WFB-ng distributor-to-Linux-receiver payload delivery, sustained bidirectional load, and H.264/RTP packet flow are now proven.
