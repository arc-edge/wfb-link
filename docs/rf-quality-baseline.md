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

- `${REMOTE_PREFIX}-preflight.json`
- `${REMOTE_PREFIX}-preflight.log`
- `${REMOTE_PREFIX}-setup.log`
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
runtime IQK or additional profile-time hardware reads. The runtime IQK profile
also adds `--i-understand-this-writes-registers` to the hardware-Mac bridge
command because it runs the TX/RX IQK one-shot sequence and writes final IQC
values before restoring saved RF/BB state.

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

`rf-quality-report` also emits
`macos.calibration.runtime_iqk_summary` whenever a runtime IQK profile report is
present. Use `risk`, `completed`, `cleanup_restored`, `sweep_count`,
`fallback_stage_count`, and `fallback_stages[]` as the compact
machine-readable calibration health signal for release gating and field notes.
Outdoor profile gating rejects a close-range gate artifact with
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

For session-acquisition debugging, the runner can send unmeasured source
payloads before the marked payload sequence:

```sh
SOURCE_WARMUP_PAYLOADS=120
```

Warmup payloads use the same WFB TX path but do not carry `PAYLOAD_MARKER`, so
the receiver counter does not count them as recovered test payloads. The runner
raises `MAX_DATAGRAMS` by the warmup FEC estimate and records
`source_warmup_payloads`, `theoretical_warmup_datagrams`, and
`theoretical_total_datagrams` in `datagram-evidence.json`.

The runner also records `${REMOTE_PREFIX}-receiver-health.json` and lifts the
same health into `datagram-evidence.json`. `rf-quality-report` exposes this
under `macos.wfb_outcome` as:

- `receiver_status`
- `receiver_session_observed`
- `receiver_unable_decrypt_count`
- `receiver_total_datagrams`
- `receiver_evidence`

When `wfb_rx` emits `RX_ANT` lines, the runner also parses them into
`receiver_evidence.receiver_health.rx_antenna_reports` and
`rx_antenna_summary`. Each report records frequency, MCS index, bandwidth,
antenna id, packet count, RSSI min/avg/max, and SNR min/avg/max from the
Linux receiver log. `rf-quality-report` also exposes the compact copy at
`macos.wfb_outcome.receiver_telemetry` so release tooling can read MCS/RSSI/SNR
health without parsing the full receiver artifact.
Outdoor profile gating now rejects a close-range gate that lacks this RX_ANT
receiver telemetry, because long-distance promotion needs RF health evidence in
addition to payload recovery.

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

Standalone IQK diagnostic artifacts from `rtl8812a-iqk-diagnostic` can be used
as RF-quality review evidence, but they are not runtime calibration evidence.
When attaching one to a range-readiness note, record the artifact path,
`iqk.mode`, `iqk.cleanup_status`, MAC/BB and AFE backup counts, RF backup counts,
page-C1 latch count, and whether all traffic flags are false. A run with only
standalone IQK evidence must remain classified as stop-gap/captured until the
full IQK calibration routine is ported and receiver-backed or spectrum-backed
evidence shows parity.
