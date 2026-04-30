# RTL8812AU Init Audit

This note records the reference points used by the dry-run init planner and the live RTL8812AU init slices. It is not a vendored driver port.

## Reference

- Repository: `https://github.com/aircrack-ng/rtl8812au`
- Commit: `734485506a30d6237c2deaad666a19f8ca5379f2`
- Files inspected:
  - `hal/rtl8812a/usb/usb_halinit.c`
  - `hal/rtl8812a/rtl8812a_hal_init.c`
  - `hal/rtl8812a/rtl8812a_phycfg.c`
  - `hal/phydm/rtl8812a/halhwimg8812a_bb.c`
  - `hal/phydm/rtl8812a/halhwimg8812a_rf.c`
  - `hal/phydm/rtl8812a/phydm_regconfig8812a.c`
  - `include/Hal8812PhyReg.h`
  - `include/Hal8812PwrSeq.h`
  - `include/hal_com_reg.h`
  - `include/rtl8812a_spec.h`
  - `include/rtl8812a_hal.h`

## Dry-Run Mapping

The current planner uses the audited source to model:

- broad init phase order from the USB HAL init path
- RTL8812 card-emulation to active power sequence shape
- RF reset before power-on and BB/RF enable points
- command-register block enable ordering
- LLT table programming count and poll-per-write behavior
- firmware download setup, Realtek firmware header detection, page selection, block write sizing, checksum polling, and ready polling
- live queue reserved-page, TX/RX DMA boundary, TXDMA queue map, and packet-buffer page-size programming
- live MAC/WMAC driver-info, network type, receive filter, rate/retry, EDCA, HW sequence, BAR, and MAC TX/RX enable programming
- live BB power gates, PHY_REG table programming, AGC_TAB table programming, and crystal-cap update from external Realtek table source
- live RF radioA/radioB table programming through RTL8812A 3-wire BB write registers
- live 20 MHz channel setup: band switch, basic-rate update, fc-area, RF channel byte, WMAC bandwidth bits, BB bandwidth fields, spur handling, and RF bandwidth bits

## Live Init Mapping

The live `init` command now reuses the smoke-tested implementations for:

- card-emulation-to-active power sequence and RF A/B reset
- RTL8812A firmware download, checksum polling, and readiness polling
- LLT page-chain programming
- queue reserved pages, TX/RX DMA boundaries, TXDMA queue map, and packet-buffer page size
- MAC/WMAC receive filter, rate/retry, EDCA, HW sequence, BAR, and MAC TX/RX enable setup
- BB PHY/AGC table programming from external Realtek source
- RF radioA/radioB table programming from external Realtek source
- 20 MHz channel setup with effective channel/bandwidth reporting

Live run on April 30, 2026 with `0x0bda:0x8812` on macOS 15.7.4 passed for channel 36 at 20 MHz and wrote 1,395 USB control writes with no bulk traffic or TX frames.

## Known Gaps

The normalized trace schema does not yet carry register payload bytes, masks, expected poll values, delays, firmware bytes, or conditional branches. The dry-run planner therefore remains a comparison scaffold, not the live init implementation.

Before RX, TX, wider bandwidth, or TX power work, compare the live init/channel sequence against a Linux USB capture from the same adapter family. Any mismatch in register address, transfer length, ordering, payload bytes, polling behavior, or timing should be resolved in favor of the observed capture unless there is a clear reason to prefer another driver path.
