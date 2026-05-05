# RF Quality Range Profiles

These profiles define the staged evidence path for treating the macOS
RTL8812AU/AWUS036ACH WFB link as range-ready. They are intentionally
receiver-backed: USB submission, descriptor acceptance, and close-range packet
visibility are useful diagnostics, but they are not long-distance RF quality
acceptance by themselves.

The first accepted target remains channel 36, 20 MHz, fixed HT MCS1,
Linux-monitor-compatible TX shape, WFB port `0`, FEC `k=8,n=12`, and 1,000
byte source payloads unless a profile states otherwise. Mac and Linux runs must
use the same adapter class, antennas, key, radio port, FEC settings, payload
size, channel, bandwidth, and fixed rate/profile before the comparison is
treated as valid.

## Profile Ladder

| Profile | `rf-quality-report --profile-kind` | Purpose | Promotion gate |
| --- | --- | --- | --- |
| Close-range sanity | `close-range` | Prove the selected Mac TX power and calibration mode still recovers WFB payloads on the bench. | Must pass before any stepped or outdoor run is accepted for the same channel, bandwidth, rate, power mode, calibration mode, FEC, and payload length. |
| Stepped or attenuated | `stepped-attenuated` | Measure margin changes under repeatable loss, separation, or attenuator steps before field range work. | Must stay within the documented Mac-vs-Linux acceptance margin or identify the calibration/power gap being tested. |
| Outdoor long-distance | `outdoor-long-distance` | Validate the operating profile in the actual field geometry. | Requires a passing close-range run with receiver RX_ANT MCS/RSSI/SNR telemetry, plus companion notes with distance/geometry, antenna orientation, adapter placement, environment, and artifacts. |

## Indoor 50 ft Exploration

Explored on May 5, 2026 with the adapter on the local Mac, the Linux receiver
reachable as `pi@drone-2f389`, channel 36 / 20 MHz, current-default TX power
and current-default stop-gap calibration. The placement was approximately
50 ft indoors from the receiver.

This is not an accepted range profile yet. The main result is that the radio
loop stayed healthy while the RF path showed low-rate erasures:

- Radio-side health stayed clean across the matrix: `radio_result=pass`, zero
  TX drops, zero TX failed submissions, and zero post-session decrypt failures.
- A 200-payload duplex profile passed at
  `/tmp/wfb-radio-run-range50ft-m2lmcs0-l2mmcs1-sym2-200-slow40-20260505-103755`:
  M2L `2/12` MCS0, L2M `2/12` MCS1, 40 ms payload interval, `200/200` both
  directions.
- The same profile did not sustain the 1000-payload gate at
  `/tmp/wfb-radio-run-range50ft-m2lmcs0-l2mmcs1-sym2-1000-slow40-20260505-103940`:
  M2L `998/1000`, L2M `999/1000`.
- Direction isolation at the same 40 ms interval passed both ways:
  `/tmp/wfb-radio-run-range50ft-m2l-only-mcs0-k2n12-1000-40ms-20260505-105249`
  recovered M2L `1000/1000`, and
  `/tmp/wfb-radio-run-range50ft-l2m-only-mcs1-k2n12-1000-40ms-20260505-105709`
  recovered L2M `1000/1000`.
- Stronger or differently shaped duplex protection did not yet produce a
  strict 1000-payload pass. `2/16`, `1/6`, `1/12`, slower 60/80/100 ms
  intervals, symmetric MCS0, and measured-source phase offsets all still
  missed payloads in at least one direction.
- The closest conservative duplex retry was
  `/tmp/wfb-radio-run-range50ft-sym-mcs0-k1n12-1000-slow100-phase50-20260505-111956`:
  M2L `1000/1000`, L2M `998/1000`, zero decrypt failures, Mac RX average SNR
  `22 dB`, RSSI average `-54 dBm`, and zero radio TX failures.

Interpretation: this evidence points at receive-window/airtime interaction,
environmental fades, or remaining RF calibration/antenna margin rather than
Mac USB submission or encrypted-payload corruption. Do not promote an indoor
50 ft profile until a strict 1000-payload duplex run recovers both directions
with zero post-session decrypt failures. The duplex runner now records
per-direction measured-source phase offsets in `source-gate.json`; use that
knob for future TDMA/scheduling experiments, but do not treat phase-offset
near-passes as accepted range evidence.

## Close-Range Sanity

Run this whenever the TX power mode, calibration mode, descriptor profile,
channel, bandwidth, payload settings, antenna, or adapter changes. Keep both
radios close enough that a healthy Linux reference has effectively no RF margin
pressure; this profile is for regression detection, not range proof.

The preferred close-range runner is:

```sh
scripts/run-rf-quality-close-range.sh
```

Use `scripts/run-rf-quality-close-range.sh --dry-run` to inspect the Mac relay,
Mac bridge, Linux peer, artifact collection, and RF-quality report steps before
touching hardware. The explicit envelope below remains useful for manual
reproduction and for comparing a failed automated run against the raw
`rf-quality-report` inputs.

Required settings:

- Channel 36, 20 MHz until another channel has a matching Linux baseline.
- Fixed `mcs1` / `linux-monitor` TX profile.
- Current preferred Mac power mode: `current-default`. EFUSE-derived TX power
  remains opt-in and receiver-gated because sustained duplex runs have shown
  state-sensitive decrypt failures.
- Current accepted calibration label: `stop-gap-captured`. The guarded
  `rtl8812a-runtime-iqk` profile is available for experimental A/B runs, but it
  is not a long-distance accepted calibration mode until it has passing
  sustained receiver-backed close-range regression matrices and stepped or
  outdoor evidence.
- At least 120 source payloads for quick checks; use 2,000 source payloads for
  an accepted reference.
- For production duplex smokes, keep `SESSION_ACQUIRE_MODE=observed` unless
  deliberately debugging first-acquisition behavior. Marked payloads should not
  start until each enabled WFB receiver path has logged `SESSION`; pre-session
  decrypt failures are acquisition evidence, while post-session decrypt failures
  still quarantine the profile.
- Linux baseline or Linux receiver artifact paths attached to the report.

Example RF-quality envelope:

