# Long-Range Tailnet Field Test

This runbook prepares a Mac ground station using the native RTL8812AU WFB path
and a drone that is reachable for control over a tailnet. The tailnet is only
for SSH orchestration and artifact collection; measured payloads still move
over WFB RF.

The goal of the first field session is packet evidence and WFB authentication
health, not maximum video throughput. Prove clean decrypt/session behavior and
receiver-backed packet recovery first, then increase video bitrate or MCS.

## One-Time Mac Prep

Build the local binaries:

```sh
cargo build -p wfb-radio-service
cargo build -p wfb-tun --bin wfb-tun-macos
scripts/build-wfb-ng-macos-codec.sh
```

Create or source the field env:

```sh
cp configs/long-range-tailnet-field.env.example /tmp/wfb-long-range-field.env
$EDITOR /tmp/wfb-long-range-field.env
source /tmp/wfb-long-range-field.env
```

Required values:

- `DRONE_HOST`: SSH target over the tailnet, for example
  `pi@drone-name.tailnet.ts.net`.
- `LOCAL_WFB_KEY`: Mac/ground-station `gs.key`.
- `DRONE_WFB_KEY`: drone-side key path, normally
  `/var/lib/arc/wfb/drone.key`.
- `CHANNEL`, `BANDWIDTH_MHZ`, `LINK_ID`, and radio ports matching the drone
  WFB profile.

Field note from 2026-05-18: the passing candidate used channel 165 / HT20. The
drone's normal Wi-Fi was on channel 157 with 80 MHz width, which overlapped the
earlier channel 161 WFB tests and cost roughly 9 dB RSSI / 8 dB SNR on downlink.

Run prep before the drone is online:

```sh
scripts/prepare-long-range-tailnet-field.sh \
  --env-file /tmp/wfb-long-range-field.env \
  --skip-drone
```

When the drone is online over the tailnet:

```sh
scripts/prepare-long-range-tailnet-field.sh \
  --env-file /tmp/wfb-long-range-field.env
```

The prep script writes a timestamped artifact directory with resolved settings,
local binary/key checks, optional tailnet probe output, drone preflight JSON,
and `field-run-commands.sh`.

Tailnet SSH reliability is not RF evidence. During the 2026-05-18 range walk,
plain `ssh pi@drone-2f389` was more reliable than forcing the tailnet IP while
the path was relay-only. A run that times out before the remote receiver is
started, or before local `radio_tx.datagrams_received` is nonzero, is invalid
and should be rerun rather than counted as a WFB pass/fail.

## Drone-Side Readiness

The drone needs:

- `ssh` reachable over the tailnet.
- Passwordless `sudo -n` for the operator used by `DRONE_HOST`.
- `wfb_rx`, `wfb_tx`, `iw`, `ip`, `tcpdump`, `timeout`, and `python3`.
- A WFB monitor interface, default `wfb0`.
- A readable drone-side WFB key at `DRONE_WFB_KEY`.

The prep script records `iw dev wfb0 info`, `ip link show`, tool availability,
noninteractive sudo status, and the SHA-256 fingerprint of the drone key file.
Do not expect `gs.key` and `drone.key` to have the same fingerprint; they are a
paired key set, not the same file. Authentication is proven by WFB session logs,
successful decrypt/recovery, and zero post-session decrypt failures.

## First RF Run

Use the generated command script:

```sh
/tmp/wfb-long-range-field-*/field-run-commands.sh
```

The first generated gate runs:

- `scripts/run-mac-wf-tun-profile-matrix.sh` with a conservative loaded tunnel
  profile, duplex side-load packets, and the local GS key.
- `scripts/run-radio-run-profile-matrix.sh` with raw WFB packet/decrypt gates
  and the drone key passed to the Linux peer runner.

The default tuple is intentionally conservative:

- Channel 165 / 20 MHz.
- `MCS0`.
- FEC 2/8 for first-contact packet evidence.
- `TX_MIN_INTERVAL_US=700`.
- `TX_BURST_LIMIT=8`.
- Manual Mac TX power index `0x20` on both RF paths. Confirm antenna gain and
  EIRP against local regulatory limits before using this beyond controlled
  field work.
