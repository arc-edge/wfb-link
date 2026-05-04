## 1. Runtime Loop Executor

- [x] 1.1 Add runtime loop run config, step, step outcome, stop reason, and outcome types.
- [x] 1.2 Implement callback-driven executor for signal stop, duration stop, max datagram stop, TX burst draining, and RX timeout calculation.
- [x] 1.3 Add runtime unit tests for TX burst draining, unbounded max-datagram stop, duration-bounded max-datagram behavior, and RX timeout clamping.

## 2. Diagnostic Adapter

- [x] 2.1 Refactor bridge-run to call the runtime loop executor with a single packet-processing callback.
- [x] 2.2 Preserve existing bridge-run failure paths, metrics, TX status post-processing, and report stop reason labels.
- [x] 2.3 Keep `radio-run` hardware smoke behavior unchanged.

## 3. Verification

- [x] 3.1 Update runtime boundary docs for loop scheduler ownership.
- [x] 3.2 Run formatting, workspace tests, strict OpenSpec validation, and diff checks.
- [x] 3.3 Commit, push, sync to hardware Mac, and run a short no-TX or RX-only smoke if practical.