```sh
cargo run -p wfb-radio-diag -- --json \
  --report /tmp/wfb-rf-quality-close-range-20mhz.json \
  rf-quality-report \
  --profile-name close-range-ch36-ht20-efuse \
  --profile-kind close-range \
  --channel 36 --bandwidth 20 \
  --tx-rate mcs1 \
  --tx-profile linux-monitor \
  --tx-power-mode efuse-derived \
  --calibration-mode stop-gap-captured \
  --wfb-link-id 0x000001 \
  --wfb-radio-port 0x00 \
  --fec-k 8 --fec-n 12 \
  --payload-len 1000 \
  --expected-payloads 2000 \
  --recovered-payloads 1999 \
  --mac-report /tmp/wfb-agent-close-range-listen.json \
  --efuse-report /tmp/wfb-live-efuse-dump.json \
  --linux-baseline fixtures/rf-quality/current-close-range-20mhz-linux-baseline.json \
  --receiver-artifact /tmp/mac-close-range-rf.pcap \
  --receiver-artifact /tmp/mac-close-range-rx-lo.pcap
```

Acceptance meaning: a passing close-range profile only proves that the selected
Mac settings are bench-functional and comparable to the close-range Linux
baseline. It does not promote the profile to long-distance accepted.
Quick smoke runs with fewer source payloads are useful for validating
orchestration, but they are not baseline-comparable reference runs unless the
Linux baseline has the same expected source payload count.
For outdoor promotion, the close-range gate must also carry receiver telemetry
from Linux `wfb_rx` RX_ANT lines so the range report can compare payload
recovery with MCS/RSSI/SNR health, not just USB submission and decoded payload
counts. The gate checks the RX_ANT frequency, MCS index, and bandwidth against
the outdoor profile tuple, so a passing bench artifact cannot promote a field
run on a different channel, rate, or bandwidth. The report also emits
`macos.wfb_outcome.receiver_signal` with antenna count, tuple consistency, RSSI
spread, SNR sample/nonzero counts, `status`, `issues[]`, and `snr_status` so
field tooling has stable RF-health fields without parsing raw receiver logs.
`status=usable` is accepted for current WFB-ng receiver logs that report valid
RSSI and tuple data but all-zero SNR; `status=degraded` blocks outdoor
promotion.
When present, `macos.wfb_outcome.channel_state` is also part of the gate:
`verify_status` must be `verified`, and observed frequency/width must match the
profile.

### Accepted Close-Range 20 MHz Run

Accepted on May 2, 2026:

- Profile: `close-range-ch36-ht20-efuse-20260502`.
- RF-quality report: `/tmp/wfb-rfq-close-efuse-quality.json`.
- Mac bridge report: `/tmp/wfb-rfq-close-efuse-listen.json`.
- EFUSE report: `/tmp/wfb-remote-macos-efuse-dump.json`.
- Linux baseline fixture:
  `fixtures/rf-quality/current-close-range-20mhz-linux-baseline.json`.
- Linux receiver artifacts on `drone-2f389.local`:
  `/tmp/rfq-close-efuse-rf.pcap`, `/tmp/rfq-close-efuse-rx.log`,
  `/tmp/rfq-close-efuse-tx.log`, and `/tmp/rfq-close-efuse-counter.json`.
- Transport setup: Linux `wfb_tx -d` sent distributor datagrams to hardware Mac
  LAN `10.42.0.162:5610`; a temporary hardware-Mac UDP relay forwarded them to
  bridge-localhost `127.0.0.1:5611`.
- Mac bridge result: `pass`, `3000/3000` datagrams received and submitted,
  `0` failed writes, `0` short writes, `3,274,120` USB bytes written.
- TX power: `efuse-derived`, channel 36 HT20 safety clamp, `22` TXAGC
  register writes.
- Calibration: `stop-gap-captured`; calibration remains labeled as stop-gap
  even though this close-range outcome is accepted.
- Receiver result: Linux `wfb_rx` recovered `2000/2000` marked `RFQCLSEF`
  source payloads, `2,000,000` decoded payload bytes.
- Linux comparison: matched profile tuple, Linux baseline recovered
  `1999/2000`; macOS recovered one additional payload in this run.
- Acceptance margin: `within_margin`, payload-loss delta `-0.05` percentage
  points versus Linux. Throughput ratio was recorded as informational only
  because bridge timing included init and relay/orchestration delay.

### Automated Close-Range Runner Validation

Validated on May 2, 2026 with `scripts/run-rf-quality-close-range.sh`:

- Local artifact directory: `/tmp/wfb-rfq-auto-live-20260502-012427`.
- RF-quality report:
  `/tmp/wfb-rfq-auto-live-20260502-012427/rf-quality-report.json`.
- Compact fixture:
  `fixtures/rf-quality/rf-quality-close-range-automation-summary.json`.
- Mac bridge result: `pass`, `3000/3000` datagrams received and submitted.
- Linux receiver counter: `2000/2000` marked `RFQCLSEF` payloads recovered.
- Report result: `pass`, `acceptance.status=baseline_comparable`,
  `comparison.status=matched`, and
  `comparison.outcome.acceptance_margin.status=within_margin`.
- Operational note: the hardware Mac checkout could not `git pull` from GitHub
  via SSH, so the live run used `SYNC_HW_REPO=0`. The runner now collects Linux
  artifacts by streaming them through the same nested SSH path used for peer
  control rather than relying on local `scp -o ProxyJump` target identity.

Runtime IQK validation on May 2, 2026:

- Local artifact directories: `/tmp/wfb-rfq-runtime-iqk-a2`,
  `/tmp/wfb-rfq-runtime-iqk-a3`, and
  `/tmp/wfb-rfq-runtime-iqk-signed-a1`.
- RF-quality reports:
  `/tmp/wfb-rfq-runtime-iqk-a2/rf-quality-report.json`,
  `/tmp/wfb-rfq-runtime-iqk-a3/rf-quality-report.json`, and
  `/tmp/wfb-rfq-runtime-iqk-signed-a1/rf-quality-report.json`.
- Mac bridge result: `pass`, `3000/3000` datagrams received and submitted.
- Linux receiver counter: `1978/2000`, `1984/2000`, and `1964/2000` marked
  payloads recovered with zero decrypt failures and six `RX_ANT` telemetry
  reports in each run.
