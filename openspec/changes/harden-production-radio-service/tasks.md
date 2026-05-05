## 1. Config File Support

- [x] 1.1 Add production `radio-run` config structs and deserialization for the service-oriented settings in the delta spec.
- [ ] 1.2 Add `radio-run --config <path>` and merge config values with existing CLI flags, with CLI flags taking precedence.
- [ ] 1.3 Add unit tests for config-only, CLI-only, CLI-overrides-config, missing required settings, and diagnostic-only field rejection.

## 2. Runtime Health Model

- [x] 2.1 Add runtime-owned production service health structs, lifecycle states, and operator-action classification helpers.
- [x] 2.2 Add a runtime JSON health writer that accepts an optional path and reports serialization/filesystem failures with stable runtime error codes.
- [x] 2.3 Add unit tests proving health classification, JSON shape, absent-path no-op behavior, and diagnostic-only field exclusion.

## 3. radio-run Health Integration

- [x] 3.1 Add `radio-run --health-file <path>` and thread the path through the production runtime execution inputs/config.
- [ ] 3.2 Write health artifacts at startup/validation, ready, and final exit boundaries without changing ready-marker semantics.
- [ ] 3.3 Ensure success, signal stop, init failure, health-write failure, TX failure/drop, and RX-forward degradation produce useful health states and final reports.

## 4. Production Service Smoke Automation

- [ ] 4.1 Add a checked-in sample production config for the accepted robust short-range profile.
- [ ] 4.2 Add or extend smoke automation so it can run `radio-run` from the config file with health-file, ready-marker, and final report artifacts.
- [ ] 4.3 Add summary validation for service health final state, zero post-session decrypt failures, zero TX failures/drops, RX forwarding snapshots, source timing, and robust tuple peer recovery.

## 5. Verification

- [ ] 5.1 `cargo fmt` clean.
- [ ] 5.2 `cargo test -p wfb-radio-runtime -p wfb-radio-diag` passes.
- [ ] 5.3 `openspec validate harden-production-radio-service --strict` and `openspec validate --specs --strict` pass.
- [ ] 5.4 Run local production smoke with config/health artifacts.
- [ ] 5.5 Run receiver-backed robust tuple service smoke and confirm M2L/L2M recovery, decrypt gates, TX gates, RX forwarding, source timing, signal summaries, and health artifacts pass.
