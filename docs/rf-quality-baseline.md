# RF Quality Baseline

This procedure captures the Linux-side evidence needed before calling a macOS run range-ready. The baseline is receiver-backed: the important output is not USB submission, but recovered WFB payloads, receiver artifacts, channel parameters, and command lines that can be compared to a macOS RF-quality report.

## Fixed 20 MHz Profile

Use this profile first for close-range sanity and long-distance comparison:

- Adapter class: RTL8812AU/AWUS036ACH-class radio on the Linux peer.
- Channel: 36, 20 MHz.
- TX rate/profile: WFB HT MCS1 traffic using the Linux monitor-injection descriptor shape.
- Key: `/var/lib/arc/wfb/drone.key` on the current bench.
- Primary video-like port: WFB radio port `0`, FEC `k=8`, `n=12`, UDP source `5600`.
- Telemetry-like lower-rate ports on the current bench: ports `1`, `3`, and RX ports `2`, `4`.
- Payload size for the accepted close-range reference: exactly 1,000 byte source payloads, 2,000 expected decoded source payloads.

Current Linux service commands observed on `drone-2f389.local`:

```sh
wfb_tx -K /var/lib/arc/wfb/drone.key -p 0 -B 20 -k 8 -n 12 -u 5600 wfb0
wfb_tx -K /var/lib/arc/wfb/drone.key -p 1 -B 20 -k 1 -n 3 -u 14660 wfb0
wfb_rx -K /var/lib/arc/wfb/drone.key -p 2 -c 127.0.0.1 -u 14661 wfb0
wfb_tx -K /var/lib/arc/wfb/drone.key -p 3 -B 20 -k 2 -n 4 -u 5601 wfb0
wfb_rx -K /var/lib/arc/wfb/drone.key -p 4 -c 127.0.0.1 -u 5701 wfb0
```

For a controlled single-port baseline, stop or isolate unrelated WFB services, pin both radios to the same channel/bandwidth, and run only the port under test. Record the exact commands in the baseline JSON.

## Capture Helper

`scripts/capture-linux-baseline.sh` collects the portable part of a Linux baseline:

```sh
OUT_DIR=/tmp/wfb-rf-baseline-$(date +%Y%m%d-%H%M%S) \
IFACE=wfb0 \
CHANNEL=36 \
BANDWIDTH_MHZ=20 \
LINK_ID=0x000001 \
RADIO_PORT=0x00 \
FEC_K=8 \
FEC_N=12 \
PAYLOAD_LEN=1000 \
SOURCE_PAYLOADS=2000 \
RECOVERED_PAYLOADS=1999 \
SUBMITTED_DATAGRAMS=3000 \
THROUGHPUT_MBPS=0.787644 \
TX_RATE=mcs1 \
TX_PROFILE=linux-monitor \
WFB_TX_CMD='wfb_tx -K /var/lib/arc/wfb/drone.key -p 0 -B 20 -k 8 -n 12 -u 5600 wfb0' \
WFB_RX_CMD='wfb_rx -K /var/lib/arc/wfb/drone.key -p 0 -c 127.0.0.1 -u 5800 wfb0' \
./scripts/capture-linux-baseline.sh
```

The helper writes `linux-baseline.json` plus command, process, link, and optional packet-capture artifacts. It does not assume `iw`, `tcpdump`, `lsusb`, or `docker` exist; missing tools are recorded as missing artifacts rather than failing the run.

## Close-Range Automation Hardening

`scripts/run-rf-quality-close-range.sh` controls the hardware Mac and the Linux
peer for the accepted close-range workflow. The Linux side now performs a
preflight before RF transmission and collects:

- `${REMOTE_PREFIX}-bridge-ready.json`
- `${REMOTE_PREFIX}-preflight.json`
- `${REMOTE_PREFIX}-preflight.log`
- `${REMOTE_PREFIX}-channel-state.json`
- `${REMOTE_PREFIX}-setup.log`
- `${REMOTE_PREFIX}-restore.json`
- `${REMOTE_PREFIX}-restore.log`
- `${REMOTE_PREFIX}-summary.json`

