# RTL8812AU EFUSE TX Power

This note records the current EFUSE-derived TXAGC port for the AWUS036ACH
RTL8812AU path. The goal is to replace broad planted TXAGC values with an
explicit, reportable calculation that can be compared against the Linux
aircrack-ng driver.

## Source Layout

`efuse-dump` decodes the RTL8812AU logical EFUSE map and summarizes the 84-byte
TX-power region beginning at logical offset `0x010`. The region is:

- `path_a_2g`: 18 bytes
- `path_a_5g`: 24 bytes
- `path_b_2g`: 18 bytes
- `path_b_5g`: 24 bytes

For the current AWUS036ACH bench adapter, the captured TX-power region is also
stored as `fixtures/rf-quality/awus036ach-ch36-efuse-tx-power.hex`.

## Linux Basis

The implementation follows the Linux driver flow that matters for channel 36
HT20:

- `hal_load_txpwr_info`: maps the EFUSE PG bytes into per-path 2.4 GHz and
  5 GHz base/diff arrays.
- `PHY_GetTxPowerIndexBase`: selects the 5 GHz channel group and applies the
  per-bandwidth/per-stream EFUSE diff.
- `PHY_GetTxPowerByRate`: adds the default PHY_REG_PG by-rate offset.
- `PHY_GetTxPowerLimit`: applies the regulatory or safety cap.
- `PHY_SetTxPowerIndex_8812A`: packs each final byte lane and writes the TXAGC
  registers.

The channel-36 group is 5 GHz group `0`. The current pure helper supports 5 GHz
OFDM, HT, and VHT TXAGC writes for path A, path B, or both paths. It does not
write CCK TXAGC for 5 GHz because the Linux `phy_set_tx_power_level_by_path`
path does not program CCK rates for 5 GHz operation.

## Safety Clamp

`--tx-power-safety-profile linux-ch36-ht20` clamps EFUSE-derived values to the
Linux-derived channel 36 HT20 caps captured from the same bench class:

- Path A: OFDM `0x1b`, 1SS HT/VHT `0x17`, 2SS HT/VHT `0x15`
- Path B: OFDM `0x1d`, 1SS HT/VHT `0x1c`, 2SS HT/VHT `0x1a`

The helper reports each lane's EFUSE base, EFUSE diff, PHY_REG_PG by-rate
offset, unclamped index, clamp profile, final index, and whether clamping
changed the value.

## Guarded Use

EFUSE-derived power is opt-in and mutually exclusive with the older manual
index override:

```bash
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-efuse-power-listen.json \
  bridge-tx-listen \
  --macos-usbhost \
  --vid 0x0bda --pid 0x8812 \
  --init-before-tx --linux-init-order \
  --firmware /path/to/rtl8812au_fw.bin \
  --channel 36 --bandwidth 20 \
  --bind 127.0.0.1:5611 \
  --max-datagrams 300 \
  --tx-power-mode efuse-derived \
  --tx-power-efuse-report /tmp/wfb-live-efuse-dump.json \
  --tx-power-safety-profile linux-ch36-ht20 \
  --i-understand-this-transmits
```

Use `--tx-power-mode manual-index --tx-power-index 0x1a` or the shorthand
`--tx-power-index 0x1a` to preserve the existing manual TXAGC behavior.

## Current Scope

This is a channel-36 HT20 bring-up implementation. It intentionally keeps the
calculation pure and reportable, with no automatic promotion to default TX
behavior. Remaining work before long-distance acceptance:

- compare macOS calibration-state probes against a Linux run on the same
  adapter/channel (`docs/rtl8812au-calibration-state.md`);
- decide whether full Linux IQK/LCK ports are justified by measured RF outcome;
- define accepted close-range and outdoor range profiles.

## Close-Range Comparison

On May 2, 2026, the hardware bench ran the same channel 36 HT20 WFB profile
through three TX-power modes. Each run used Linux `wfb_tx -d` with
`k=8,n=12`, 120 source payloads of exactly 1000 bytes, and Mac
`bridge-tx-listen --macos-usbhost --init-before-tx` with `--max-datagrams 180`.
The Linux peer was pinned to channel 36 HT20 for the run and the normal
`arc-wfb-link-1` container was restarted afterward.

| Mode | Mac report | TXAGC control | Mac injected | Linux recovered | Linux artifacts |
| --- | --- | --- | ---: | ---: | --- |
| Current default | `/tmp/wfb-agent-rfq-default-listen.json` | existing captured/default state | 180/180 | 120/120 | `/tmp/rfq-default-rx.log`, `/tmp/rfq-default-tx.log`, `/tmp/rfq-default-rf.pcap` |
| Manual index `0x1a` | `/tmp/wfb-agent-rfq-manual1a-listen.json` | 24 TXAGC registers set to `0x1a1a1a1a` | 180/180 | 120/120 | `/tmp/rfq-manual1a-rx.log`, `/tmp/rfq-manual1a-tx.log`, `/tmp/rfq-manual1a-rf.pcap` |
| EFUSE-derived | `/tmp/wfb-agent-rfq-efuse-listen.json` | 22 5 GHz OFDM/HT/VHT TXAGC registers from EFUSE group 0 + Linux channel-36 HT20 clamp | 180/180 | 120/120 | `/tmp/rfq-efuse-rx.log`, `/tmp/rfq-efuse-tx.log`, `/tmp/rfq-efuse-rf.pcap` |

The EFUSE-derived report includes the decoded source region, selected
`5g_group_00`, path A/B programming, per-lane EFUSE base and diff, default
PHY_REG_PG by-rate offset, unclamped index, clamp, before/write/after register
evidence, and the final Linux-derived channel 36 HT20 values such as
`0x0c24=0x1b1b1b1b`, `0x0c2c=0x17171717`, `0x0c34=0x15151515`,
`0x0e24=0x1d1d1d1d`, `0x0e2c=0x1c1c1c1c`, and `0x0e34=0x1a1a1a1a`.

This close-range result proves that the EFUSE-derived mode does not regress
basic WFB recovery on the bench. It is not yet a long-distance RF-quality
acceptance result; the next work is calibration-state evidence and stepped or
outdoor comparison against the Linux baseline.