- Report result: `pass`, `acceptance.status=baseline_comparable`,
  `comparison.status=matched`, and
  `comparison.outcome.acceptance_margin.status=within_margin`.
- Calibration note: `runtime_iqk.status=fallback_applied` because path-A RX IQK
  fell back in the first two full receiver-backed runs. After signed 11-bit
  candidate selection was matched to Linux, the signed-selection run reported
  `runtime_iqk.status=completed` with both RX paths selected. Keep the profile
  experimental until stepped or outdoor evidence shows whether runtime IQK
  improves distance margin.
- Fresh telemetry-gated rerun:
  `/tmp/wfb-rfq-prod-runtime-iqk-telemetry-gate/rf-quality-report.json`
  recovered `1982/2000`, had zero decrypt failures, and remained
  `baseline_comparable` / `within_margin`, but `runtime_iqk_summary.risk` was
  `fallback_applied` because path-A RX IQK failed and used fallback IQC. This
  confirms the runtime-IQK TX path is close-range usable while the calibration
  result is still not clean enough for outdoor gate promotion.
- Bounded multi-sweep rerun:
  `/tmp/wfb-rfq-prod-runtime-iqk-multisweep-a1/rf-quality-report.json`
  recovered `1995/2000`, had zero decrypt failures, and remained
  `baseline_comparable` / `within_margin`. The report recorded
  `runtime_iqk_summary.sweep_count=3`, but path-A RX IQK fell back in all
  three sweeps, so `runtime_iqk_summary.risk` stayed `fallback_applied`.
  Multi-sweep retry improves evidence quality, but it is not the path-A RX IQK
  root fix.
- Upstream RX-trigger parity rerun:
  `/tmp/wfb-rfq-runtime-iqk-peer-trigger-full-a1/rf-quality-report.json`
  recovered `2000/2000`, submitted `3000/3000` bridge datagrams, logged zero
  decrypt failures, and reported `runtime_iqk_summary.risk=completed` after
  sweep 2 with cleanup restored. The fix keeps every TX-ready path triggered
  on each RX IQK retry, matching the Linux loop, instead of stopping triggers
  for a path after that path's RX stage has finished. A shorter 400-payload
  smoke at `/tmp/wfb-rfq-runtime-iqk-peer-trigger-smoke-a1` produced the same
  completed-risk shape. This is the current runtime-IQK close-range reference;
  stepped or outdoor evidence is still required before making runtime IQK the
  long-distance accepted calibration mode. A regenerated report with signal
  health fields,
  `/tmp/wfb-rfq-runtime-iqk-peer-trigger-full-a1/rf-quality-report-signal-health.json`,
  reports `receiver_signal.status=usable` with `issues=["snr_all_zero"]`, two
  antennas, tuple-consistent `5180/MCS1/20`, and RSSI averages `-34..-15 dBm`.
- Latest-format production gate:
  `/tmp/wfb-rfq-runtime-iqk-prod-gate-a1/rf-quality-report.json` recovered
  `1978/2000`, submitted `3000/3000`, logged zero decrypt failures, and stayed
  `baseline_comparable` / `within_margin` with a `1.05` percentage-point loss
  delta versus Linux. Runtime IQK completed in sweep 3 with cleanup restored
  and `runtime_iqk_summary.risk=completed`. The report includes
  `receiver_signal.status=usable` with `issues=["snr_all_zero"]`,
  tuple-consistent `5180/MCS1/20`, RSSI averages `-24..-15 dBm`, and
  `channel_state.verify_status=verified` for channel 36 / 20 MHz. This is the
  current latest-schema close-range gate for runtime IQK; it is still not a
  stepped or outdoor distance result.
- Runtime-owned sweep extraction gate:
  `/tmp/wfb-rfq-runtime-iqk-extraction-post-sweep-runtime-iqk-a1/rf-quality-report.json`
  recovered `1991/2000`, submitted `3000/3000`, logged zero decrypt failures,
  stayed `baseline_comparable` / `within_margin`, and completed runtime IQK in
  sweep 2 with cleanup restored and no TX/RX fallback on either path. The
  paired current-default non-regression run at
  `/tmp/wfb-rfq-runtime-iqk-extraction-post-sweep-default-a1/rf-quality-report.json`
  recovered `1996/2000`. This validates moving the guarded IQK sweep/report
  into `wfb-radio-runtime`; it still does not replace stepped or outdoor
  evidence for long-distance acceptance.
- Runtime-owned calibration profile executor gate:
  `/tmp/wfb-rfq-runtime-iqk-peeriso-warmup-a1/rf-quality-report.json`
  recovered `1993/2000` measured payloads with
  `SOURCE_WARMUP_PAYLOADS=400`, observed/submitted `3599/3600` total WFB
  datagrams including the unmeasured warmup estimate, logged zero decrypt
  failures, verified clean Linux peer isolation before receiver start, stayed
  `baseline_comparable` / `within_margin`, and completed runtime IQK in sweep
  1 with cleanup restored. The newer post-cleanup-fill gate at
  `/tmp/wfb-rfq-runtime-iqk-fill-2000-20260504-153130/rf-quality-report.json`
  recovered `1999/2000`, matched the Linux baseline loss exactly, logged zero
  decrypt failures, completed IQK in sweep 2, and applied 20 selected IQC fill
  writes after cleanup. This is the current hardened close-range gate for the
  runtime-owned profile executor. The paired current-default comparator at
  `/tmp/wfb-rfq-current-default-2000-20260504-153545/rf-quality-report.json`
  recovered `2000/2000` with zero decrypt failures on the same local production
  flow, so runtime IQK remains an experimental A/B profile until controlled
  distance or attenuation shows a margin advantage.
- Remote sustained duplex regression gate:
  `/tmp/wfb-radio-profile-matrix-remote-iqk-m2l5-l2m3-1000-repeat2-20260504-231004`
  used the currently accepted production-smoke tuple
  (`current-default` TX power, M2L `5/12` MCS1, L2M `3/12` MCS2, 20 ms source
  interval) but enabled `rtl8812a-runtime-iqk`. Runtime IQK completed in both
  repeats, and the original total-decrypt gate rejected one repeat with `94`
  Mac-to-Linux decrypt failures plus another with `128` Linux-to-Mac decrypt
  failures. Follow-up log parsing showed those decrypt failures all occurred
  before the receiver's first `SESSION`; after session acquisition there were
  zero decrypt failures. This supersedes the earlier "byte corruption" framing:
  runtime IQK remains experimental, but the repeatable issue to solve is WFB
  session acquisition and measured-payload stability under duplex load, not
  proven post-session payload corruption.
