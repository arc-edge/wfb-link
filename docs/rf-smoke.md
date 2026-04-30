# RF Smoke

`rf-smoke` is the guarded live diagnostic for the RTL8812A RF table setup slice. It claims the adapter, verifies firmware/MAC/BB readiness, parses external Realtek radio tables, evaluates the same condition markers used by the driver, and writes selected RF entries through the RTL8812A path-specific 3-wire BB registers.

The command reads the RF table source from an external checkout instead of vendoring Realtek GPL table data into this repository.

## Command

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-rf-smoke.json rf-smoke \
  --vid 0x0bda --pid 0x8812 \
  --rf-source /tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c \
  --i-understand-this-writes-registers
```

Default condition inputs match `bb-smoke`:

- `support_interface=0x02` for USB
- `board_type=0xd8` for GLNA/GPA/ALNA/APA branches
- `type_glna/type_gpa/type_alna/type_apa=0x0000`

## RF Write Encoding

The Linux path writes RF registers through `phy_RFSerialWrite`:

```text
encoded = ((rf_offset & 0xff) << 20) | (data & 0x000fffff)
path A BB write register = 0x0c90
path B BB write register = 0x0e90
```

`rf-smoke` applies the same encoding and sleeps for `0xffe` delay markers.

## Live Result

Live run on April 30, 2026 with `0x0bda:0x8812` on macOS 15.7.4 passed:

- `radioA`: 432 raw pairs, 62 condition marker pairs, 160 skipped write pairs, 206 writes applied, 4 delays applied.
- `radioB`: 424 raw pairs, 64 condition marker pairs, 167 skipped write pairs, 193 writes applied.
- USB counters: 3 control reads, 399 control writes, 0 bulk IN reads, 0 bulk OUT writes, 0 TX frames.
- Post-run `reg-smoke` passed with `REG_SYS_FUNC_EN=0x1f`, `REG_MCUFWDL=0xc6`, and `REG_CR=0x06ff`.

## Source Mapping

- RF table loader: `hal/rtl8812a/rtl8812a_rf6052.c`, `PHY_RF6052_Config_8812`.
- RF table readers: `hal/phydm/rtl8812a/halhwimg8812a_rf.c`.
- RF write semantics and delay markers: `hal/phydm/rtl8812a/phydm_regconfig8812a.c`.
- RF serial write encoding and path registers: `hal/rtl8812a/rtl8812a_phycfg.c`, `phy_RFSerialWrite`; `include/Hal8812PhyReg.h`.

## Boundaries

`rf-smoke` does not tune a channel, start RX, write bulk OUT, or transmit frames. The verified smoke stages and 20 MHz channel selection are now integrated into live `init`; the next radio slice is live RX over bulk IN.
