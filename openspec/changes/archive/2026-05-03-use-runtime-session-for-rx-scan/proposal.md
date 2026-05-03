## Why

`rx-scan` is the standalone live receive diagnostic and WFB forwarding probe, but it still owns raw bulk-IN reads. Moving it onto `RuntimeRadioSession` keeps receive behavior aligned with `bridge-run` and exercises runtime RX parsing in another live command.

## What Changes

- Open `rx-scan` live USB transport into a runtime radio session.
- Run bulk-IN capture through `RuntimeRadioSession::read_rx_packets`.
- Keep existing PCAP, JSONL, WFB forwarding, and JSON report behavior stable.

## Capabilities

### Modified Capabilities

- `wfb-radio-runtime`: standalone RX scan uses runtime session I/O and parser outcomes.

## Impact

- Affects `rx-scan` internals only.
- No intended CLI or JSON schema changes.