- EFUSE-derived TX-power regression gate:
  `/tmp/wfb-radio-profile-matrix-remote-efuse-m2l5-l2m3-1000-repeat2-20260504-231630`
  originally failed one of two repeats with `117` Linux-to-Mac decrypt failures
  under the same accepted duplex tuple. Follow-up parsing likewise showed the
  decrypt failures were all pre-session acquisition events. EFUSE-derived power
  remains an explicit A/B mode, not a default, until it passes sustained
  post-session-gated matrices with stable measured-payload recovery.
- No-warmup runtime-IQK profile evidence remains useful only for session
  acquisition debugging. The successful no-warmup A/B run at
  `/tmp/wfb-rfq-runtime-cal-profile-api-runtime-iqk-a2/rf-quality-report.json`
  recovered `1984/2000`, observed/submitted `2999/3000` WFB datagrams under
  the one-datagram short-run tolerance, logged zero decrypt failures, stayed
  `baseline_comparable` / `within_margin`, and completed runtime IQK in sweep 1
  with cleanup restored. The earlier
  `/tmp/wfb-rfq-runtime-cal-profile-api-runtime-iqk-a1/rf-quality-report.json`
  run logged `2191` decrypt failures and recovered only `377/2000`; the later
  peer-isolated no-warmup run at
  `/tmp/wfb-rfq-runtime-cal-profile-api-runtime-iqk-peeriso-a1/rf-quality-report.json`
  still logged `2142` decrypt failures and recovered only `425/2000`. Those
  rejected runs show the failure is not residual Linux WFB traffic alone; the
  measured gate now uses an unmeasured warmup to let the receiver acquire the
  WFB session before marked payload accounting starts.
- Linux peer-isolation smoke:
  `/tmp/wfb-rfq-peer-isolation-smoke-a1/rf-quality-report.json` is not a
  baseline-comparable reference because it used 80 payloads, but it verified
  that the runner now records six pre-stop WFB service processes, zero
  residual WFB processes after the settle interval, `peer_isolation_status=ok`,
  verified channel 36 / 20 MHz, and `80/80` recovered with zero decrypt
  failures.

Telemetry-gated default rerun on May 2, 2026:

- Local artifact directory: `/tmp/wfb-rfq-prod-default-telemetry-gate2`.
- RF-quality report:
  `/tmp/wfb-rfq-prod-default-telemetry-gate2/rf-quality-report.json`.
- Mac bridge result: `pass`, `3000/3000` datagrams received and submitted.
- Linux receiver counter: `1986/2000` marked payloads recovered with zero
  decrypt failures and six `RX_ANT` reports.
- Receiver telemetry: MCS1, 20 MHz, latest RSSI averages `-24 dBm` on antenna
  `0x1` and `-16 dBm` on antenna `0x0`; SNR fields were reported as `0 dB` by
  the Linux receiver.
- Report result: `pass`, `acceptance.status=baseline_comparable`,
  `comparison.status=matched`, and
  `comparison.outcome.acceptance_margin.status=within_margin`.
- This is the current close-range artifact shape expected by outdoor gates:
  payload recovery, Linux-margin comparison, and
  `macos.wfb_outcome.receiver_telemetry` are all present.

Ready-marker default rerun on May 2, 2026:

- Local artifact directory: `/tmp/wfb-rfq-prod-ready-marker-default-a1`.
- RF-quality report:
  `/tmp/wfb-rfq-prod-ready-marker-default-a1/rf-quality-report.json`.
- Bridge readiness:
  `/tmp/wfb-rfq-prod-ready-marker-default-a1/bridge-ready-wait.log` observed
  `${REMOTE_PREFIX}-bridge-ready.json` after `1s`. The marker records
  `same_session_init_result=pass`, channel 36 / 5180 MHz, 20 MHz bandwidth,
  `max_datagrams=3000`, and TX power control applied before the receive loop.
- Mac bridge result: `pass`, `3000/3000` datagrams received and submitted.
- Linux receiver counter: `1988/2000` marked payloads recovered, zero decrypt
  failures, six `RX_ANT` reports, and session observed.
- Receiver signal: tuple-consistent `RX_ANT` at `5180/MCS1/20`, two antennas,
  RSSI average range `-24..-16 dBm`, and SNR fields reported as `0 dB`.
- Restore evidence: `${REMOTE_PREFIX}-restore.json` was collected and
  `macos.wfb_outcome.receiver_evidence.linux_restore.status=ok`;
  `missing-artifacts.txt` was empty.
- Report result: `pass`, `acceptance.status=baseline_comparable`,
  `comparison.status=matched`, and
  `comparison.outcome.acceptance_margin.status=within_margin`; loss delta was
  `0.55` percentage points versus the Linux baseline.
- This supersedes `/tmp/wfb-rfq-prod-default-telemetry-gate2` as the current
  close-range artifact shape because it includes bridge readiness, receiver
  signal, and Linux restore evidence.

Runtime LCK negative A/B on May 2, 2026:

- No warmup:
  `/tmp/wfb-rfq-prod-lck-telemetry-gate/rf-quality-report.json`.
- With `SOURCE_WARMUP_PAYLOADS=120`:
  `/tmp/wfb-rfq-prod-lck-warmup-telemetry/rf-quality-report.json`.
- Both runs submitted all expected datagrams and carried RX_ANT telemetry, but
  both were `degraded_comparison` / `outside_margin` because the Linux receiver
  logged thousands of decrypt failures and recovered only `392/2000` and
  `536/2000` marked payloads.
- Do not use `TX_CALIBRATION_PROFILE=rtl8812a-lck` as a range candidate until
  the session/decrypt regression is understood and a fresh close-range gate
  passes.

## Stepped Or Attenuated

Use this profile before outdoor work when an RF attenuator, repeatable
separation, fixed obstacle path, or controlled indoor route is available. The
goal is to find where the Mac profile diverges from the Linux baseline and
whether that divergence points at TXAGC, RFE state, IQK/LCK, antenna/path
placement, or receiver metadata.

