## Why

The live RX/TX command paths now use `RuntimeRadioSession`, but runtime-boundary documentation and one init report note still describe TX/RX as future separate work. That makes the repo misleading after the runtime migration.

## What Changes

- Update runtime-boundary documentation to show current runtime ownership.
- Update README status to mention runtime session ownership for live RX/TX commands.
- Correct the live init note so it distinguishes init-only behavior from available runtime TX/RX commands.

## Capabilities

### Modified Capabilities

- `userspace-usb-radio`: runtime ownership documentation reflects current live RX/TX session behavior.

## Impact

- Documentation and diagnostic note text only.
- No behavior or CLI schema changes.
