## Overview

The recovery flow intentionally keeps crypto/FEC in stock WFB-NG code. The
native radio service remains responsible for USB radio init, RX forwarding to a
WFB-NG aggregator UDP socket, and TX distributor datagram injection.

The data path is:

```text
macOS utun
  <-> scripts/wfb-mac-wf-tun.py
  <-> WFB-NG wfb_tx/wfb_rx in UDP mode
  <-> wfb-radio-service distributor/aggregator sockets
  <-> AWUS036ACH
```

This avoids a premature Rust port of the WFB-NG session crypto and FEC codec.

## Direction Defaults

Stock WFB-NG GS tunnel defaults are:

- RX from drone: radio port `0x20`
- TX to drone: radio port `0xa0`
- GS IP: `10.5.0.1/24`
- Drone IP: `10.5.0.2/24`

The recovery runner exposes these as environment overrides because this bench
has also used custom ports during RF smoke tests.

## Key Requirement

The Mac must have a readable matching GS-side WFB-NG keypair, normally
`gs.key`. Without that keypair, `wfb_rx` cannot decrypt the drone session and
`wfb_tx` cannot create a session the drone accepts. The scripts fail before
touching radio hardware if `WFB_KEY` is unset or unreadable.

## macOS Notes

macOS gates `utun` creation/configuration behind root privileges, so the runner
invokes the tunnel shim with `sudo -n`. The remote hardware Mac currently has
passwordless sudo, so the path is automatable there.
