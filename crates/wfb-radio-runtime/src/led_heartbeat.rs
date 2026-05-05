//! Periodic LED heartbeat for the production WFB runtime.
//!
//! Drives the visible enclosure LED of an RTL8812AU-class adapter at a
//! configurable cadence so an operator has a "host is alive" indicator
//! during a long-running session. Mirrors the slow blink the Linux
//! `rtl8812au` driver produces.
//!
//! See `docs/led-smoke.md` for the operator-confirmed mapping the
//! heartbeat targets:
//!
//! - `REG_LEDCFG0` (0x004c) = visible blue enclosure LED on AWUS036ACH
//! - `0x28` = LED on, `0x20` = LED off (normal mode, led0)
//!
//! The heartbeat is per-iteration polled rather than driven by a tokio
//! timer or background thread. The production bridge loop already
//! iterates many times per second; calling `maybe_toggle` once per
//! iteration is free, and tying the heartbeat to the loop's clock means
//! a stalled loop produces a stalled LED — which is exactly the signal
//! we want.

use std::time::{Duration, Instant};

use radio_core::rtl8812au::Rtl8812auUsbTransport;
use serde::Serialize;

/// Address of the visible-LED control register (RTL8812AU `REG_LEDCFG0`).
/// Confirmed against the AWUS036ACH blue enclosure LED in
/// `docs/led-smoke.md`.
pub const REG_LEDCFG0: u16 = 0x004c;

/// `REG_LEDCFG0` value that turns the visible LED on (normal mode, led0).
/// Operator-confirmed; bit 5 (mode) + bit 3 (force on).
pub const LED_ON_VALUE: u8 = 0x28;

/// `REG_LEDCFG0` value that turns the visible LED off (normal mode, led0).
/// Operator-confirmed; bit 5 (mode) only.
pub const LED_OFF_VALUE: u8 = 0x20;

/// Default heartbeat half-period: 500 ms (1 Hz overall blink).
pub const DEFAULT_HEARTBEAT_HALF_PERIOD_MS: u64 = 500;

/// Lower bound on configurable half-period. Below this the LED appears
/// solid-on to operators and the USB control endpoint sees pointless
/// traffic.
pub const MIN_HEARTBEAT_HALF_PERIOD_MS: u64 = 50;

/// Upper bound on configurable half-period. Above this an operator
/// might mistake a single blink for "blinks once and stops."
pub const MAX_HEARTBEAT_HALF_PERIOD_MS: u64 = 5000;

/// USB control transfer timeout for an LED write. Generous; LED writes
/// are not on the critical path.
const LED_WRITE_TIMEOUT: Duration = Duration::from_millis(100);

/// Configuration for the LED heartbeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LedHeartbeatConfig {
    pub enabled: bool,
    pub half_period_ms: u64,
}

impl Default for LedHeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            half_period_ms: DEFAULT_HEARTBEAT_HALF_PERIOD_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedHeartbeatConfigError {
    pub message: String,
}

impl std::fmt::Display for LedHeartbeatConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LedHeartbeatConfigError {}

impl LedHeartbeatConfig {
    /// Reject half-periods outside `[MIN_HEARTBEAT_HALF_PERIOD_MS,
    /// MAX_HEARTBEAT_HALF_PERIOD_MS]`. A disabled heartbeat is always
    /// valid regardless of half-period.
    pub fn validate(&self) -> Result<(), LedHeartbeatConfigError> {
        if !self.enabled {
            return Ok(());
        }
        if self.half_period_ms < MIN_HEARTBEAT_HALF_PERIOD_MS {
            return Err(LedHeartbeatConfigError {
                message: format!(
                    "heartbeat half-period {} ms is below minimum {} ms",
                    self.half_period_ms, MIN_HEARTBEAT_HALF_PERIOD_MS
                ),
            });
        }
        if self.half_period_ms > MAX_HEARTBEAT_HALF_PERIOD_MS {
            return Err(LedHeartbeatConfigError {
                message: format!(
                    "heartbeat half-period {} ms is above maximum {} ms",
                    self.half_period_ms, MAX_HEARTBEAT_HALF_PERIOD_MS
                ),
            });
        }
        Ok(())
    }

    fn half_period(&self) -> Duration {
        Duration::from_millis(self.half_period_ms)
    }
}

/// Bounded counters surfaced via the production runtime report so the
/// operator can confirm the heartbeat actually drove USB writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct LedHeartbeatCounters {
    pub toggles_attempted: u64,
    pub toggles_succeeded: u64,
    pub toggles_failed: u64,
}

/// Periodic LED toggler. Owns no transport reference; the caller passes
/// one in on each `maybe_toggle` / `turn_off` call.
#[derive(Debug)]
pub struct LedHeartbeat {
    config: LedHeartbeatConfig,
    counters: LedHeartbeatCounters,
    state_on: bool,
    next_toggle_at: Instant,
}

