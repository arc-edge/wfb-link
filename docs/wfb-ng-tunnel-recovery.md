# WFB-NG Tunnel Recovery on macOS

This recovery path wires the native macOS RTL8812AU radio service into the Arc
WFB-NG tunnel profile:

```text
macOS utun
  <-> wfb-tun-macos
  <-> WFB-NG wfb_tx/wfb_rx UDP codec
  <-> wfb-radio-service distributor/aggregator UDP
  <-> AWUS036ACH USB radio
```

The Arc GS tunnel defaults are:

- link ID: `0x000000` (the drone-side `wfb-link` currently omits `wfb-ng -i`)
- RX from drone: stream/radio port `3`
- TX to drone: stream/radio port `4`
- FEC: `k=2,n=4`

The drone profile is the inverse. The default tunnel IPs are `10.5.0.1/24` for
GS and `10.5.0.2/24` for drone. Stock WFB-NG tunnel ports (`0x20` / `0xa0`) can
still be supplied as overrides when testing against an unmodified WFB-NG peer.

## Build the local WFB-NG codec binaries

```bash
scripts/build-wfb-ng-macos-codec.sh
```

The wrapper builds only `wfb_tx` and `wfb_rx`. On macOS these are intended for
UDP distributor/aggregator mode, not direct raw Wi-Fi interface mode. It also
builds `wfb_keygen` so matching `drone.key`/`gs.key` files can be regenerated if
the original WFB-NG password is known.

## Run the recovery tunnel

Build the Rust tunnel bridge before running the wrapper:

```bash
cargo build -p wfb-tun --bin wfb-tun-macos
```

The tunnel needs the matching GS-side WFB-NG keypair file on the Mac. Stock
WFB-NG creates `drone.key` for the drone and `gs.key` for the ground station;
the Mac is acting as the GS here, so `WFB_KEY` should normally point to
`gs.key`, not the drone-side file.

```bash
WFB_KEY=/path/to/gs.key \
scripts/run-mac-wf-tun-recovery.sh
```

If the original WFB-NG password is known, regenerate a matching pair on macOS:

```bash
mkdir -p /tmp/wfb-keys
(cd /tmp/wfb-keys && /path/to/wfb_keygen 'original-password')
WFB_KEY=/tmp/wfb-keys/gs.key scripts/run-mac-wf-tun-recovery.sh
```

Once `wf-tun.log` shows the utun bridge started, try:

```bash
ssh pi@10.5.0.2
```

Useful overrides:

```bash
LINK_ID=0x000000
TUN_RX_RADIO_PORT=3
TUN_TX_RADIO_PORT=4
LOCAL_IP=10.5.0.1
PEER_IP=10.5.0.2
MCS=1
FEC_K=2
FEC_N=4
TX_MIN_INTERVAL_US=700  # optional TX pacing for loaded bidirectional runs
```

The wrapper defaults to the Rust `wfb-tun-macos` binary. The original Python
helper remains available only as a development fallback with
`TUN_IMPL=python`; adopters should use the Rust path.

For stock WFB-NG GS tunnel semantics, override:

```bash
LINK_ID=0x000001 TUN_RX_RADIO_PORT=0x20 TUN_TX_RADIO_PORT=0xa0 FEC_K=1 FEC_N=2 \
scripts/run-mac-wf-tun-recovery.sh
```

## Loaded Tunnel Gate

`scripts/run-mac-wf-tun-profile-matrix.sh` can run independent WFB data load
beside the tunnel probe:

```bash
DATA_LOAD_MODE=duplex DATA_LOAD_EXPECTED_PAYLOADS=100 \
DATA_LOAD_MIN_M2L_UNIQUE=95 DATA_LOAD_MIN_L2M_UNIQUE=95 \
DATA_LOAD_INTERVAL_SEC=0.040 scripts/run-mac-wf-tun-profile-matrix.sh
```

The accepted loaded production gate is now available directly:

```bash
PROFILE_SET=loaded REPEATS=3 scripts/run-mac-wf-tun-profile-matrix.sh
```

When `PROFILE_SET=loaded` is used without a custom `PROFILE_FILE`, the matrix
defaults to the accepted short-range profile: `TX_MIN_INTERVAL_US=700`,
`DATA_LOAD_MODE=duplex`, 100 expected side payloads per direction, 40 ms side
payload spacing, and a 1 s/1 s/100 ms TDD SSH-download probe. The matrix
summary includes TX ingress/processed/submitted/pending counts and gates TX
submission failures, TX ingress queue-send failures, and excessive pending TX
backlog.

The first named-profile hardware run used `400 us` pacing and passed at
`/tmp/wfb-mac-wf-tun-loaded-profile-20260507-002904`: 262,144 SSH-download
bytes in `8.640 s`, side streams `100/100` in both directions, TX ingress
`476`, TX processed/submitted `471`, pending TX ingress `5`, and zero TX
submission or ingress queue-send failures.

`TX_MIN_INTERVAL_US` applies optional microsecond pacing between Mac TX
datagram submissions. It is intended for loaded bidirectional profiles where a
Linux peer is transmitting at the same time and has no shared airtime scheduler
with the Mac.

The current accepted short-range loaded gate uses `TX_MIN_INTERVAL_US=700` with
40 ms marked side payloads, 100 expected each direction, and an exact 100/100
side-stream minimum during a 256 KiB SSH download. On May 7, 2026,
`/tmp/wfb-mac-wf-tun-rust-loaded-default-20260507-012952` accepted the Rust
tunnel gate after `500 us` and `600 us` Rust repeats recovered only 90/100 and
98/100 Mac-to-Linux side payloads. The accepted run moved 262,144 bytes in
`8.065 s`,
recovered side streams `100/100` in both directions, reported zero tunnel
drops/corrupt/truncated messages, zero radio TX failures, zero ingress
queue-send failures, and one pending TX ingress datagram.

Earlier no-pacing and burst-only telemetry showed every Mac TX ingress datagram
being processed and submitted while the Linux peer still missed side payloads,
so the failure was downstream airtime/contention rather than Mac UDP ingress
loss. Sub-millisecond pacing narrowed the useful local range: `250 us` and
`350 us` were still lossy, `400 us` passed an earlier repeat gate but later
regressed under the loaded side gate, and `500 us`/`600 us` were close but not
strictly lossless with the Rust tunnel bridge. `700 us` is the current
short-range default that preserves side traffic while staying under the 10 s
SSH-download gate in the latest validation.
