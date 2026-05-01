# RTL8812AU Upstream Port Checklist

Derived from a cold-init usbmon capture against an ALFA AWUS036ACH on
`drone-2f389` using the aircrack-ng RTL8812AU driver.

## Current Picture

The project is no longer missing the static BB/RF tables. Live init loads:

- 215 PHY writes
- 132 AGC writes
- 206 RadioA writes
- 193 RadioB writes

The remaining TX gap is in upstream runtime code that overwrites placeholder
values left by the static tables. The important symptom is:

- `REG_TXPAUSE=0x00`
- `REG_EDCA_BE_PARAM=0x005ea42b`
- `REG_CR=0x06ff`
- BB/RF tables loaded
- queue descriptors can be accepted
- data-class TX still fails peer decode, or the BE queue reports TXDMA status

The static `phy_reg` table touches many TX-critical addresses, but with
placeholder or RX-leaning values. Linux later rewrites those addresses from
runtime functions.

## Tier 1 Runtime Ports

These are required before WFB data TX should be considered complete.

### `_PHY_RFEPinSetting_8812A`

Source: `hal/rtl8812a/rtl8812a_phycfg.c`

Purpose: switch the RF frontend mux from post-table RX-leaning state to a
TX-capable RFE configuration selected by EFUSE `RFE_TYPE`.

Registers:

- `0x0C1C` / `0x0E1C`: TX path enable bitmap
- `0x0CB0` / `0x0EB0`: RFE pinmux
- `0x0CB4` / `0x0EB4`: RFE inversion
- `0x0CB8` / `0x0EB8`: RFE timing

The captured AWUS036ACH HT20 path currently uses little-endian register
readback values:

- `0x0CB0 = 0x54337717`
- `0x0CB4 = 0x01000077`
- `0x0CB8 = 0x00508242`
- `0x0EB0 = 0x54337717`
- `0x0EB4 = 0x01000077`
- `0x0EB8 = 0x00508242`

### `phy_set_tx_power_level_by_path`

Source: `hal/rtl8812a/rtl8812a_phycfg.c`, `hal/hal_com_phycfg.c`

Purpose: compute per-rate TXAGC values from EFUSE power tables and regulatory
caps. The project already decodes the EFUSE source bytes; the missing work is
writing the computed values into the BB TX power registers.

Key registers:

- `0x0C20`-`0x0C4C`
- `0x0C50` / `0x0C54`
- `0x0C90` / `0x0C94`
- mirror set in `0x0Exx`
- selected MAC/BB control values such as `0x0668`, `0x0670`, `0x0718`

### `phy_iq_calibrate_8812a`

Source: `hal/phydm/halphyrf/rtl8812a/halphyrf_8812a.c`

Purpose: run IQ calibration and write BB IQK result/shadow registers. A full
port is larger; planted constants are enough for bench bring-up but should not
be treated as final RF quality.

Captured stop-gap values currently used by the diagnostic path:

- `0x0C58 = 0x30000c1c`
- `0x0C5C = 0x00000058`
- `0x0E58 = 0x30000c1c`
- `0x0E5C = 0x00000058`
- `0x0C90 = 0x01817d24`
- `0x0E90 = 0x01817d24`

### `rtl8812au_init_mac_addr`

Purpose: write the EFUSE MAC address into `REG_MACID` (`0x0610`-`0x0615`).
This is trivial but matters for retry and station-context paths.

## Tier 2 Runtime Ports

These improve sustained data TX and should be ported after Tier 1 behavior is
named rather than hidden in captured constants.

- `_init_protection_8812`
- `_init_rate_fallback_8812`
- `_init_xmit_priority_xmit_8812au`
- `_init_beacon_parameters_8812`
- `_init_rx_setting_8812`
- `_init_wmac`

Parsing `array_mp_8812a_mac_reg` from
`hal/phydm/rtl8812a/halhwimg8812a_mac.c` is a useful single-shot improvement.
The current same-session init already applies the MAC table during live
bench/listener paths, but the main init audit should keep it visible as a
distinct phase.

## May 1, 2026 Evidence

Management/probe TX is RF-confirmed:

- `bridge-tx-bench` sent 200/200 probe frames; Linux monitor captured 199
  `WFBMACRF1` frames.
- `bridge-tx-listen` sent 50/50 20 MHz probe datagrams; Linux monitor captured
  49 `WFBMACRF1` frames.

Data-class TX is not solved yet:

- MGNT-queue data submissions drain from chip-side queue status but are not
  decoded by the Linux monitor.
- BE-queue data submissions still set `REG_TXDMA_STATUS=0x00000401` and move
  `REG_Q0_INFO`, so BE remains a bad path.
- A mismatched 40 MHz radiotap bit against 20 MHz channel init caused an
  otherwise valid listener probe run to disappear from RF capture; descriptor
  bandwidth bits must be treated as authoritative.

Key artifacts:

- `/tmp/wfb-probe-status.json`
- `/tmp/mac-probe-status.pcap`
- `/tmp/wfb-listen-probe20-status.json`
- `/tmp/mac-listen-probe20-status.pcap`
- `/tmp/wfb-bench-wfbdata20-status.json`
- `/tmp/wfb-bench-wfbdata-be-status.json`
- `/tmp/wfb-bench-qosdata-status.json`
