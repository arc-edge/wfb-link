## ADDED Requirements

### Requirement: Linux Peer Command Preflight
The system SHALL perform a Linux peer command preflight before starting automated RF transmission and SHALL record the discovered command paths, missing commands, and selected missing-command policy as artifacts.

#### Scenario: Required command is missing
- **WHEN** the Linux peer preflight cannot find a required command such as `python3`, `sudo`, `timeout`, `wfb_rx`, or `wfb_tx`
- **THEN** the automation MUST fail before RF transmission and MUST record the missing command in the preflight artifact

#### Scenario: Optional command is missing
- **WHEN** an optional Linux command such as `iw`, `tcpdump`, `docker`, `ip`, or `ps` is unavailable
- **THEN** the automation MUST record the missing command and either skip the dependent step or fail according to the configured policy

### Requirement: Linux Peer Channel Evidence
The system SHALL record Linux peer channel-state evidence whenever the required tools are available, and SHALL mark the run degraded when channel-setting evidence is unavailable but the run policy allows continuing.

#### Scenario: Channel command is available
- **WHEN** `iw` is available during Linux setup
- **THEN** the automation MUST set the requested channel/bandwidth, capture interface info after the command, and include the setup output in collected artifacts

#### Scenario: Channel command is unavailable but allowed
- **WHEN** `iw` is unavailable and the run policy does not require it
- **THEN** the automation MUST continue only with a degraded setup log/preflight status that says channel state was not verified
