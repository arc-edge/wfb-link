## Why

Operators using the macOS RTL8812AU runtime have no visible indication that
the radio is initialized and the host is alive. The Linux `rtl8812au` driver
drives the dongle's enclosure LED at a slow steady cadence during normal
operation, which gives operators a useful "this radio is breathing" signal
in the field.

`wfb-radio-diag` already has confirmed software-LED control via the
`led-smoke` command (operator-confirmed `REG_LEDCFG0 = 0x28/0x20` toggle on
the AWUS036ACH visible blue enclosure LED) and a TX-activity LED hook on
`tx-once` / `tx-repeat`. What is missing is a continuous heartbeat tied to
the production `radio-run` flow itself.

## What Changes

- Add a runtime-owned `LedHeartbeat` that toggles the visible enclosure LED
  at a configurable cadence during a `radio-run` session.
- Hook the heartbeat into the production bridge loop so it ticks on every
  loop iteration but only issues USB control writes when the toggle interval
  has elapsed.
- Default to enabled at 1 Hz overall (500 ms on, 500 ms off) on `led0` in
  normal mode (the operator-confirmed visible enclosure LED for the
  AWUS036ACH).
- Add `--no-heartbeat-led` and `--heartbeat-led-half-period-ms` flags to
  `radio-run` so operators can disable the heartbeat or change its cadence.
- Best-effort turn-off on session end so the LED is not left in the on state
  after `radio-run` exits.
- Surface heartbeat counters (`toggles_attempted`, `toggles_succeeded`,
  `toggles_failed`) in the production runtime report so operators can
  confirm the heartbeat actually ran.

The heartbeat is opt-out, not opt-in, because the absence of a blink would
otherwise be operator-misleading (looks identical to "host is asleep" or
"chip lost USB").

## Capabilities

### Modified Capabilities

- `wfb-radio-runtime`: gains a runtime LED heartbeat helper and a
  per-session toggle counter shape.
- `production-runtime`: production runtime command surfaces a heartbeat-LED
  configuration knob and reports its toggle counters in the run report.

## Impact

- Affected crates: `wfb-radio-runtime` (new module), `wfb-radio-diag`
  (radio-run argument parsing, loop hook, report integration).
- Affected commands: `radio-run`. `runtime-flow`, `bridge-tx-listen`,
  `bridge-tx-bench`, and other diagnostic commands are unchanged.
- Affected hardware: AWUS036ACH and other RTL8812AU-class adapters whose
  visible enclosure LED maps to `REG_LEDCFG0` in normal mode (the only
  configuration operator-confirmed in `docs/led-smoke.md`).
- No protocol or wire-format changes. Heartbeat traffic is on the existing
  USB control endpoint at 1 control transfer per 500 ms; trivially below
  the noise floor of the existing init + register-probe traffic.
- Best-effort: a transient USB write failure during the heartbeat does
  not stop the radio. Failures are counted and surfaced in the report.
