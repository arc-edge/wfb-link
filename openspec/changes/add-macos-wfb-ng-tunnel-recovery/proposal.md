## Why

When the drone's normal Wi-Fi/AP control path is unavailable, the remaining
recovery path may be the WFB-backed IP tunnel already running on the Linux
side. The native macOS radio service can move WFB frames, but macOS still needs
a GS-side tunnel shim and WFB-NG UDP codec wiring before operators can try
`ssh pi@10.5.0.2` over RF.

## What Changes

- Add a macOS `utun` bridge that implements the WFB-NG tunnel payload format
  of repeated `u16be length + raw IP packet` records plus empty keepalives.
- Add a macOS build wrapper for WFB-NG `wfb_tx` and `wfb_rx` in UDP
  distributor/aggregator mode.
- Add a recovery orchestrator that starts `wfb-radio-service`, WFB-NG codec
  processes, and the `utun` bridge with Arc GS tunnel defaults.
- Add production RX observations for WFB-prefixed channel IDs so recovery runs
  can distinguish ambient Wi-Fi from real WFB frames on unexpected link/port
  tuples.
- Document the required WFB-NG keypair and default tunnel ports/IPs.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `production-runtime`: document and script an operator recovery path that
  composes the production radio service with WFB-NG tunnel codec processes.

## Impact

- Affected scripts: new macOS tunnel bridge, WFB-NG codec build wrapper, and
  recovery runner.
- Affected docs: WFB-NG tunnel recovery instructions.
- Runtime telemetry gains WFB channel-ID observations; the rest is
  orchestration around the existing production service boundary.
