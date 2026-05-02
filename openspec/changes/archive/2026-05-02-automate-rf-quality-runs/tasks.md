## 1. OpenSpec Setup

- [x] 1.1 Create proposal, design, and delta spec for RF-quality run automation
- [x] 1.2 Validate the new OpenSpec change strictly

## 2. Runner Implementation

- [x] 2.1 Add a profile-scoped close-range RF-quality runner script
- [x] 2.2 Add dry-run output for Mac relay, Mac bridge, Linux setup, sender, receiver, and cleanup commands
- [x] 2.3 Implement local artifact directory creation and remote artifact collection
- [x] 2.4 Generate an `rf-quality-report` envelope when required artifacts are available

## 3. Documentation

- [x] 3.1 Document runner configuration, defaults, and manual fallback path
- [x] 3.2 Link the runner from the RF-quality range profiles and runbook

## 4. Verification

- [x] 4.1 Run shell syntax validation for the new script
- [x] 4.2 Run targeted RF-quality report tests
- [x] 4.3 Run OpenSpec strict validation for the automation change
