## Why

`radio-run` now uses runtime-owned execution and passes local plus receiver-backed
service smokes, but operators still start the production flow through the
diagnostic binary. Moving the reviewed production profile behind a smaller
production service binary reduces accidental exposure to diagnostic command
surfaces and gives future launchd/systemd packaging a stable executable.

## What Changes

- Add a production-focused binary entry point for the existing `radio-run`
  service flow.
- Reuse the checked-in production config, health-file, ready-marker, final
  report, heartbeat, TX/RX, and calibration semantics that already passed the
  robust receiver-backed smoke.
- Preserve `wfb-radio-diag radio-run` as a compatibility path during migration.
- Keep diagnostic-only experiments, PCAP/JSONL outputs, trace replay, generic
  register pokes, and TX-status probes out of the production binary.
- Update smoke automation so production gates can run the smaller binary while
  retaining an explicit fallback to the diagnostic compatibility command.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `production-runtime`: add a standalone production service binary contract and
  smoke-gate expectations while preserving diagnostic compatibility.

## Impact

- Affected crates: a new or existing production binary target plus
  `wfb-radio-diag` command adapter code where reusable parsing/config mapping
  must be shared.
- Affected runtime APIs: only if small command-facing adapters are needed; the
  existing runtime-owned execution path should remain the source of truth.
- Affected scripts: production and duplex smoke automation should be able to
  select the production service binary.
- Affected docs/OpenSpec: production runtime command-surface requirements and
  migration notes.
