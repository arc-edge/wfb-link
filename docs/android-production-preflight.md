# Android Production Preflight

Use this checklist before treating an Android WFB Link run as product evidence.

## Hardware

- Prefer direct OTG from the Android phone to the RTL8812AU adapter, or a
  known-good powered OTG hub.
- Avoid passive charge-through Y-cables for RF validation. On the Pixel 7 Pro
  test phone, the direct connection produced a clean 5-minute duplex soak while
  the passive Y-cable path produced severe packet loss and occasional USB bulk
  write failures.
- Keep both stock antennas attached and similarly oriented for short bench
  runs. Record any antenna, range, door/wall, or hub changes with the result.

Expected direct-OTG USB state from `adb shell dumpsys usb`:

```text
host_connected=true
source_power=true
sink_power=false
usb_charging=false
```

## Peer

- The Linux peer must be on the same channel and bandwidth as Android.
- The phone-side `gs.key` must match the peer-side `drone.key`.
- Stop any production container or service that also owns the peer WFB adapter
  before isolated smoke tests, then restart it afterward.
- For channel 161 HT20 bench tests, the current peer uses:
  - uplink/control radio port `6`
  - downlink/video radio port `4`
  - link id `1`

## App Assets

Before starting an SDK session, the product app must provide readable paths for:

- `gs.key`
- `rtl8812aefw.bin`
- `halhwimg8812a_mac.c`
- `halhwimg8812a_bb.c`
- `halhwimg8812a_rf.c`

The AAR should include `libwfb_android.so` and, for managed raw streams,
`libwfb_tx_exec.so` plus `libwfb_rx_exec.so`.

## SDK Run

- Claim the RTL8812AU data interface and pass the live `UsbDeviceConnection`,
  bulk IN endpoint, and bulk OUT endpoint to `WfbUsbHandoff`.
- Start `WfbLinkManager.startManagedStreams(...)` on a caller-owned executor.
- Leave `validationTrafficEnabled(false)` for product use.
- Send raw uplink UDP to the configured TX stream local port.
- Bind the configured RX stream local port and read raw downlink UDP there.

Smoke tests may set `validationTrafficEnabled(true)` or use
`scripts/run-android-managed-soak.sh` to generate payloads and collect evidence.

## Acceptance Gates

For a short-range bench run, expect:

- `result.health.ok == true`
- `result.health.hasTxDrops() == false`
- `result.health.reachedRuntimeStop() == true`
- `result.helperStatus.helpersExitedCleanly() == true`
- RX signal samples present when downlink traffic was sent:
  `result.rxSignal.rssiDbm.sampleCount > 0`

Reference direct-OTG result from May 11, 2026:

- 5-minute duplex soak
- Android uplink: Linux recovered `2963/3000`
- Linux downlink: Android recovered `1390/1400`
- No Android crash or managed failure lines
