## Approach

Generated benchmark traffic can use the same session-backed `RadioTx` pattern as `bridge-run`, but its descriptor options come from `BridgeTxBenchArgs`, including hardware sequence, first-segment, fallback, queue, and MACID controls. A small bench-specific adapter will apply those options before calling `RuntimeRadioSession::submit_80211_frame`.

Exact `--packet-hex` replay bypasses descriptor construction by design, so the runtime needs a raw packet submit method. The method will select the session bulk-OUT endpoint, submit the already-built descriptor-prefixed packet, update `TxSubmitCounters`, and mirror those deltas into `RuntimeRadioCounters`.

Register probes and guarded register experiments stay diagnostic-side and borrow `session.transport` for control transfers.
