## 1. Runtime Loop Planning

- [x] 1.1 Add runtime-owned WFB loop config, repeated RX forward target, loop plan, and validation result types.
- [x] 1.2 Add runtime loop validation for TX bind expansion, RX timeout, TX burst limit, and WFB forwarding targets.
- [x] 1.3 Add runtime unit tests for valid forwarding, self-contained repeated targets, defaulted target failures, and invalid bounds.

## 2. Radio-Run Adapter Migration

- [x] 2.1 Route `radio-run` through the runtime bridge-loop plan before constructing diagnostic bridge adapter args.
- [x] 2.2 Remove duplicated `radio-run` WFB forwarding validation from `wfb-radio-diag`.
- [x] 2.3 Add CLI tests proving `radio-run` emits runtime-owned pre-open errors from the loop plan.

## 3. Documentation And Verification

- [x] 3.1 Document the temporary runtime-plan/diagnostic-execution boundary and next migration step.
- [x] 3.2 Run formatting, workspace tests, strict OpenSpec validation, and diff checks.
- [x] 3.3 Commit, push, sync to hardware Mac, and run a short no-TX or RX-only smoke if practical.
