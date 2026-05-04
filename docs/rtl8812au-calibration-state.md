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
- IQK result/shadow registers plus the upstream IQK tone, PI, AGC, and
  before/after power readback registers.
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

Live sustained close-range run after receiver-session hardening:

- Artifact directory: `/tmp/wfb-rfq-rtl8812a-lck-sustained-hardened`.
- macOS bridge result: `pass`; submitted `3000/3000` datagrams.
- Linux WFB recovery: `1970/2000` marked source payloads.
- Receiver health: `receiver_session_observed = true`,
  `receiver_unable_decrypt_count = 0`, `receiver_status = "partial_payloads"`.
- Linux-baseline comparison: `baseline_comparable`, `within_margin`.
- Payload-loss delta versus the Linux baseline: `1.45` percentage points.
- macOS/Linux throughput ratio: `0.854542432051679`.

Follow-up telemetry-gated close-range runs on May 2, 2026 reversed that
interpretation for the current bench state:

- No warmup: `/tmp/wfb-rfq-prod-lck-telemetry-gate/rf-quality-report.json`
  submitted `3000/3000` datagrams, but Linux recovered only `392/2000` marked
  payloads, logged `2151` decrypt failures, and reported
  `acceptance.status = "degraded_comparison"`.
- With `SOURCE_WARMUP_PAYLOADS=120`:
  `/tmp/wfb-rfq-prod-lck-warmup-telemetry/rf-quality-report.json` submitted
  `3180/3180` datagrams including the warmup FEC estimate, but Linux recovered
  only `536/2000` marked payloads and still logged `2119` decrypt failures.
- Both runs retained RX_ANT telemetry, so this was not a missing-metadata case.
  Latest reported RSSI averages stayed near `-24 dBm` on antenna `0x1` and
  `-16 dBm` on antenna `0x0`.

Interpretation: LCK is not a production candidate right now. The failure looks
like a receiver session/decrypt path problem under the LCK profile rather than
a lack of RF visibility, and warmup did not resolve it. Keep LCK available for
diagnostics, but do not promote it for range work until a repeatable root cause
or a passing receiver-backed A/B run exists.

## Read-Only IQK Probe Profile

`bridge-tx-listen`, `bridge-run`, and `bridge-tx-bench` now expose:

```sh
--tx-calibration-profile rtl8812a-iqk-probe
```

This is not runtime IQK. It is a non-perturbing pre-TX marker profile for the
full `phy_iq_calibrate_8812a` port. The profile intentionally performs no
additional hardware reads in the TX calibration hook; IQK final-state evidence
comes from the existing `rf_calibration_pre_tx.iqk` probe, which now covers
expanded IQK result, tone, PI, AGC, and before/after power registers from
`Hal8812PhyReg.h`.

Reports label this under `tx_calibration_profile.iqk` with
`mode = "deferred_hardware_probe"` and `read_only = true`. MAC/BB backup,
AFE backup, RF serial backup offsets `0x65`, `0x8f`, `0x00`, and page-C1 latch
reads at `0x0cb8`/`0x0eb8` are deliberately skipped in this pre-TX profile.
Hardware smoke showed that even read-only profile-time IQK probing can perturb
WFB recovery, so those reads should move to a standalone non-TX diagnostic or
the full IQK port rather than the live TX path.

`rf-quality-report` lifts the safe profile evidence into
`macos.calibration.profile_report` and `macos.calibration.iqk`, but the
calibration mode remains stop-gap/unknown unless a real runtime IQK routine is
selected later.

Hardware validation after the receiver-session hardening:

- Current-default control: `/tmp/wfb-rfq-rtl8812a-current-default-session-hardened`.
- IQK marker profile: `/tmp/wfb-rfq-rtl8812a-iqk-marker-session-hardened`.
- Both runs submitted `149/149` bridge datagrams and recovered `100/100`
  marked Linux receiver payloads.
- Both reports show `receiver_session_observed = true`,
  `receiver_unable_decrypt_count = 0`, and
  `short_run_datagram_tolerance_applied = true`.
- The IQK marker report uses `tx_calibration_profile.profile =
  "rtl8812a_iqk_probe"` and `tx_calibration_profile.iqk.mode =
  "deferred_hardware_probe"` with no profile-time IQK register reads.

Sustained close-range validation after the same hardening:

- Current-default control:
  `/tmp/wfb-rfq-rtl8812a-current-default-sustained-hardened`.
  Submitted `3000/3000` bridge datagrams, recovered `1973/2000` marked
  receiver payloads, observed the WFB session, saw zero decrypt failures, and
  remained `baseline_comparable`/`within_margin` against the Linux baseline.
  Payload-loss delta was `1.30` percentage points and the macOS/Linux
  throughput ratio was `0.8543180614248586`.
