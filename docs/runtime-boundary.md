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
- Runtime-owned production WFB loop planning for TX UDP bind expansion, RX
  forwarding target validation, loop bounds, WFB metadata, and report-neutral
  RX/TX loop telemetry. `radio-run` now validates this plan before adapting into
  the existing diagnostic execution loop.
- Runtime-owned TX UDP ingress socket binding, receive-buffer configuration,
  receiver thread lifecycle, queued datagram shape, and shutdown. The diagnostic
  bridge loop now consumes runtime queued datagrams while the remaining USB/RF
  execution loop is still being migrated.
- Runtime-owned bridge loop scheduler for signal stop, duration stop,
  max-datagram stop, TX burst draining, and RX timeout clamping. Diagnostic
  commands now provide packet-specific TX/RX callbacks to this executor instead
  of owning the loop cadence directly.
- Runtime-owned queued WFB TX datagram handler for radiotap parsing, TX option
  override application, RTL8812AU descriptor preview, radio submission, and
  report-neutral TX counters/metadata.
- Runtime-owned parsed RX packet handler for frame/drop/tail accounting,
  PHY/RSSI/SNR/noise metadata coverage, frame type counters, WFB RX forwarding
  socket lifecycle, forwarded byte counts, and report-neutral forward
  snapshots.

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

The active cutover has moved production WFB loop planning, TX ingress socket
threads, bridge-loop scheduling, queued TX datagram handling, parsed RX packet
accounting, and WFB RX forwarding into runtime ownership. PCAP/JSONL output and
diagnostic report mutation still live in the diagnostic adapter while the
boundary shifts.

## Still Diagnostic-Owned

- Full RTL8812AU init orchestration, table loading, and diagnostic phase reporting.
- Runtime IQK/LCK register orchestration and evidence reports while parity is still being hardened.
- WFB bridge loop ready-marker file writing, PCAP/JSONL output, diagnostic
  report mutation, TX status probes, and RF-quality automation.
- CLI parsing and human-facing diagnostic reports, except for the thin
  production `radio-run` command adapter.
- Legacy standalone smoke commands that still claim `ClaimedUsbDevice` directly while their report shapes remain diagnostic-only.

## Migration Order

1. Move full RTL8812AU init phase execution behind runtime APIs while keeping `wfb-radio-diag` as a harness that calls those APIs.
2. Move calibration execution once IQK/LCK parity is stable enough to expose as runtime behavior rather than diagnostic experiment.
3. Move remaining bridge-loop report adaptation and production command execution
   harness code out of `wfb-radio-diag`.
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

After the `move-bridge-loop-runtime` slice on May 4, 2026, the same short
hardware-Mac smoke was rerun through the runtime-owned WFB loop plan. The run
completed with `result=pass`, init readiness `ready`, 14/14 phases completed,
and 4 RX frames with PHY status, valid RSSI, SNR, and derived noise. Artifact:
`/tmp/wfb-radio-run-loop-plan-smoke.json` on the hardware Mac deploy checkout.

After the `move-tx-ingress-runtime` slice on May 4, 2026, the smoke was rerun
with TX UDP ingress socket setup and receiver thread lifecycle owned by
`wfb-radio-runtime`. The run completed with `result=pass`, init readiness
`ready`, 14/14 phases completed, and 5 RX frames with PHY status, valid RSSI,
SNR, and derived noise. No TX datagrams were injected. Artifact:
`/tmp/wfb-radio-run-tx-ingress-smoke.json` on the hardware Mac deploy checkout.

After the `move-loop-executor-runtime` slice on May 4, 2026, the smoke was
rerun with bridge-loop scheduling and stop conditions owned by
`wfb-radio-runtime`. The run completed with `result=pass`,
`stop_reason=duration_elapsed`, init readiness `ready`, 14/14 phases completed,
and 5 RX frames with PHY status, valid RSSI, SNR, and derived noise. No TX
datagrams were injected. Artifact:
`/tmp/wfb-radio-run-loop-executor-smoke.json` on the hardware Mac deploy
checkout.

After the `move-tx-handler-runtime` slice on May 4, 2026, the smoke was rerun
with queued TX datagram processing owned by `wfb-radio-runtime`. The run
completed with `result=pass`, `stop_reason=duration_elapsed`, init readiness
`ready`, 14/14 phases completed, and 5 RX frames with PHY status, valid RSSI,
SNR, and derived noise. No TX datagrams were injected. Artifact:
`/tmp/wfb-radio-run-tx-handler-smoke.json` on the hardware Mac deploy checkout.
