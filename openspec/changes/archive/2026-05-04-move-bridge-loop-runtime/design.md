## Context

`radio-run` validates a production config in `wfb-radio-runtime`, then adapts
that config back into `BridgeRunArgs` and `runtime-flow` for execution. That was
the right first cut because the bridge loop is hardware-proven. The remaining
problem is that production WFB loop semantics are still partly encoded in
diagnostic-only argument structs: TX bind address expansion, RX forward target
validation, WFB metadata defaults, and loop telemetry mapping.

The next cutover should move those semantics into runtime-owned types before
moving hardware execution itself. That gives production tests a stable target
and keeps hardware behavior unchanged while the boundary shifts.

## Goals / Non-Goals

**Goals:**

- Add runtime-owned WFB loop config and validated loop plan types.
- Validate WFB RX forwarding and TX ingress bounds before USB open.
- Route `radio-run` through the runtime loop plan before building the temporary
  diagnostic execution adapter.
- Keep the current hardware bridge loop behavior and reports compatible.

**Non-Goals:**

- Rewriting same-session init orchestration in this slice.
- Moving all socket threads and USB read/write loops into runtime in one patch.
- Changing default TX descriptor/rate behavior.
- Removing `bridge-run`, `runtime-flow`, PCAP capture, JSONL capture, or
  diagnostic register experiments.

## Decisions

- Move loop planning before loop execution. Planning is pure, testable, and
  lower risk; execution still touches sockets, signals, USB, and hardware init.
- Let `wfb-radio-runtime` depend on `wfb-bridge` for stable WFB channel ID and
  forwarding config types instead of duplicating protocol validation.
- Keep production RX artifact knobs out of `radio-run` for now. PCAP and raw
  JSONL capture remain diagnostic-owned until the runtime loop owns file output
  policy.
- Return runtime errors for invalid WFB routing so `radio-run` can fail before
  USB open with the same runtime-owned report style as other production
  validation failures.

## Risks / Trade-offs

- Adding `wfb-bridge` to `wfb-radio-runtime` increases crate coupling.
  Mitigation: keep the dependency one-way and limited to WFB channel/forwarding
  protocol structs that are already production domain types.
- There will be a temporary two-step boundary: runtime loop plan, diagnostic
  execution adapter. Mitigation: document this explicitly and keep tests around
  both the plan and the adapter.
- Runtime-owned validation could diverge from diagnostic bridge validation.
  Mitigation: route `radio-run` through the runtime plan and keep diagnostic
  `bridge-run` tests as compatibility coverage.

## Migration Plan

1. Add runtime-owned loop config/plan/validation helpers.
2. Route `radio-run` through the loop plan and use the plan to build bridge
   adapter arguments.
3. Move socket binding and receiver thread setup into runtime.
4. Move RX/TX loop execution into runtime once socket setup is owned there.
5. Leave diagnostic bridge commands as wrappers around runtime execution.
