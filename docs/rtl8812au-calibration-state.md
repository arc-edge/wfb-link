# RTL8812AU Calibration State Reporting

This note records the RF calibration evidence now emitted by the macOS
RTL8812AU bridge diagnostics for range-quality work.

## Report Fields

`bridge-tx-bench`, `bridge-tx-listen`, and `bridge-run` now include
`rf_calibration_pre_tx` immediately before live TX. Same-session init reports
also include `same_session_init.calibration_state` with probes at:

- `before_channel`: after static BB/RF tables, before channel setup.
- `after_channel`: after channel setup and captured runtime TX bring-up tail.
- `before_tx`: after the late TX scheduler tail and before command-level TX
  overrides.

Each probe records readback groups for:

- RFE pinmux, inversion, and timing registers.
- IQK result and shadow registers.
- LCK-related RF 3-wire latch registers.
- TX/RF path state registers.
- Thermal-meter status. Runtime thermal readback is not ported yet, so the
  probe labels it as unavailable; `rf-quality-report` attaches EFUSE thermal
  and IQK/LCK bytes when an EFUSE artifact is supplied.

## Stop-Gap Labeling

The current 5 GHz 20/40 MHz path still applies captured Linux runtime tail
values for RFE, IQK, TX scale, TX BB control, and default TXAGC bring-up.
Those values are now reported as `stop_gap=true` with
`stop_gap_registers[].source = "captured_linux_runtime_tail"`.

This makes close-range success distinguishable from long-distance RF quality:
the packet flow can be accepted as a bench result while the calibration source
remains explicitly marked as a range-readiness risk until runtime IQK/LCK/RFE
calibration is ported or measured unnecessary.

## RF-Quality Envelope

`rf-quality-report` now lifts calibration probes from macOS bridge artifacts
into `macos.calibration`:

- `probes`: the full calibration probe timeline.
- `rfe_pinmux`, `rfe_inversion`, `rfe_timing`, `rf_path`, `iqk`, `lck`: the
  final `before_tx` readback groups.
- `thermal`: the final probe thermal status plus EFUSE
  `EEPROM_THERMAL_METER_8812` and `EEPROM_IQK_LCK_8812` bytes when available.
- `stop_gap_sources`: why the run is still labeled stop-gap/static.

The next measurement step is to compare these macOS probe values against a
Linux baseline on the same adapter, channel, and bandwidth before deciding
whether a partial IQK/LCK approximation is enough for distance work.

## Bench Validation

On May 2, 2026, the remote macOS hardware Mac ran a one-frame
`bridge-tx-bench --macos-usbhost --init-before-tx --linux-init-order` smoke on
channel 36 HT20. The run passed, submitted one 128-byte USB TX packet, emitted
three init-time probes (`before_channel`, `after_channel`, `before_tx`), and
emitted top-level `rf_calibration_pre_tx`.

Key final pre-TX readbacks:

- RFE pinmux path A: `0x54337717`.
- IQK result path A: `0x30000c1c`.
- Captured tail stop-gap register count: 42.

Artifacts on the hardware Mac:

- Mac bridge report: `/tmp/wfb-agent-calibration-probe-smoke.json`.
- RF-quality envelope: `/tmp/wfb-agent-calibration-rfq.json`.
