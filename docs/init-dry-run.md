# Init Dry-Run Planner

`wfb-radio-diag init --dry-run` is a hardware-free scaffold for the future RTL8812AU initialization path.

It does not claim USB, issue vendor requests, download firmware to an adapter, or configure RF state. It loads a local RTL8812A firmware image, skips the 32-byte Realtek firmware header when present, chunks the download payload using the current planned firmware download chunk size, and builds a normalized transfer skeleton for report and trace tooling.

The current skeleton is derived from a source audit of `aircrack-ng/rtl8812au` commit `734485506a30d6237c2deaad666a19f8ca5379f2`. It captures register addresses, phase ordering, transfer direction, transfer widths, and firmware block sizing. It does not yet encode payload bytes or timing, and Linux USB captures remain the source of truth before any live macOS init attempt.

## Command

```sh
cargo run -p wfb-radio-diag -- --json init \
  --dry-run \
  --firmware /path/to/rtl8812a.bin \
  --trace-out /tmp/planned-init.json
```

The command returns `pass` when the local firmware image can be loaded, the dry-run plan can be built, and any requested trace file can be written.

## Output

The JSON report includes:

- `firmware`: raw firmware length, byte sum, chunk size, and chunk count.
- `init_dry_run`: reference source metadata, planned transfer count, per-phase transfer counts, and the planned transfer list.
- `phases`: completed dry-run phases for firmware planning, power-on planning, firmware planning, and MAC/BB/RF planning.

When `--trace-out` is supplied, the output file contains only the normalized `UsbTraceEvent` array. That makes it directly usable with:

```sh
cargo run -p wfb-radio-diag -- --json trace-compare \
  --expected /tmp/planned-init.json \
  --observed /tmp/planned-init.json
```

## Planned Sequence

The sequence currently includes:

- preflight reads of clock, command, and firmware-download state
- RF path reset and RTL8812 card-emulation to active power sequencing
- MAC reset checks and command-register block enable writes
- all 256 LLT entry writes with completion polls
- firmware download setup, Realtek header skip, page selection, 196-byte register-block writes, 8-byte remainder writes, 1-byte tail writes, checksum polls, and firmware-ready polls
- MAC table, packet-buffer, queue, DMA, RX aggregation, WMAC, retry, AMPDU, BB/RF, and initial channel setup skeleton transfers

## Current Limitations

The transfer sequence is source-derived but not capture-proven. The normalized events record request type, request, value/register address, index, direction, and length, but they do not yet carry register payload bytes, poll masks, delays, firmware bytes, or conditional branches. The next real implementation step is to align this plan against a known-good Linux RTL8812AU USB capture and close any register, payload, and timing gaps before issuing transfers on macOS.
