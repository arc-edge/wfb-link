## 1. Specification

- [x] 1.1 Create proposal, design, and delta spec for hardware-Mac deploy sync
- [x] 1.2 Validate the OpenSpec change strictly

## 2. Implementation

- [x] 2.1 Add opt-in deploy sync settings to the close-range runner
- [x] 2.2 Implement local rsync deployment with safe excludes and separate default path
- [x] 2.3 Refuse deploy sync when destination equals the working checkout unless explicitly overridden
- [x] 2.4 Include deploy settings in dry-run output and run configuration

## 3. Documentation

- [x] 3.1 Document deploy mode in the RF-quality runbook

## 4. Verification

- [x] 4.1 Run shell syntax and shellcheck validation
- [x] 4.2 Run dry-run coverage for deploy mode
- [x] 4.3 Run targeted RF-quality tests
- [x] 4.4 Run OpenSpec strict validation
