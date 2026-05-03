## 1. Command And Report Shape

- [x] 1.1 Add CLI arguments and report structs for a guarded `rtl8812a-iqk-diagnostic` command.
- [x] 1.2 Reuse or extend IQK evidence structs so standalone output includes mode, evidence-only semantics, cleanup status, and all deep evidence groups.

## 2. Deep IQK Evidence Collection

- [x] 2.1 Implement MAC/BB, AFE, and normal-page IQK register collection from the existing inventory constants.
- [x] 2.2 Implement guarded RF serial backup reads for path A and path B with selector restore reporting.
- [x] 2.3 Implement guarded page-C1 latch reads with page-select restore reporting.
- [x] 2.4 Ensure the diagnostic performs no WFB TX/RX, synthetic TX, or bulk-IN RX loop.

## 3. Documentation And Validation

- [x] 3.1 Add unit tests for command parsing, report semantics, inventory coverage, and cleanup-status reporting.
- [x] 3.2 Document the standalone diagnostic and how it differs from the live `rtl8812a-iqk-probe` TX profile.
- [x] 3.3 Run formatting, workspace tests, strict OpenSpec validation, deploy to the hardware Mac, run the standalone diagnostic, and run a short WFB smoke after it to verify cleanup.
