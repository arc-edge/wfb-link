## Context

The macOS RTL8812AU bring-up now has working close-range WFB TX/RX with Linux-parity TX power overrides and an opt-in RTL8812AU LCK runtime sequence. IQK remains the largest RF-quality gap: the current usable path plants captured IQK result constants, while the Linux aircrack-ng driver backs up MAC/BB/AFE/RF state, runs a multi-stage IQK sweep, fills result registers, and restores state.

Full IQK is invasive enough that it should not be ported as one opaque change. The next safe step is to expose the same upstream backup and result surfaces as a read-only probe, including the page-C1 latches and RF serial state needed to compare the macOS hardware state against Linux.

## Goals / Non-Goals

**Goals:**

- Add a non-perturbing `rtl8812a-iqk-probe` TX calibration profile that marks IQK staging state without claiming to perform calibration.
- Expand RF-calibration diagnostics to include the upstream IQK tone, PI, AGC, and before/after power registers used by `phy_iq_calibrate_8812a`.
- Preserve current working LCK and Linux-parity TX behavior while making IQK state visible in RF-quality artifacts.
- Harden RF-quality smoke reporting around short WFB FEC runs where the observed datagram count can be one packet lower than the theoretical ceiling while payload recovery still succeeds.

**Non-Goals:**

- Port the full IQK tone sweep, candidate selection, and IQC fill algorithm in this slice.
- Replace planted IQK constants for long-distance use before runtime IQK has been validated.
- Change regulatory power clamps, rate selection, or channel/bandwidth policy.

## Decisions

- Start with a non-perturbing IQK marker profile rather than a partial write sequence. The upstream IQK path touches queue pause, beacon control, AFE, RF path registers, and page-switched BB registers; hardware smoke showed profile-time IQK reads can perturb WFB recovery, so the live profile adds no extra hardware reads and relies on `rf_calibration_pre_tx.iqk` for final-state evidence.
- Store IQK evidence inside the existing TX calibration profile report. RF-quality tooling already captures `tx_calibration_profile`, so adding `iqk` beside `lck` keeps the report format coherent.
- Keep RF serial/page-C1 IQK evidence out of the live pre-TX profile for now. Hardware smoke showed the deeper probe can perturb WFB recovery even when selectors are restored, so the production-safe profile reports that evidence as deferred rather than risking the working TX path.
- Treat short-run datagram mismatch as report evidence, not silent success. The bridge should still report submitted datagrams exactly, while the RF-quality wrapper should avoid invalidating recovered-payload evidence solely because a tiny FEC run emitted one fewer datagram than the theoretical ceiling.

## Risks / Trade-offs

- Read-only IQK staging does not improve range by itself -> The output explicitly labels the profile as `deferred_hardware_probe` and documents that full IQK remains required for production RF quality.
- Deep IQK probe reads can perturb TX even when they restore selectors -> The live pre-TX profile skips all additional profile-time hardware reads; RF serial/page-C1 evidence moves to a standalone diagnostic or the full IQK port.
- Expanded IQK register inventory may include page-dependent addresses whose meaning differs by selected page -> Reports use the existing normal-page `rf_calibration_pre_tx.iqk` evidence and defer page-C1 latch evidence.
- Short-run FEC tolerance could hide bridge accounting bugs -> The tolerance is limited to RF-quality wrapper interpretation and reports expected versus observed datagram counts.