The preflight searches `LINUX_REMOTE_PATH` in addition to the remote shell
path. Default:

```sh
LINUX_REMOTE_PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
```

Required commands are `python3`, `sudo`, `timeout`, `wfb_rx`, and `wfb_tx`.
Missing required commands block the run before RF. The runner also blocks
before RF if non-interactive sudo is unavailable, the configured WFB key is not
readable through sudo, or `ip` confirms that the configured interface is
missing. `docker`, `iw`, `ip`, `tcpdump`, `pkill`, `ps`, `grep`, and `date` are
optional by default and are recorded as degraded preflight state when absent.
Set `LINUX_REQUIRE_IW=1` to fail before RF if the runner cannot set and verify
the Linux interface channel. The preflight JSON records `sudo_noninteractive`,
`iface_status`, `wfb_key_status`, and `docker_service_state` so failed field
runs can distinguish missing peer prerequisites from RF loss.
By default the runner also requires Linux peer isolation before measured
traffic starts:

```sh
LINUX_REQUIRE_PEER_ISOLATION=1
LINUX_PEER_SETTLE_SECONDS=2
```

With isolation required, `ps` and `grep` become required evidence tools. The
setup phase records `arc-wfb-link`, `wfb_rx`, and `wfb_tx` process matches
before service shutdown, waits for the settle interval after Docker stop and
stale-process cleanup, then fails before RF if any of those processes remain.
The channel-state JSON embeds `peer_isolation_status`,
`peer_process_matches_before_stop`, and
`peer_process_matches_after_stop`. This prevents a competing WFB transmitter or
stale receiver from turning into misleading decrypt-error or packet-loss
evidence.
The runner keeps `LINK_ID` in report-friendly form, such as `0x000001`, but
passes decimal `WFB_CLI_LINK_ID` to Linux `wfb_tx` and `wfb_rx`. WFB-ng's CLI
does not reliably parse hex link IDs; a hex string can produce on-air
`57:42:00:00:00:00` frames while the report says link `0x000001`.
The restore JSON records the post-run service action, service state, and WFB
process matches after the controlled sender/receiver processes are stopped, so
cleanup failures are visible in machine-readable run evidence.
The Mac ready marker is written after adapter init and calibration setup,
immediately before the receive loop. The default Mac command remains
`bridge-tx-listen`, and `MAC_RADIO_COMMAND=radio-run` can now opt into the
production command path while preserving the same ready-marker wait and
datagram evidence flow. `radio-run` now carries RF-quality TX-power arguments
through the production adapter, so `TX_POWER_MODE=efuse-derived` uses
runtime-owned EFUSE TXAGC planning and live register writes for the production
command path. The runner waits up to `BRIDGE_READY_WAIT_SECONDS` for that
marker before starting Linux traffic, which avoids classifying startup races as
RF loss.
The Linux peer also writes `${REMOTE_PREFIX}-channel-state.json` after the
controlled channel-set step. It records whether `iw` and sudo were available,
the requested channel/bandwidth, the observed `iw dev IFACE info` channel and
width when available, and `verify_status` (`verified`, `mismatch`,
`set_unverified`, `set_failed`, or `skipped`). The same object is embedded in
`datagram-evidence.json` and `${REMOTE_PREFIX}-summary.json` so missing `iw` or
channel drift is visible without scraping `setup.log`.
The runner also writes local `pcap-channel-evidence.json` from the copied Linux
RF pcap. It counts radiotap frequency tags and is embedded in
`datagram-evidence.json` as `pcap_channel_evidence`. `verified` means all
frequency-tagged frames stayed on the requested channel; `off_channel_frames`
or `requested_frequency_absent` makes the RF-quality outcome fail the
production margin even when payload recovery looks good.
`rf-quality-report` lifts that object to `macos.wfb_outcome.channel_state`.
New-format outdoor gates reject a close-range report when
`channel_state.verify_status` is present and not `verified`, or when observed
frequency/width differs from the promoted profile.
The same report path treats required peer isolation as part of the production
margin: if `peer_isolation_required=true` and `peer_isolation_status` is not
`ok`, the report marks the receiver-backed outcome outside margin instead of
accepting the payload result as clean RF evidence.
Validation smoke:
`/tmp/wfb-rfq-channel-state-smoke-a1/rf-quality-report.json` recovered
`80/80`, submitted `120/120`, collected an empty `missing-artifacts.txt`, and
recorded `channel_state.verify_status=verified` for channel 36 / 20 MHz. The
restore evidence also captured post-restore `iw` state showing the normal Linux
service moved `wfb0` back to channel 161 / 20 MHz. Because this was an
80-payload smoke against the 2,000-payload Linux reference, it is orchestration
evidence only; regenerated reports classify that payload-count mismatch as an
invalid baseline comparison
(`/tmp/wfb-rfq-channel-state-smoke-a1/rf-quality-report-channel-state-top.json`).

