## Why

Production smoke automation currently proves the service path only with the
default RF profile, while the production service itself already supports guarded
EFUSE-derived TX power and calibration profile selection. That leaves the
validated remote-adapter EFUSE smoke path as a manual command instead of a
repeatable production gate.

## What Changes

- Extend `scripts/run-production-radio-smoke.sh` with the same RF profile
  environment controls used by duplex and RF-quality automation.
- Pass TX power mode/source/safety profile and TX calibration profile through
  to both service and diagnostic `radio-run` smoke commands.
- Include the selected RF profile and TX power evidence in smoke summaries and
  dry-run output.
- Validate the new path with dry-run coverage and a remote hardware-Mac
  tx-positive service smoke using the attached adapter.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `production-runtime`: Production smoke automation must be able to exercise
  guarded RF profiles rather than only the default profile.

## Impact

- `scripts/run-production-radio-smoke.sh`
- Production smoke documentation in `README.md` and runtime boundary notes as
  needed
- Remote hardware-Mac smoke artifacts under `/tmp`
