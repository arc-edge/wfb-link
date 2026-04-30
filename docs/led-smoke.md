# LED Smoke

`wfb-radio-diag led-smoke` drives RTL8812AU software LED registers through guarded vendor control writes.

The command is intentionally separate from TX/RX. It only claims the USB interface, writes LEDCFG registers, verifies readback, and releases the device. It does not tune a channel, submit bulk OUT, run RX, or transmit RF.

## Example

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-led-smoke-led0.json led-smoke \
  --vid 0x0bda --pid 0x8812 \
  --pin led0 \
  --mode normal \
  --action blink \
  --blink-count 6 \
  --interval-ms 250 \
  --i-understand-this-writes-registers
```

Useful sweeps when the visible enclosure LED is unknown:

```sh
cargo run -p wfb-radio-diag -- --json led-smoke --vid 0x0bda --pid 0x8812 --pin led0 --mode normal --action blink --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json led-smoke --vid 0x0bda --pid 0x8812 --pin led1 --mode normal --action blink --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json led-smoke --vid 0x0bda --pid 0x8812 --pin led2 --mode normal --action blink --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json led-smoke --vid 0x0bda --pid 0x8812 --pin led0 --mode antdiv --action blink --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json led-smoke --vid 0x0bda --pid 0x8812 --pin led0 --mode minicard --action blink --i-understand-this-writes-registers
cargo run -p wfb-radio-diag -- --json led-smoke --vid 0x0bda --pid 0x8812 --pin led1 --mode minicard --action blink --i-understand-this-writes-registers
```

## Modes

- `normal`: upstream RTL8812AU non-minicard, non-antenna-diversity path. `led0`, `led1`, and `led2` map to `REG_LEDCFG0`, `REG_LEDCFG1`, and `REG_LEDCFG2`.
- `antdiv`: upstream antenna-diversity LED0 path. It maps LED0 to `REG_LEDCFG2` high bits.
- `minicard`: upstream minicard/USB-solo/USB-combo path. It maps LED0 and LED1 to `REG_LEDCFG2`.

## Live Result

On April 30, 2026, macOS 15.7.4 with `0x0bda:0x8812` passed LED readback for:

- `/tmp/wfb-live-led-smoke-led0.json`: normal `led0`, `REG_LEDCFG0`, toggled `0x20`/`0x28`.
- `/tmp/wfb-live-led-smoke-led1.json`: normal `led1`, `REG_LEDCFG1`, toggled `0x20`/`0x28`.
- `/tmp/wfb-live-led-smoke-led2.json`: normal `led2`, `REG_LEDCFG2`, toggled `0x20`/`0x28`.
- `/tmp/wfb-live-led-smoke-antdiv-led0.json`: antdiv `led0`, `REG_LEDCFG2`, toggled `0xe0`/`0xe8`.
- `/tmp/wfb-live-led-smoke-minicard-led0.json`: minicard `led0`, `REG_LEDCFG2`, toggled `0xe0`/`0xe8`.
- `/tmp/wfb-live-led-smoke-minicard-led1.json`: minicard `led1`, `REG_LEDCFG2`, toggled `0x28`/`0x08`.

The visible blue enclosure LED was then operator-confirmed with `/tmp/wfb-live-led-confirm-normal-led0.json`: normal `led0`, `REG_LEDCFG0`, 6 pulses at 1000 ms. `REG_LEDCFG0` is the mapping used by the TX activity LED hook.

The other results prove userspace LED register control and readback, but they do not map to the visible enclosure LED on the attached unit unless separately observed.

## TX Activity LED

`tx-once` and `tx-repeat` now support an explicit `--tx-led` hook. On the attached unit it uses `--tx-led-pin led0 --tx-led-mode normal` by default, turns the LED on around software bulk-OUT submission activity, holds it for `--tx-led-hold-ms`, then turns it off.

This LED means the host submitted TX work over USB. It does not claim RF success; true RF TX indication still needs either firmware TX-status evidence or an independent receiver.