impl LedHeartbeat {
    /// Create a new heartbeat. The first toggle fires `half_period`
    /// after `started`; until then `maybe_toggle` is a no-op.
    ///
    /// Panics if `config.validate()` would have returned an error.
    /// Callers are expected to validate before constructing — a bad
    /// half-period should fail at CLI parse time, not here.
    pub fn new(config: LedHeartbeatConfig, started: Instant) -> Self {
        debug_assert!(
            config.validate().is_ok(),
            "LedHeartbeat::new called with unvalidated config"
        );
        let next_toggle_at = started + config.half_period();
        Self {
            config,
            counters: LedHeartbeatCounters::default(),
            state_on: false,
            next_toggle_at,
        }
    }

    /// Read the current snapshot of toggle counters.
    pub fn counters(&self) -> LedHeartbeatCounters {
        self.counters
    }

    /// Read the configuration.
    pub fn config(&self) -> LedHeartbeatConfig {
        self.config
    }

    /// Current state-on flag (for tests and reporting).
    pub fn state_on(&self) -> bool {
        self.state_on
    }

    /// If the heartbeat is enabled and `now >= next_toggle_at`, flip the
    /// LED state and write to `REG_LEDCFG0`. Otherwise, no-op. USB write
    /// failures are counted but not propagated — a flaky LED must never
    /// stop the radio.
    pub fn maybe_toggle<T: Rtl8812auUsbTransport>(&mut self, transport: &T, now: Instant) {
        if !self.config.enabled {
            return;
        }
        if now < self.next_toggle_at {
            return;
        }
        self.state_on = !self.state_on;
        // Schedule the next toggle from `now` rather than from the
        // previous `next_toggle_at`, so a slow loop iteration doesn't
        // produce a flurry of catch-up toggles. The blink slows under
        // load, which is fine.
        self.next_toggle_at = now + self.config.half_period();
        let value = if self.state_on {
            LED_ON_VALUE
        } else {
            LED_OFF_VALUE
        };
        self.write_register(transport, value);
    }

    /// Best-effort turn-off. Intended to be called once at session end.
    /// Counts as one toggle attempt to keep the operator's "actual
    /// blinks" math honest.
    pub fn turn_off<T: Rtl8812auUsbTransport>(&mut self, transport: &T) {
        if !self.config.enabled {
            return;
        }
        self.state_on = false;
        self.write_register(transport, LED_OFF_VALUE);
    }