Required settings:

- Reuse the exact close-range profile tuple: channel, bandwidth, rate/profile,
  TX power mode, calibration mode, WFB link/radio port, FEC, and payload size.
- Record each step as a separate RF-quality report with a profile name that
  includes the step, such as `step-00`, `step-10db`, or `step-30m-indoor`.
- Keep transmitter and receiver placement fixed except for the one variable
  being stepped.
- Attach receiver logs, pcap or frame JSONL artifacts, WFB payload counters,
  and any attenuator or geometry note.

Example envelope for an attenuation step:

```sh
cargo run -p wfb-radio-diag -- --json \
  --report /tmp/wfb-rf-quality-step-20db.json \
  rf-quality-report \
  --profile-name step-20db-ch36-ht20-efuse \
  --profile-kind stepped-attenuated \
  --channel 36 --bandwidth 20 \
  --tx-rate mcs1 \
  --tx-profile linux-monitor \
  --tx-power-mode efuse-derived \
  --calibration-mode stop-gap-captured \
  --wfb-link-id 0x000001 \
  --wfb-radio-port 0x00 \
  --fec-k 8 --fec-n 12 \
  --payload-len 1000 \
  --expected-payloads 2000 \
  --recovered-payloads 1960 \
  --distance-or-geometry "20 dB fixed attenuator on TX path" \
  --antenna-orientation "conducted path, antennas removed for attenuated step" \
  --adapter-placement "Mac adapter on short USB extension, clear of chassis" \
  --environment-note "bench conducted attenuation step" \
  --companion-artifact /tmp/mac-step-20db-notes.md \
  --mac-report /tmp/wfb-agent-step-20db-listen.json \
  --linux-baseline /tmp/linux-step-20db-baseline.json \
  --receiver-artifact /tmp/mac-step-20db-rf.pcap \
  --receiver-artifact /tmp/mac-step-20db-rx.log
```

Interpretation:

- If Mac and Linux are both near-perfect, increase attenuation or separation.
- If Mac falls behind Linux while close-range still passes, inspect calibration
  comparison, TXAGC evidence, RF path state, and receiver metadata before
  changing power.
- If higher TX power reduces payload recovery, treat it as an RF quality
  failure rather than a power shortage.

## Outdoor Long-Distance

Use this profile only after the same tuple has a passing close-range report.
Outdoor runs are noisy, so they must include field notes and artifacts that make
the result reproducible enough to interpret. The `rf-quality-report` command
enforces this with `--close-range-report`: an `outdoor-long-distance` report
fails if that file is missing, is not a passing `close-range` report with
`baseline_comparable` acceptance, or differs in channel, bandwidth, fixed rate,
TX profile, TX power mode, calibration mode, WFB tuple, payload length, or
expected payload count. It also fails when the close-range receiver telemetry is
missing or the RX_ANT frequency/MCS/bandwidth tuple does not match the outdoor
profile.

Required settings:

- Same profile tuple as the passing close-range report.
- Distance or geometry estimate, including line-of-sight or obstruction notes.
- Antenna orientation and polarization at both ends.
- Adapter placement, cable length, and whether the adapter is near carbon,
  metal, batteries, USB hubs, or high-current wiring.
- Weather and interference notes where relevant.
- Mac report, Linux baseline or receiver report, receiver logs, and any pcap,
  frame JSONL, SDR, or spectrum artifacts.
- Service restore notes for the Linux peer after the controlled run.

Example envelope:

```sh
cargo run -p wfb-radio-diag -- --json \
  --report /tmp/wfb-rf-quality-field-300m.json \
  rf-quality-report \
  --profile-name field-300m-ch36-ht20-efuse \
  --profile-kind outdoor-long-distance \
  --channel 36 --bandwidth 20 \
  --tx-rate mcs1 \
  --tx-profile linux-monitor \
  --tx-power-mode efuse-derived \
  --calibration-mode stop-gap-captured \
  --wfb-link-id 0x000001 \
  --wfb-radio-port 0x00 \
  --fec-k 8 --fec-n 12 \
  --payload-len 1000 \
  --expected-payloads 2000 \
  --recovered-payloads 1900 \
  --close-range-report /tmp/wfb-rf-quality-close-range-20mhz.json \
  --distance-or-geometry "300 m line-of-sight across open field" \
  --antenna-orientation "both vertical, broadside to path" \
  --adapter-placement "Mac adapter on 1 m USB extension, clear of battery and frame" \
  --environment-note "dry, light wind, sparse 5 GHz AP activity" \
  --companion-artifact /tmp/mac-field-300m-notes.md \
  --mac-report /tmp/wfb-agent-field-300m-listen.json \
  --linux-baseline /tmp/linux-field-300m-baseline.json \
  --receiver-artifact /tmp/mac-field-300m-rf.pcap \
  --receiver-artifact /tmp/mac-field-300m-notes.md
```

Acceptance meaning: an outdoor run can be range-accepted only when the
close-range gate passes, the Linux comparison tuple is valid, the receiver
payload result is inside the accepted margin, and the field notes explain the
test geometry well enough to repeat the run.

The first longer-distance 20 MHz acceptance run is deferred until the radios can
be placed in a real outdoor, separated, or attenuated geometry. The current
accepted result is close-range only and must not be described as a range result.

### Indoor 100 ft Exploratory Notes

On May 4, 2026, the radios were separated by roughly 100 ft on the same floor
through several doors. Treat these artifacts as indoor range exploration only,
not accepted long-distance evidence:

- The default duplex profile on channel 36, current-default calibration, FEC
  `8/12`, MCS1 failed (`0/80` Mac-to-Linux, `21/80` Linux-to-Mac) at
  `/tmp/wfb-radio-run-distance-100ft-current-default-smoke-20260504-160256`.
- Slowing source cadence and increasing redundancy helped. Channel 36,
  current-default calibration, symmetric FEC `4/12`, MCS1, 20 ms payload
  interval recovered `80/80` Mac-to-Linux and `72/80` Linux-to-Mac with zero
  decrypt failures at
  `/tmp/wfb-radio-run-distance-100ft-current-default-fec4x12-interval20ms-20260504-161901`.