The current hardened automation evidence is
`/tmp/wfb-rfq-prod-ready-marker-default-a1/rf-quality-report.json` from May 2,
2026. It passed as `baseline_comparable` / `matched` / `within_margin`, with
`3000/3000` bridge submissions, `1988/2000` Linux receiver payloads recovered,
zero decrypt failures, tuple-consistent `RX_ANT` telemetry at `5180/MCS1/20`,
bridge-ready evidence before RF traffic, restore JSON, and an empty
`missing-artifacts.txt`.
After moving the Mac adapter to the local Mac and hardening the peer against
NetworkManager scan drift, the May 4, 2026 local direct channel-36 EFUSE run
`/tmp/wfb-rfq-local-direct-ch36-peerhard-efuse-full-a1/rf-quality-report.json`
recovered `1993/2000`, logged zero decrypt failures, verified monitor-mode
channel 36 / 20 MHz, and passed as
`baseline_comparable` / `matched` / `within_margin`.
The pcap-channel-evidence smoke
`/tmp/wfb-rfq-local-direct-ch36-pcap-evidence-smoke-a1/rf-quality-report.json`
recovered `80/80` and recorded
`pcap_channel_evidence.status=verified`, with all `186` frequency-tagged RF
pcap frames on `5180 MHz`.
After fixing Linux WFB-ng link-ID CLI conversion, the patched local smoke
`/tmp/wfb-rfq-local-direct-link-decimal-smoke-a1/rf-quality-report.json`
again recovered `80/80` with zero decrypt failures, recorded
`receiver_evidence.link_id=0x000001`, `wfb_cli_link_id=1`, verified channel 36,
and kept pcap channel evidence `verified` with all `208` frequency-tagged
frames on `5180 MHz`.
New-format reports classify all-zero WFB-ng SNR as `receiver_signal.status=usable`
rather than `complete` when the RX_ANT tuple and RSSI evidence are otherwise
valid. The current latest-schema runtime-IQK reference is
`/tmp/wfb-rfq-runtime-iqk-prod-gate-a1/rf-quality-report.json`: it passed as
`baseline_comparable` / `matched` / `within_margin`, recovered `1978/2000`,
submitted `3000/3000`, completed runtime IQK by sweep 3 with cleanup restored,
recorded `channel_state.verify_status=verified`, and restored the Linux service
after the run.

The automation also accepts the targeted calibration profile:

```sh
TX_CALIBRATION_PROFILE=linux-parity-ch36-ht20 \
CALIBRATION_MODE=targeted-linux-parity \
./scripts/run-rf-quality-close-range.sh
```

For opt-in runtime LCK testing:

```sh
TX_CALIBRATION_PROFILE=rtl8812a-lck \
./scripts/run-rf-quality-close-range.sh
```

For read-only IQK staging evidence:

```sh
TX_CALIBRATION_PROFILE=rtl8812a-iqk-probe \
./scripts/run-rf-quality-close-range.sh
```

For guarded runtime IQK testing:

```sh
TX_CALIBRATION_PROFILE=rtl8812a-runtime-iqk \
./scripts/run-rf-quality-close-range.sh
```

