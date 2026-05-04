## 1. Runtime RX Handler

- [x] 1.1 Add runtime RX forward runtime, forward snapshot, RX outcome telemetry, and handler result types.
- [x] 1.2 Implement RX forward runtime creation from loop plans and report-neutral snapshot extraction.
- [x] 1.3 Implement parsed RX packet outcome processing for counters, frame types, metadata coverage, and WFB forwarding.
- [x] 1.4 Add runtime unit tests for frame/drop/tail accounting, forwarding with aggregator, filtering without aggregator, and send failure reporting.

## 2. Diagnostic Adapter

- [x] 2.1 Refactor bridge-run RX callback to call the runtime RX handler.
- [x] 2.2 Preserve existing bridge-run PCAP/JSONL side effects, RX report fields, WFB forward report shape, and error labels.
- [x] 2.3 Keep `radio-run` no-TX/RX-only smoke behavior unchanged.

## 3. Verification

- [x] 3.1 Update runtime boundary docs for parsed RX packet handler and RX forwarding ownership.
- [x] 3.2 Run formatting, workspace tests, strict OpenSpec validation, and diff checks.
- [x] 3.3 Commit, push, sync to hardware Mac, and run a short no-TX or RX-only smoke if practical.
