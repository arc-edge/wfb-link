## Context

The Linux `rtl8812au` driver drives the dongle's enclosure LED in a slow
heartbeat pattern during normal operation. The macOS userspace runtime
currently only flashes the LED via two paths:

- `led-smoke`: an operator-triggered one-shot command that exercises
  software LED registers for hardware verification, not continuous use.
- `tx-activity-led` hook on `tx-once` / `tx-repeat`: blinks per TX
  submission burst, only inside diagnostic commands.

`docs/led-smoke.md` records the operator-confirmed visible LED mapping for
the AWUS036ACH:

> `REG_LEDCFG0` is the mapping used by the TX activity LED hook.
> Operator-confirmed visible state: `0x28` on, `0x20` off.

Production `radio-run` sessions don't drive the LED at all. From the
operator's perspective, a running radio looks identical to a hung one or
a dongle with USB issues.

## Goals / Non-Goals

**Goals:**
- Drive the visible enclosure LED at a slow steady cadence during the
  production runtime flow.
- Default-on so absence of a blink is itself a real signal.
- Configurable cadence and opt-out for operators who don't want the
  LED traffic on the USB control endpoint.
- Report toggle counters so the operator can confirm the heartbeat ran.
- Best-effort: do not abort the radio on a transient USB write failure.

**Non-Goals:**
- TX-activity-rate driven blink (already covered by `tx-activity-led`
  on diagnostic commands).
- RX-activity blink (out of scope; future enhancement if useful).
- Multi-pin / multi-mode heartbeat (only the operator-confirmed
  `led0`/`normal` configuration is targeted; other pin/mode combinations
  remain available via `led-smoke` for hardware exploration).
- Runtime-IQK or calibration evidence in the heartbeat (separate
  concerns).

## Decisions

### 1. Heartbeat is per-iteration polled, not async-timer-driven

**Decision:** the heartbeat exposes a synchronous `maybe_toggle(transport,
now)` call that the production bridge loop invokes on every iteration.
The heartbeat tracks `next_toggle_at: Instant` internally and skips the
USB write if the deadline has not elapsed.

**Why:** the production bridge loop already iterates many times per
second (TX poll + RX poll). Adding a single `Instant::now()` comparison
per iteration is free. Adding a separate async timer or thread would
require sharing the transport across thread boundaries, which the
current `RuntimeUsbTransport` enum does not support cleanly. The polled
approach also keeps the heartbeat synchronous with the loop's clock,
so a stalled loop produces a stalled LED — informative, not misleading.

**Alternatives considered:**
- *Spawn a tokio task / std thread.* Cleaner separation but introduces
  transport-sharing complexity and would mask a stalled main loop.
- *Toggle from a separate USB worker.* Same as above; also adds
  contention on the USB control endpoint.

### 2. Heartbeat is opt-out, not opt-in

**Decision:** default `enabled = true` for `radio-run`. Operators can
disable via `--no-heartbeat-led`.

**Why:** the heartbeat's value is precisely "the LED is off ⇒
something is wrong." If it's opt-in, an operator who never explicitly
enabled it would interpret a dark LED as "everything is fine, just
not blinking" — defeating the whole point. Default-on means a dark
LED is meaningful.

USB-traffic cost: at 1 Hz the heartbeat issues 2 control transfers per
second. Existing `radio-run` runs already do hundreds of control
transfers during init plus many per second during stats probes; the
heartbeat is not the dominant USB consumer.

### 3. Default cadence: 1 Hz overall (500 ms on / 500 ms off)

**Decision:** half-period 500 ms by default, configurable via
`--heartbeat-led-half-period-ms`. Bounded to a sane range
([50 ms, 5000 ms]).

**Why:** 1 Hz is fast enough to be unambiguously alive (operators
visually distinguish from a 0.1 Hz heartbeat that could be confused
with "blinks once and stops"), slow enough not to feel busy. Matches
the visual cadence operators are used to from typical "host alive"
LEDs on networking gear.

### 4. Pin and mode are fixed at the operator-confirmed values

**Decision:** heartbeat always uses `LedPin::Led0` + `LedMode::Normal`
(equivalently `REG_LEDCFG0 = 0x28/0x20`). No CLI override.

**Why:** that is the only mapping operator-confirmed to drive the
visible enclosure LED on the AWUS036ACH. Other pins/modes can be
explored via `led-smoke` against unattested hardware. If a future
adapter requires a different mapping, this constraint relaxes; until
then a CLI override is a footgun.

### 5. Best-effort writes; failures counted, never propagated

**Decision:** heartbeat USB write errors increment
`toggles_failed`, do not return an error to the caller, and do not
abort the loop.

**Why:** a flaky LED control register must never bring down the
radio. The radio's actual TX/RX path uses bulk endpoints, not
control; an LED control transfer failing is fully orthogonal to a
TX/RX path failure. The counter surfaces the situation if it
actually happens.

### 6. LED is turned off at session end

**Decision:** when the production runtime flow returns (success or
failure), `LedHeartbeat::turn_off` is called once before the session
is dropped.

**Why:** leaving the LED in the "on" state after exit is
operator-confusing — looks like the radio is still running. Turning
it off explicitly is the visible signal that `radio-run` exited.

## Risks / Trade-offs

- **[Risk]** USB control endpoint contention with rapid stats probes
  or calibration probes happening on the same control endpoint.
  **Mitigation:** 1 Hz cadence is extremely sparse; existing init
  flows do hundreds of control transfers per second without issue.
  In the worst case the heartbeat write is queued behind other
  control work and arrives slightly late; no functional consequence
  beyond LED jitter.
- **[Risk]** A future RTL8812AU variant (e.g. AWUS036ACHM) maps the
  visible enclosure LED to a different pin/mode. **Mitigation:**
  the `--no-heartbeat-led` flag is the immediate workaround; a
  future change can broaden the pin/mode selection once a second
  visible LED mapping has operator confirmation. Documented in
  `docs/led-smoke.md`.
- **[Risk]** Operators interpret the heartbeat as a TX-activity
  indicator. **Mitigation:** the report's `heartbeat_led` counters
  are clearly labeled. The heartbeat docstring + flag help text
  describe it as "host is alive," not "TX is happening." The
  pre-existing `tx-activity-led` hook covers actual TX-rate blink
  for diagnostic commands.

## Migration Plan

This is a strictly additive change. Without `--no-heartbeat-led` the
behavior of `radio-run` differs only in:

1. The visible enclosure LED blinks at 1 Hz (previously: stayed in
   whatever state the last LED-touching code path left it in).
2. The runtime report includes a `heartbeat_led` block with toggle
   counters.

No CLI breaks, no report-schema breaks, no openspec capability
removals. The feature can be reverted by a single revert of the
introducing change.

## Open Questions

- **Q1:** Should the heartbeat run during `runtime-flow` as well as
  `radio-run`? **Lean: yes**, because `runtime-flow` is also a
  production-shape command. Defer to follow-up if there's a reason
  to keep `runtime-flow` LED-silent.
- **Q2:** Should the heartbeat encode a state by varying cadence
  (e.g., faster blink while paired, slower while idle)? **Lean: no
  for v1.** Just "alive" is enough for first iteration. Operators
  can request encoded patterns later.