- 1 s / 1 s / 100 ms TDD windows.
- `SESSION_ACQUIRE_MODE=observed`.
- `DECRYPT_FAILURE_GATE=post-session`.
- `MAX_M2L_DECRYPT_FAILURES=0` and `MAX_L2M_DECRYPT_FAILURES=0`.

After first-contact passes at short range, move outward in steps. Keep channel,
bandwidth, MCS, FEC, payload size, key, link ID, and ports fixed while changing
only distance/antenna geometry.

For manual isolation, use a three-phase run:

1. Start the drone `wfb_rx`, UDP counter, and `tcpdump`, and require a clear
   `started` response over SSH.
2. Run the Mac radio service plus local `wfb_tx` source and confirm
   `radio_tx.datagrams_received` and `submitted_frames` are nonzero.
3. Collect the drone counter and `wfb_rx` log. Only then classify the RF result.

## Packet And Auth Evidence To Preserve

Keep these artifacts from every run:

- Mac radio service report and health JSON.
- WFB tunnel or profile matrix summary.
- Drone `wfb_rx` logs showing `SESSION`, RX_ANT/RSSI metadata when available,
  and no post-session decrypt failures.
- Drone `tcpdump -i wfb0` RF pcap.
- `iw dev wfb0 info` and channel-state output.
- Field notes from `docs/rf-quality-field-notes-template.md`.

Useful manual drone capture when isolating a run:

```sh
ssh "$DRONE_HOST" \
  "sudo -n timeout 90 tcpdump -i '$DRONE_IFACE' -s 256 -w /tmp/wfb-long-range-rf.pcap"
```

Copy it back afterward:

```sh
scp "$DRONE_HOST:/tmp/wfb-long-range-rf.pcap" /tmp/
```

## 2026-05-18 Field Baseline

At the roughly 200 m problem distance, control/telemetry could survive while
video dropped on channel 161. Raw packet tests showed downlink still worked, but
uplink RF was marginal: the drone could see Mac-origin frames around -86 to
-89 dBm and `wfb_rx` logged decrypt failures. Moving WFB to channel 165 avoided
the drone Wi-Fi channel 157 / 80 MHz overlap and improved the downlink report
from about -71 dBm / 7 dB SNR to about -62 dBm / 15 dB SNR.

Best uplink candidate from that session:

- Channel 165 / HT20.
- MCS0, FEC 2/8.
- `TX_MIN_INTERVAL_US=700`, `TX_BURST_LIMIT=8`.
- `TX_POWER_MODE=manual-index`, `TX_POWER_INDEX=0x20`,
  `TX_POWER_PATH=both`.
- 45 s run, 160 measured payloads at 25 ms source cadence.

Result: the drone recovered 160/160 measured payloads, with no missing sequence
numbers and no duplicates. The drone pcap saw Mac frames averaging about
-77 dBm. The local downlink report during the same run averaged about -59 dBm
RSSI and 17 dB SNR.

Artifacts from that pass were written under:

- `/tmp/wfb-long-range-field-drone-2f389-attempt14-uplink-ch165-manual20-k2n8-burst8`
- Drone-side `/tmp/wfb-lr-20260518-uplink-ch165-manual20-k2n8-burst8-a10`

Mac tunnel ping tests still require local passwordless sudo or an entered sudo
credential because macOS gates `utun` creation/configuration. When sudo is not
available, raw WFB packet/decrypt tests are the valid RF evidence source.

## 2026-05-18 Range Walk Results

Use these results as gates, not as a link budget. Small antenna/terrain changes
were large enough to move the link from no recovery to clean recovery.

