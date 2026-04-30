# TX Repeat

`tx-repeat` is the guarded live repeated-TX diagnostic. It assumes `init` has already completed on the requested channel, then claims the adapter and submits the same validated IEEE 802.11 frame a bounded number of times with explicit pacing.

## Command

```sh
FRAME_HEX=$(tr -d '[:space:]' < fixtures/frames/wfb-data-frame.hex)

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-repeat.json tx-repeat \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --count 3 --interval-ms 100 \
  --frame-hex "$FRAME_HEX" \
  --i-understand-this-transmits
```

Optional descriptor flags can be added explicitly:

```sh
  --short-gi --ldpc --stbc
```

Explicit TX descriptor rates can be selected for direct HT/VHT diagnostics:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-repeat-vht-rate.json tx-repeat \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 80 \
  --count 3 --interval-ms 200 \
  --frame-hex "$FRAME_HEX" \
  --tx-rate vht2ss-mcs9 \
  --short-gi --ldpc --stbc \
  --tx-led --tx-led-hold-ms 700 \
  --tx-status --tx-status-delay-ms 50 \
  --i-understand-this-transmits
```

Optional software TX activity LED indication can be added explicitly:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-repeat-led.json tx-repeat \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --count 3 --interval-ms 200 \
  --frame-hex "$FRAME_HEX" \
  --tx-led --tx-led-hold-ms 700 \
  --i-understand-this-transmits
```

Optional read-only TX status sampling can be added explicitly:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-tx-repeat-status.json tx-repeat \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --count 3 --interval-ms 200 \
  --frame-hex "$FRAME_HEX" \
  --tx-led --tx-led-hold-ms 700 \
  --tx-status --tx-status-delay-ms 50 \
  --i-understand-this-transmits
```

## Live Result

Live run on April 30, 2026 with `0x0bda:0x8812` on macOS 15.7.4 passed as a bounded repeated USB bulk-OUT submission test:

- Channel: 36, 5180 MHz, 20 MHz bandwidth.
- Bulk OUT endpoint: `0x02`.
- Input frame: 24-byte IEEE 802.11 data frame from `fixtures/frames/wfb-data-frame.hex`.
- Descriptor-prefixed packet length: 64 bytes.
- Count and interval: 3 frames, 100 ms spacing.
- Elapsed time: 208 ms.
- TX counters: 3 attempted, 3 submitted, 0 rejected, 0 failed, 0 short writes, 192 bytes written.
- Report: `/tmp/wfb-live-tx-repeat.json`.

The repeat report also includes derived USB-side rates when elapsed time is non-zero:

- `submitted_per_second`
- `usb_bytes_per_second`
- `cpu.user_us`, `cpu.system_us`, `cpu.total_us`, and `cpu.percent_one_core` on Unix hosts

A later 20 MHz burst on April 30, 2026 used `--count 50 --interval-ms 1` after reinitializing channel 36 at 20 MHz. It submitted 50 of 50 packets with no failed or short writes, wrote 3,200 USB bytes in 65 ms, reported about 769 submitted frames/s, and used about 2.48 ms of process CPU time, or 3.8% of one core during the short run. Report: `/tmp/wfb-live-tx-repeat-20mhz-burst-cpu.json`.

A later live run with `--tx-led --tx-led-hold-ms 700` also passed. It submitted 3 of 3 packets to endpoint `0x02`, wrote 192 USB bytes, and held the confirmed visible LED path, normal `led0` / `REG_LEDCFG0`, on across the software TX burst before turning it off. The LED report contains 4 control reads, 2 control writes, and no LED error. Report: `/tmp/wfb-live-tx-repeat-led.json`.

A later live run with `--tx-led --tx-status --tx-status-delay-ms 50` also passed. It submitted 3 of 3 packets to endpoint `0x02`, wrote 192 USB bytes, held the visible LED path across the burst, read 15 status registers before and after the burst, and reported no changed status registers in that window. The status probe used 30 control reads and had no error. Report: `/tmp/wfb-live-tx-repeat-status.json`.

A later live run after 80 MHz init also passed with `--bandwidth 80 --tx-led --tx-status --tx-status-delay-ms 50`. It submitted 3 of 3 packets to endpoint `0x02`, wrote 192 USB bytes, held the visible LED path across the burst, and reported no changed status registers in that post-burst window. Report: `/tmp/wfb-live-tx-repeat-80mhz.json`.

A later live run after 80 MHz init with `--tx-rate vht2ss-mcs9 --short-gi --ldpc --stbc --tx-led --tx-status --tx-status-delay-ms 50` also passed. It submitted 3 of 3 packets to endpoint `0x02`, wrote 192 USB bytes, reported `tx_options.rate.vht.mcs=9`, `tx_options.rate.vht.nss=2`, `tx_options.bandwidth=mhz80`, and all three optional descriptor flags. The status probe reported no changed registers in that post-burst window. Report: `/tmp/wfb-live-tx-repeat-vht-rate.json`.

This proves a bounded, paced bulk-OUT loop against the initialized adapter. It is not a packet-loss or RF throughput measurement until a second monitor receiver or Linux WFB peer records what arrived over the air.

## Boundaries

`tx-repeat` does not run init, retune the channel, start an RX loop, or verify over-the-air reception. Live repeated TX requires all of:

- `--frame-hex`
- `--count`
- `--interval-ms`
- `--channel`
- `--i-understand-this-transmits`

`--tx-rate`, `--short-gi`, `--ldpc`, and `--stbc` are visible opt-ins that set TX descriptor fields and are echoed in the JSON `tx_options` field. Supported `--tx-rate` forms include legacy rates such as `ofdm6m`, HT rates such as `mcs7`, and VHT rates such as `vht2ss-mcs9`.

`--tx-led` is also a visible opt-in. It reports `tx_activity_led` in JSON and indicates software bulk-OUT submission activity only, not RF transmission success.

`--tx-status` reports `tx_status.pre`, `tx_status.post`, and `tx_status.changed` in JSON. It is read-only RTL8812AU register telemetry around the USB TX burst, not RF transmission success.
