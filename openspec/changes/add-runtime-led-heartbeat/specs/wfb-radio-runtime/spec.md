## ADDED Requirements

### Requirement: Runtime LED Heartbeat Toggle

The runtime library SHALL provide an LED heartbeat helper that toggles a
configured RTL8812AU LED register at a configurable half-period during a
production session. The helper SHALL be opt-out via configuration, default
to enabled, and SHALL drive the visible enclosure LED via `REG_LEDCFG0`
using the operator-confirmed `0x28` (on) / `0x20` (off) values.

#### Scenario: Heartbeat toggles after half-period elapses

- **WHEN** the heartbeat is enabled and the configured half-period has
  elapsed since the last toggle
- **THEN** the runtime library SHALL issue exactly one USB control write
  to `REG_LEDCFG0` with the next state value
- **AND** the heartbeat SHALL alternate between on (`0x28`) and off
  (`0x20`) on subsequent toggles

#### Scenario: Heartbeat skips writes within half-period

- **WHEN** the heartbeat is enabled and `maybe_toggle` is called before
  the half-period has elapsed since the last toggle
- **THEN** the runtime library SHALL NOT issue any USB control write
- **AND** the heartbeat counters SHALL NOT change

#### Scenario: Heartbeat is disabled

- **WHEN** the heartbeat is configured with `enabled = false`
- **THEN** `maybe_toggle` SHALL NOT issue any USB control write
- **AND** `turn_off` SHALL NOT issue any USB control write

#### Scenario: USB write failures are counted but not propagated

- **WHEN** the heartbeat attempts a toggle and the underlying USB
  control write returns an error
- **THEN** `toggles_failed` SHALL be incremented
- **AND** `toggles_succeeded` SHALL NOT be incremented
- **AND** the failure SHALL NOT be returned to the caller

### Requirement: Runtime LED Heartbeat Counters

The runtime library SHALL expose toggle counters that reflect attempted,
succeeded, and failed USB writes, so a calling production runtime can
report them without sampling internal heartbeat state.

#### Scenario: Counters reflect successful toggles

- **WHEN** the heartbeat performs N successful toggles
- **THEN** `toggles_attempted` SHALL equal N
- **AND** `toggles_succeeded` SHALL equal N
- **AND** `toggles_failed` SHALL equal 0

#### Scenario: Counters distinguish failed toggles

- **WHEN** the heartbeat attempts N toggles and M of them fail at the
  USB control write
- **THEN** `toggles_attempted` SHALL equal N
- **AND** `toggles_succeeded` SHALL equal N - M
- **AND** `toggles_failed` SHALL equal M

### Requirement: Runtime LED Heartbeat Off On Session End

The runtime library SHALL provide a best-effort `turn_off` method that
issues a single USB control write to `REG_LEDCFG0` with the off value,
intended to be called once when the production runtime flow exits.

#### Scenario: Turn-off issues an off write when enabled

- **WHEN** `turn_off` is called with the heartbeat enabled
- **THEN** the runtime library SHALL issue exactly one USB control write
  with the off value (`0x20`)

#### Scenario: Turn-off is a no-op when disabled

- **WHEN** `turn_off` is called with the heartbeat configured
  `enabled = false`
- **THEN** the runtime library SHALL NOT issue any USB control write
