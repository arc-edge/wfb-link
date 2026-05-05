## Why

The direct `radio-run` runtime path now passes local and receiver-backed smokes,
but running it as a production component still depends on long CLI invocations
and external script interpretation of readiness. We need a service-oriented
slice that makes the accepted production profile reproducible, observable, and
safe to supervise before spending more time on distance-specific RF work.

## What Changes

- Add a production runtime configuration file path for `radio-run` so operators
  can run a named, reviewed profile without reconstructing a long flag list.
- Add a service-oriented run mode that preserves the current `radio-run` data
  path while writing explicit ready, health, and final-state artifacts that a
  supervisor can consume.
- Add runtime-owned health classification for init readiness, heartbeat status,
  TX drops/failures, RX forwarding, peer/receiver-backed gates when supplied,
  stop reason, and structured shutdown outcome.
- Add smoke automation for the accepted robust short-range tuple so production
  service changes are gated by a receiver-backed baseline, not only USB TX
  submission.
- Keep runtime IQK, EFUSE-derived TX power, and longer-distance promotion
  opt-in and receiver-gated; this change does not alter RF calibration
  algorithms or promote experimental profiles.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `production-runtime`: add production config-file loading and service health
  artifacts for `radio-run`.
- `wfb-radio-runtime`: add runtime-owned service health/final-state reporting
  helpers that do not depend on diagnostic structs.
- `rf-quality-run-automation`: add a repeatable production-service smoke gate
  using the accepted robust short-range receiver-backed tuple.

## Impact

- Affected crates: `wfb-radio-diag`, `wfb-radio-runtime`.
- Affected scripts: production smoke and duplex/profile automation that should
  be able to consume the new config and health files.
- Affected docs/OpenSpec: production runtime and RF-quality automation
  acceptance criteria.
- No WFB packet format changes, no calibration default changes, and no removal
  of existing CLI flags.
