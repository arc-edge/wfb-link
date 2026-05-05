## ADDED Requirements

### Requirement: Production Runtime LED Heartbeat Hook

The production runtime command SHALL invoke the runtime LED heartbeat at
each iteration of the production bridge loop, and SHALL invoke its
turn-off method exactly once when the production runtime flow returns.

#### Scenario: Heartbeat is invoked from the bridge loop

- **WHEN** the production runtime command runs the bridge loop with
  heartbeat enabled (default)
- **THEN** the production command SHALL call the heartbeat's
  `maybe_toggle` on every loop iteration with the current `Instant::now()`

#### Scenario: Heartbeat is turned off on flow exit

- **WHEN** the production runtime flow returns (success or failure)
- **THEN** the production command SHALL call the heartbeat's `turn_off`
  exactly once before the runtime session is dropped

### Requirement: Production Runtime Heartbeat Configuration Flags

The production runtime command SHALL accept `--no-heartbeat-led` to
disable the heartbeat and `--heartbeat-led-half-period-ms <ms>` to
override the toggle half-period within bounded limits.

#### Scenario: Default flags result in enabled 500 ms half-period

- **WHEN** the operator runs the production runtime command without
  any heartbeat flags
- **THEN** the heartbeat SHALL be enabled
- **AND** the half-period SHALL be 500 ms

#### Scenario: --no-heartbeat-led disables the heartbeat

- **WHEN** the operator passes `--no-heartbeat-led`
- **THEN** the heartbeat SHALL be configured with `enabled = false`

#### Scenario: --heartbeat-led-half-period-ms within range applies

- **WHEN** the operator passes `--heartbeat-led-half-period-ms <ms>`
  with `50 <= ms <= 5000`
- **THEN** the heartbeat half-period SHALL be `ms` milliseconds

#### Scenario: --heartbeat-led-half-period-ms out of range is rejected

- **WHEN** the operator passes `--heartbeat-led-half-period-ms <ms>`
  with `ms < 50` or `ms > 5000`
- **THEN** the production runtime command SHALL fail at argument
  validation before opening the radio

### Requirement: Production Runtime Report Surfaces Heartbeat Counters

The production runtime report SHALL include a `heartbeat_led` block with
toggle counters, the configured half-period, and the enabled flag, so
operators can verify the heartbeat ran.

#### Scenario: Report includes heartbeat block on a normal run

- **WHEN** the production runtime command exits after a normal run
- **THEN** its report SHALL include a `heartbeat_led` field with at
  least: `enabled`, `half_period_ms`, `toggles_attempted`,
  `toggles_succeeded`, `toggles_failed`

#### Scenario: Report reflects disabled heartbeat

- **WHEN** the operator ran the command with `--no-heartbeat-led`
- **THEN** the report's `heartbeat_led.enabled` SHALL be false
- **AND** the toggle counters SHALL be zero
