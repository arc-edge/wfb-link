# Queue/DMA Smoke Test

`wfb-radio-diag queue-dma-smoke` is the first live diagnostic for RTL8812A queue and DMA register programming.

It claims the supported RTL8812AU adapter, verifies that the command register block-enable mask and firmware-ready bits are set, derives the queue layout from the discovered USB bulk OUT endpoint count, writes reserved-page and DMA boundary registers, verifies readback, and releases the interface when the process exits.

It does not enable MAC receive, program BB/RF tables, tune a channel, start RX, write USB bulk OUT, or transmit frames.

## Command

```sh
cargo run -p wfb-radio-diag -- --json queue-dma-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers
```

Run `power-on-smoke`, `firmware-smoke`, and `llt-smoke` first after plugging in or resetting the adapter.

## Guardrails

- The command fails unless `--i-understand-this-writes-registers` is present.
- The command fails before queue/DMA writes if `REG_CR` does not show the expected power-on block-enable bits.
- The command fails before queue/DMA writes if `REG_MCUFWDL` does not show firmware-ready state.
- The command supports the upstream 2, 3, and 4 bulk-OUT endpoint queue layouts.
- The report includes endpoint count, queue select mask, reserved-page counts, RX/TX boundaries, per-register readback, and USB counters.

## Live Result

On macOS 15.7.4 with `0x0bda:0x8812` at bus `1`, address `1`, `queue-dma-smoke` passed after the previous power-on, firmware, and LLT smoke stages:

- bulk OUT endpoint count: `3`
- queue select mask: `0x07`
- HPQ pages: `0x10`
- LPQ pages: `0x10`
- NPQ pages: `0x00`
- public queue pages: `0xd8`
- `REG_RQPN` write value: `0x80d81010`
- `REG_RQPN` readback page bits: `0x00d81010` (`LD_RQPN` self-clears)
- TX total page number: `0xf8`
- TX page boundary: `0xf9`
- RX DMA boundary: `0x3e7f`
- control reads: `22`
- control writes: `10`
- bulk reads/writes: `0`
- TX frames: `0`

Post-queue `reg-smoke` also passed, with `REG_MCUFWDL = 0xc6` and `REG_CR = 0x063f`.

## Source Mapping

The sequence is derived from `aircrack-ng/rtl8812au` commit `734485506a30d6237c2deaad666a19f8ca5379f2`:

- `hal/rtl8812a/usb/usb_halinit.c`: `_ConfigChipOutEP_8812`, `_InitQueueReservedPage_8812AUsb`, `_InitTxBufferBoundary_8812AUsb`, `_InitQueuePriority_8812AUsb`, `_InitPageBoundary_8812AUsb`, and `_InitTransferPageSize_8812AUsb`
- `include/rtl8812a_hal.h`: `NORMAL_PAGE_NUM_HPQ_8812`, `NORMAL_PAGE_NUM_LPQ_8812`, `NORMAL_PAGE_NUM_NPQ_8812`, `TX_TOTAL_PAGE_NUMBER_8812`, `TX_PAGE_BOUNDARY_8812`, and `RX_DMA_BOUNDARY_8812`
- `include/hal_com_reg.h`: `REG_RQPN_NPQ`, `REG_RQPN`, `REG_TRXFF_BNDY`, `REG_TDECTRL`, `REG_TRXDMA_CTRL`, `REG_PBP`, and packet-buffer page-size macros
