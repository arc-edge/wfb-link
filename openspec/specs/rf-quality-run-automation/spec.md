# rf-quality-run-automation Specification

## Purpose

Define the operator-facing automation for reproducible RF-quality runs across the local checkout, hardware Mac, and Linux WFB peer.
## Requirements
### Requirement: Close-Range Run Orchestration
The system SHALL provide a single operator-facing command that orchestrates the accepted close-range RF-quality workflow across the hardware Mac and Linux WFB peer.

#### Scenario: Close-range automation run starts
- **WHEN** the operator invokes the automation command with the required host, channel, payload, and report settings
- **THEN** the command starts the Mac-side relay and bridge listener, waits for the bridge ready marker, prepares the Linux WFB peer, runs the Linux sender and receiver, and records the produced artifact paths

#### Scenario: Production radio command is selected
- **WHEN** the operator selects the production `radio-run` Mac command for an automated close-range run
- **THEN** the command MUST start `radio-run` with runtime WFB TX ingress settings and the selected TX-power mode/source, wait for the same ready marker before Linux traffic, and generate datagram evidence from the production report's nested TX counters

#### Scenario: Close-range automation run rejects missing settings
- **WHEN** required host, repository, firmware, key, or network settings are missing
- **THEN** the command fails before starting RF transmission and reports the missing setting

### Requirement: Linux Peer Control
The system SHALL control the Linux WFB peer in a bounded and reversible way during automated RF-quality runs.

#### Scenario: Linux peer is prepared
- **WHEN** an automated run begins
- **THEN** the command stops the configured WFB service container, pins the monitor interface to the requested channel and bandwidth, starts bounded `tcpdump`, `wfb_rx`, and `wfb_tx` processes, and records setup output

#### Scenario: Linux peer isolation is required
- **WHEN** peer-isolation policy is enabled for an automated run
- **THEN** the command MUST record WFB peer processes before service shutdown, wait for the configured settle interval after service shutdown and stale-process cleanup, verify that no `arc-wfb-link`, `wfb_rx`, or `wfb_tx` processes remain before starting measured receiver traffic, and fail before RF traffic if residual processes are still present

#### Scenario: Linux peer is restored
- **WHEN** an automated run finishes or fails after Linux setup begins
- **THEN** the command attempts to stop test processes, restart the configured WFB service container, and record service restore output

### Requirement: Measured Payload Warmup
The system SHALL support unmeasured source-payload warmup before marked payload accounting so receiver session acquisition does not distort RF-quality results.

#### Scenario: Warmup payloads are configured
- **WHEN** an automated run configures nonzero source warmup payloads
- **THEN** the command MUST send warmup payloads before marked payloads, exclude warmup markers from recovered-payload accounting, increase the expected total WFB datagram budget by the warmup FEC estimate, and record warmup payload/datagram counts in the run evidence

#### Scenario: Session acquisition settle is configured
- **WHEN** an automated duplex run observes required WFB receiver sessions after
  warmup
- **THEN** the command MAY wait a configured settle interval before marked
  payloads and MUST record that interval in the source-gate evidence

#### Scenario: Warmup is disabled
- **WHEN** an automated run sets source warmup payloads to zero
- **THEN** the command MUST preserve first-session acquisition evidence and allow receiver decrypt errors to mark the generated RF-quality report outside the production acceptance margin

### Requirement: Artifact Collection
The system SHALL collect Mac and Linux run artifacts into a timestamped local output directory.

#### Scenario: Artifacts are collected
- **WHEN** an automated run completes
- **THEN** the command copies the Mac bridge report and Linux receiver logs, counters, setup logs, restore logs, and captures into the local output directory when those artifacts are available

#### Scenario: Artifact collection is partial
- **WHEN** a remote artifact cannot be copied
- **THEN** the command records the failed artifact path and continues collecting remaining artifacts

### Requirement: RF Quality Report Generation
The system SHALL generate an `rf-quality-report` envelope from the automated run inputs and collected evidence.

#### Scenario: Report generation succeeds
- **WHEN** the Mac bridge report and receiver payload count are available
- **THEN** the command runs `rf-quality-report` with the profile tuple, Linux baseline, receiver artifacts, EFUSE report, Mac report, and recovered payload count

#### Scenario: Report generation is skipped
- **WHEN** the required report inputs are unavailable
- **THEN** the command leaves the raw artifacts in place and reports which inputs prevented RF-quality envelope generation

### Requirement: Dry Run Visibility
The system SHALL support a dry-run mode for inspecting the remote commands without claiming USB or transmitting RF.

#### Scenario: Dry run requested
- **WHEN** the operator invokes the automation command with dry-run enabled
- **THEN** the command prints the Mac and Linux commands that would run and exits without starting remote processes or writing RF-transmit commands

### Requirement: Hardware Mac Deploy Sync
The system SHALL support an opt-in deploy mode that copies the local checkout to a separate hardware-Mac run directory before starting RF-quality automation.

#### Scenario: Deploy sync is requested
- **WHEN** the operator enables deploy sync with a deploy path distinct from the hardware-Mac working checkout path
- **THEN** the command copies the local checkout to the deploy path, excludes repository metadata and build outputs, runs the bridge from the deploy path, and records the deploy path in run configuration

#### Scenario: Deploy sync would overwrite the working checkout
- **WHEN** the operator enables deploy sync with a deploy path equal to the configured hardware-Mac working checkout path
- **THEN** the command fails before syncing and reports that deploy sync requires a separate destination unless explicitly overridden

#### Scenario: Deploy sync is not requested
- **WHEN** the operator leaves deploy sync disabled
- **THEN** the command preserves the existing hardware-Mac checkout behavior and does not copy local files to the hardware Mac

### Requirement: Linux Peer Command Preflight
The system SHALL perform a Linux peer command preflight before starting automated RF transmission and SHALL record the discovered command paths, missing commands, and selected missing-command policy as artifacts.

#### Scenario: Required command is missing
- **WHEN** the Linux peer preflight cannot find a required command such as `python3`, `sudo`, `timeout`, `wfb_rx`, or `wfb_tx`
- **THEN** the automation MUST fail before RF transmission and MUST record the missing command in the preflight artifact

#### Scenario: Duplex smoke peer preflight fails
- **WHEN** the production duplex smoke runner cannot reach the Linux peer or
  cannot find the commands required for its monitor/radiotap setup
- **THEN** the runner MUST fail before claiming the Mac radio and MUST write
  peer-preflight evidence when the peer is reachable

#### Scenario: Optional command is missing
- **WHEN** an optional Linux command such as `iw`, `tcpdump`, `docker`, `ip`, or `ps` is unavailable
- **THEN** the automation MUST record the missing command and either skip the dependent step or fail according to the configured policy

#### Scenario: Peer isolation evidence command is missing
- **WHEN** peer isolation is required and `ps` or `grep` is unavailable on the Linux peer
- **THEN** the automation MUST fail before RF transmission and record `peer_isolation_requires_ps_and_grep` in the preflight policy blockers

### Requirement: Linux Peer Channel Evidence
The system SHALL record Linux peer channel-state evidence whenever the required tools are available, and SHALL mark the run degraded when channel-setting evidence is unavailable but the run policy allows continuing.

#### Scenario: Channel command is available
- **WHEN** `iw` is available during Linux setup
- **THEN** the automation MUST set the requested channel/bandwidth, capture interface info after the command, and include the setup output in collected artifacts

#### Scenario: Channel command is unavailable but allowed
- **WHEN** `iw` is unavailable and the run policy does not require it
- **THEN** the automation MUST continue only with a degraded setup log/preflight status that says channel state was not verified
