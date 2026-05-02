# rf-quality-run-automation Specification

## Purpose

Define the operator-facing automation for reproducible RF-quality runs across the local checkout, hardware Mac, and Linux WFB peer.

## Requirements
### Requirement: Close-Range Run Orchestration
The system SHALL provide a single operator-facing command that orchestrates the accepted close-range RF-quality workflow across the hardware Mac and Linux WFB peer.

#### Scenario: Close-range automation run starts
- **WHEN** the operator invokes the automation command with the required host, channel, payload, and report settings
- **THEN** the command starts the Mac-side relay and bridge listener, prepares the Linux WFB peer, runs the Linux sender and receiver, and records the produced artifact paths

#### Scenario: Close-range automation run rejects missing settings
- **WHEN** required host, repository, firmware, key, or network settings are missing
- **THEN** the command fails before starting RF transmission and reports the missing setting

### Requirement: Linux Peer Control
The system SHALL control the Linux WFB peer in a bounded and reversible way during automated RF-quality runs.

#### Scenario: Linux peer is prepared
- **WHEN** an automated run begins
- **THEN** the command stops the configured WFB service container, pins the monitor interface to the requested channel and bandwidth, starts bounded `tcpdump`, `wfb_rx`, and `wfb_tx` processes, and records setup output

#### Scenario: Linux peer is restored
- **WHEN** an automated run finishes or fails after Linux setup begins
- **THEN** the command attempts to stop test processes, restart the configured WFB service container, and record service restore output

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
