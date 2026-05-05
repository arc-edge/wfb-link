## 1. Service RF Profile Controls

- [x] 1.1 Add service CLI and TOML config fields for TX-power mode/source and calibration profile.
- [x] 1.2 Resolve and validate service RF profile controls before USB open, preserving CLI-over-config precedence.
- [x] 1.3 Add service unit tests for config defaults, CLI overrides, and invalid profile rejection.

## 2. RF-Quality Automation

- [x] 2.1 Update close-range RF-quality automation to pass TX-power/profile controls to `wfb-radio-service`.
- [x] 2.2 Preserve dry-run visibility and evidence fields for the selected Mac radio command and RF profile tuple.

## 3. Verification

- [x] 3.1 Run formatting, service/runtime/diag tests, script syntax checks, and service RF-quality dry-run coverage.
- [x] 3.2 Validate the OpenSpec change strictly and update docs for the service RF profile controls.
