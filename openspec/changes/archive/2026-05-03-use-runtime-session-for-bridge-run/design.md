## Approach

`bridge-run` will mirror the already-migrated `bridge-tx-once` and `bridge-tx-listen` paths:

- Wrap the existing macOS USBHost or libusb open result in `RuntimeRadioSession`.
- Select endpoints through session helper methods.
- Use `BridgeTxSessionRadio` for TX submissions.
- Continue direct register diagnostics through `Rtl8812auRegisterAccess::new(&session.transport)`.

The RX loop remains byte-for-byte compatible in this change by reading from `session.transport` directly and feeding the existing `process_rx_buffer` parser. A follow-up can move RX packet reads behind `RuntimeRadioSession::read_rx_packets` once the runtime result shape carries all report metadata the diagnostic bridge currently tracks.
