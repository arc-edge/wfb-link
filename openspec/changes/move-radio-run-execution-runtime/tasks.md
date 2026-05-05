## 1. Runtime Execution API

- [ ] 1.1 Add a runtime-owned production flow execution input type that carries parsed firmware/table/calibration inputs needed after CLI/file loading.
- [ ] 1.2 Add a runtime-owned production flow execution function that validates config and returns `ProductionRuntimeFlowReport` on pre-USB failures without opening the adapter.
- [ ] 1.3 Add unit tests proving invalid production config fails before USB claim and serializes the existing production report shape.

## 2. Move Runtime Flow Orchestration

- [ ] 2.1 Move same-session init orchestration and ready-marker writing from the diagnostic adapter path into the runtime execution function.
- [ ] 2.2 Move TX ingress receiver setup, bridge-loop invocation, heartbeat lifecycle, and stop-reason handling into the runtime execution function.
- [ ] 2.3 Move RX/TX telemetry aggregation into runtime execution so it returns `ProductionRuntimeFlowReport` directly without diagnostic `RuntimeFlowReport` adaptation.
- [ ] 2.4 Preserve runtime-owned TX power, targeted parity, LCK, runtime IQK, RX forwarding, source-ready, and heartbeat report fields.

## 3. Thin Diagnostic Adapter

- [ ] 3.1 Update `radio_run_report` so it maps CLI/file inputs into runtime-owned execution inputs and calls the runtime execution API directly.
- [ ] 3.2 Keep `runtime-flow` and `bridge-run` diagnostic commands working through their existing diagnostic report paths.
- [ ] 3.3 Confirm `radio-run` still rejects diagnostic-only register pokes, TX status probes, PCAP/JSONL output, and trace replay from its command surface.

## 4. Tests And Smokes

- [ ] 4.1 `cargo fmt` clean.
- [ ] 4.2 `cargo test -p wfb-radio-runtime -p wfb-radio-diag` passes.
- [ ] 4.3 `openspec validate move-radio-run-execution-runtime --strict` and `openspec validate --specs --strict` pass.
- [ ] 4.4 Run `scripts/run-production-radio-smoke.sh --mode both` locally or on the hardware Mac and verify RX-only plus TX-positive gates still pass.
- [ ] 4.5 Run a short receiver-backed duplex `radio-run` smoke and verify peer recovery, decrypt gates, TX failures/drops, RX forwarding snapshots, source timing, signal summaries, and heartbeat reporting remain compatible.
