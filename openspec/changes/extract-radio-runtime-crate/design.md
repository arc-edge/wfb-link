## Context

`wfb-radio-diag` has become the only place where runtime radio policy exists. It parses TX calibration profiles, decides which profiles require live-register authorization, maps profiles into RF-quality calibration classifications, and owns the command paths that exercise runtime IQK/LCK and WFB TX/RX tests. That was appropriate for bring-up, but it blocks a production bridge/runtime split because the eventual runtime cannot depend on a diagnostic binary.

The current implementation already has separate crates for chipset primitives (`radio-core`) and WFB protocol/bridge parsing (`wfb-bridge`). The missing layer is a reusable runtime crate that owns stable policy and gradually absorbs hardware session orchestration.

## Goals / Non-Goals

**Goals:**

- Add a `wfb-radio-runtime` crate as the production-facing radio runtime boundary.
- Move stable TX calibration profile policy out of `wfb-radio-diag`.
- Keep CLI behavior, report behavior, and hardware test behavior unchanged.
- Document which logic remains diagnostic-only and which logic belongs in the runtime crate next.

**Non-Goals:**

- Do not move macOS USBHost session ownership in this first slice.
- Do not move IQK/LCK register execution code yet.
- Do not redesign the WFB TX/RX loops.
- Do not change RF-quality acceptance gates or report schemas.

## Decisions

- Create a new crate instead of expanding `radio-core`.

  `radio-core` is chipset/protocol primitive code. Runtime policy needs to describe link profiles, calibration intent, safety gates, and later long-lived session orchestration. Keeping that in `wfb-radio-runtime` prevents `radio-core` from becoming a stateful application layer.

- Extract policy before transport.

  Moving USBHost and IQK execution first would be high-risk because those paths are still actively used as hardware diagnostics. Calibration profile policy is stable, well covered by tests, and directly needed by both diagnostic and future runtime entry points.

- Keep diag-owned CLI enums and convert into runtime enums.

  The CLI is still diagnostic surface area, so `wfb-radio-diag` should retain clap-specific types. The runtime crate exposes clap-free policy types and methods. This keeps runtime reusable by a future daemon or library caller without inheriting diagnostic CLI dependencies.

- Preserve existing report classifications.

  The runtime crate returns calibration intent categories; `wfb-radio-diag` converts them into existing report enums. This avoids report churn while establishing the dependency direction.

## Risks / Trade-offs

- Partial extraction can feel cosmetic if no behavior moves. → Move actual safety and classification decisions, add direct runtime crate tests, and wire diag through the crate.
- The first runtime API may be too narrow. → Keep types small and avoid exposing hardware execution traits until the next migration slice is clear.
- Diagnostic and runtime type names can drift. → Add conversion tests in `wfb-radio-diag` and policy tests in `wfb-radio-runtime`.

## Migration Plan

1. Add `wfb-radio-runtime` to the workspace.
2. Move calibration profile policy into the new crate.
3. Add documentation for the runtime boundary and the next migration targets.
4. Wire `wfb-radio-diag` through the runtime policy and keep all existing tests passing.
5. In later changes, migrate USB session lifecycle, TX/RX loop orchestration, runtime IQK/LCK execution, and telemetry types into the runtime crate.

## Open Questions

- Whether the runtime crate should eventually expose a synchronous trait API, an async service API, or both.
- Whether macOS USBHost transport should move into the runtime crate directly or into a separate platform crate.
