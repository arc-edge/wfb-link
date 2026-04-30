# EFUSE Dump

`efuse-dump` reads the RTL8812AU physical EFUSE bytes through `REG_EFUSE_CTRL`, decodes the 512-byte logical map, and summarizes identity, RFE, thermal, and TX-power table offsets.

The command is write-gated because an EFUSE read still writes EFUSE control, grant, and loader-clock selector registers. It does not program EFUSE, tune a channel, issue bulk traffic, or transmit frames.

## Command

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-live-efuse-dump.json efuse-dump \
  --vid 0x0bda \
  --pid 0x8812 \
  --raw-out /tmp/wfb-live-efuse-raw.bin \
  --logical-map-out /tmp/wfb-live-efuse-logical.bin \
  --i-understand-this-writes-control-registers
```

Useful fields:

- `efuse.raw_hex`: physical EFUSE bytes read from the device.
- `efuse.logical_map_hex`: decoded 512-byte logical EFUSE map.
- `efuse.decoded_packets`: physical packet headers mapped into logical sections.
- `efuse.summary.raw_used_bytes`: physical bytes consumed before the terminating EFUSE header.
- `efuse.summary.terminating_offset`: physical offset of the first terminator header.
- `efuse.summary.named_bytes`: selected RTL8812AU identity, RFE, thermal, and board-option bytes.
- `efuse.summary.tx_power`: the 84-byte TX-power area split into path A/B and 2G/5G regions.

## Live Result

On April 30, 2026, macOS 15.7.4 with the attached `0x0bda:0x8812` adapter passed `efuse-dump`:

- Report: `/tmp/wfb-live-efuse-dump.json`.
- Raw physical dump: `/tmp/wfb-live-efuse-raw.bin`.
- Decoded logical map: `/tmp/wfb-live-efuse-logical.bin`.
- Decoded packets: 49.
- Physical EFUSE used before terminator: 378 bytes.
- USB identity from EFUSE: `0x0bda:0x8812`.
- EFUSE MAC address: `00:c0:ca:ba:bd:9f`.
- Thermal meter byte: `0x22`.
- PA type byte: `0x33`.
- 5 GHz TX BB swing byte: `0x55`.
- RFE option byte: `0x03`.
- TX power region: 84 bytes, 66 non-`0xff` bytes.

Region summary:

```text
path_a_2g: 18 bytes, 14 non-0xff
path_a_5g: 24 bytes, 19 non-0xff
path_b_2g: 18 bytes, 14 non-0xff
path_b_5g: 24 bytes, 19 non-0xff
```

This proves the Mac userspace path can retrieve the power-table source bytes. It does not yet prove the mapping from EFUSE byte values to final per-rate RF TX power indexes, so explicit TX power control remains disabled.

## macOS IOUSBHost Result

On April 30, 2026, the same adapter on `rownd@rownds-macbook-pro` running macOS 26 was visible in IOKit but not enumerable through libusb. The macOS-only `macos-efuse-dump` command used IOUSBHost default-control transfers instead of a libusb interface claim and returned the same decoded EFUSE identity and power-region summary:

- Report: `/tmp/wfb-remote-macos-efuse-dump.json`.
- Raw physical dump: `/tmp/wfb-remote-macos-efuse-raw.bin`.
- Decoded logical map: `/tmp/wfb-remote-macos-efuse-logical.bin`.
- Decoded packets: 49.
- Physical EFUSE used before terminator: 378 bytes.
- USB identity from EFUSE: `0x0bda:0x8812`.
- EFUSE MAC address: `00:c0:ca:ba:bd:9f`.
- RFE option byte: `0x03`.
- TX power region: 84 bytes, 66 non-`0xff` bytes.

This proves the macOS 26 fallback can perform guarded register read/write control sequences. It does not prove bulk endpoint access, RX, TX, or full init on that host.
