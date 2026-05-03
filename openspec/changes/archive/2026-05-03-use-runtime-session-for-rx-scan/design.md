## Approach

The `rx-scan` command will mirror the migrated `bridge-run` RX loop:

- Wrap the live transport open result in `RuntimeRadioSession`.
- Use session endpoint metadata in reports and missing-endpoint failures.
- Borrow `session.transport` for same-session init and monitor receive-filter control writes.
- Call `RuntimeRadioSession::read_rx_packets` inside the capture loop.
- Feed returned `ParsedRxPacket` outcomes into the shared diagnostic packet processor used by `bridge-run`.

The capture helper continues to return the same `RxFixtureReport` and `DiagnosticCounters` shape so existing report consumers are not disturbed.
