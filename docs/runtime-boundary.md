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
- Runtime-owned targeted Linux-parity calibration override planning and
  register-write execution for channel 36 / HT20.
- Runtime-owned RTL8812AU LCK calibration execution, RF-serial helper reports,
  register read/write evidence, cleanup handling, and counter deltas.
- Runtime-owned RTL8812AU IQK setup-plan, candidate-selection, and TX/RX IQC
  fill-plan helpers. These preserve the upstream MAC/AFE/RF prerequisites,
  masks, path-specific latch registers, fallback rules, and signed-component
  tolerance used by the live IQK sweep.
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
- Runtime IQK live register orchestration, backup/restore execution, one-shot
  sweep execution, and evidence reports while parity is still being hardened.
  Targeted parity, LCK execution, and IQK setup/candidate/IQC planning have
  moved into the runtime library.
- WFB bridge loop ready-marker file writing, PCAP/JSONL output, diagnostic
  report mutation, TX status probes, and RF-quality automation.
- CLI parsing and human-facing diagnostic reports, except for the thin
  production `radio-run` command adapter.
- Legacy standalone smoke commands that still claim `ClaimedUsbDevice` directly while their report shapes remain diagnostic-only.

## Migration Order

1. Move full RTL8812AU init phase execution behind runtime APIs while keeping `wfb-radio-diag` as a harness that calls those APIs.
2. Before moving calibration execution, run the close-range 2000-payload A/B
   against the current runtime-flow/radio-run code and verify the runtime
   extraction did not regress the May 2 evening baseline tuple
   `1973/1980/1970` recovered payloads. Only start calibration-extraction Step
   2 after that non-regression gate passes. This gate passed on May 4, 2026:
   the accepted current-default, IQK marker, and LCK reruns recovered
   `1989/1988/1992` payloads with zero decrypt failures.
3. Move calibration execution once IQK/LCK parity is stable enough to expose as runtime behavior rather than diagnostic experiment. Targeted parity, LCK execution, and IQK setup/candidate/IQC planning are runtime-owned; IQK live sweep execution remains the next calibration-extraction target.
4. Move remaining bridge-loop report adaptation and production command execution
   harness code out of `wfb-radio-diag`.
5. Continue moving production telemetry types for calibration state, USB
   transfer counters, queue state, and WFB flow counters into
   `wfb-radio-runtime`; RX/TX flow counters and adapter-side RSSI/SNR/noise
   frame metadata are runtime-owned now.
6. Keep legacy smoke probes diagnostic-only unless a production workflow needs them.

The diagnostic binary should continue to be able to run every bring-up probe. Production behavior should live in runtime APIs first, then in a thinner runtime-oriented command surface.

## Latest Calibration Extraction Smoke

On May 4, 2026, after moving RTL8812AU LCK execution into
`wfb-radio-runtime`, the close-range receiver-backed LCK gate was rerun through
`scripts/run-rf-quality-close-range.sh`. The run completed with `result=pass`,
`acceptance=baseline_comparable`, `comparison=matched`, `within_margin`,
`3000/3000` submitted datagrams, `1981/2000` recovered payloads, zero decrypt
failures, and Linux channel state verified at channel 36 / 20 MHz. Artifact:
`/tmp/wfb-rfq-runtime-lck-extraction-a1/rf-quality-report.json`.

The targeted Linux-parity override path was also moved into
`wfb-radio-runtime` and rerun once on hardware at
`/tmp/wfb-rfq-runtime-targeted-extraction-a1/rf-quality-report.json`. That
experimental profile submitted `3000/3000` datagrams but recovered `0/2000`,
observed no WFB session, and classified as `degraded_comparison` /
`outside_margin` with channel 36 / 20 MHz verified. Keep the targeted override
profile as diagnostic-only evidence until its register values are corrected or
validated independently; it is not part of the production-ready path.

The first IQK extraction slices moved pure setup planning, candidate selection,
and TX/RX IQC fill-plan helpers into `wfb-radio-runtime`. These slices do not
touch hardware: the diagnostic command still owns guarded backup/restore,
one-shot IQK sweep execution, setup-plan application, and JSON evidence
reporting. Focused runtime and diagnostic IQK helper tests verify MAC/AFE/RF
prerequisites, masks, latch registers, fallback behavior, invalid-path
rejection, and signed-component selection tolerance.

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

After the `move-rx-handler-runtime` slice on May 4, 2026, the smoke was rerun
with parsed RX packet accounting and WFB RX forwarding owned by
`wfb-radio-runtime`. The run completed with `result=pass`,
`stop_reason=duration_elapsed`, init readiness `ready`, 14/14 phases completed,
24 parsed RX frames, and 20 SNR-bearing RX frames. No TX datagrams were
injected. Artifact: `/tmp/wfb-radio-run-rx-handler-smoke.json` on the hardware
Mac deploy checkout.

After archiving the runtime cutover specs on May 4, 2026,
`scripts/run-production-radio-smoke.sh --mode both` was run on the hardware Mac
deploy checkout. The RX-only run completed with `result=pass`,
`stop_reason=duration_elapsed`, 28 RX buffers/frames, and zero TX datagrams.
The TX-positive run injected 64 synthetic WFB distributor datagrams through
`radio-run`, completed with `result=pass`, submitted all 64 frames, and reported
zero TX failures or drops. Artifacts:
`/tmp/wfb-prod-radio-smoke-20260504-001308/radio-run-rx-only.json` and
`/tmp/wfb-prod-radio-smoke-20260504-001308/radio-run-tx-positive.json` on the
hardware Mac deploy checkout.
