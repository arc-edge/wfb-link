# Power-On Smoke Test

`wfb-radio-diag power-on-smoke` is the first live diagnostic that writes RTL8812AU registers.

It claims the supported adapter, runs the RTL8812AU card-emulation-to-active power sequence, enables command-register DMA/protocol/scheduler blocks, performs RF path A/B reset writes, reports before/after readback for each write, and releases the interface when the process exits.

It does not download firmware, initialize LLT or queues, tune a channel, start RX, write bulk OUT, or transmit frames.

## Command

```sh
cargo run -p wfb-radio-diag -- --json power-on-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers
```

Use `--bus` and `--address` as well if multiple matching radios are attached.

## Guardrails

- The command fails unless `--i-understand-this-writes-registers` is present.
- Each write is surrounded by readback checks.
- Polling steps abort on timeout.
- Any readback mismatch aborts the remaining sequence.
- The report counters distinguish control reads, control writes, bulk reads, and bulk writes.

## Source Mapping

The sequence is derived from `aircrack-ng/rtl8812au` commit `734485506a30d6237c2deaad666a19f8ca5379f2`:

- `include/Hal8812PwrSeq.h`: `RTL8812_TRANS_CARDEMU_TO_ACT`
- `hal/rtl8812a/usb/usb_halinit.c`: `_InitPowerOn_8812AU` command-register enable and RF reset writes

Linux USB captures are still required before treating this as a complete initialization path.
