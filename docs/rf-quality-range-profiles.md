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
- Current preferred Mac power mode: `efuse-derived` with
  `linux-ch36-ht20` safety clamp.
- Current accepted calibration label: `stop-gap-captured`. The guarded
  `rtl8812a-runtime-iqk` profile is available for experimental A/B runs, but it
  is not a long-distance accepted calibration mode until it has a passing
  receiver-backed close-range comparison and stepped or outdoor evidence.
- At least 120 source payloads for quick checks; use 2,000 source payloads for
  an accepted reference.
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
  1 with cleanup restored. This is the current hardened close-range gate for
  the runtime-owned profile executor.
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

Use `docs/rf-quality-field-notes-template.md` for the companion note artifact.
The structured fields in the report are the summary; the companion artifact is
where longer notes, maps, photos, service-restore output, and spectrum evidence
belong.

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
- The close-range gate lacks RX_ANT receiver telemetry, or the RX_ANT
  frequency, MCS index, or bandwidth differs from the outdoor profile.
- The close-range gate includes `macos.wfb_outcome.receiver_signal.status` and
  it is `degraded`.
- The close-range gate includes `macos.wfb_outcome.channel_state` and its
  verification failed or observed frequency/width differs from the profile.
- The Linux baseline differs in channel, bandwidth, fixed rate/profile, WFB
  link/radio port, FEC, payload length, expected source payload count, antenna
  setup, or adapter class.
- The run uses HT40/VHT80 without separate evidence that the actual transmitted
  and decoded PPDU is wider than 20 MHz.
- Stop-gap calibration is still active and the stepped or outdoor result falls
  outside the Linux baseline margin.
- `rtl8812a-runtime-iqk` was selected but the Mac report shows
  `tx_calibration_profile.runtime_iqk.cleanup_status != "restored"` or any
  per-path TX/RX stage used fallback unexpectedly. Check
  `runtime_iqk.sweep_summaries[]` and `runtime_iqk_summary.sweep_count` before
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