    fn write_register<T: Rtl8812auUsbTransport>(&mut self, transport: &T, value: u8) {
        self.counters.toggles_attempted = self.counters.toggles_attempted.saturating_add(1);
        match transport.write_vendor(
            REG_LEDCFG0,
            0,
            std::slice::from_ref(&value),
            LED_WRITE_TIMEOUT,
        ) {
            Ok(_) => {
                self.counters.toggles_succeeded = self.counters.toggles_succeeded.saturating_add(1);
            }
            Err(_) => {
                self.counters.toggles_failed = self.counters.toggles_failed.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use radio_core::UsbError;
    use std::cell::RefCell;

    /// Test transport that records every register write and can be
    /// programmed to fail after N writes.
    struct FakeTransport {
        writes: RefCell<Vec<(u16, u16, Vec<u8>)>>,
        fail_after: RefCell<Option<usize>>,
    }

    impl FakeTransport {
        fn new() -> Self {
            Self {
                writes: RefCell::new(Vec::new()),
                fail_after: RefCell::new(None),
            }
        }

        fn fail_after(self, n: usize) -> Self {
            *self.fail_after.borrow_mut() = Some(n);
            self
        }

        fn writes(&self) -> Vec<(u16, u16, Vec<u8>)> {
            self.writes.borrow().clone()
        }
    }

    impl Rtl8812auUsbTransport for FakeTransport {
        fn read_vendor(
            &self,
            _value: u16,
            _index: u16,
            _data: &mut [u8],
            _timeout: Duration,
        ) -> Result<usize, UsbError> {
            Ok(0)
        }

        fn write_vendor(
            &self,
            value: u16,
            index: u16,
            data: &[u8],
            _timeout: Duration,
        ) -> Result<usize, UsbError> {
            let writes_before = self.writes.borrow().len();
            if let Some(threshold) = *self.fail_after.borrow() {
                if writes_before >= threshold {
                    return Err(UsbError::Backend(format!(
                        "synthetic failure after {threshold} writes"
                    )));
                }
            }
            self.writes.borrow_mut().push((value, index, data.to_vec()));
            Ok(data.len())
        }
    }

    fn cfg(half_period_ms: u64) -> LedHeartbeatConfig {
        LedHeartbeatConfig {
            enabled: true,
            half_period_ms,
        }
    }

    #[test]
    fn skips_write_before_half_period() {
        let mut hb = LedHeartbeat::new(cfg(500), Instant::now());
        let t = FakeTransport::new();
        hb.maybe_toggle(&t, Instant::now() + Duration::from_millis(100));
        assert!(t.writes().is_empty());
        assert_eq!(hb.counters(), LedHeartbeatCounters::default());
    }

    #[test]
    fn writes_once_after_half_period() {
        let started = Instant::now();
        let mut hb = LedHeartbeat::new(cfg(100), started);
        let t = FakeTransport::new();
        hb.maybe_toggle(&t, started + Duration::from_millis(150));
        let writes = t.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, REG_LEDCFG0);
        assert_eq!(writes[0].2, vec![LED_ON_VALUE]); // first toggle is on
        assert_eq!(hb.counters().toggles_attempted, 1);
        assert_eq!(hb.counters().toggles_succeeded, 1);
        assert_eq!(hb.counters().toggles_failed, 0);
    }

    #[test]
    fn alternates_state_across_toggles() {
        let started = Instant::now();
        let mut hb = LedHeartbeat::new(cfg(100), started);
        let t = FakeTransport::new();

        for i in 1..=4 {
            hb.maybe_toggle(&t, started + Duration::from_millis(150 * i));
        }
        let writes = t.writes();
        assert_eq!(writes.len(), 4);
        assert_eq!(writes[0].2, vec![LED_ON_VALUE]);
        assert_eq!(writes[1].2, vec![LED_OFF_VALUE]);
        assert_eq!(writes[2].2, vec![LED_ON_VALUE]);
        assert_eq!(writes[3].2, vec![LED_OFF_VALUE]);
    }

    #[test]
    fn one_call_produces_at_most_one_toggle_even_after_long_gap() {
        // After a long stall, the heartbeat catches up by emitting only
        // one toggle this iteration; the next one waits another half
        // period. This avoids a USB write storm post-stall.
        let started = Instant::now();
        let mut hb = LedHeartbeat::new(cfg(100), started);
        let t = FakeTransport::new();
        hb.maybe_toggle(&t, started + Duration::from_millis(10_000));
        assert_eq!(t.writes().len(), 1);
        // Immediately calling again: no second toggle, even though
        // wall-clock would justify many.
        hb.maybe_toggle(&t, started + Duration::from_millis(10_000));
        assert_eq!(t.writes().len(), 1);
    }

    #[test]
    fn failed_writes_count_failures_not_successes() {
        let started = Instant::now();
        let mut hb = LedHeartbeat::new(cfg(100), started);
        let t = FakeTransport::new().fail_after(0);

        hb.maybe_toggle(&t, started + Duration::from_millis(150));
        assert!(t.writes().is_empty());
        assert_eq!(hb.counters().toggles_attempted, 1);
        assert_eq!(hb.counters().toggles_succeeded, 0);
        assert_eq!(hb.counters().toggles_failed, 1);
        // Failure does not propagate; subsequent toggles still attempt.
        hb.maybe_toggle(&t, started + Duration::from_millis(300));
        assert_eq!(hb.counters().toggles_attempted, 2);
        assert_eq!(hb.counters().toggles_failed, 2);
    }

    #[test]
    fn disabled_heartbeat_is_full_noop() {
        let started = Instant::now();
        let mut hb = LedHeartbeat::new(
            LedHeartbeatConfig {
                enabled: false,
                half_period_ms: 100,
            },
            started,
        );
        let t = FakeTransport::new();
        hb.maybe_toggle(&t, started + Duration::from_millis(10_000));
        hb.turn_off(&t);
        assert!(t.writes().is_empty());
        assert_eq!(hb.counters(), LedHeartbeatCounters::default());
    }

    #[test]
    fn turn_off_writes_off_value_when_enabled() {
        let started = Instant::now();
        let mut hb = LedHeartbeat::new(cfg(100), started);
        let t = FakeTransport::new();
        // Toggle to on first.
        hb.maybe_toggle(&t, started + Duration::from_millis(150));
        assert!(hb.state_on());
        hb.turn_off(&t);
        let writes = t.writes();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[1].2, vec![LED_OFF_VALUE]);
        assert!(!hb.state_on());
    }

    #[test]
    fn validate_rejects_half_period_below_minimum() {
        let cfg = LedHeartbeatConfig {
            enabled: true,
            half_period_ms: 10,
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.message.contains("below minimum"));
    }

    #[test]
    fn validate_rejects_half_period_above_maximum() {
        let cfg = LedHeartbeatConfig {
            enabled: true,
            half_period_ms: 10_000,
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.message.contains("above maximum"));
    }

    #[test]
    fn validate_accepts_disabled_with_any_half_period() {
        let cfg = LedHeartbeatConfig {
            enabled: false,
            half_period_ms: 0,
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_accepts_default() {
        assert!(LedHeartbeatConfig::default().validate().is_ok());
    }
}
