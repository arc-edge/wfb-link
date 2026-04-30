# MAC Smoke Test

`wfb-radio-diag mac-smoke` is the first live diagnostic for RTL8812A MAC and WMAC register programming.

It claims the supported RTL8812AU adapter, verifies that command-register block-enable and firmware-ready bits are set, writes the source-derived MAC/WMAC register subset, verifies readback, and releases the interface when the process exits.

It does not program BB/RF tables, tune a channel, start RX, write USB bulk OUT, or transmit frames.

## Command

```sh
cargo run -p wfb-radio-diag -- --json mac-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers
```

Run `power-on-smoke`, `firmware-smoke`, `llt-smoke`, and `queue-dma-smoke` first after plugging in or resetting the adapter.

On macOS 26, use the IOUSBHost fallback after the IOUSBHost power-on, firmware, LLT, and queue/DMA stages if `usb-probe` cannot enumerate the adapter through libusb:

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-mac-smoke.json macos-mac-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers
```

## Guardrails

- The command fails unless `--i-understand-this-writes-registers` is present.
- The command fails before MAC writes if `REG_CR` does not show the expected power-on block-enable bits.
- The command fails before MAC writes if `REG_MCUFWDL` does not show firmware-ready state.
- Every write is followed by readback verification; `REG_BAR_MODE_CTRL` masks bit 7 because the hardware reads that bit back clear after accepting the upstream write value.
- The report includes the receive configuration, retry limit, per-register readback, and USB counters.

## Live Result

On macOS 15.7.4 with `0x0bda:0x8812` at bus `1`, address `1`, `mac-smoke` passed after the previous power-on, firmware, LLT, and queue/DMA smoke stages:

- receive configuration: `0x740060ce`
- retry limit: `0x3030`
- `REG_BAR_MODE_CTRL` write value: `0x0201ffff`
- `REG_BAR_MODE_CTRL` readback masked value: `0x0201ff7f`
- final `REG_CR` low byte: `0xff`
- control reads: `50`
- control writes: `24`
- bulk reads/writes: `0`
- TX frames: `0`

Post-MAC `reg-smoke` also passed, with `REG_MCUFWDL = 0xc6` and `REG_CR = 0x06ff`.

On April 30, 2026, the remote macOS 26 hardware Mac passed `macos-mac-smoke` after the IOUSBHost power-on, firmware, LLT, and queue/DMA smoke stages against the attached `0x0bda:0x8812` adapter:

- Report: `/tmp/wfb-remote-macos-mac-smoke.json`.
- Receive configuration: `0x740060ce`.
- Retry limit: `0x3030`.
- Control reads: `50`.
- Control writes: `24`.
- Bulk reads/writes: `0`.

This proves the IOUSBHost fallback can run MAC/WMAC register setup through default-control transfers. It does not prove interface claim, bulk endpoints, RX, TX, or full init on macOS 26.

## Source Mapping

The sequence is derived from `aircrack-ng/rtl8812au` commit `734485506a30d6237c2deaad666a19f8ca5379f2`:

- `hal/rtl8812a/usb/usb_halinit.c`: `_InitDriverInfoSize_8812A`, `_InitNetworkType_8812A`, `_InitWMACSetting_8812A`, `_InitAdaptiveCtrl_8812AUsb`, `_InitEDCA_8812AUsb`, `_InitRetryFunction_8812A`, and the final `MACTXEN | MACRXEN` write in `rtl8812au_hal_init`
- `include/hal_com_reg.h`: `REG_RX_DRVINFO_SZ`, `REG_CR`, `REG_RCR`, `REG_MAR`, `REG_RXFLTMAP1`, `REG_RRSR`, `REG_SPEC_SIFS`, `REG_RETRY_LIMIT`, EDCA/SIFS registers, `REG_FWHW_TXQ_CTRL`, `REG_ACKTO`, `REG_HWSEQ_CTRL`, `REG_BAR_MODE_CTRL`, and the RCR/rate/retry bit definitions
- `include/rtw_recv.h`: `DRVINFO_SZ`
