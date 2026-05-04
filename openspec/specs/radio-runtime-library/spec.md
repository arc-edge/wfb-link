# radio-runtime-library Specification

## Purpose

Define reusable production-facing radio runtime responsibilities that should not depend on diagnostic CLI parsing or report formatting.
## Requirements
### Requirement: Runtime Calibration Policy
The system SHALL expose calibration profile policy from a reusable runtime library rather than requiring callers to depend on the diagnostic binary.

#### Scenario: Calibration profile classified
- **WHEN** a caller selects a supported TX calibration profile
- **THEN** the runtime library reports whether it is the default profile, which calibration class it represents, and whether it requires live register write authorization

#### Scenario: Runtime IQK protected
- **WHEN** a caller selects the runtime RTL8812A IQK profile
- **THEN** the runtime library marks the profile as requiring live register write authorization

### Requirement: Diagnostic Commands Use Runtime Policy
Diagnostic commands SHALL call the runtime library for stable calibration policy while retaining diagnostic-only CLI parsing and hardware execution code.

#### Scenario: Diagnostic authorization check
- **WHEN** a diagnostic command validates a selected calibration profile
- **THEN** it uses the runtime library decision to accept or reject the command before opening the adapter

#### Scenario: Diagnostic report classification
- **WHEN** a diagnostic RF-quality report classifies calibration state before TX
- **THEN** it uses the runtime library calibration class and preserves the existing report enum values

### Requirement: Runtime Boundary Documentation
The system SHALL document which radio responsibilities are owned by the runtime crate and which remain diagnostic-only pending later migration.

#### Scenario: Developer reviews runtime boundary
- **WHEN** a developer needs to move another radio path out of the diagnostic binary
- **THEN** the repository documentation identifies the current runtime-owned responsibilities and the next migration targets

### Requirement: macOS USBHost Runtime Transport
The runtime library SHALL provide the macOS IOUSBHost retained-session transport used for RTL8812AU register access and bulk pipe transfers.

#### Scenario: Runtime transport imported by diagnostic caller
- **WHEN** the diagnostic binary builds on macOS
- **THEN** it imports the macOS USBHost device and session types from the runtime library rather than from a local diagnostic module

#### Scenario: Transport traits preserved
- **WHEN** a caller opens a macOS USBHost device or retained session through the runtime library
- **THEN** the transport still implements the RTL8812AU register transport and USB bulk transfer traits used by existing init, TX, and RX code

### Requirement: Unified Runtime USB Transport
The runtime library SHALL provide a single live USB transport type that can carry either a libusb-claimed adapter or a macOS USBHost retained session.

#### Scenario: Register access through runtime transport
- **WHEN** live RTL8812AU register code receives the runtime USB transport
- **THEN** it can issue vendor register reads and writes through the runtime transport trait implementation

#### Scenario: Bulk transfer through runtime transport
- **WHEN** live RX or TX code receives the runtime USB transport
- **THEN** it can issue USB bulk reads and writes through the runtime transport trait implementation

#### Scenario: Diagnostic caller uses runtime transport
- **WHEN** diagnostic live commands claim or open an adapter
- **THEN** they construct the runtime USB transport type rather than a diagnostic-only transport enum

### Requirement: macOS Transport Open Policy
The runtime library SHALL own macOS USBHost retained-session open policy for supported RTL8812AU adapters.

#### Scenario: macOS endpoint config validated
- **WHEN** a caller supplies macOS USBHost endpoint configuration
- **THEN** the runtime library validates endpoint direction, supported bulk OUT endpoint count, and selected bulk OUT endpoint membership

#### Scenario: macOS retained session opened
- **WHEN** a caller supplies a supported VID/PID selector and valid macOS USBHost configuration
- **THEN** the runtime library opens a retained IOUSBHost session and returns the runtime USB transport, adapter metadata, endpoint layout, and initial USB control-write evidence

#### Scenario: macOS open rejected
- **WHEN** a caller supplies an unsupported selector, missing VID/PID, unsupported bus/address selector, invalid endpoints, or an IOUSBHost open failure
- **THEN** the runtime library returns a structured runtime error with a stable code and human-readable message

### Requirement: Diagnostic Uses Runtime Open Policy
Diagnostic live commands SHALL use runtime-owned open policy when constructing `RuntimeUsbTransport`.

#### Scenario: Diagnostic opens adapter
- **WHEN** a diagnostic bridge, init, TX, or RX live path opens a runtime USB transport
- **THEN** it calls the runtime open API and maps the runtime result into existing diagnostic report fields

### Requirement: Cross-Backend Runtime Open Policy
The runtime library SHALL provide a cross-backend open API for supported RTL8812AU USB adapters.