When `CALIBRATION_MODE` is omitted, the script derives
`targeted-linux-parity` for `TX_CALIBRATION_PROFILE=linux-parity-ch36-ht20`
and `runtime-approximation` for `TX_CALIBRATION_PROFILE=rtl8812a-lck` or
`TX_CALIBRATION_PROFILE=rtl8812a-runtime-iqk`.
`rtl8812a-iqk-probe` remains `stop-gap-captured` because it is a marker for
safe IQK readback already captured by the bridge report; it does not perform
runtime IQK or additional profile-time hardware reads. The targeted Linux
parity, LCK, and runtime IQK profiles add
`--i-understand-this-writes-registers` to the hardware-Mac bridge command
because they apply live register writes before RF traffic.

May 2, 2026 hardware evidence for `TX_CALIBRATION_PROFILE=rtl8812a-runtime-iqk`
lives at `/tmp/wfb-rfq-runtime-iqk-a2/rf-quality-report.json` and
`/tmp/wfb-rfq-runtime-iqk-a3/rf-quality-report.json`. Both are
baseline-comparable and within margin at close range, but the calibration
reports still say `runtime_iqk.status=fallback_applied` because path-A RX IQK
uses fallback IQC in the full receiver-backed runs. A one-frame smoke at
`/tmp/wfb-rtl8812a-runtime-iqk-iqc-readback.json` did complete both RX paths and
now records final RX IQC latch readbacks. Do not promote this to a production
or range-ready profile until path-A RX stability is resolved or deliberately
accepted with stronger range evidence.

After fixing IQK candidate selection to use signed 11-bit comparison like the
Linux driver, `/tmp/wfb-rfq-runtime-iqk-signed-a1/rf-quality-report.json`
completed both TX/RX IQK paths in the sustained receiver-backed flow, recovered
`1964/2000` marked payloads, and remained
`baseline_comparable`/`within_margin`. Keep the profile experimental for range
work until it has stepped or outdoor evidence, but this run establishes the
signed-selection path as close-range safe.

After matching the RX IQK retry loop to Linux by re-triggering every TX-ready
path on each RX retry,
`/tmp/wfb-rfq-runtime-iqk-peer-trigger-full-a1/rf-quality-report.json`
completed runtime IQK by sweep 2, restored cleanup state, recovered
`2000/2000`, submitted `3000/3000`, and stayed
`baseline_comparable`/`within_margin` with zero decrypt failures. This
supersedes the earlier fallback-applied runtime-IQK artifacts for close-range
gating; stepped or outdoor distance evidence is still required before making
runtime IQK the default long-distance profile.
The latest-schema rerun at
`/tmp/wfb-rfq-runtime-iqk-prod-gate-a1/rf-quality-report.json` completed by
sweep 3, recovered `1978/2000`, and adds first-class channel-state evidence.
It replaces the older runtime-IQK artifacts as the best close-range gate shape,
while preserving the same distance-evidence requirement.

After moving full runtime-IQK sweep orchestration into `wfb-radio-runtime`, the
post-extraction close-range pair on May 4, 2026 stayed within the same gate:
current-default
`/tmp/wfb-rfq-runtime-iqk-extraction-post-sweep-default-a1/rf-quality-report.json`
recovered `1996/2000`, and runtime-IQK
`/tmp/wfb-rfq-runtime-iqk-extraction-post-sweep-runtime-iqk-a1/rf-quality-report.json`
recovered `1991/2000`. Both submitted `3000/3000`, logged zero decrypt
failures, verified channel 36 / 20 MHz, and passed as
`baseline_comparable` / `matched` / `within_margin`. The runtime-IQK run
completed in sweep 2 with cleanup restored and no per-path fallback.

