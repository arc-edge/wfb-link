## Why

`RuntimeRadioSession` now owns live transport state, but TX submission and RX bulk parsing are still performed directly by diagnostic code through lower-level crates. Production bridge code needs session-level I/O methods that update runtime counters consistently.

## What Changes

- Add session TX submission that selects the bulk OUT endpoint, calls RTL8812AU TX descriptor submission, and updates runtime counters.
- Add session RX bulk-read parsing that selects the bulk IN endpoint, parses aggregated RTL8812AU RX buffers, and updates runtime counters.
- Preserve existing low-level `radio-core` APIs for callers that need descriptor-only behavior.

## Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime session can submit 802.11 frames and read parsed RX packets.
- `userspace-usb-radio`: Runtime counters reflect live USB bulk I/O and RX/TX frame outcomes.

## Impact

- Affects runtime crate API.
- No diagnostic command behavior changes are intended in this slice.