- Per-direction controls showed Mac-to-Linux was the stronger leg: M2L-only
  `4/12` at 20 ms recovered `80/80` with zero decrypt failures at
  `/tmp/wfb-radio-run-distance-100ft-m2l-only-fec4x12-interval20ms-20260504-162339`.
- The best short indoor profile was asymmetric: M2L `4/12` at MCS1, L2M
  `3/12` at MCS2, channel 36, current-default calibration, 20 ms payload
  interval. Two 80-payload smokes recovered `80/80` plus `76/80` and `80/80`
  plus `78/80`, both with zero decrypt failures:
  `/tmp/wfb-radio-run-distance-100ft-asym-m2l4x12-l2m3x12-mcs2-interval20ms-20260504-163813`
  and
  `/tmp/wfb-radio-run-distance-100ft-asym-m2l4x12-l2m3x12-mcs2-interval20ms-repeat-20260504-164421`.
- The same asymmetric profile did not pass a 200-payload acceptance gate:
  `181/200` Mac-to-Linux, `164/200` Linux-to-Mac, and 66 Linux-to-Mac decrypt
  failures at
  `/tmp/wfb-radio-run-distance-100ft-asym-m2l4x12-l2m3x12-mcs2-200p-interval20ms-20260504-164549`.
  Keep it as a promising smoke profile, not a production/range default.
- Channel 161 with the current channel-36 stop-gap calibration failed
  completely at this placement. EFUSE-derived TX power and runtime IQK also
  remain receiver-gated for range work.

Use `docs/rf-quality-field-notes-template.md` for the companion note artifact.
The structured fields in the report are the summary; the companion artifact is
where longer notes, maps, photos, service-restore output, and spectrum evidence
belong.

## Profile Matrix Automation

Use `scripts/run-radio-run-profile-matrix.sh` to compare production
`radio-run` profiles without manually stitching artifacts together. The matrix
runner wraps `scripts/run-radio-run-duplex-smoke.sh`, can rsync the checkout to
a remote hardware Mac, repeats each profile, pulls artifacts back, and writes:

- `matrix-summary.json`: machine-readable ranked runs and profile groups.
- `matrix-summary.md`: a compact table with pass status, recovery rates,
  decrypt failures, and artifact paths.
- `runs/<profile>-rN/`: the underlying duplex smoke artifacts.

Example short-range remote hardware run:

```sh
HW_MAC_HOST=rownd@rownds-macbook-pro.tail5c793f.ts.net \
HW_DEPLOY=1 \
HW_DEPLOY_PATH=projects/arc/wfb-mac-radio-deploy \
MAC_LAN_IP=10.42.0.162 \
LINUX_LAN_IP=10.42.0.1 \
LINUX_HOST=pi@drone-2f389.local \
PROFILE_SET=short \
REPEATS=1 \
EXPECTED_PAYLOADS=80 \
SOURCE_WARMUP_PAYLOADS=100 \
scripts/run-radio-run-profile-matrix.sh
```

`PROFILE_SET=short` currently covers the default `8/12` MCS1 profile,
symmetric `4/12` MCS1 at a 20 ms source interval, and the current accepted
short-range sustained candidate: M2L `5/12` MCS1 plus L2M `3/12` MCS2 at
20 ms. `PROFILE_SET=range` also keeps the older higher-overhead M2L `4/12`
candidate and one lower-overhead reverse-link candidate for comparison. For
operator-defined experiments, set `PROFILE_FILE` to a pipe-delimited list:

```text
name|description|m2l_k|m2l_n|l2m_k|l2m_n|m2l_mcs|l2m_mcs|interval_sec|m2l_min_pct|l2m_min_pct
```

The matrix runner separates `short_smoke_pass` from `accepted`. A run is only
`accepted` when it uses at least `MATRIX_SUSTAINED_PAYLOADS` expected payloads
(default `200`), the wrapped smoke passes, no decrypt failures occur, and TX
reports no dropped datagrams or failed submissions.

Use `scripts/summarize-radio-run-evidence.py` on a single duplex smoke run or a
profile-matrix output directory when payload counts miss strict acceptance. It
reads the collected `summary.json`, peer counters, and source-timing evidence,
then prints per-direction recovery, missing-sequence clusters, source lateness,
TX failures/drops, signal summary, and a source-vs-RF-vs-decrypt assessment
without claiming the radio device or transmitting RF.

May 5, 2026 local-Mac poor-SNR evidence adds one stricter smoke tuple for the
current adapter placement. M2L `4/12` MCS1 plus L2M `3/12` MCS2 passed the
200-payload gate at `/tmp/wfb-radio-run-duplex-local-m2l4-l2m3-settle-20260505-101147`
but missed `4/1000` L2M payloads in the sustained rerun. Lowering only L2M to
MCS1 fixed L2M but missed `2/1000` M2L payloads. The accepted sustained tuple is
symmetric `3/12` MCS1, 20 ms source pacing, 100 unmeasured warmup payloads, and
`SESSION_ACQUIRE_SETTLE_SECONDS=1`; it passed at
`/tmp/wfb-radio-run-duplex-local-sym3-mcs1-settle-1000-20260505-101900` with
`1000/1000` recovered in both directions, zero decrypt failures, zero TX
drops/failures, and average Mac RX SNR around 13 dB. Treat this as a local
poor-SNR smoke profile, not a long-distance acceptance.

Remote hardware currently requires the hardware Mac to reach the Linux peer
over SSH and UDP, and `LINUX_LAN_IP` must be the peer address reachable from
that hardware Mac. In the current remote-Mac topology, the Linux peer is
`10.42.0.1` on `wlan1`; using `192.168.122.77` caused Linux-to-Mac forwarding
to report `0/80` despite a working RF direction. Set `LINUX_LAN_IP=auto` when
the peer has multiple addresses and the correct Mac-visible source address
should be resolved from the Linux route to `MAC_LAN_IP`. The auto path was
validated at
`/tmp/wfb-radio-profile-matrix-remote-asym-smoke-auto-20260504-190319`: it
resolved `auto` to `10.42.0.1`, recovered `80/80` both directions, and logged
zero decrypt failures.

