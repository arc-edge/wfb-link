## 1. Shared Production Command Support

- [x] 1.1 Extract `radio-run` config-file structs, deserialization, CLI override merge, and validation helpers into reusable production command support outside the diagnostic `main.rs`.
- [x] 1.2 Update `wfb-radio-diag radio-run` to use the shared support while preserving its existing CLI, JSON report, ready marker, health artifact, and error behavior.
- [x] 1.3 Add unit tests proving shared config resolution matches the current config-only, CLI-only, CLI-overrides-config, missing required setting, and diagnostic-only rejection behavior.

## 2. Production Service Binary

- [x] 2.1 Add a workspace production binary crate or target for the standalone service command.
- [x] 2.2 Implement the config-first service CLI with supported overrides, report path, ready marker, health file, and live-operation acknowledgements.
- [x] 2.3 Ensure the service binary runs the runtime-owned production flow and emits the same production report/health/ready schemas as `radio-run`.
- [x] 2.4 Add unit or integration tests proving the service CLI excludes diagnostic-only command surfaces and resolves equivalent config to the diagnostic compatibility command.

## 3. Smoke Automation

- [x] 3.1 Add a smoke command selector so production and receiver-backed scripts can run the service binary or diagnostic compatibility command explicitly.
- [x] 3.2 Record the selected command surface in smoke summaries.
- [x] 3.3 Run local production smoke through the service binary.
- [x] 3.4 Run receiver-backed robust tuple smoke through the service binary.

## 4. Documentation and Verification

- [x] 4.1 Update README or docs to describe the production service binary and diagnostic compatibility boundary.
- [x] 4.2 `cargo fmt` clean.
- [x] 4.3 `cargo test -p wfb-radio-runtime -p wfb-radio-diag` plus the new production binary package passes.
- [x] 4.4 `openspec validate add-production-radio-service-binary --strict` and `openspec validate --specs --strict` pass.