After moving TX calibration profile execution itself behind the runtime API,
`/tmp/wfb-rfq-runtime-cal-profile-api-runtime-iqk-a2/rf-quality-report.json`
validated the same `rtl8812a-runtime-iqk` profile path with the diagnostic
command reduced to report adaptation. That no-warmup run recovered
`1984/2000`, logged zero decrypt failures, completed runtime IQK in sweep 1,
and passed within margin. Two other no-warmup runtime-IQK profile runs are
rejected evidence: `/tmp/wfb-rfq-runtime-cal-profile-api-runtime-iqk-a1` logged
`2191` decrypt failures and
`/tmp/wfb-rfq-runtime-cal-profile-api-runtime-iqk-peeriso-a1` logged `2142`
decrypt failures even though peer isolation was clean. The failure pattern is
WFB session acquisition, not residual peer traffic.

The accepted hardened profile-executor artifact is
`/tmp/wfb-rfq-runtime-iqk-peeriso-warmup-a1/rf-quality-report.json`. It used
`SOURCE_WARMUP_PAYLOADS=400`, recovered `1993/2000` marked payloads, logged
zero decrypt failures, verified channel 36 / 20 MHz, recorded
`peer_isolation_status=ok`, completed runtime IQK in sweep 1 with cleanup
restored, and passed as `baseline_comparable` / `matched` / `within_margin`.

The peer-isolation smoke after this hardening is
`/tmp/wfb-rfq-peer-isolation-smoke-a1/rf-quality-report.json`. It is an
80-payload orchestration smoke, not a baseline-comparable reference, but it
verified the new evidence fields against the real peer: six running WFB service
processes were recorded before stop, zero remained after the settle interval,
`peer_isolation_status=ok`, channel 36 / 20 MHz was verified, and the receiver
recovered `80/80` with zero decrypt failures.

`rf-quality-report` also emits
`macos.calibration.runtime_iqk_summary` whenever a runtime IQK profile report is
present. Use `risk`, `completed`, `cleanup_restored`, `sweep_count`,
`fallback_stage_count`, `fallback_stages[]`, `selected_iqc_fill_applied`, and
`selected_iqc_fill_register_count` as the compact machine-readable calibration
health signal for release gating and field notes. Outdoor profile gating
rejects a close-range gate artifact with
`runtime_iqk_summary.risk` other than `completed`.

Short FEC smoke runs can emit one fewer WFB datagram than the theoretical
`ceil(expected_payloads * fec_n / fec_k)` count while still recovering every
source payload. The script now records this as
`datagram-evidence.json` and attaches it as a receiver artifact. Tune with:

```sh
DATAGRAM_SHORTFALL_TOLERANCE=1
```

This does not make a failed bridge artifact disappear; it preserves the
expected-versus-observed datagram evidence so the RF-quality envelope can still
be interpreted when receiver recovery is complete.
The same artifact records both `link_id` and `wfb_cli_link_id` so future runs
can confirm the reported WFB tuple and Linux CLI tuple stayed aligned.

For session acquisition, the runner sends unmeasured source payloads before the
marked payload sequence. The current default is:

```sh
SOURCE_WARMUP_PAYLOADS=400
```

Warmup payloads use the same WFB TX path but do not carry `PAYLOAD_MARKER`, so
the receiver counter does not count them as recovered test payloads. The runner
raises `MAX_DATAGRAMS` by the warmup FEC estimate and records
`source_warmup_payloads`, `theoretical_warmup_datagrams`, and
`theoretical_total_datagrams` in `datagram-evidence.json`.
Set `SOURCE_WARMUP_PAYLOADS=0` only when deliberately testing first-session
acquisition. Runtime-IQK no-warmup runs have shown decrypt-heavy startup
failures even with clean peer isolation; the warmup default keeps the measured
payload window focused on steady-state RF behavior.

The runner also records `${REMOTE_PREFIX}-receiver-health.json` and lifts the
same health into `datagram-evidence.json`. `rf-quality-report` exposes this
under `macos.wfb_outcome` as:

- `receiver_status`
- `receiver_session_observed`
- `receiver_unable_decrypt_count`
- `receiver_total_datagrams`
- `receiver_evidence`

Any nonzero `receiver_unable_decrypt_count` now marks the receiver-backed
outcome outside the production acceptance margin. With peer isolation and
warmup enabled, decrypt errors are treated as evidence of session acquisition
failure or corrupt WFB frames, not as acceptable close-range noise.

