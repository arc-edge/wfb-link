# Firmware Smoke Test

`wfb-radio-diag firmware-smoke` is the first live diagnostic that downloads RTL8812A firmware on macOS.

It claims the supported RTL8812AU adapter, enables MCU firmware-download mode, skips the 32-byte Realtek firmware header when present, writes firmware payload bytes through vendor control transfers, polls the firmware checksum bit, releases download mode, resets the 8051, polls the firmware-ready bit, and releases the interface when the process exits.

It does not initialize LLT or queues, tune a channel, start RX, write USB bulk OUT, or transmit frames.

## Command

```sh
cargo run -p wfb-radio-diag -- --json firmware-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --firmware /tmp/rtl8812aefw.bin \
  --i-understand-this-writes-registers
```

Run `power-on-smoke` first after plugging in or resetting the adapter.

On macOS 26, use the IOUSBHost fallback after `macos-power-on-smoke` if `usb-probe` cannot enumerate the adapter through libusb:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-firmware-smoke.json macos-firmware-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --firmware /tmp/rtl8812aefw.bin \
  --i-understand-this-writes-registers
```

## Guardrails

- The command fails unless `--i-understand-this-writes-registers` is present.
- Firmware images are loaded from an explicit path and size-checked before USB writes.
- Realtek firmware signatures `0x950*` and `0x210*` are treated as 32-byte header containers; only the payload is written.
- Firmware writes use the upstream USB block sizes: 196-byte register blocks, 8-byte remainder blocks, and 1-byte tail writes.
- The report includes payload offset, payload length, signature, checksum poll attempts, ready poll attempts, final `REG_MCUFWDL`, and USB counters.

## Live Result

On macOS 15.7.4 with `0x0bda:0x8812` at bus `1`, address `1`, the downloaded Linux firmware blob was:

- source: `/tmp/rtl8812aefw.bin`
- SHA-256: `d40396544ee56c9dab43a458344b8936aa3d878c1582e96a62e9346bdfbdf50f`
- raw length: `27516`
- signature: `0x9501`
- payload offset: `32`
- payload length: `27484`

The live pass wrote `27484` firmware payload bytes in `290` control writes, checksum polling passed on the first read, firmware-ready polling passed after `20` reads, and final `REG_MCUFWDL` was `0x000607c6`.

On April 30, 2026, the remote macOS 26 hardware Mac passed `macos-firmware-smoke` after `macos-power-on-smoke` against the attached `0x0bda:0x8812` adapter:

- Report: `/tmp/wfb-remote-macos-firmware-smoke.json`.
- Firmware SHA-256: `d40396544ee56c9dab43a458344b8936aa3d878c1582e96a62e9346bdfbdf50f`.
- Firmware payload written: `27484` bytes.
- Firmware control writes: `290`.
- Checksum poll attempts: `1`.
- Ready poll attempts: `18`.
- Final `REG_MCUFWDL`: `0x000607c6`.
- Bulk reads/writes: `0`.

This proves the IOUSBHost fallback can run firmware download and readiness polling through default-control transfers. It does not prove interface claim, bulk endpoints, RX, TX, or full init on macOS 26.

## Source Mapping

The sequence is derived from `aircrack-ng/rtl8812au` commit `734485506a30d6237c2deaad666a19f8ca5379f2`:

- `hal/rtl8812a/rtl8812a_hal_init.c`: `FirmwareDownload8812`, `_FWDownloadEnable_8812`, `_WriteFW_8812`, `_BlockWrite_8812`, `_PageWrite_8812`, `polling_fwdl_chksum`, `_FWFreeToGo8812`, and `_8051Reset8812`
- `include/rtl8812a_hal.h`: firmware header signature detection
- `include/hal_com_reg.h`: `REG_MCUFWDL`, `MCUFWDL_RDY`, `FWDL_ChkSum_rpt`, `WINTINI_RDY`, and `REG_RSV_CTRL`