On May 4, 2026, after local reachability was restored, the remote matrix first
accepted the asymmetric short-range sustained profile only after increasing
unmeasured source warmup to 100 payloads:

- Short matrix with `LINUX_LAN_IP=10.42.0.1` and 20 warmup:
  `/tmp/wfb-radio-profile-matrix-remote-short-lanfix-20260504-184012`.
  Baseline `8/12` recovered M2L `80/80` and L2M `64/80`; symmetric `4/12`
  recovered M2L `80/80` and L2M `52/80` with 73 reverse-link decrypt failures;
  asymmetric M2L `4/12` MCS1 plus L2M `3/12` MCS2 recovered `80/80` and
  `79/80` with zero decrypt failures.
- Sustained 200-payload asymmetric run with 20 warmup passed once at
  `/tmp/wfb-radio-profile-matrix-remote-asym-200-20260504-184452`, then failed
  one of two repeats at
  `/tmp/wfb-radio-profile-matrix-remote-asym-200-repeat2-20260504-184700`.
  The failed repeat recovered L2M `172/200` with missing measured sequences
  `0..27`, matching startup acquisition rather than steady-state loss.
- Sustained 200-payload M2L `4/12` plus L2M `3/12` run with 100 warmup passed two of two
  repeats at
  `/tmp/wfb-radio-profile-matrix-remote-asym-200-warm100-repeat2-20260504-185114`:
  both directions recovered `200/200`, decrypt failures were zero, TX drops and
  failed submissions were zero, and Mac-side average SNR was about `11 dB`.

The longer 1000-payload gate showed that M2L `4/12` was still not a production
default: `/tmp/wfb-radio-profile-matrix-remote-asym-1000-repeat3-20260504-221757`
accepted two of three repeats, but one repeat logged 132 L2M decrypt failures.
Direction isolation then showed L2M by itself was clean:
`/tmp/wfb-radio-profile-matrix-remote-l2m-only-1000-repeat2-20260504-222709`
accepted MCS0, MCS1, and MCS2 L2M-only profiles with zero decrypt failures.
That points to bidirectional interaction/load, not a simple reverse-link MCS2
limit.

The current short-range production smoke candidate is M2L `5/12` MCS1 plus L2M
`3/12` MCS2 at 20 ms:

- M2L `8/12` reduced Mac TX airtime but failed every 1000-payload repeat on
  M2L recovery:
  `/tmp/wfb-radio-profile-matrix-remote-duplex-m2l8-l2m3-1000-repeat3-20260504-224434`.
- M2L `6/12` passed one repeat but failed another with 465 L2M decrypt
  failures:
  `/tmp/wfb-radio-profile-matrix-remote-duplex-midfec-1000-repeat2-20260504-225255`.
- M2L `5/12` accepted both repeats in that same matrix, then accepted one
  additional 1000-payload repeat at
  `/tmp/wfb-radio-profile-matrix-remote-duplex-m2l5-l2m3-1000-extra-20260504-230344`.
Across those three M2L `5/12` repeats, decrypt failures were zero, TX drops
and failed submissions were zero, M2L recovery was `1000/1000`, `1000/1000`,
and `988/1000`, and L2M recovery was `984/1000`, `999/1000`, and `997/1000`.
The stronger 2000-payload gate also passed two of two repeats at
`/tmp/wfb-radio-profile-matrix-remote-duplex-m2l5-l2m3-2000-repeat2-20260504-232305`:
decrypt failures were zero, TX drops and failed submissions were zero, M2L
recovery was `1993/2000` and `1990/2000`, and L2M recovery was `1987/2000` and
`1978/2000`.

Runtime IQK and EFUSE-derived TX power remain receiver-gated, but the original
1000-payload A/B failures are now classified as acquisition-gate regressions
rather than proven post-session payload corruption. The older runtime-IQK run
at `/tmp/wfb-radio-profile-matrix-remote-iqk-m2l5-l2m3-1000-repeat2-20260504-231004`
and EFUSE run at
`/tmp/wfb-radio-profile-matrix-remote-efuse-m2l5-l2m3-1000-repeat2-20260504-231630`
logged decrypt failures before the receiver's first `SESSION`; post-session
decrypt failures were zero.

The duplex smoke now defaults to `SESSION_ACQUIRE_MODE=observed`: it sends
unmeasured warmup traffic, keeps sending warmup probes while waiting for each
enabled receiver to log `SESSION`, and only then starts marked payload
accounting. A corrected one-repeat A/B at
`/tmp/wfb-radio-calibration-active-sessiongate-duplex-20260505-003700` passed
all three variants with zero pre-session and post-session decrypt failures:
current-default recovered M2L `997/1000` and L2M `995/1000`, runtime IQK
recovered M2L `997/1000` and L2M `998/1000`, and EFUSE-derived recovered M2L
`994/1000` and L2M `989/1000`. Runtime IQK completed in sweep 2 with cleanup
restored and 20 selected IQC fill writes applied. Treat those experimental
profiles as `experimental-pass-needs-soak`, not production defaults, until
repeat-count and longer-distance evidence catch up.

`scripts/run-radio-run-duplex-smoke.sh` still defaults to
`SOURCE_WARMUP_PAYLOADS=100`; set `SESSION_ACQUIRE_MODE=disabled` or reduce
warmup only when deliberately testing first-acquisition behavior. These
short-range results do not overturn the earlier 100 ft result, where the older
M2L `4/12` profile failed a 200-payload acceptance gate.

## Acceptance Margins

`rf-quality-report` records the outcome margin under
`comparison.outcome.acceptance_margin`. Version `rf-quality-margin-v1` uses
receiver-backed WFB outcomes as the primary signal:

- Payload loss delta: macOS loss may exceed the Linux baseline by at most `2.0`
  percentage points for the same profile tuple.
- Expected source payloads: macOS and Linux baseline expected payload counts
  must match before the report is `baseline_comparable`; short smokes are
  `invalid_comparison` against the 2,000-payload reference baseline.
- Throughput ratio: the report records macOS-vs-Linux throughput ratio and the
  target floor is `0.85`, but the current bridge timing includes init,
  relay/orchestration delay, and TX loop time. Until the timing window is
  aligned with the Linux baseline, `throughput_evaluated` remains false and the
  ratio is informational rather than a failing margin.