When `wfb_rx` emits `RX_ANT` lines, the runner also parses them into
`receiver_evidence.receiver_health.rx_antenna_reports` and
`rx_antenna_summary`. Each report records frequency, MCS index, bandwidth,
antenna id, packet count, RSSI min/avg/max, and SNR min/avg/max from the
Linux receiver log. `rf-quality-report` also exposes the compact copy at
`macos.wfb_outcome.receiver_telemetry` so release tooling can read MCS/RSSI/SNR
health without parsing the full receiver artifact. It also publishes
`macos.wfb_outcome.receiver_signal`, a typed summary with antenna count, unique
RX_ANT tuple count, tuple consistency, RSSI average min/max/spread, SNR average
sample/nonzero counts, `status`, `issues[]`, `snr_status`, `snr_confidence`,
and `snr_usable` for automated range-readiness checks. `status=complete` means
tuple, RSSI, and nonzero SNR evidence are present; `status=usable` means tuple
and RSSI are usable but the receiver only reported zero or missing SNR. In that
case `snr_usable=false` keeps the zero-only SNR from being treated as a measured
0 dB margin. `status=degraded` means the tuple or RSSI evidence is not
trustworthy enough for outdoor promotion.
Outdoor profile gating now rejects a close-range gate that lacks this RX_ANT
receiver telemetry or whose RX_ANT frequency/MCS/bandwidth tuple differs from
the outdoor profile. If a new-format gate includes
`receiver_signal.status=degraded`, outdoor promotion is rejected even when raw
RX_ANT rows exist. Long-distance promotion needs RF health evidence in addition
to payload recovery, and it has to prove that the receiver observed the same RF
tuple being promoted.

This matters because WFB can receive strong RF frames but recover zero payloads
when the receiver misses the session frame. That condition now appears as
`receiver_status = "missing_session"` instead of looking like a generic RF or
TX-power failure. The runner passes `LINK_ID` through to both `wfb_tx -i` and
`wfb_rx -i`, so the WFB tuple in the report matches the Linux peer commands.

## Current Close-Range Baseline

The current close-range 20 MHz reference is the sustained 1,000-byte run from May 1, 2026:

- macOS bridge artifact: `/tmp/wfb-agent-sust1000-listen.json`
- RF capture: `/tmp/mac-sust1000-rf.pcap`
- receiver-side capture: `/tmp/mac-sust1000-rx-lo.pcap`
- Linux TX log: `/tmp/wfb-tx-sust1000.log` on `drone-2f389.local`
- Submitted distributor datagrams: 3,000
- Recovered decoded source payloads: 1,999 of 2,000
- macOS bridge datagram throughput: about `0.787644` Mbps

The repo fixture `fixtures/rf-quality/current-close-range-20mhz-linux-baseline.json` stores these artifact paths and parameters in the format accepted by `rf-quality-report`.

EFUSE-derived TXAGC testing is opt-in. For channel 36 HT20 comparisons against
the current Linux baseline, run Mac TX commands with:

```bash
--tx-power-mode efuse-derived \
--tx-power-efuse-report /tmp/wfb-live-efuse-dump.json \
--tx-power-safety-profile linux-ch36-ht20
```

The mode computes per-path/per-rate TXAGC values from the decoded EFUSE
TX-power region, applies the Linux channel-36 HT20 safety caps, writes the
TXAGC registers before TX, and records the full lane-by-lane calculation in the
Mac report. See `docs/rtl8812au-efuse-tx-power.md`.

Build an RF-quality envelope from the current artifacts:

