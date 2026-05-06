# WFB-NG Tunnel Recovery on macOS

This recovery path wires the native macOS RTL8812AU radio service into stock
WFB-NG tunnel semantics:

```text
macOS utun
  <-> scripts/wfb-mac-wf-tun.py
  <-> WFB-NG wfb_tx/wfb_rx UDP codec
  <-> wfb-radio-service distributor/aggregator UDP
  <-> AWUS036ACH USB radio
```

The stock WFB-NG GS tunnel ports are:

- RX from drone: `0x20`
- TX to drone: `0xa0`

The drone profile is the inverse. The default tunnel IPs are `10.5.0.1/24`
for GS and `10.5.0.2/24` for drone.

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
LINK_ID=0x000001
TUN_RX_RADIO_PORT=0x20
TUN_TX_RADIO_PORT=0xa0
LOCAL_IP=10.5.0.1
PEER_IP=10.5.0.2
MCS=1
FEC_K=1
FEC_N=2
```