- Receiver metadata: when the Linux receiver emits WFB-ng `RX_ANT` lines, the
  runner preserves RSSI/SNR/MCS/bandwidth telemetry and the report marks
  receiver metadata as `available`. Outdoor promotion now requires the
  close-range RX_ANT frequency/MCS/bandwidth tuple to match the profile. RSSI
  and SNR are surfaced in `macos.wfb_outcome.receiver_signal`. The signal
  summary is `complete` when tuple/RSSI/nonzero-SNR evidence is present,
  `usable` when tuple/RSSI are present but SNR is all-zero or missing, and
  `degraded` when tuple or RSSI evidence is malformed. Zero-only SNR is also
  labeled with `snr_confidence=receiver_reported_zero_only` and
  `snr_usable=false`, so release checks can avoid treating it as a real 0 dB
  measurement. Outdoor promotion rejects `degraded`; RSSI/SNR values remain
  diagnostic field-note inputs rather than scored pass/fail margins.
- RF pcap channel evidence: when `pcap_channel_evidence` is present, the
  production margin rejects `off_channel_frames` and
  `requested_frequency_absent`. A passing close-range gate for range promotion
  should show `verified` so NetworkManager scan drift or a mistuned receiver
  cannot masquerade as clean RF recovery.

If the profile parameters match Linux but the payload or throughput margin is
outside this envelope, the RF-quality report marks acceptance as a degraded
comparison. This is the signal to inspect TXAGC, RFE state, calibration probes,
antenna/path placement, and receiver artifacts before increasing power.

## Rejection Cases

Do not classify a run as range-ready when any of the following are true:

- The report only proves USB bulk submission or descriptor construction.
- The same channel/rate/bandwidth/power/calibration tuple does not have a
  passing close-range report.
- The close-range gate report contains
  `macos.calibration.runtime_iqk_summary.risk` and it is not `completed`.
  For runtime IQK, `completed` means the sweep completed, cleanup restored,
  no TX/RX path used fallback, and selected IQC fill applied after cleanup.
- The close-range gate lacks RX_ANT receiver telemetry, or the RX_ANT
  frequency, MCS index, or bandwidth differs from the outdoor profile.
- The close-range gate includes `macos.wfb_outcome.receiver_signal.status` and
  it is `degraded`.
- The close-range gate includes `macos.wfb_outcome.channel_state` and its
  verification failed or observed frequency/width differs from the profile.
- The close-range gate includes
  `macos.wfb_outcome.receiver_evidence.pcap_channel_evidence.status` and it is
  `off_channel_frames` or `requested_frequency_absent`.
- The Linux baseline differs in channel, bandwidth, fixed rate/profile, WFB
  link/radio port, FEC, payload length, expected source payload count, antenna
  setup, or adapter class.
- The run uses HT40/VHT80 without separate evidence that the actual transmitted
  and decoded PPDU is wider than 20 MHz.
- Stop-gap calibration is still active and the stepped or outdoor result falls
  outside the Linux baseline margin.
- `rtl8812a-runtime-iqk` was selected but the Mac report shows
  `tx_calibration_profile.runtime_iqk.cleanup_status != "restored"` or any
  per-path TX/RX stage used fallback unexpectedly, or
  `tx_calibration_profile.runtime_iqk.selected_iqc_fill_applied != true`.
  Check `runtime_iqk.sweep_summaries[]`, `runtime_iqk_summary.sweep_count`, and
  `runtime_iqk_summary.selected_iqc_fill_register_count` before
  deciding whether the failure is a one-off sweep or repeatable calibration
  instability.

## Wide-Bandwidth Evidence

HT40 and VHT80 profiles are experimental until a report includes actual
wide-PPDU evidence. Tuning both radios to an HT40 or VHT80 channel context is
not enough: the current proven WFB-ng path can still transmit and decode 20 MHz
PPDUs while the channel context is wider.

`rf-quality-report` separates these fields:

- `profile.bandwidth_mhz`: the selected channel-context bandwidth.
- `bandwidth_evidence.observed_frame_bandwidth_mhz`: the frame or PPDU
  bandwidth reported by an evidence source, when available.
- `bandwidth_evidence.status`: whether the evidence matches the context,
  shows a narrower observed PPDU, is wider than the context, or is not supplied.
- `bandwidth_evidence.source` and `bandwidth_evidence.artifacts`: where the
  observation came from.

Use these options when recording wide-mode evidence:

```sh
--bandwidth 40 \
--observed-ppdu-bandwidth 20 \
--observed-ppdu-source "linux monitor radiotap plus Mac RX descriptor JSONL" \
--observed-ppdu-artifact /tmp/wfb-agent-rxmeta40a.jsonl \
--observed-ppdu-artifact /tmp/mac-stock40stablea-rf.pcap
```

Evidence sources, in order of preference:

- Linux receiver metadata or monitor radiotap that reports frame bandwidth for
  the decoded WFB frames.
- Mac RTL8812AU RX descriptor JSONL from `rx-scan --frame-jsonl` when the Mac
  is receiving the peer's frames.
- SDR or spectrum capture that shows occupied bandwidth during the same WFB
  payload window.

The current HT40 artifacts remain classified as channel-context WFB flow, not
proven 40 MHz PPDU operation, because both Linux monitor metadata and Mac RX
descriptors reported the WFB MCS1 frames as 20 MHz.

### Controlled HT40 Evidence Classification

Classified on May 2, 2026:

- RF-quality evidence report: `/tmp/wfb-rfq-ht40-context-evidence.json`.
- Mac HT40 TX artifact: `/tmp/wfb-agent-default40a-listen.json`.
- Mac RX descriptor artifacts: `/tmp/wfb-agent-rxmeta40a.json` and
  `/tmp/wfb-agent-rxmeta40a.jsonl`.
- Channel context: channel 36, HT40, `profile.bandwidth_mhz=40`.
- Observed frame/PPDU bandwidth: `20` MHz from Linux monitor metadata plus Mac
  RTL8812AU RX descriptor JSONL.
- Classification:
  `bandwidth_evidence.status=context_only_narrower_observed`.
- Result: WFB flow in an HT40 channel context is verified, but this is not
  proven 40 MHz PPDU operation and must not be used for HT40 range claims.
