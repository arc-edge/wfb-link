# WFB Radio Runtime Specification

## Purpose

Define runtime-owned live radio behavior that standalone diagnostics and future production commands share.
## Requirements
### Requirement: Standalone Runtime RX Capture
The system SHALL capture standalone live RX traffic through the userspace USB radio runtime.

#### Scenario: RX scan captures runtime-parsed frames
- **WHEN** `rx-scan` receives a bulk-IN read containing supported RTL8812AU RX packet metadata
- **THEN** it processes the runtime-parsed packet outcomes and records frame, drop, and incomplete-tail counters

#### Scenario: RX scan forwards matching WFB payloads
- **WHEN** a runtime-parsed RX frame matches the configured WFB channel filter
- **THEN** `rx-scan` forwards the WFB payload to the configured UDP aggregator and records forwarding counters

### Requirement: Standalone Runtime TX
The system SHALL submit standalone live TX diagnostics through the userspace USB radio runtime.

#### Scenario: Single-frame TX uses runtime session
- **WHEN** `tx-once` receives a valid IEEE 802.11 frame and explicit transmit authorization
- **THEN** it submits the frame through the runtime radio session and records TX submit counters

#### Scenario: Repeated TX uses runtime session
- **WHEN** `tx-repeat` receives a valid IEEE 802.11 frame, repeat count, interval, and explicit transmit authorization
- **THEN** it submits each frame through the runtime radio session and records throughput and submit counters

### Requirement: Production Runtime Full Flow
The system SHALL provide a production-facing WFB runtime flow that opens,
initializes, receives, and transmits through runtime APIs, runtime-owned loop
planning, runtime-owned TX ingress setup, runtime-owned loop scheduling,
runtime-owned queued TX datagram handling, and runtime-owned parsed RX packet
handling.

#### Scenario: Production flow starts
- **WHEN** a caller starts the production runtime flow with a supported adapter
  selector, channel, bandwidth, WFB UDP settings, TX-power policy,
  calibration profile, and required authorization
- **THEN** the command opens the adapter through runtime open policy,
  initializes it through runtime same-session init, applies runtime-owned
  TX-power register programming when requested, and performs RX/TX through
  `RuntimeRadioSession`

#### Scenario: Production flow starts TX ingress
- **WHEN** the production runtime flow reaches WFB loop execution
- **THEN** TX UDP socket binding and datagram receiver threads are created
  through `wfb-radio-runtime`

#### Scenario: Production flow schedules loop in runtime
- **WHEN** the production runtime flow runs interleaved TX and RX work
- **THEN** loop cadence and stop conditions are controlled by
  `wfb-radio-runtime`

#### Scenario: Production flow handles TX datagrams in runtime
- **WHEN** the production runtime flow receives a queued TX datagram
- **THEN** WFB TX datagram parsing, TX option override application, descriptor
  preview, and radio submission are performed through `wfb-radio-runtime`

#### Scenario: Production flow handles RX outcomes in runtime
- **WHEN** the production runtime flow receives parsed RX packet outcomes
- **THEN** RX outcome accounting, metadata coverage counters, frame type
  counters, and WFB RX forwarding are performed through `wfb-radio-runtime`

#### Scenario: Production flow rejects diagnostic-only dependencies
- **WHEN** the production runtime flow is built
- **THEN** it MUST NOT depend on diagnostic command argument structs or
  diagnostic report structs for radio initialization, RX/TX loop planning, TX
  ingress setup, loop scheduling, TX-power register programming, TX datagram
  handling, RX packet handling, or emitted production reports

#### Scenario: Production flow rejects diagnostic register experiments
- **WHEN** a caller starts `runtime-flow` or `radio-run` with diagnostic-only
  register pokes or TX-status probes
- **THEN** the command rejects the request before opening USB

#### Scenario: Production flow validates WFB loop settings
- **WHEN** a caller starts `radio-run` with invalid WFB forwarding settings,
  zero RX timeout, or zero TX burst limit
- **THEN** runtime-owned validation rejects the request before socket binding or
  USB open

#### Scenario: Production flow reports readiness
- **WHEN** initialization completes
- **THEN** the production runtime flow reports adapter identity, channel,
  bandwidth, calibration class, selected calibration profile evidence when a
  profile executes, init phase status, runtime-owned RX/TX flow counters, RX
  metadata coverage counters, RX outcome/frame-type counters, USB counters, and
  last error state through production-facing telemetry

### Requirement: Runtime LED Heartbeat Toggle
The runtime library SHALL provide an LED heartbeat helper that toggles a
configured RTL8812AU LED register at a configurable half-period during a
production session. The helper SHALL be opt-out via configuration, default to
enabled, and SHALL drive the visible enclosure LED via `REG_LEDCFG0` using the
operator-confirmed `0x28` on and `0x20` off values.

#### Scenario: Heartbeat toggles after half-period elapses
- **WHEN** the heartbeat is enabled and the configured half-period has elapsed
  since the last toggle
- **THEN** the runtime library issues exactly one USB control write to
  `REG_LEDCFG0` with the next state value
- **AND** the heartbeat alternates between on (`0x28`) and off (`0x20`) on
  subsequent toggles