```sh
cargo run -p wfb-radio-diag -- --json \
  --report /tmp/wfb-rf-quality-close-range-20mhz.json \
  rf-quality-report \
  --profile-name current-close-range-20mhz \
  --profile-kind close-range \
  --channel 36 --bandwidth 20 \
  --tx-rate mcs1 \
  --tx-profile linux-monitor \
  --tx-power-mode current-default \
  --calibration-mode stop-gap-captured \
  --wfb-link-id 0x000001 \
  --wfb-radio-port 0x00 \
  --fec-k 8 --fec-n 12 \
  --payload-len 1000 \
  --expected-payloads 2000 \
  --recovered-payloads 1999 \
  --mac-report /tmp/wfb-agent-sust1000-listen.json \
  --linux-baseline fixtures/rf-quality/current-close-range-20mhz-linux-baseline.json \
  --receiver-artifact /tmp/mac-sust1000-rf.pcap \
  --receiver-artifact /tmp/mac-sust1000-rx-lo.pcap
```

The comparison is valid only when channel, bandwidth, TX rate/profile, WFB link/radio port, FEC settings, and payload length match. If any of those fields differ, the report still emits successfully but marks the comparison invalid or degraded.

May 2, 2026 hardened close-range sustained reruns used the same Linux baseline
and the updated receiver-session handling:

- `current-default`: `/tmp/wfb-rfq-rtl8812a-current-default-sustained-hardened`,
  `1973/2000` recovered, zero decrypt failures, `within_margin`.
- `rtl8812a-iqk-probe`: `/tmp/wfb-rfq-rtl8812a-iqk-marker-sustained-hardened`,
  `1980/2000` recovered, zero decrypt failures, `within_margin`.
- `rtl8812a-lck`: `/tmp/wfb-rfq-rtl8812a-lck-sustained-hardened`,
  `1970/2000` recovered, zero decrypt failures, `within_margin`.

These are software sanity checks for the current bench geometry, not distance
acceptance. They are useful before changing calibration code because all three
profiles submitted `3000/3000` datagrams, observed the WFB session, and stayed
inside the configured Linux payload-loss margin.

Before starting the runtime calibration-extraction slice, the May 2 tuple above
was rerun against the current runtime-flow/radio-run cutover code on May 4,
2026:

- `current-default`: `/tmp/wfb-rfq-runtime-cutover-current-default-a2`,
  `1989/2000` recovered, zero decrypt failures, `within_margin`.
- `rtl8812a-iqk-probe`: `/tmp/wfb-rfq-runtime-cutover-iqk-marker-a1`,
  `1988/2000` recovered, zero decrypt failures, `within_margin`.
- `rtl8812a-lck`: `/tmp/wfb-rfq-runtime-cutover-lck-a1`,
  `1992/2000` recovered, zero decrypt failures, `within_margin`.

The non-regression gate passed: all accepted reruns recovered at least as many
payloads as the May 2 hardened tuple under the same close-range bench geometry.
One earlier current-default run
(`/tmp/wfb-rfq-runtime-cutover-current-default-a1`) recovered only `416/2000`
with decrypt failures after missing the WFB session; the immediate short control
and full rerun recovered normally, so keep session/decrypt fields in the gate
instead of treating that artifact as RF loss.

After TX-power planning/execution moved into `wfb-radio-runtime`, a short
production-command smoke verified that `radio-run` carries
`TX_POWER_MODE=efuse-derived` through the same RF-quality harness:
`/tmp/wfb-rfq-radio-run-efuse-smoke-b1/rf-quality-report.json` recovered
`80/80` measured payloads, submitted `149/150` total WFB datagrams including
warmup within the short-run tolerance, reported zero decrypt failures, verified
channel 36 / 20 MHz, and emitted production `tx_power_control` evidence for 22
runtime-owned EFUSE-derived TXAGC writes across paths A/B. The run is not a
replacement for the 2000-payload non-regression gate; its comparison is
intentionally invalid because the payload count differs from the baseline
fixture.

After moving the AWUS036ACH to this Mac on May 4, 2026, local USBHost smoke
tests passed before running receiver-backed RF checks: the RX-only
`radio-run` smoke passed at `/tmp/wfb-local-radio-smoke-20260504-114912`
with 65 RSSI/SNR/noise-bearing frames, and the TX-positive EFUSE smoke passed
with 64 submitted datagrams, 64 USB bulk-out writes, and zero TX failures.
The RF-quality runner now supports this topology with `LOCAL_HW=1` or
`HW_MAC_HOST=local`; when the Linux receiver is only reachable from the old
rownd bench Mac, use `LINUX_SSH_JUMP=...` and `LINUX_SSH_NESTED=1`.

