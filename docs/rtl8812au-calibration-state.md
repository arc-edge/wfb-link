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

`rf-quality-report --linux-baseline` also recognizes Linux `trace-registers`
final-register maps. When the baseline contains the RF/RFE/IQK/LCK register
addresses tracked by the macOS probe, the report emits
`comparison.calibration` with compared register count and per-register
mismatches.

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

## Linux Calibration Comparison

Using the channel-36 Linux WFB-TX final-register map
`/tmp/linux-rtl8812au-ch36-wfbtx-final-registers.json` as
`--linux-baseline`, the RF-quality comparison produced
`/tmp/wfb-agent-calibration-linux-compare.json`.

Summary:

- macOS calibration registers: 15.
- Linux calibration registers: 15.
- Compared registers: 15.
- Mismatches: 6.

Observed mismatches:

| Register | macOS | Linux final |
| --- | --- | --- |
| `0x0c1c` path A TX scale | `0x2d400003` | `0x40000003` |
| `0x0c90` path A RF latch / TX BB control | `0x01817d24` | `0x01807c09` |
| `0x0cb0` path A RFE pinmux | `0x54337717` | `0x54337770` |
| `0x0e1c` path B TX scale | `0x2d400003` | `0x40000003` |
| `0x0e90` path B RF latch / TX BB control | `0x01817d24` | `0x01807c09` |
| `0x0eb0` path B RFE pinmux | `0x54337717` | `0x54337770` |

The current macOS path is therefore close-range functional but not
Linux-calibration-equivalent. The mismatch is now explicit enough to use in
stepped or long-distance RF-quality decisions instead of treating packet
recovery alone as calibration parity.

## Targeted Linux-Parity Profile

`bridge-tx-listen`, `bridge-run`, and `bridge-tx-bench` now expose:

```sh
--tx-calibration-profile linux-parity-ch36-ht20
```

For channel 36 HT20 only, this applies the six Linux-final values listed in
the mismatch table above after init and before TX:

- Path A/B TX scale: `0x40000003`.
- Path A/B RFE pinmux: `0x54337770`.
- Path A/B TX BB control: `0x01807c09`.

The command report records these writes under `tx_calibration_profile` with
before/write/after readback. The final `rf_calibration_pre_tx` probe is labeled
`targeted_linux_parity`, and `rf-quality-report` lifts the profile report into
`macos.calibration.profile_report`.

This is not full Linux runtime calibration. It is a targeted, guarded A/B tool
for the exact channel-36/HT20 parity gap we can test now. Runtime IQK and LCK
remain separate from this final-register replay because the Linux routines are
sequence-sensitive and should be validated against receiver-backed or
spectrum-backed evidence.

Live close-range smoke on May 2, 2026 showed this profile is not safe to
promote yet:

- `current-default` calibration recovered `100/100` marked source payloads.
- `linux-parity-ch36-ht20` applied all six writes with matching readback but
  recovered `0/100` marked source payloads.
- Both runs had Linux peer preflight `status=ok`, `iw` available, and channel
  36 HT20 confirmed.

Interpretation: copying the final Linux register values in isolation is not
equivalent to running the Linux calibration sequence. At least one of these
values depends on prior sequence state, IQK/LCK state, or surrounding RF/BB
runtime writes. Keep this profile opt-in for diagnostics only.

## Runtime LCK Profile

`bridge-tx-listen`, `bridge-run`, and `bridge-tx-bench` also expose:

```sh
--tx-calibration-profile rtl8812a-lck
```

This ports the small RTL8812A local-oscillator calibration sequence from the
Linux driver. The routine now performs RF serial readback on path A, reads and
preserves RF `CHNLBW`, pauses packet TX when the chip is not in continuous-TX
mode, sets RF `LCK` bit 14, triggers RF `CHNLBW` bit 15, waits 150 ms, clears
RF `LCK`, restores `REG_TXPAUSE`, and restores RF `CHNLBW`.

The command report records this under `tx_calibration_profile.lck` with:

- the `REG_SINGLE_TONE_CONT_TX_JAGUAR` and `REG_TXPAUSE` inputs;
- RF readback source evidence for PI/SI mode and the selected readback
  register;
- RF `LCK` and `CHNLBW` before/trigger/restore/after values;
- the 150 ms calibration delay and best-effort cleanup behavior on error.

This is a real runtime calibration step, but it is still not full Linux
calibration parity. IQK remains unported, and the default profile remains
`current-default` until LCK is validated against receiver-backed or
spectrum-backed distance evidence.

Live short smoke on May 2, 2026:

- Artifact directory: `/tmp/wfb-rfq-rtl8812a-lck-smoke-pass`.
- Linux peer preflight: `status=ok`, no missing required or optional commands,
  `iw=/usr/sbin/iw`.
- macOS bridge result: `pass`; submitted `149/149` datagrams.
- Linux WFB recovery: `100/100` marked source payloads.
- LCK readback used PI mode via `0x0d04`.
- RF `CHNLBW`: `0x17d24 -> 0x1fd24 -> 0x17d24`.
- RF `LCK`: `0x1a78d -> 0x1e78d -> 0x1a78d`.
- `REG_TXPAUSE`: `0x00 -> 0xff -> 0x00`.

This validates RF readback and the basic LCK sequence on the attached
AWUS036ACH. It does not validate long-distance RF quality; the run used a
short 256-byte/100-payload smoke profile and is not comparable to the sustained
1,000-byte Linux baseline.

## IQK/LCK Porting Decision

Decision as of May 2, 2026: keep the current captured/partial calibration path
for default close-range testing, add the targeted Linux-parity profile for
controlled A/B runs, add the guarded LCK runtime profile for opt-in testing,
and do not treat any of these as long-distance accepted calibration until
stepped or outdoor evidence supports that claim.

Rationale:

- Receiver-backed close-range runs already recovered all expected WFB payloads
  for the current default/captured, manual TXAGC, and EFUSE-derived TXAGC
  modes.
- The Linux calibration comparison still shows six RFE/TX-scale/TX-BB-control
  differences, so close-range success is not calibration parity.
- Full `phy_iq_calibrate_8812a` is sequence-sensitive and remains the largest
  calibration blocker. LCK is now available as the first runtime-calibration
  candidate and should be tested as an A/B profile before making it default.

Next action:

- Run stepped/attenuated or outdoor 20 MHz profiles with the current
  EFUSE-derived TX power mode and captured calibration labels intact.
- Run close-range and later stepped/outdoor profiles both with
  `--tx-calibration-profile current-default` and
  `--tx-calibration-profile linux-parity-ch36-ht20`.
- Add `--tx-calibration-profile rtl8812a-lck` to the same A/B matrix once the
  adapter and receiver geometry can show whether LCK improves stability,
  margin, or decode rate.
- Do not use `linux-parity-ch36-ht20` as a production profile until it recovers
  close-range payloads. Its current value is as a negative control proving that
  final-register replay alone is insufficient.
- Port full IQK when receiver/spectrum evidence points to IQ imbalance, EVM,
  or asymmetric path quality that the targeted profile and EFUSE-derived TXAGC
  do not fix.

Follow-up after the accepted May 2, 2026 close-range profile: the channel 36
HT20 EFUSE-derived run recovered `2000/2000` marked WFB source payloads and was
inside the Linux payload-loss margin. That was enough to defer deeper runtime
calibration at the time. LCK has since been ported as an opt-in profile; full
IQK is still deferred until receiver-backed or spectrum-backed evidence points
to IQ imbalance, EVM, or path asymmetry.
