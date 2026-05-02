## 1. RF Readback

- [x] 1.1 Add RTL8812AU RF serial readback constants and helper functions for path A/B
- [x] 1.2 Record RF read source evidence, including PI/SI mode and readback register
- [x] 1.3 Add unit tests for RF read planning and masked RF value handling

## 2. Guarded LCK

- [x] 2.1 Add an opt-in `rtl8812a-lck` TX calibration profile
- [x] 2.2 Implement the upstream 8812A LCK sequence with TX pause, RF LCK bit toggle, RF CHNLBW trigger, 150 ms delay, and restore
- [x] 2.3 Record before/write/after/restore evidence in command reports
- [x] 2.4 Keep the default profile unchanged and keep LCK distinct from full IQK/Linux parity

## 3. Reporting And Docs

- [x] 3.1 Include runtime LCK evidence in RF calibration probes or calibration-profile reports
- [x] 3.2 Update calibration docs to describe the LCK port and remaining IQK blocker
- [x] 3.3 Run OpenSpec validation, Rust formatting/tests, and hardware smoke when reachable