| Approx distance | Profile | Result | Notes |
| --- | --- | --- | --- |
| 400 ft / 122 m | channel 165, MCS0, FEC 2/8, TX index `0x20` | Pass, 160/160 | Drone RX_ANT around -76 to -78 dBm. Mac downlink averaged about -61 dBm RSSI and 15 dB SNR. |
| 800-1000 ft / about 300 m | same baseline | Pass after geometry adjustment, 160/160 | Earlier attempts at the same distance failed. The pass saw Mac downlink around -66 dBm / 13 dB SNR and drone RX_ANT around -82 to -86 dBm. |
| 350 m | same baseline | Fail, 0/160 | Mac transmitted cleanly, but the drone did not observe a WFB session for the test port. Mac downlink averaged about -73 dBm / 6 dB SNR. |
| 350 m | diagnostic TX index `0x28`, FEC 2/8 | Fail, 36/160 | Proved the path was present but below useful margin. Drone RX_ANT was around -88 to -90 dBm with decrypt/loss events. |
| 350 m | diagnostic TX index `0x28`, FEC 1/8, slower source cadence | Fail, 93/160 | Improved recovery but still missed too many payloads for control/video confidence. Drone RX_ANT remained around -88 to -90 dBm. |

The practical conclusion is that channel 165 fixed a local interference problem
from Wi-Fi overlap, but the 350 m failure is now a link-margin/geometry problem.
Do not compensate by defaulting production to higher TXAGC indexes; improve
antenna orientation, height, line of sight, and video bitrate/MCS first, then
rerun the receiver-backed gates.

Artifacts from the range walk were written under:

- `/tmp/wfb-long-range-field-drone-2f389-20260518-final-400ft-182858`
- `/tmp/wfb-long-range-field-drone-2f389-20260518-800ft-retry3-190750`
- `/tmp/wfb-long-range-field-drone-2f389-20260518-350m-r6-194021`
- `/tmp/wfb-long-range-field-drone-2f389-20260518-350m-p28-194318`
- `/tmp/wfb-long-range-field-drone-2f389-20260518-350m-p28-k1n8-194604`

## Applying This To arc-uas

Apply the field result as an explicit long-range bring-up profile, not as the
default flight/video profile:

- Put WFB on channel 165 / HT20 when the drone's normal Wi-Fi remains on channel
  157 / 80 MHz. Re-check with `iw dev` if either interface changes channel.
- Use MCS0, FEC 2/8, `TX_MIN_INTERVAL_US=700`, `TX_BURST_LIMIT=8`, and 1 s RX /
  1 s TX / 100 ms guard TDD windows for range validation.
- Keep Mac TX power at manual index `0x20` unless a controlled diagnostic run
  explicitly asks for another value and EIRP is checked against antenna gain.
- Keep video conservative until the raw packet/auth gate passes at the target
  distance. Passing control/telemetry is not enough evidence for video.
- Record WFB session lines, RX_ANT, decrypt failures, drone pcap, and Mac
  radio-service JSON for each promotion step.

The config values to mirror into `arc-uas` are:

```ini
WFB_CHANNEL=165
WFB_BANDWIDTH_MHZ=20
WFB_MCS=0
WFB_FEC_K=2
WFB_FEC_N=8
WFB_TX_MIN_INTERVAL_US=700
WFB_TX_BURST_LIMIT=8
WFB_TX_POWER_MODE=manual-index
WFB_TX_POWER_INDEX=0x20
WFB_TX_POWER_PATH=both
WFB_AIRTIME_MODE=tdd
WFB_TDD_FIRST_WINDOW=rx
WFB_TDD_RX_WINDOW_MS=1000
WFB_TDD_TX_WINDOW_MS=1000
WFB_TDD_GUARD_MS=100
```

For a real 1 km to 3 km target, `arc-uas` should expose this as a selectable
`long_range_validation` or `field_gate` profile and require a clean validation
run before enabling higher video bitrate, higher MCS, or wider bandwidth.

## Promotion Rule

Do not treat a profile as 1 km or 3 km ready because control still works. Video
must have receiver-backed packet recovery, clean WFB decrypt/session health,
and preserved RF captures at the target distance. If control/telemetry survives
but video drops, lower video MCS/bitrate or improve antenna geometry before
raising power or widening bandwidth.
