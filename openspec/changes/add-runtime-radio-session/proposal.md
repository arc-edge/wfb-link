## Why

The runtime crate owns USB opening policy and several executable init phases, but callers still receive loose transport, endpoint, adapter, and counter values. A production bridge needs a stable runtime session object that owns these pieces together.

## What Changes

- Add a `RuntimeRadioSession` that owns the opened runtime transport, adapter metadata, endpoint layout, and runtime counters.
- Add session constructors from open config and existing transport-open results.
- Add convenience accessors for register access and bulk endpoint selection.
- Update the diagnostic opener to use the session boundary before converting to its legacy local shape.

## Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime crate exposes a first-class live radio session.
- `userspace-usb-radio`: Live adapter opening is represented by runtime session state rather than loose diagnostic-only values.

## Impact

- Affects runtime crate API and diagnostic open wiring.
- No intended USB wire behavior changes.
- Full diagnostic call-site conversion can continue incrementally.
