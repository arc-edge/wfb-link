## Why

The runtime crate owns transport opening and early register execution, but EFUSE MAC discovery and REG_MACID programming remain diagnostic-local. Production bridge initialization needs the same local-MAC behavior without depending on `wfb-radio-diag`.

## What Changes

- Add runtime EFUSE physical-read support sufficient to decode the adapter MAC address.
- Add runtime EFUSE logical-map MAC extraction helpers.
- Add runtime REG_MACID read/program execution helpers.
- Update diagnostic MACID helpers to call runtime execution while preserving existing report output.

## Capabilities

### Modified Capabilities
- `radio-runtime-library`: Runtime crate can read the RTL8812AU EFUSE MAC and program REG_MACID.
- `userspace-usb-radio`: Initialization MAC address assignment is runtime-owned instead of diagnostic-owned.

## Impact

- Affects runtime crate API and diagnostic MACID wiring.
- No intended USB wire change for MACID programming or EFUSE MAC reads.
- Full EFUSE dump reporting remains diagnostic-owned.