#### Scenario: Heartbeat skips writes within half-period
- **WHEN** the heartbeat is enabled and `maybe_toggle` is called before the
  half-period has elapsed since the last toggle
- **THEN** the runtime library does not issue any USB control write
- **AND** the heartbeat counters do not change

#### Scenario: Heartbeat is disabled
- **WHEN** the heartbeat is configured with `enabled = false`
- **THEN** `maybe_toggle` does not issue any USB control write
- **AND** `turn_off` does not issue any USB control write

#### Scenario: USB write failures are counted but not propagated
- **WHEN** the heartbeat attempts a toggle and the underlying USB control write
  returns an error
- **THEN** `toggles_failed` is incremented
- **AND** `toggles_succeeded` is not incremented
- **AND** the failure is not returned to the caller

### Requirement: Runtime LED Heartbeat Counters
The runtime library SHALL expose toggle counters that reflect attempted,
succeeded, and failed USB writes, so a calling production runtime can report
them without sampling internal heartbeat state.

#### Scenario: Counters reflect successful toggles
- **WHEN** the heartbeat performs N successful toggles
- **THEN** `toggles_attempted` equals N
- **AND** `toggles_succeeded` equals N
- **AND** `toggles_failed` equals 0

#### Scenario: Counters distinguish failed toggles
- **WHEN** the heartbeat attempts N toggles and M of them fail at the USB
  control write
- **THEN** `toggles_attempted` equals N
- **AND** `toggles_succeeded` equals N - M
- **AND** `toggles_failed` equals M

### Requirement: Runtime LED Heartbeat Off On Session End
The runtime library SHALL provide a best-effort `turn_off` method that issues a
single USB control write to `REG_LEDCFG0` with the off value, intended to be
called once when the production runtime flow exits.

#### Scenario: Turn-off issues an off write when enabled
- **WHEN** `turn_off` is called with the heartbeat enabled
- **THEN** the runtime library issues exactly one USB control write with the
  off value (`0x20`)

#### Scenario: Turn-off is a no-op when disabled
- **WHEN** `turn_off` is called with the heartbeat configured `enabled = false`
- **THEN** the runtime library does not issue any USB control write

### Requirement: Production Bridge Loop Iteration Tick Hook
The runtime library SHALL expose a per-outer-iteration tick callback on the
production bridge loop executor so consumers can drive periodic state such as
LED heartbeat, watchdog kicks, or throttle pacing without taking their own
clock reading. The callback SHALL fire once at the top of each outer iteration
after stop and deadline checks pass and before any TX burst or RX poll work.

#### Scenario: Iteration tick fires per outer iteration
- **WHEN** the executor enters a new outer iteration that is not short-circuited
  by signal stop, duration deadline, or TX-datagram limit
- **THEN** the executor invokes the iteration-tick callback with the current
  `Instant`
- **AND** the callback is invoked before any `TryTx` or `ReadRx` step is
  dispatched

#### Scenario: Iteration tick is skipped on signal stop
- **WHEN** the executor's stop-requested check returns true at the top of an
  iteration
- **THEN** the executor does not invoke the iteration-tick callback
- **AND** the executor returns with `stop_reason = signal`

### Requirement: Runtime-Owned Production Flow Execution
The runtime library SHALL provide a production flow execution API that owns the
end-to-end `radio-run` hardware lifecycle without depending on diagnostic
command argument structs or diagnostic report structs.

#### Scenario: Runtime execution runs the production flow
- **WHEN** a caller supplies a validated production runtime configuration and
  required runtime-owned inputs
- **THEN** the runtime library opens the adapter, initializes the radio, applies
  selected TX-power and calibration policy, writes the ready marker, starts TX
  ingress, runs the production bridge loop, processes RX/TX work, drives the LED
  heartbeat, and returns a `ProductionRuntimeFlowReport`

#### Scenario: Runtime execution rejects invalid config before USB
- **WHEN** a caller supplies invalid production runtime configuration
- **THEN** the runtime library returns a failed `ProductionRuntimeFlowReport`
  without opening or claiming the USB adapter

#### Scenario: Runtime execution preserves production telemetry
- **WHEN** the runtime-owned production flow exits
- **THEN** the report includes adapter identity, endpoints, channel, bandwidth,
  init readiness, calibration evidence, TX-power evidence when present, RX
  signal and frame-type counters, RX forwarding snapshots, TX counters, USB
  counters, heartbeat counters, stop reason, result, and error state

### Requirement: Runtime Flow Excludes Diagnostic Side Outputs
The runtime-owned production flow SHALL keep diagnostic-only side outputs and
experiments outside the production execution API.

#### Scenario: Diagnostic captures remain outside runtime execution
- **WHEN** a production flow runs through the runtime execution API
- **THEN** the API does not require or emit PCAP paths, frame JSONL paths, trace
  replay artifacts, TX-status probes, or generic register experiment reports

#### Scenario: Diagnostic commands remain available
- **WHEN** an operator needs PCAP/JSONL captures, TX-status probes, generic
  register writes, trace replay, or bring-up reports
- **THEN** those workflows remain available through diagnostic commands rather
  than the production runtime execution API

