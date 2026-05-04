## 1. Runtime TX Handler

- [x] 1.1 Add runtime TX handler config, metadata, outcome, and error types.
- [x] 1.2 Implement one queued-datagram runtime TX handler using WFB parsing, TX override application, descriptor preview, and `RuntimeRadioSession` submission.
- [x] 1.3 Add runtime unit tests for successful submission, malformed datagram handling, descriptor-build rejection, and radio submit failure.

## 2. Diagnostic Adapter

- [x] 2.1 Refactor bridge-run TX callback to call the runtime TX handler.
- [x] 2.2 Preserve existing bridge-run last-datagram report fields, bridge counters, submit counters, TX status failure post-processing, and throughput byte counters.
- [ ] 2.3 Keep `radio-run` no-TX/RX-only smoke behavior unchanged.

## 3. Verification

- [x] 3.1 Update runtime boundary docs for queued TX datagram handler ownership.
- [x] 3.2 Run formatting, workspace tests, strict OpenSpec validation, and diff checks.
- [ ] 3.3 Commit, push, sync to hardware Mac, and run a short no-TX or RX-only smoke if practical.
