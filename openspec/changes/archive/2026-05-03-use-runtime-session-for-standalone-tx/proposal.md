## Why

`tx-once` and `tx-repeat` are the remaining standalone live TX commands that still submit frames through raw transport handles. Moving them onto `RuntimeRadioSession` closes the last direct live frame-submission paths in the diagnostic binary.

## What Changes

- Open standalone live TX commands into runtime radio sessions.
- Submit single and repeated TX frames through `RuntimeRadioSession::submit_80211_frame`.
- Keep LED/status register probes, submit counters, and JSON reports stable.

## Capabilities

### Modified Capabilities

- `wfb-radio-runtime`: standalone TX diagnostics use runtime session TX submission.

## Impact

- Affects `tx-once` and `tx-repeat` internals only.
- No intended CLI or JSON schema changes.