#### Scenario: libusb adapter opened
- **WHEN** a caller selects the libusb backend with a supported adapter selector
- **THEN** the runtime library discovers the matching supported adapter, claims it, and returns runtime transport, adapter metadata, and endpoint layout

#### Scenario: macOS USBHost adapter opened
- **WHEN** a caller selects the macOS USBHost backend with valid macOS USBHost configuration
- **THEN** the runtime library opens the retained IOUSBHost session through the existing runtime macOS open policy

#### Scenario: open failure reported
- **WHEN** discovery, selection, claiming, endpoint validation, or backend support fails
- **THEN** the runtime library returns a structured runtime error with a stable code and human-readable message

### Requirement: RTL8812AU Init Phase Sequence
The runtime library SHALL define the RTL8812AU same-session initialization phase sequence.

#### Scenario: Default phase order requested
- **WHEN** a caller requests the default same-session init sequence
- **THEN** the runtime library returns firmware before LLT and preserves the remaining MAC, queue, BB, RF, channel, calibration, and TX scheduler phases

#### Scenario: Linux phase order requested
- **WHEN** a caller requests Linux-order same-session init
- **THEN** the runtime library returns LLT before firmware and preserves the remaining phase order

### Requirement: Diagnostic Uses Runtime Init Sequence
Diagnostic same-session init SHALL use runtime-owned phase ordering and runtime-owned same-session init execution policy.

#### Scenario: Diagnostic executes LLT and firmware
- **WHEN** diagnostic same-session init decides whether LLT or firmware runs first
- **THEN** it uses the runtime init sequence rather than diagnostic-only branching

#### Scenario: Diagnostic wraps runtime init result
- **WHEN** a diagnostic command runs retained same-session init
- **THEN** it calls the runtime same-session init API and maps runtime phase, counter, and calibration evidence into existing diagnostic report fields

### Requirement: Runtime Same-Session Init API
The runtime library SHALL expose a report-neutral RTL8812AU same-session initialization API for production callers and diagnostics.

#### Scenario: Runtime init executes on a runtime session
- **WHEN** a caller provides a `RuntimeRadioSession`, validated init configuration, normalized init assets, and required hardware-write authorization
- **THEN** the runtime library performs the same-session init phases and returns phase identifiers, counter deltas, calibration decisions, and final readiness state without using diagnostic command argument or report types

#### Scenario: Runtime init rejects unsafe calibration
- **WHEN** a caller selects a calibration profile that requires live register write authorization without providing that authorization
- **THEN** the runtime library rejects initialization before applying the profile and returns a structured runtime error

### Requirement: Runtime Calibration Selection
The runtime library SHALL provide production-facing calibration selection for RTL8812AU init.

#### Scenario: Default production calibration selected
- **WHEN** a caller selects the default calibration profile
- **THEN** the runtime library classifies the profile as production-safe default behavior and does not run targeted parity, captured IQK/LCK, or runtime IQK sequences

#### Scenario: Experimental calibration selected
- **WHEN** a caller selects targeted parity, captured IQK/LCK, or runtime IQK calibration
- **THEN** the runtime library classifies the profile as experimental, records the evidence source, and reports whether receiver-backed validation is still required

### Requirement: Runtime LCK Calibration Execution
The runtime library SHALL execute the guarded RTL8812AU LCK calibration sequence without depending on diagnostic command or report types.

#### Scenario: Runtime LCK runs
- **WHEN** a caller provides RTL8812AU register access and runtime counters for the LCK profile
- **THEN** the runtime library pauses packet TX when needed, reads and preserves RF channel state, enters LCK mode, triggers the RF CHNLBW calibration bit, waits for the calibration window, exits LCK mode, restores state, and returns report-neutral register and RF-serial evidence

#### Scenario: Diagnostic command adapts runtime LCK
- **WHEN** a diagnostic command enables the LCK calibration profile
- **THEN** it calls the runtime LCK executor and only maps counters and serialization into the diagnostic report

### Requirement: Runtime Targeted Calibration Execution
The runtime library SHALL plan and execute targeted Linux-parity calibration override writes without depending on diagnostic command or report types.

#### Scenario: Targeted profile runs
- **WHEN** a caller enables the supported channel 36 / HT20 targeted Linux-parity profile
- **THEN** the runtime library returns the selected RFE, TX-scale, and TX-BB register writes and can execute them with report-neutral before/write/after evidence

#### Scenario: Unsupported targeted profile rejected
- **WHEN** a caller enables the targeted Linux-parity profile on an unsupported channel or bandwidth
- **THEN** the runtime library rejects the profile before applying targeted override writes

#### Scenario: Diagnostic command adapts targeted calibration
- **WHEN** a diagnostic command enables the targeted Linux-parity profile
- **THEN** it calls the runtime targeted calibration executor and only maps counters and serialization into the diagnostic report

