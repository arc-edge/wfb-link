## 1. OpenSpec And Docs

- [x] 1.1 Archive the completed `add-android-sdk-integration-surface` change
      after syncing its delta specs to main specs.
- [x] 1.2 Add Android product integration hardening proposal, design, specs,
      and tasks.
- [x] 1.3 Update Android SDK docs with lifecycle API, named stream config,
      Gradle sample, CI validation, and stop semantics.

## 2. SDK Lifecycle API

- [x] 2.1 Add Java session/status/callback classes for managed runtime
      execution.
- [x] 2.2 Add `WfbLinkManager.startManagedStreams(...)` that runs the existing
      blocking native path on a caller-provided executor and returns a session
      handle.
- [x] 2.3 Add cooperative stop-request state and duplicate-start/terminal-state
      validation.

## 3. Named Stream Config

- [x] 3.1 Add Java managed stream, direction, payload kind, criticality, and TX
      profile config classes.
- [x] 3.2 Extend `WfbManagedStreamsConfig` to accept named streams while
      preserving existing one-up/one-down builder defaults.
- [x] 3.3 Validate duplicate names/ports and explicitly reject unsupported
      Android native stream shapes before USB runtime execution.

## 4. Consumer Samples And CI

- [x] 4.1 Add a Gradle-style Android consumer sample that imports the local AAR
      and demonstrates USB handoff plus lifecycle API usage.
- [x] 4.2 Add a script to compile/validate the Gradle-style sample without
      depending on the smoke package.
- [x] 4.3 Add GitHub Actions for Rust tests and Android SDK build/consumer
      compile checks.

## 5. Validation

- [x] 5.1 Run `cargo test --workspace`.
- [x] 5.2 Run `scripts/build-android-smoke.sh check`.
- [x] 5.3 Run `scripts/build-android-sdk-aar.sh`.
- [x] 5.4 Run direct and Gradle-style Android consumer compile smokes.
- [x] 5.5 Confirm OpenSpec validation for the new change.
