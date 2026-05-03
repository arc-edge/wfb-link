## 1. IQK Probe Implementation

- [x] 1.1 Add RTL8812AU IQK register inventory constants for upstream MAC/BB, AFE, RF backup offsets, page-C1 latches, and IQK result/power registers.
- [x] 1.2 Add a `rtl8812a-iqk-probe` TX calibration profile and structured report fields that label the profile as read-only evidence.
- [x] 1.3 Implement the bounded IQK marker profile: perform no additional live pre-TX hardware reads, rely on `rf_calibration_pre_tx.iqk` for safe final-state evidence, and label profile-time/RF-serial/page-C1 deep probe evidence as deferred after hardware smoke showed it can perturb WFB recovery.
- [x] 1.4 Expand RF-calibration probes and documentation so IQK state is visible beside existing RFE, TXAGC, LCK, and RF path evidence.

## 2. RF-Quality Smoke Hardening

- [x] 2.1 Update close-range RF-quality automation to record expected-versus-observed datagram evidence for short FEC runs.
- [x] 2.2 Ensure recovered WFB payload evidence is not discarded solely because a short smoke emitted one fewer datagram than the theoretical FEC ceiling.

## 3. Validation

- [x] 3.1 Add focused unit tests for IQK profile selection, page-select/register inventory constants, and read-only report labeling.
- [x] 3.2 Run local formatting, workspace tests, and strict OpenSpec validation.
- [x] 3.3 Deploy to the hardware Mac and run a close-range IQK probe smoke without regressing recovered WFB payloads.
