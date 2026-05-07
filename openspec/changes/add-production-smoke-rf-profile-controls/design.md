## Context

`wfb-radio-service` now owns production RF profile selection, including
EFUSE-derived TX power and TX calibration profile controls. The duplex and
RF-quality runners can pass those settings through, but
`run-production-radio-smoke.sh` still starts service/diagnostic `radio-run`
without RF profile arguments. The remote-adapter EFUSE smoke path therefore has
to be run manually and is easy to drift from the production gate.

## Goals / Non-Goals

**Goals:**

- Make production smoke automation accept the same RF profile environment tuple
  used by the rest of the production/RF-quality scripts.
- Keep current-default behavior unchanged when no RF profile variables are set.
- Surface the selected RF profile and service TX power evidence in smoke
  summaries for review.
- Validate both dry-run rendering and live remote tx-positive service execution.

**Non-Goals:**

- No new TX power calculation or calibration behavior.
- No receiver-backed Linux peer orchestration in this script.
- No change to production service CLI semantics.

## Decisions

- Reuse environment controls instead of adding new positional CLI flags. The
  existing production scripts already rely on env-var driven run profiles, and
  this keeps remote `ssh ... env bash -s` execution simple.
- Pass TX power args only when `TX_POWER_MODE != current-default`. That preserves
  existing smoke defaults and avoids requiring EFUSE artifacts for the baseline
  current-default gate.
- Require the write-register acknowledgement only for non-default TX power
  modes. This matches the guarded service/diagnostic command surface; ordinary
  transmit paths no longer require a separate acknowledgement flag.
- Add RF profile fields to generated summaries rather than parsing logs. The
  JSON reports already contain authoritative TX power control and calibration
  evidence.

## Risks / Trade-offs

- EFUSE mode needs an artifact on the hardware Mac -> Fail early if the selected
  mode needs `EFUSE_REPORT` and the file is unavailable.
- Diagnostic and service binaries use slightly different guarded write flags ->
  Keep a command-specific flag selection in the remote script.
- Receiver-backed RF quality is still a separate gate -> This smoke only proves
  production service initialization, RX telemetry, and clean local TX
  submission on the attached adapter.
