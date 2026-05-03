## Why

Long-distance WFB quality depends on runtime RF calibration, not only final
register snapshots. The current macOS path can transmit to a Linux peer at
close range, but calibration is still labeled as stop-gap because runtime
LCK/IQK parity is incomplete against the Linux RTL8812AU driver.

LCK is the smallest useful runtime calibration slice. The upstream 8812A
routine uses RF serial readback, pauses packet TX, toggles the RF LCK and
channel/BW registers, waits for the local oscillator calibration window, then
restores the RF channel state. Implementing this first gives us a real,
reportable calibration routine without jumping straight into the much larger
IQK port.

## What Changes

- Add RTL8812AU RF serial readback support for path A/B using the Linux
  `phy_RFSerialRead` sequence.
- Add a guarded `rtl8812a-lck` TX calibration profile that runs the upstream
  LCK sequence after init and before TX.
- Record LCK before/after evidence including RF reads, RF writes, TX pause
  behavior, delays, and restore values.
- Keep the routine opt-in and explicitly distinct from full IQK/Linux parity.
- Update docs and tests so RF-quality reports can treat LCK as a runtime
  calibration step while still marking IQK as remaining work.

## Capabilities

### Modified Capabilities

- `userspace-usb-radio`: gains RF readback and a guarded runtime LCK
  calibration profile.
- `rf-quality-and-range`: calibration evidence can include a real runtime LCK
  routine while still preserving stop-gap labels for IQK/captured values.

## Impact

- Affected crate: `wfb-radio-diag`.
- Affected commands: `bridge-tx-listen`, `bridge-run`, and
  `bridge-tx-bench` when TX calibration profiles are enabled.
- Affected docs: calibration-state and RF-quality baseline notes.