- IQK marker profile:
  `/tmp/wfb-rfq-rtl8812a-iqk-marker-sustained-hardened`.
  Submitted `3000/3000` bridge datagrams, recovered `1980/2000` marked
  receiver payloads, observed the WFB session, saw zero decrypt failures, and
  remained `baseline_comparable`/`within_margin`. Payload-loss delta was
  `0.95` percentage points and the macOS/Linux throughput ratio was
  `0.8567657134390011`.

Interpretation: the read-only IQK marker is safe for sustained close-range
testing and does not itself improve or replace IQK. It exists to label the
current state honestly while the full runtime IQK port is still pending.

## Runtime IQK Profile

`bridge-tx-listen`, `bridge-run`, and `bridge-tx-bench` also expose the guarded
runtime IQK profile:

```sh
--tx-calibration-profile rtl8812a-runtime-iqk \
--i-understand-this-writes-registers
```

This profile ports the bounded RTL8812A Linux IQK flow into the retained
userspace USB session. It backs up MAC/BB, AFE, RF serial offsets, page-C1
latches, `rHSSIRead_Jaguar`, `REG_AGC_TABLE_JAGUAR`, and `REG_TXPAUSE`; applies
the upstream MAC/AFE/RF IQK setup; runs TX and RX one-shot IQK with the upstream
retry limits; fills selected or fallback TX/RX IQC values; and then restores
the saved state.

Reports label this under `tx_calibration_profile.runtime_iqk` with:

- `sweep_index`, `sweep_count`, `max_sweeps`, and `sweep_summaries` for the
  bounded retry wrapper around the Linux-derived IQK sweep;
- per-path `tx` and `rx` stage status, retry count, max ready-poll delay,
  per-attempt ready/fail/raw-result evidence, candidates, selected IQC value,
  fallback flag, and fill plan;
- `backup` and `cleanup` evidence, including restore counts and
  `cleanup_status`;
- `before_iqk_registers`, `after_iqk_registers`, and final affected IQK
  register readback, including the path-A/path-B RX IQC latches at `0x0c10`
  and `0x0e10`;
- USB counter deltas for the calibration sequence.

`rf-quality-report` lifts the same data into
`macos.calibration.runtime_iqk` and adds
`macos.calibration.runtime_iqk_summary` with compact `risk`, `completed`,
`cleanup_restored`, sweep-count, and fallback-stage fields for production
gating.

This is now a real runtime IQK implementation, but it remains experimental for
range work until receiver-backed A/B evidence exists for the same channel,
bandwidth, rate, TX power mode, payload, FEC, and antenna geometry. Treat any
`cleanup_status != "restored"` or unexpected per-path fallback as a calibration
failure, even if close-range WFB payload recovery still passes.

Hardware validation on May 2, 2026:

- Guarded one-frame runtime IQK smokes:
  `/tmp/wfb-rtl8812a-runtime-iqk-smoke.json`,
  `/tmp/wfb-rtl8812a-runtime-iqk-smoke-2.json`, and
  `/tmp/wfb-rtl8812a-runtime-iqk-smoke-3.json`.
- Follow-up IQC-readback smoke:
  `/tmp/wfb-rtl8812a-runtime-iqk-iqc-readback.json`.
- Baseline-compatible receiver-backed run:
  `/tmp/wfb-rfq-runtime-iqk-a2/rf-quality-report.json`; follow-up:
  `/tmp/wfb-rfq-runtime-iqk-a3/rf-quality-report.json`.
- Signed-selection receiver-backed run:
  `/tmp/wfb-rfq-runtime-iqk-signed-a1/rf-quality-report.json`.
- Attempt-evidence one-frame smoke:
  `/tmp/wfb-rtl8812a-runtime-iqk-attempts.json`.
- The first close-range run submitted and observed `3000/3000` WFB datagrams,
  recovered `1978/2000` marked payloads, matched the Linux baseline tuple, and
  stayed `within_margin` with a `1.05` percentage-point payload-loss delta.
- The follow-up close-range run also submitted and observed `3000/3000` WFB
  datagrams, recovered `1984/2000` marked payloads, matched the Linux baseline
  tuple, and stayed `within_margin` with a `0.75` percentage-point payload-loss
  delta.
- The signed-selection close-range run submitted and observed `3000/3000` WFB
  datagrams, recovered `1964/2000` marked payloads, matched the Linux baseline
  tuple, and stayed `within_margin` with a `1.75` percentage-point payload-loss
  delta and `0.8603379958870349` macOS/Linux throughput ratio.
