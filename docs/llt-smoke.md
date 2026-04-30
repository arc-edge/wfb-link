# LLT Smoke Test

`wfb-radio-diag llt-smoke` is the first live diagnostic for RTL8812A linked-list table programming.

It claims the supported RTL8812AU adapter, verifies that the command register block-enable mask is set, writes the 256-entry LLT page chain through `REG_LLT_INIT`, polls each write until the LLT operation reports idle, and releases the interface when the process exits.

It does not download firmware, program queue/DMA registers, configure MAC/BB/RF tables, tune a channel, start RX, write USB bulk OUT, or transmit frames.

## Command

```sh
cargo run -p wfb-radio-diag -- --json llt-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers
```

Run `power-on-smoke` first after plugging in or resetting the adapter. Firmware is not required for the LLT sequence in the audited Linux init path, but running it after `firmware-smoke` also passed in the current bench setup.

## Guardrails

- The command fails unless `--i-understand-this-writes-registers` is present.
- The command fails before LLT writes if `REG_CR` does not show the expected power-on block-enable bits.
- Each LLT entry write is followed by a bounded poll of `REG_LLT_INIT`.
- The report includes the TX page boundary, last TX page entry, entries written, maximum poll attempts observed, and USB counters.

## Live Result

On macOS 15.7.4 with `0x0bda:0x8812` at bus `1`, address `1`, `llt-smoke` passed after the previous power-on and firmware smoke stages:

- TX page boundary: `0xf9`
- last TX page entry: `0xff`
- LLT entries written: `256`
- max poll attempts observed: `1`
- control reads: `257`
- control writes: `256`
- bulk reads/writes: `0`
- TX frames: `0`

Post-LLT `reg-smoke` also passed, with `REG_MCUFWDL = 0xc6` and `REG_CR = 0x063f`.

## Source Mapping

The sequence is derived from `aircrack-ng/rtl8812au` commit `734485506a30d6237c2deaad666a19f8ca5379f2`:

- `hal/rtl8812a/rtl8812a_hal_init.c`: `_LLTWrite_8812A` and `InitLLTTable8812A`
- `include/hal_com_reg.h`: `REG_LLT_INIT`, `_LLT_INIT_ADDR`, `_LLT_INIT_DATA`, `_LLT_OP`, `_LLT_OP_VALUE`, and `POLLING_LLT_THRESHOLD`
- `hal/rtl8812a/usb/usb_halinit.c`: `TX_PAGE_BOUNDARY_8812` selection in the USB init path
