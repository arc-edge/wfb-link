## Why

`wfb-radio-service` is now the production entry point, but RF-quality and
calibration experiments still need to fall back to diagnostic `radio-run` for
TX-power and calibration profile selection. Production-readiness evidence should
exercise the same command surface we expect to ship.

## What Changes

- Add production service CLI/config controls for the runtime TX-power mode and
  calibration profile already supported by the runtime flow.
- Preserve the existing service guardrails: RF-changing settings still require
  explicit transmit authorization, and invalid profile or power-mode names fail
  before USB open.
- Update RF-quality automation so `MAC_RADIO_COMMAND=radio-service` can run the
  same TX-power/profile experiments currently limited to `radio-run`.
- Include the selected production command and RF profile controls in smoke and
  RF-quality evidence so comparisons remain command-specific.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `production-runtime`: Extend the standalone service command surface to expose
  production-safe RF profile controls.
- `rf-quality-run-automation`: Allow close-range RF-quality automation to select
  the service command for TX-power and calibration profile runs.

## Impact

- Affected code: `crates/wfb-radio-service`, runtime config mapping tests, RF
  quality automation scripts, smoke evidence summarization, docs.
- No new dependencies or breaking changes.
