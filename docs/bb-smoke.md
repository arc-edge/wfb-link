# BB Smoke

`bb-smoke` is the guarded live diagnostic for the RTL8812A baseband setup slice. It claims the adapter, verifies firmware/MAC readiness, enables the BB/RF power gates used by `PHY_BBConfig8812`, parses external Realtek BB tables, writes selected `PHY_REG` and `AGC_TAB` entries through vendor control transfers, and applies the RTL8812A crystal-cap update.

The command intentionally reads the table source from an external checkout instead of vendoring Realtek GPL table data into this repository.

## Command

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-bb-smoke.json bb-smoke \
  --vid 0x0bda --pid 0x8812 \
  --bb-source /tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c \
  --i-understand-this-writes-registers
```

Default condition inputs target the attached AWUS036ACH-class RTL8812AU path:

- `support_interface=0x02` for USB
- `board_type=0xd8` for GLNA/GPA/ALNA/APA branches
- `type_glna/type_gpa/type_alna/type_apa=0x0000`
- `crystal_cap=0x20`

All of these are visible CLI parameters so EFUSE-derived values can replace the defaults later.

## Live Result

Live run on April 30, 2026 with `0x0bda:0x8812` on macOS 15.7.4 passed:

- `PHY_REG`: 235 raw pairs, 14 condition marker pairs, 6 skipped write pairs, 215 writes applied.
- `AGC_TAB`: 334 raw pairs, 10 condition marker pairs, 192 skipped write pairs, 132 writes applied.
- Setup writes passed for `REG_SYS_FUNC_EN`, `REG_RF_CTRL`, `REG_RF_B_CTRL_8812`, and `REG_MAC_PHY_CTRL`.
- USB counters: 12 control reads, 352 control writes, 0 bulk IN reads, 0 bulk OUT writes, 0 TX frames.
- Post-run `reg-smoke` passed with `REG_SYS_FUNC_EN=0x1f`, `REG_MCUFWDL=0xc6`, and `REG_CR=0x06ff`.

Remote macOS 26 IOUSBHost fallback run on April 30, 2026 also passed after power-on, firmware, LLT, queue/DMA, and MAC/WMAC smoke stages:

- Command: `macos-bb-smoke`
- Report: `/tmp/wfb-remote-macos-bb-smoke.json`
- Source checkout: `aircrack-ng/rtl8812au` commit `7344855`
- `PHY_REG` writes applied: 215
- `AGC_TAB` writes applied: 132
- USB counters: 12 control reads, 352 control writes, 0 bulk IN reads, 0 bulk OUT writes, 0 TX frames.
- No libusb enumeration or interface claim was used.

## Source Mapping

- BB power gate sequence: `hal/rtl8812a/rtl8812a_phycfg.c`, `PHY_BBConfig8812`.
- PHY and AGC table readers: `hal/phydm/rtl8812a/halhwimg8812a_bb.c`.
- Table write semantics and delay markers: `hal/phydm/rtl8812a/phydm_regconfig8812a.c`.
- Crystal-cap update: `hal/hal_com.c`, `hal_set_crystal_cap`.

## Boundaries

`bb-smoke` does not program RF radio tables, tune a channel, start RX, write bulk OUT, or transmit frames. RF setup is the next init slice because RF writes are not direct register writes; the Realtek path encodes RF register/data pairs into the BB 3-wire serial registers for each RF path.