- A fresh telemetry-gated runtime-IQK rerun at
  `/tmp/wfb-rfq-prod-runtime-iqk-telemetry-gate/rf-quality-report.json`
  recovered `1982/2000`, logged zero decrypt failures, and stayed
  `within_margin`, but path-A RX IQK failed with `rx_iqk_failed_flag` and used
  fallback IQC (`0x200/0x000`), so `runtime_iqk_summary.risk` was
  `fallback_applied`.
- A bounded three-sweep runtime-IQK rerun at
  `/tmp/wfb-rfq-prod-runtime-iqk-multisweep-a1/rf-quality-report.json`
  recovered `1995/2000`, logged zero decrypt failures, stayed `within_margin`
  with a `0.2` percentage-point loss delta, and recorded receiver `RX_ANT`
  telemetry. All three IQK sweeps restored cleanup state and completed TX on
  both paths plus RX on path B, but path-A RX still fell back in every sweep.
  This rules out a single unlucky sweep as the only cause of the path-A RX
  instability.
- The RX-trigger parity fix in `run_rtl8812a_runtime_iqk_rx_oneshot` now keeps
  every TX-ready path triggered on each RX IQK retry, matching the upstream
  Linux loop. Hardware evidence:
  `/tmp/wfb-rfq-runtime-iqk-peer-trigger-smoke-a1/rf-quality-report.json`
  completed in sweep 2 with `400/400`; the full run at
  `/tmp/wfb-rfq-runtime-iqk-peer-trigger-full-a1/rf-quality-report.json`
  completed in sweep 2 with `2000/2000`, zero decrypt failures, restore ok,
  and `runtime_iqk_summary.risk=completed`. This resolves the path-A RX
  fallback seen in `/tmp/wfb-rfq-prod-runtime-iqk-multisweep-a1` for
  close-range gates.
- Latest-format close-range evidence at
  `/tmp/wfb-rfq-runtime-iqk-prod-gate-a1/rf-quality-report.json` completed
  runtime IQK in sweep 3, restored cleanup state, recovered `1978/2000`, logged
  zero decrypt failures, and remained `within_margin`. This artifact includes
  first-class `channel_state.verify_status=verified` and
  `receiver_signal.status=usable`, so it is the current production-gate shape
  for runtime IQK close-range validation.
- Runtime IQK cleanup restored successfully in each run. TX IQK succeeded on
  paths A and B, RX IQK succeeded on path B, and the latest RX-trigger parity
  runs completed RX IQK on path A in the receiver-backed flow. Keep this
  profile experimental for default long-distance use until stepped or outdoor
  evidence shows whether runtime IQK improves distance margin.
- The attempt-evidence smoke showed path-A RX was ready immediately on every
  attempt, but alternated usable candidates with explicit hardware fail flags
  (`0x0000ee00`/`0x0000ef00`) before fallback. That points at IQK candidate
  quality or sequence sensitivity rather than a ready-poll timeout.
- Candidate selection now compares IQK X/Y components as signed 11-bit values,
  matching the Linux driver's left-shift/arithmetic-right-shift behavior before
  tolerance checks. Follow-up one-frame smokes still often saw path-A RX
  fallback because no two of the three path-A RX candidates were within the
  upstream signed tolerance on both X and Y. The captured Linux final-state map
  also shows RX IQC fallback-shaped values at `0x0c10` and `0x0e10`, so this
  may be normal Linux-parity behavior for this adapter/channel rather than a
  ready-poll bug.
- In the receiver-backed signed-selection run, both RX paths completed:
  path A selected `0x1fc/0x006` after five retries and path B selected
  `0x1fb/0x003` after one retry. This proves the signed candidate-selection fix
  can carry through to final RX IQC fill in the sustained WFB flow.

## Standalone IQK Diagnostic

`wfb-radio-diag` also exposes a standalone deep IQK evidence command:

```sh
cargo run -p wfb-radio-diag -- --json \
  --report /tmp/wfb-rtl8812a-iqk-diagnostic.json \
  rtl8812a-iqk-diagnostic \
  --macos-usbhost \
  --channel 36 \
  --bandwidth 20 \
  --firmware /tmp/rtl8812aefw.bin \
  --i-understand-this-writes-registers
```

This command runs full same-session RTL8812AU init and then collects IQK
evidence without starting WFB TX, WFB RX, synthetic TX, or a bulk-IN RX loop.
It deliberately lives outside the live pre-TX calibration profile because
RF-serial and page-C1 probing previously perturbed WFB recovery when mixed into
the TX path.

The report records:

- upstream MAC/BB backup registers from `_phy_iq_calibrate_8812a`;
- upstream AFE backup registers;
- RF backup offsets `0x65`, `0x8f`, and `0x00` for paths A and B;
- page-C1 latch reads at `0x0cb8` and `0x0eb8`;
- normal-page IQK result, tone, PI, AGC, and before/after power registers;
- `rHSSIRead_Jaguar` and page-select restore readback;
- traffic flags proving the diagnostic did not submit WFB or synthetic TX.

