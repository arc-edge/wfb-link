# Live Init

`init` is the integrated guarded bring-up command for the RTL8812AU path. It claims the adapter once, parses the external Realtek BB/RF tables, then runs the already smoke-tested power-on, firmware, LLT, queue/DMA, MAC, BB, RF, and selected channel setup phases in order.

## Command

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-init.json init \
  --vid 0x0bda --pid 0x8812 \
  --channel 36 --bandwidth 20 \
  --firmware /tmp/rtl8812aefw.bin \
  --i-understand-this-writes-registers
```

The live channel phase currently supports 20, 40, and selected 5 GHz 80 MHz channel groups. It programs the vendor path for band switch, basic rate, fc-area, RF channel byte, WMAC bandwidth bits, secondary-channel mapping, BB bandwidth fields, spur handling, and RF bandwidth bits.

On macOS 26, add `--macos-usbhost --vid 0x0bda --pid 0x8812` when libusb cannot enumerate the adapter. This uses a retained IOUSBHost interface session for the same init sequence.

## Live Result

Live run on April 30, 2026 with `0x0bda:0x8812` on macOS 15.7.4 passed:

- Adapter: bus `1`, address `1`, high-speed USB, bulk IN `0x81`, bulk OUT `0x02/0x03/0x04`.
- Firmware: `/tmp/rtl8812aefw.bin`, 27,516 bytes total, 27,484 byte Realtek download payload.
- Power-on/RF reset: 14 steps.
- Firmware: 27,484 payload bytes downloaded; readiness polling passed.
- LLT: 256 entries written.
- Queue/DMA: 3 bulk OUT endpoint layout, `RQPN=0x80d81010`.
- BB: 215 PHY writes, 132 AGC writes.
- RF: 206 radioA writes, 193 radioB writes, 4 delay markers.
- Channel: requested and effective channel 36, 5180 MHz, 20 MHz bandwidth.
- Aggregate USB counters: 494 control reads, 1,395 control writes, 0 bulk IN reads, 0 bulk OUT writes, 0 TX frames.
- Post-run `reg-smoke` passed with `REG_SYS_FUNC_EN=0x1f`, `REG_MCUFWDL=0xc6`, and `REG_CR=0x06ff`.

Reports from the run:

- `/tmp/wfb-pre-init-usb-probe.json`
- `/tmp/wfb-live-init.json`
- `/tmp/wfb-post-init-reg-smoke.json`
- `/tmp/wfb-live-init-channel.json`
- `/tmp/wfb-post-channel-reg-smoke.json`

A later April 30, 2026 run on channel 36 at 40 MHz also passed:

- Effective channel: 36, 5180 MHz, 40 MHz bandwidth.
- Channel phase: 20 steps, including 40 MHz WMAC bits, primary-subchannel mapping, BB RF mode, CCA-on-secondary, CCK sideband, and RF bandwidth bits.
- Aggregate USB counters: 467 control reads, 1,382 control writes, 0 bulk IN reads, 0 bulk OUT writes, 0 TX frames.
- Report: `/tmp/wfb-live-init-channel40.json`.

A later April 30, 2026 run on channel 36 at 80 MHz also passed:

- Effective channel: 36, 5180 MHz, 80 MHz bandwidth.
- Channel phase: 19 steps, including 80 MHz WMAC bit, 80 MHz primary 40/20 subchannel mapping, BB RF mode, CCA-on-secondary, ADC buffer clock, L1 peak threshold, and RF bandwidth bits.
- Aggregate USB counters: 466 control reads, 1,381 control writes, 0 bulk IN reads, 0 bulk OUT writes, 0 TX frames.
- Report: `/tmp/wfb-live-init-channel80.json`.

The remote macOS 26 IOUSBHost retained-session path also passed on April 30, 2026:

- Command: `init --macos-usbhost --vid 0x0bda --pid 0x8812 --channel 36 --bandwidth 20`.
- Result: all power-on, firmware, LLT, queue/DMA, MAC, BB, RF, and channel phases completed.
- Aggregate USB counters: 491 control reads, 1,396 control writes, 0 bulk IN reads, 0 bulk OUT writes.
- Report: `/tmp/wfb-remote-macos-init-usbhost.json`.

The same remote macOS 26 IOUSBHost path also passed channel 36 at 80 MHz:

- Command: `init --macos-usbhost --vid 0x0bda --pid 0x8812 --channel 36 --bandwidth 80`.
- Channel phase: 19 steps.
- Aggregate USB counters: 462 control reads, 1,382 control writes, 0 bulk IN reads, 0 bulk OUT writes.
- Report: `/tmp/wfb-remote-macos-init-80mhz-usbhost.json`.

## Boundaries

`init` still does not start the RX bulk-IN loop, submit bulk-OUT TX frames, run IQK, or apply EFUSE-derived TX power tables. It proves that the Mac can claim the AWUS036ACH, drive the core RTL8812AU control-transfer bring-up sequence from userspace, and program conservative 20/40/80 MHz channel settings.
