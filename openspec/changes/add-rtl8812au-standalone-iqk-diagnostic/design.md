## Context

The safe `rtl8812a-iqk-probe` TX calibration profile intentionally performs no
extra live hardware reads because earlier RF-serial and page-C1 probing
perturbed WFB payload recovery. The next IQK step needs those deeper surfaces,
but it must run as an isolated diagnostic after full init and before any WFB
traffic so failures or side effects are easy to attribute.

The codebase already has the required building blocks: retained RTL8812AU init,
normal-page RF calibration readback, RF serial read helpers used by LCK, and
inventory constants for upstream IQK backup register sets. This change wires
those pieces into a standalone command instead of the pre-TX calibration hook.

## Goals / Non-Goals

**Goals:**

- Add a guarded `rtl8812a-iqk-diagnostic` command that initializes the adapter
  and reads deep IQK evidence without submitting WFB or synthetic TX frames.
- Capture MAC/BB backup registers, AFE backup registers, RF backup offsets for
  both paths, page-C1 latch registers, and normal-page IQK result/tone/power
  registers in a structured report.
- Restore page selection and RF/HSSI readback state after deep reads.
- Make the report useful for comparing macOS state to Linux
  `phy_iq_calibrate_8812a` traces and for deciding the next full IQK port step.

**Non-Goals:**

- Run the IQK tone sweep, select IQK candidates, or write final IQC values.
- Claim long-distance readiness or replace the current stop-gap captured IQK
  constants.
- Add continuous TX, spectrum analysis, or outdoor range automation.

## Decisions

- Implement this as a standalone command, not a TX calibration profile. The
  diagnostic is allowed to switch BB pages and RF serial selectors because no
  WFB receiver outcome is measured in the same process.
- Require an explicit hardware-write acknowledgement. Even though the command
  is evidence-only with respect to calibration, page switching and RF serial
  selector setup are hardware writes.
- Reuse the same IQK report shape used by `tx_calibration_profile.iqk`, with an
  expanded mode label, so `rf-quality-report` and docs can eventually attach
  standalone evidence without inventing a second vocabulary.
- Treat cleanup as reportable best effort. If a read fails, the command still
  attempts to restore selectors and reports cleanup errors separately from the
  primary failure.

## Risks / Trade-offs

- Page-C1 or RF serial reads may still perturb a later bridge run -> run this
  diagnostic outside live WFB TX and validate cleanup with a short WFB smoke.
- RF serial backup offsets may not exactly match the upstream stage that uses
  them -> label output as evidence for porting, not calibration success.
- A cleanup failure could leave the adapter in an unexpected state -> report
  cleanup status and recommend replug/re-init before production testing.
- The diagnostic adds another hardware-facing path -> keep it guarded,
  bounded, JSON-first, and covered by unit tests for command/report semantics.