The diagnostic sets `iqk.mode = "standalone_deep_evidence"`,
`iqk.evidence_only = true`, and `iqk.cleanup_status = "restored"` when selector
cleanup readback matches the saved state. It still does not run the IQK tone
sweep, select IQK candidates, or write final IQC values, so it must not be
treated as runtime IQK calibration or long-distance acceptance.

## IQK/LCK Porting Decision

Decision as of May 2, 2026: keep the current captured/partial calibration path
for default close-range testing, add the targeted Linux-parity profile for
controlled A/B runs, add the guarded LCK runtime profile for opt-in testing,
add the read-only IQK probe for staged evidence, add the guarded runtime IQK
profile for experimental A/B testing, and do not treat any of these as
long-distance accepted calibration until stepped or outdoor evidence supports
that claim.

Rationale:

- Receiver-backed close-range runs already recovered all expected WFB payloads
  for the current default/captured, manual TXAGC, and EFUSE-derived TXAGC
  modes.
- The Linux calibration comparison still shows six RFE/TX-scale/TX-BB-control
  differences, so close-range success is not calibration parity.
- Full `phy_iq_calibrate_8812a` is sequence-sensitive and remains the largest
  calibration risk. LCK is available as a runtime-calibration candidate, and
  runtime IQK is now available as a guarded TX/RX one-shot profile that must be
  validated against receiver evidence before production use.

Next action:

- Run stepped/attenuated or outdoor 20 MHz profiles with the current
  EFUSE-derived TX power mode and captured calibration labels intact.
- Run close-range and later stepped/outdoor profiles both with
  `--tx-calibration-profile current-default` and
  `--tx-calibration-profile linux-parity-ch36-ht20`.
- Add `--tx-calibration-profile rtl8812a-lck` to the same A/B matrix once the
  adapter and receiver geometry can show whether LCK improves stability,
  margin, or decode rate.
- Add `--tx-calibration-profile rtl8812a-runtime-iqk` to the close-range A/B
  matrix only with explicit write authorization, then inspect
  `tx_calibration_profile.runtime_iqk.cleanup_status`, per-path fallback flags,
  and receiver RSSI/SNR/MCS telemetry before considering stepped tests.
- Use the sustained hardened close-range runs as the current software sanity
  baseline before changing IQK/LCK/RFE code: current-default recovered
  `1973/2000`, IQK marker recovered `1980/2000`, and LCK recovered `1970/2000`,
  all with WFB session observed, zero decrypt errors, and Linux-margin pass.
- Use `--tx-calibration-profile rtl8812a-iqk-probe` on close-range smokes to
  label staged IQK evidence while relying on `rf_calibration_pre_tx.iqk` for
  the safe final-state register readback.
- Keep standalone IQK diagnostics available as non-traffic evidence; use the
  runtime IQK profile when the goal is actual IQC selection and fill.
- Do not use `linux-parity-ch36-ht20` as a production profile until it recovers
  close-range payloads. Its current value is as a negative control proving that
  final-register replay alone is insufficient.
- Use receiver/spectrum evidence to decide whether runtime IQK improves
  imbalance, EVM, or asymmetric path quality over EFUSE-derived TXAGC alone.

Follow-up after the accepted May 2, 2026 close-range profile: the channel 36
HT20 EFUSE-derived run recovered `2000/2000` marked WFB source payloads and was
inside the Linux payload-loss margin. That was enough to defer deeper runtime
calibration at the time. LCK and runtime IQK have since been ported as opt-in
profiles; both still need receiver-backed or spectrum-backed A/B evidence
before being promoted for distance work.

May 4, 2026 production `radio-run` A/B on the current local 6 ft bench keeps
that caution in place. Strict current-default validation failed the peer gate
because the bench placement was noisy (`74/80` Mac-to-Linux and `77/80`
Linux-to-Mac at `/tmp/wfb-radio-run-duplex-strict-20260504-140119`) while the
production loop stayed clean. `rtl8812a-lck` was similar (`78/80` and `76/80`
at `/tmp/wfb-radio-run-duplex-lck-strict-20260504-140606`). The
`rtl8812a-runtime-iqk` profile is not production-usable yet: it forwarded the
Linux-to-Mac direction cleanly (`80/80`) but Mac-to-Linux recovered `0/80`, and
Linux `wfb_rx` logged many unable-decrypt packets even though `radio-run`
submitted all `149/149` TX frames. The IQK report for
`/tmp/wfb-radio-run-duplex-iqk-strict-20260504-140426` showed
`status=fallback_applied` with path A RX IQK fallback on every sweep, so runtime
IQK remains experimental until the fallback/fill path is corrected and rerun
against receiver evidence.
