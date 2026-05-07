# TX Once

`tx-once` is the live TX diagnostic. It assumes `init` has already completed on the requested channel, then claims the adapter, validates one operator-supplied IEEE 802.11 frame, builds the RTL8812AU 40-byte TX descriptor, and writes exactly one descriptor-prefixed packet to the bulk-OUT endpoint.

## Command

```sh
FRAME_HEX=$(tr -d '[:space:]' < fixtures/frames/wfb-data-frame.hex)

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-once.json tx-once \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --frame-hex "$FRAME_HEX"
```

On macOS 26, add `--macos-usbhost --vid 0x0bda --pid 0x8812` to use the retained IOUSBHost interface session instead of libusb.

Optional TX descriptor flags are explicit:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-once-flags.json tx-once \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --frame-hex "$FRAME_HEX" \
  --short-gi --ldpc --stbc
```

Explicit TX descriptor rates can be selected for direct HT/VHT diagnostics:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-once-vht-rate.json tx-once \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 80 \
  --frame-hex "$FRAME_HEX" \
  --tx-rate vht2ss-mcs9 \
  --short-gi --ldpc --stbc \
  --tx-led --tx-led-hold-ms 700 \
  --tx-status --tx-status-delay-ms 50
```

Optional software TX activity LED indication uses the confirmed visible LED by default:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-once-led.json tx-once \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --frame-hex "$FRAME_HEX" \
  --tx-led --tx-led-hold-ms 700
```

Optional read-only TX status sampling records selected chip registers before and after the USB submission:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-once-status.json tx-once \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --frame-hex "$FRAME_HEX" \
  --tx-led --tx-led-hold-ms 700 \
  --tx-status --tx-status-delay-ms 50
```

Run live `init` first:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-init-before-tx.json init \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --firmware /tmp/rtl8812aefw.bin \
  --i-understand-this-writes-registers
```

## Live Result

Live run on April 30, 2026 with `0x0bda:0x8812` on macOS 15.7.4 passed as a single USB bulk-OUT submission test:

- Channel: 36, 5180 MHz, 20 MHz bandwidth.
- Bulk OUT endpoint: `0x02`.
- Input frame: 24-byte IEEE 802.11 data frame from `fixtures/frames/wfb-data-frame.hex`.
- Descriptor-prefixed packet length: 64 bytes.
- USB write result: 64 bytes written.
- TX options: OFDM 6 Mbps, 20 MHz, 12 retries, no SGI, no LDPC, no STBC.
- TX counters: 1 attempted, 1 submitted, 0 rejected, 0 failed, 0 short writes.
- Report: `/tmp/wfb-live-tx-once.json`.

A second live run with `--short-gi --ldpc --stbc` also passed and reported all three flags under `tx_options`. Report: `/tmp/wfb-live-tx-once-flags.json`.

A later live run with `--tx-led --tx-led-hold-ms 700` also passed. It submitted one 64-byte packet to bulk OUT endpoint `0x02` and toggled the confirmed visible LED path, normal `led0` / `REG_LEDCFG0`, around the software TX submission. The LED report contains two steps, `on` then `off`, with 4 control reads, 2 control writes, and no LED error. Report: `/tmp/wfb-live-tx-once-led.json`.

A later live run with `--tx-led --tx-status --tx-status-delay-ms 50` also passed. It submitted one 64-byte packet, toggled the visible LED path, read 15 status registers before and after TX, and reported one chip-side delta: `REG_TXPKT_EMPTY` changed from `0x0fff` to `0x0ffe`. The status probe used 30 control reads and had no error. Report: `/tmp/wfb-live-tx-once-status.json`.

A later live run after 80 MHz init also passed with `--bandwidth 80 --tx-led --tx-status --tx-status-delay-ms 50`. It submitted one 64-byte packet with `tx_options.bandwidth=mhz80`, toggled the visible LED path, and reported `REG_TXPKT_EMPTY` changing from `0x0fff` to `0x0ffe`. Report: `/tmp/wfb-live-tx-once-80mhz.json`.

A later live run after 80 MHz init with `--tx-rate vht2ss-mcs9 --short-gi --ldpc --stbc --tx-led --tx-status --tx-status-delay-ms 50` also passed. It submitted one 64-byte packet with `tx_options.rate.vht.mcs=9`, `tx_options.rate.vht.nss=2`, `tx_options.bandwidth=mhz80`, and all three optional descriptor flags set. The status probe reported `REG_TXPKT_EMPTY` changing from `0x0fff` to `0x0ffe` and `REG_SCH_TX_CMD` changing from `0x00` to `0xc4`. Report: `/tmp/wfb-live-tx-once-vht-rate.json`.

The remote macOS 26 retained IOUSBHost path also passed:

- `tx-once --macos-usbhost`: one 64-byte descriptor-prefixed packet written to endpoint `0x02`, 1 attempted, 1 submitted, 0 failed, 0 short writes. Report: `/tmp/wfb-remote-macos-tx-once-usbhost.json`.
- `tx-once --macos-usbhost --tx-led --tx-status`: LED on/off readback passed, 15 status registers were read before and after TX, and one 64-byte bulk-OUT packet was submitted. Report: `/tmp/wfb-remote-macos-tx-once-led-status-usbhost.json`.
- `tx-once --macos-usbhost --bandwidth 80 --tx-rate vht2ss-mcs9 --short-gi --ldpc --stbc`: one 64-byte descriptor-prefixed packet written to endpoint `0x02`, with the VHT rate and descriptor flags echoed in JSON. Report: `/tmp/wfb-remote-macos-tx-once-vht-usbhost.json`.

This proves the Mac can claim the initialized adapter and submit one TX packet to the RTL8812AU bulk-OUT endpoint. It does not yet prove RF radiation or peer reception; that needs an independent monitor receiver or Linux WFB peer on the same channel.

## Boundaries

`tx-once` does not run init, retune the channel, start an RX loop, repeat frames, or verify over-the-air reception. Live TX requires `--frame-hex` so the command never invents a frame or transmits by accident. `--tx-rate`, `--short-gi`, `--ldpc`, and `--stbc` only set descriptor options; peer reception still needs independent RF verification. `--tx-led` indicates software bulk-OUT submission activity only; it is not an RF TX confirmation. `--tx-status` is read-only register telemetry around the USB TX submission and is also not RF proof.

Use `--dry-run` to build the descriptor-prefixed packet without touching USB:

```sh
cargo run -p wfb-radio-diag -- --json tx-once \
  --channel 36 \
  --frame-hex "$FRAME_HEX" \
  --dry-run
```
