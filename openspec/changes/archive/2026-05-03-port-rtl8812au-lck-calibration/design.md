## Context

The Linux RTL8812AU driver initializes the RF tables and then runs runtime
calibration code. Static table parity is not enough because the runtime code
does stateful RF operations. LCK is tractable: upstream `_phy_lc_calibrate_8812a`
backs up RF `CHNLBW` on path A, pauses packet TX when not in continuous TX,
sets RF `LCK` bit 14, triggers RF `CHNLBW` bit 15, waits 150 ms, clears RF
`LCK` bit 14, unpauses TX, and restores `CHNLBW`.

The missing primitive is RF serial readback. Upstream `phy_RFSerialRead` for
8812A selects the RF offset through `rHSSIRead_Jaguar` (`0x8b0`) with mask
`0xff`, chooses PI/SI readback by sampling bit 2 of `0xc00` or `0xe00`, and
then reads the 20-bit value from `0xd04/0xd44` (PI) or `0xd08/0xd48` (SI).

## Decisions

### Decision: Implement RF readback before LCK writes

The LCK routine must preserve RF channel state. We will not use hardcoded
restore values. The code will read RF `CHNLBW` and RF `LCK` before applying the
sequence and will report every observed value.

### Decision: Make LCK an explicit calibration profile

The default `current-default` profile remains unchanged. LCK is exposed as an
opt-in profile so receiver-backed A/B tests can compare the current path against
runtime LCK without hiding the change.

### Decision: Restore state even on error where possible

If the sequence fails after TX pause or after changing RF registers, the routine
will attempt best-effort unpause/restore and include any restore failure in the
error text or report surface.

## Risks / Trade-offs

- RF readback may behave differently on chip cuts or PI/SI mode. Mitigation:
  report PI/SI source registers and keep the first version conservative.
- LCK can perturb an otherwise working close-range path. Mitigation: opt-in
  profile, explicit reports, and immediate A/B smoke against current default.
- LCK alone does not fix IQ imbalance or full Linux parity. Mitigation: keep
  IQK listed as remaining calibration work in docs and RF-quality labels.

## Validation Plan

1. Run `openspec validate port-rtl8812au-lck-calibration --strict`.
2. Add unit tests for RF read plan constants, RF write masking, profile
   selection, and calibration labels.
3. Run `cargo fmt --all -- --check` and `cargo test --workspace`.
4. If the hardware path is reachable, deploy with rsync and run a short
   LCK-profile smoke against the Linux peer, comparing against recent
   current-default close-range evidence.
