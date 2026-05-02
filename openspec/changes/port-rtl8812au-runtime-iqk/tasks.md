## 1. Profile And Report Shape

- [x] 1.1 Add a guarded `rtl8812a-runtime-iqk` TX calibration profile enum value and authorization tests.
- [x] 1.2 Add runtime IQK report structs for per-path TX/RX status, retry counts, selected IQC values, fallback use, cleanup status, and USB counters.
- [x] 1.3 Keep default, `rtl8812a-lck`, and `rtl8812a-iqk-probe` behavior unchanged.

## 2. IQK Helper Port

- [x] 2.1 Port `_iqk_tx_fill_iqc_8812a` and `_iqk_rx_fill_iqc_8812a` into dry-run/planned masked BB write helpers with unit tests.
- [x] 2.2 Port IQK MAC/BB, AFE, RF, BB page-select, and HSSI backup/restore helpers with cleanup reporting.
- [x] 2.3 Port the bounded IQK MAC/RF/AFE setup writes needed before one-shot TX/RX IQK.

## 3. Runtime IQK Execution

- [x] 3.1 Implement the TX IQK one-shot loops with upstream retry limits, readiness/failure checks, candidate averaging, and per-path reports.
- [x] 3.2 Implement the RX IQK one-shot loops that depend on successful TX IQK candidates.
- [x] 3.3 Apply selected or fallback TX/RX IQC values, restore saved state, and report before/after register evidence.

## 4. Validation

- [x] 4.1 Add unit tests for command parsing, profile gating, IQC fill plans, failure labels, and report serialization.
- [x] 4.2 Update calibration and RF-quality docs to describe runtime IQK status and remaining range gates.
- [x] 4.3 Run formatting, workspace tests, strict OpenSpec validation, deploy to the hardware Mac, run a guarded runtime IQK smoke, and run receiver-backed A/B close-range validation.
