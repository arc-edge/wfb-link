# Register Smoke Test

`wfb-radio-diag reg-smoke` is the first live hardware diagnostic after USB enumeration.

It claims the supported RTL8812AU adapter, performs a small set of vendor control reads, reports the register values, and releases the interface when the process exits. It does not issue USB control writes, bulk transfers, RF changes, firmware downloads, channel changes, or TX operations.

## Command

```sh
cargo run -p wfb-radio-diag -- --json reg-smoke \
  --vid 0x0bda \
  --pid 0x8812
```

Use `--bus` and `--address` as well if multiple matching radios are attached.

## Registers

The command currently reads:

- `REG_SYS_FUNC_EN` at `0x0002`
- `REG_APS_FSMCO` at `0x0004`
- `REG_SYS_CLKR` at `0x0008`
- `REG_RF_CTRL` at `0x001f`
- `REG_MCUFWDL` at `0x0080`
- `REG_CR` at `0x0100`

Full-width reads from these addresses prove that libusb can claim the adapter and that RTL8812AU vendor control reads work on the Mac before we attempt any init writes.