The local-adapter close-range RF evidence split cleanly by offered packet
rate. At the default `PAYLOAD_INTERVAL_SEC=0.0005`, full-count sustained
`radio-run` tests submitted every datagram but degraded at the receiver:
`/tmp/wfb-rfq-local-radio-run-efuse-sustained-a1` recovered `809/2000` with
2006 decrypt failures, and
`/tmp/wfb-rfq-local-radio-run-current-default-sustained-a2` recovered
`832/2000` with 2237 decrypt failures. The same topology recovered normally
when paced at `PAYLOAD_INTERVAL_SEC=0.002`:
`/tmp/wfb-rfq-local-radio-run-current-default-paced2000-a1` recovered
`1999/2000` with zero decrypt failures, and
`/tmp/wfb-rfq-local-radio-run-efuse-paced2000-a1` recovered `1990/2000`
with zero decrypt failures. Treat the default-rate failures as pacing,
backpressure, or relay-topology evidence rather than EFUSE TXAGC evidence
until a direct local drone link removes the rownd UDP relay from the path.

After the drone became directly reachable from this Mac, the close-range runner
was hardened to mark `wfb0` unmanaged in NetworkManager and force it back to
monitor mode before pinning the channel. The channel-state evidence now records
`nm_unmanage_status`, `monitor_set_status`, `observed_type`, and channel/width
verification. This was needed because an early channel-161 smoke captured a
NetworkManager-like scan burst across 2.4 GHz and 5 GHz even though the preflight
channel check had passed.

With the adapter moved from roughly 1 ft to roughly 6 ft from the Linux peer,
the direct local channel-36 check is healthy:
`/tmp/wfb-rfq-local-direct-ch36-peerhard-efuse-full-a1/rf-quality-report.json`
recovered `1993/2000`, logged zero decrypt failures, verified channel 36 / 20
MHz with `wfb0` in monitor mode, stayed `baseline_comparable` / `matched` /
`within_margin`, and reported usable receiver RSSI evidence. The paired
runtime-IQK A/B artifact
`/tmp/wfb-rfq-local-direct-ch36-peerhard-runtime-iqk-full-a1/rf-quality-report.json`
recovered `1942/2000` with 638 decrypt failures and is
`degraded_comparison` / `outside_margin`. Keep the current-default/EFUSE profile
as the local production gate for this geometry; do not promote runtime IQK from
this A/B result.

After moving LCK execution into `wfb-radio-runtime`,
`/tmp/wfb-rfq-runtime-lck-extraction-a1/rf-quality-report.json` reran the LCK
profile and recovered `1981/2000`, submitted `3000/3000`, reported zero decrypt
failures, verified channel 36 / 20 MHz, and stayed `within_margin`. This keeps
the LCK runtime extraction comparable with both the May 2 tuple and the May 4
pre-extraction gate.

The targeted Linux-parity override runtime extraction was also smoke-tested at
`/tmp/wfb-rfq-runtime-targeted-extraction-a1/rf-quality-report.json`. It
submitted `3000/3000` but recovered `0/2000`, observed no WFB session, and
classified as `degraded_comparison` / `outside_margin` with channel 36 / 20 MHz
verified. Treat this as evidence that the targeted override values still need
RF correction; do not use that profile as a range-readiness gate.

Standalone IQK diagnostic artifacts from `rtl8812a-iqk-diagnostic` can be used
as RF-quality review evidence, but they are not runtime calibration evidence.
When attaching one to a range-readiness note, record the artifact path,
`iqk.mode`, `iqk.cleanup_status`, MAC/BB and AFE backup counts, RF backup counts,
page-C1 latch count, and whether all traffic flags are false. A run with only
standalone IQK evidence must remain classified as stop-gap/captured until the
full IQK calibration routine is ported and receiver-backed or spectrum-backed
evidence shows parity.
