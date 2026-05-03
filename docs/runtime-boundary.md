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
  `runtime-flow` report.

## Still Diagnostic-Owned

- Full RTL8812AU init orchestration, table loading, and diagnostic phase reporting.
- Runtime IQK/LCK register orchestration and evidence reports while parity is still being hardened.
- WFB bridge loop orchestration, socket setup, ready-marker file writing,
  PCAP/JSONL output, and RF-quality automation.
- CLI parsing and human-facing diagnostic reports.
- Legacy standalone smoke commands that still claim `ClaimedUsbDevice` directly while their report shapes remain diagnostic-only.

## Migration Order

1. Move full RTL8812AU init phase execution behind runtime APIs while keeping `wfb-radio-diag` as a harness that calls those APIs.
2. Move calibration execution once IQK/LCK parity is stable enough to expose as runtime behavior rather than diagnostic experiment.
3. Define a smaller production bridge binary or API surface that wraps the runtime session without diagnostic-only report machinery.
4. Continue moving production telemetry types for calibration state, USB
   transfer counters, queue state, and WFB flow counters into
   `wfb-radio-runtime`; RX/TX flow counters and adapter-side RSSI/SNR/noise
   frame metadata are runtime-owned now.
5. Keep legacy smoke probes diagnostic-only unless a production workflow needs them.

The diagnostic binary should continue to be able to run every bring-up probe. Production behavior should live in runtime APIs first, then in a thinner runtime-oriented command surface.