### Requirement: Runtime IQK Planning Helpers
The runtime library SHALL provide report-neutral RTL8812AU IQK setup planning
and application, state backup/restore, candidate selection, one-shot outcome
state, TX one-shot execution, sweep summaries, and TX/RX IQC fill-plan helpers
for calibration callers.

#### Scenario: IQK setup plan generated by runtime
- **WHEN** a caller provides RTL8812AU band, RFE type, and external PA flags
- **THEN** the runtime library returns the upstream MAC gating, AFE setup, RF
  setup, tone, mixer, and page-C1 setup plan without depending on diagnostic
  report structs

#### Scenario: IQK setup plan applied by runtime
- **WHEN** a caller provides RTL8812AU register access, runtime counters, and
  the generated IQK setup plan
- **THEN** the runtime library applies register, masked BB, and RF-serial setup
  writes and reports the applied action count without depending on diagnostic
  command code

#### Scenario: IQK state backed up and restored by runtime
- **WHEN** a caller provides RTL8812AU register access and runtime counters for
  the runtime IQK profile
- **THEN** the runtime library backs up the upstream MAC/BB, AFE, page-C1
  latch, HSSI selector, TX pause, and RF serial state and restores that state
  with report-neutral cleanup evidence

#### Scenario: IQK candidate selected by runtime
- **WHEN** a caller provides repeated RTL8812AU IQK X/Y candidates
- **THEN** the runtime library selects a candidate using the upstream signed
  component tolerance and returns the masked IQC value without depending on
  diagnostic report structs

#### Scenario: IQK one-shot outcome modeled by runtime
- **WHEN** a caller runs TX or RX IQK one-shot attempts
- **THEN** the runtime library provides report-neutral attempt, stage, path,
  and sweep summary types that track ready/failed state, delay counts, retry
  counts, candidates, selected IQC values, fallback use, and failure labels

#### Scenario: IQK TX one-shot executed by runtime
- **WHEN** a caller provides RTL8812AU register access and runtime counters for
  the TX IQK one-shot phase
- **THEN** the runtime library drives the upstream TX IQK trigger, ready polling,
  failed-flag handling, candidate capture, retry limit, and fallback stage
  reporting without depending on diagnostic command types

#### Scenario: IQK fill plan generated by runtime
- **WHEN** a caller provides a valid RF path and selected TX or RX IQK X/Y
  values
- **THEN** the runtime library returns the upstream BB page-select, latch,
  correction-enable, fallback, and IQC masked-write plan for that path

#### Scenario: Diagnostic command adapts IQK planning
- **WHEN** the diagnostic runtime-IQK command builds candidate and IQC fill
  reports
- **THEN** it calls the runtime planning, TX one-shot, and outcome-state helpers
  and remains responsible only for RX one-shot register sequencing, sweep
  orchestration, and report formatting

### Requirement: Runtime RX Metadata
The runtime library SHALL expose parsed RTL8812AU RX descriptor metadata needed by production bridge and RF-quality callers.

#### Scenario: RTL8812AU PHY status parsed
- **WHEN** a runtime RX packet includes RTL8812AU OFDM/HT/VHT PHY status bytes
- **THEN** the parsed frame includes best-path RSSI, SNR, SNR source, and a derived noise estimate from the documented PHYDM 1st-type layout

#### Scenario: PHY status lacks SNR
- **WHEN** a runtime RX packet has no PHY status or only a short/non-OFDM PHY status block
- **THEN** the parsed frame preserves fallback RSSI behavior and leaves SNR/noise metadata absent rather than fabricating values

### Requirement: Production Flow Types
The runtime library SHALL expose report-neutral production flow configuration
and report types for full WFB RX/TX operation.

#### Scenario: Production config is diagnostic-free
- **WHEN** a caller builds production flow configuration
- **THEN** the type includes transport selection, adapter selector, channel,
  bandwidth, WFB socket settings, firmware path, calibration profile, duration,
  ready-marker, and required authorization state without including
  diagnostic-only register experiment fields

#### Scenario: Production report is diagnostic-free
- **WHEN** a production flow finishes or fails before opening hardware
- **THEN** the runtime-owned report records stable production telemetry fields
  without depending on diagnostic report structs

### Requirement: Production Flow Validation
The runtime library SHALL validate production flow configuration before hardware
open when a failure can be determined without touching USB.

#### Scenario: Missing authorization rejected
- **WHEN** production flow configuration requests RF transmit or live register
  calibration without the required explicit authorization
- **THEN** validation fails before opening USB with a stable runtime error code

#### Scenario: Invalid runtime bounds rejected
- **WHEN** production flow configuration includes invalid runtime bounds such as
  an unusable channel, unsupported bandwidth, or zero TX burst limit
- **THEN** validation fails before opening USB with a stable runtime error code
