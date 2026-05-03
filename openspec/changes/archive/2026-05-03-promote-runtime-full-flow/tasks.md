## 1. Runtime Init API

- [x] 1.1 Add report-neutral runtime init config/result types for RTL8812AU same-session init
- [x] 1.2 Move reusable same-session init phase orchestration into `wfb-radio-runtime`
- [x] 1.3 Return phase identifiers, counter deltas, calibration decisions, and readiness state from runtime init
- [x] 1.4 Add focused unit tests for init config defaults, calibration guardrails, and phase result conversion

## 2. Diagnostic Wrappers

- [x] 2.1 Update retained same-session diagnostic commands to call the runtime init API
- [x] 2.2 Preserve existing diagnostic report fields by adapting runtime init evidence
- [x] 2.3 Remove diagnostic-only ownership of init execution where runtime equivalents exist

## 3. Production Flow

- [x] 3.1 Add a thin production-facing command for open-init-run WFB runtime flow
- [x] 3.2 Ensure production flow depends on runtime config/session types instead of diagnostic arg/report structs
- [x] 3.3 Expose production telemetry for adapter identity, channel, calibration class, init phases, RX/TX counters, and last error

## 4. Calibration Policy

- [x] 4.1 Make runtime calibration profile selection explicit for production callers
- [x] 4.2 Label default, targeted parity, captured IQK/LCK, and runtime IQK profiles with evidence source and validation status
- [x] 4.3 Enforce live-write authorization before experimental calibration writes

## 5. Verification

- [x] 5.1 Run `openspec validate promote-runtime-full-flow --strict`
- [x] 5.2 Run `cargo test --workspace`
- [x] 5.3 Run available hardware smoke for runtime init/production flow when the attached adapter is reachable
- [x] 5.4 Commit and push the completed implementation slice
