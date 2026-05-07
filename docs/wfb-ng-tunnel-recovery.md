# WFB-NG Tunnel Recovery on macOS

This recovery path wires the native macOS RTL8812AU radio service into the Arc
WFB-NG tunnel profile:

```text
macOS utun
  <-> scripts/wfb-mac-wf-tun.py
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
```

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

The current accepted short-range loaded gate is a 95/100 side-stream minimum
with 40 ms marked payloads. Exact 100/100 side delivery during a 256 KiB SSH
download is not accepted yet: May 6 telemetry showed every Mac TX ingress
datagram was processed and submitted with zero pending queue depth, while the
Linux peer still missed several Mac-to-Linux side payloads. That makes the
remaining issue peer airtime/contention rather than Mac UDP ingress loss.
