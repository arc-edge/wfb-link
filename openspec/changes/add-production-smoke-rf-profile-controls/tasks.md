## 1. Smoke Runner RF Profile Controls

- [x] 1.1 Add production-smoke environment variables for TX power mode/source/safety profile and TX calibration profile, including dry-run and remote/local env propagation.
- [x] 1.2 Build command-specific RF profile argument lists for service and diagnostic `radio-run`, including guarded write authorization for non-default TX power modes.
- [x] 1.3 Record selected RF profile and report TX power evidence in smoke summaries and operator output.

## 2. Validation

- [x] 2.1 Validate shell/Python syntax plus OpenSpec strict validation.
- [x] 2.2 Run dry-run coverage for service EFUSE profile rendering.
- [x] 2.3 Run a remote hardware-Mac tx-positive service smoke with EFUSE-derived TX power and current/default-safe calibration profile.
