## 1. Runtime helper

- [x] 1.1 Add `crates/wfb-radio-runtime/src/led_heartbeat.rs` with `LedHeartbeatConfig`, `LedHeartbeat`, and `LedHeartbeatCounters` types.
- [x] 1.2 Implement `LedHeartbeat::new`, `maybe_toggle`, `turn_off`, `counters`, and `config` accessors.
- [x] 1.3 Pin the heartbeat to `REG_LEDCFG0 = 0x004c` with `0x28` (on) / `0x20` (off) values matching the `led-smoke` operator-confirmed visible state.
- [x] 1.4 Bound the half-period to `[50 ms, 5000 ms]` at config-validation time via `LedHeartbeatConfig::validate`.
- [x] 1.5 Re-export the heartbeat types from `wfb-radio-runtime::lib`.

## 2. Production runtime hook

- [x] 2.1 Add `--no-heartbeat-led` and `--heartbeat-led-half-period-ms <ms>` flags to `RadioRunArgs`.
- [x] 2.2 Translate CLI flags into `LedHeartbeatConfig` during runtime config validation; reject out-of-bounds half-period before opening the radio.
- [x] 2.3 Instantiate `LedHeartbeat` in the bridge run setup; pass `&session.transport` and `Instant::now()` to `maybe_toggle` once per bridge loop iteration.
- [x] 2.4 Call `LedHeartbeat::turn_off` exactly once before the runtime session is dropped at the end of `bridge_run_report`.
- [x] 2.5 Add a per-outer-iteration tick callback to `run_production_bridge_loop` so the executor itself drives periodic state. Wrap the bridge-run session in `RefCell` for the loop body so the heartbeat tick and the step handler can take disjoint borrows; move the heartbeat tick from inside the step handler into the executor's iteration-tick callback.

## 3. Report integration

- [x] 3.1 Add `heartbeat_led: HeartbeatLedReport { enabled, half_period_ms, toggles_attempted, toggles_succeeded, toggles_failed }` to the `BridgeRunReport` JSON shape (surfaces directly in `bridge-run` JSON output and via the embedded bridge report from any caller).
- [ ] 3.2 Promote the field through `RuntimeFlowReport` and `ProductionRuntimeFlowReport` so `radio-run` exposes it as a top-level field in its JSON output. _(Deferred: today the heartbeat counters appear inside the `bridge-run` report payload but not at the runtime-flow level. Behavior is unaffected.)_
- [ ] 3.3 Render a single human-readable line in the `radio-run` text output summarizing the heartbeat (`heartbeat-led: enabled, 500 ms, 14 toggles (14 ok, 0 failed)`). _(Deferred: paired with 3.2 since the human print path is at the runtime-flow level.)_

## 4. Tests

- [x] 4.1 Unit test: `maybe_toggle` issues no USB write before half-period elapses.
- [x] 4.2 Unit test: `maybe_toggle` after half-period elapses issues exactly one write with the next state value.
- [x] 4.3 Unit test: alternation across multiple toggles (on → off → on → off).
- [x] 4.4 Unit test: failed USB write increments `toggles_failed` and not `toggles_succeeded`.
- [x] 4.5 Unit test: disabled heartbeat is fully a no-op for `maybe_toggle` and `turn_off`.
- [x] 4.6 Unit test: `turn_off` issues one off write when enabled.
- [x] 4.7 Unit test: `LedHeartbeatConfig::validate` rejects half-periods below 50 ms and above 5000 ms.
- [x] 4.8 Unit test: `RadioRunArgs` parses the new flags with reasonable defaults and rejects out-of-bounds half-period.
- [x] 4.9 Unit test: `run_production_bridge_loop` invokes the iteration-tick callback once per outer iteration that does work, and skips it when stop is requested before any iteration runs.

## 5. Validation

- [x] 5.1 `openspec validate add-runtime-led-heartbeat` passes.
- [x] 5.2 `cargo fmt` clean.
- [x] 5.3 `cargo build -p wfb-radio-runtime -p wfb-radio-diag` clean.
- [x] 5.4 `cargo test -p wfb-radio-runtime -p wfb-radio-diag` passes including the new tests.
- [ ] 5.5 Operator verification on the hardware Mac: run `radio-run --macos-usbhost --channel 36 --bandwidth 20 ... --duration-ms 5000 --i-understand-this-writes-registers` and visually confirm the AWUS036ACH enclosure LED blinks at the configured cadence; `heartbeat_led.toggles_succeeded` ≥ 8 at 500 ms half-period over 5 s.
