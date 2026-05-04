# Runtime Boundary

`wfb-radio-runtime` is the production-facing runtime layer for native WFB radio operation. It owns the live USB session, shared runtime policy, selected RTL8812AU init helpers, and frame I/O APIs that diagnostic commands now call.

## Runtime-Owned Now

- TX calibration profile identity.
- Calibration profile safety policy, including whether a profile requires live register write authorization.
- Calibration class policy used by RF-quality reports and future runtime state.
- Unified runtime USB transport over libusb claims and macOS USBHost retained sessions.
- macOS IOUSBHost direct-control and retained-session transport wrappers.
- macOS USBHost endpoint validation, synthetic adapter metadata, and retained-session open policy.
- Runtime libusb adapter selection/claim policy for bridge, init, TX, RX, and IQK runtime transport paths.
- macOS RTL8812AU register and bulk-transfer trait implementations.
- RTL8812AU same-session init phase identities and default/Linux-order phase sequencing policy.
- Runtime execution helpers for the TX scheduler tail, monitor/no-link receive filter, and EFUSE MACID programming.
- Runtime radio session metadata, endpoint selection, counters, and error classification.
- Runtime 802.11 TX submission through descriptor construction and bulk OUT.
- Runtime descriptor-prefixed raw TX packet replay for trace-parity and benchmark paths.
- Runtime RX bulk-IN reads with RTL8812AU RX descriptor parsing, parser outcome
  counters, and RTL8812AU OFDM PHY-status RSSI/SNR/noise extraction.
- Runtime-owned full-flow RX/TX telemetry structs used by the production-shaped
  `runtime-flow` report, including RX metadata coverage counters for PHY status,
  valid RSSI, SNR, and derived noise.
- Runtime-owned production WFB flow configuration, validation, report, init
  readiness, error, and USB-selection snapshot types. These are used by the
  production `radio-run` command and are serializable without diagnostic-only
  register experiment fields.

## Production Command

`wfb-radio-diag radio-run` is the first production cutover entry point. It
accepts the operational full-flow settings only: adapter selection, channel,
bandwidth, firmware path, TX UDP bind addresses, optional WFB RX forwarding,
runtime bounds, calibration profile, macOS USBHost backend settings, and the
explicit TX/register-write acknowledgements.

`radio-run` always maps into `wfb-radio-runtime::ProductionRuntimeFlowConfig`
and validates that config before USB open. The command does not expose
diagnostic register pokes, TX status probes, TXDMA-clear experiments, PCAP, or
raw frame JSONL capture. Those remain available through diagnostic commands.

During this cutover slice, `radio-run` still executes by adapting the validated
runtime-owned config into the existing hardware-proven `runtime-flow`/bridge
loop. Its emitted JSON is `ProductionRuntimeFlowReport` from
`wfb-radio-runtime`, not the diagnostic `RuntimeFlowReport`.

## Still Diagnostic-Owned

- Full RTL8812AU init orchestration, table loading, and diagnostic phase reporting.
- Runtime IQK/LCK register orchestration and evidence reports while parity is still being hardened.
- WFB bridge loop orchestration, socket setup, ready-marker file writing,
  PCAP/JSONL output, and RF-quality automation.
- CLI parsing and human-facing diagnostic reports, except for the thin
  production `radio-run` command adapter.
- Legacy standalone smoke commands that still claim `ClaimedUsbDevice` directly while their report shapes remain diagnostic-only.

## Migration Order

1. Move full RTL8812AU init phase execution behind runtime APIs while keeping `wfb-radio-diag` as a harness that calls those APIs.
2. Move calibration execution once IQK/LCK parity is stable enough to expose as runtime behavior rather than diagnostic experiment.
3. Move full bridge-loop execution into `wfb-radio-runtime` so `radio-run` no
   longer adapts through diagnostic bridge internals.
4. Continue moving production telemetry types for calibration state, USB
   transfer counters, queue state, and WFB flow counters into
   `wfb-radio-runtime`; RX/TX flow counters and adapter-side RSSI/SNR/noise
   frame metadata are runtime-owned now.
5. Keep legacy smoke probes diagnostic-only unless a production workflow needs them.

The diagnostic binary should continue to be able to run every bring-up probe. Production behavior should live in runtime APIs first, then in a thinner runtime-oriented command surface.

## Latest Runtime-Flow Smoke

On May 3, 2026, a short hardware-Mac `runtime-flow --macos-usbhost` smoke on
channel 36/20 MHz verified that production-shaped RX telemetry carries the new
metadata coverage counters. The run completed with
`stop_reason=duration_elapsed`, read 124 RX buffers, parsed 144 frames, and
reported 124 frames each for PHY status, valid RSSI, SNR, and derived noise.
Artifact: `/tmp/wfb-runtime-flow-rxmeta.json` on the hardware Mac deploy
checkout.

## Latest Radio-Run Smoke

On May 4, 2026, after the production cutover slice, a short hardware-Mac
`radio-run --macos-usbhost --vid 0x0bda --pid 0x8812` smoke on channel 36/20
MHz verified the production command and runtime-owned report. The run completed
with `result=pass`, `stop_reason=duration_elapsed`, init readiness `ready`,
14/14 init phases completed, 6 RX buffers/frames parsed, and 6 frames each for
PHY status, valid RSSI, SNR, and derived noise. No TX datagrams were injected in
that smoke. Artifact: `/tmp/wfb-radio-run-smoke.json` on the hardware Mac
deploy checkout.
