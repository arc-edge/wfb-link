# macOS IOUSBHost Fallback

`wfb-radio-diag` has macOS-only IOUSBHost diagnostics for adapters that IOKit can see but libusb cannot enumerate.

This matters on macOS 26 because the attached RTL8812AU appeared in the IOUSB plane as an `IOUSBHostDevice`, but not as a registered, matched, configured interface tree that libusb could list or claim.

## Commands

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-usb-state.json macos-usb-state \
  --vid 0x0bda \
  --pid 0x8812

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-reg-smoke.json macos-reg-smoke \
  --vid 0x0bda \
  --pid 0x8812

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-efuse-dump.json macos-efuse-dump \
  --vid 0x0bda \
  --pid 0x8812 \
  --raw-out /tmp/wfb-remote-macos-efuse-raw.bin \
  --logical-map-out /tmp/wfb-remote-macos-efuse-logical.bin \
  --i-understand-this-writes-control-registers
```

## April 30, 2026 Remote Result

On `rownd@rownds-macbook-pro`, `usb-probe --all` did not list the RTL8812AU through libusb. IOKit did list it:

- Product: `802.11n NIC`
- VID/PID: `0x0bda:0x8812`
- Serial: `123456`
- Location ID: `0x01100000`
- USB link speed: 480 Mbps
- IOKit flags: `!registered`, `!matched`, `active`
- Configurations: `1`
- Current configuration: absent before manual configuration, `1` after a direct IOUSBHost configure attempt
- Interface children: absent

The integrated IOUSBHost transport then passed direct default-control register reads:

```text
REG_SYS_FUNC_EN 0x0002 u8  = 0x1c
REG_APS_FSMCO   0x0004 u32 = 0x20020002
REG_SYS_CLKR    0x0008 u16 = 0xfc23
REG_RF_CTRL     0x001f u8  = 0x00
REG_MCUFWDL     0x0080 u8  = 0x05
REG_CR          0x0100 u16 = 0xeaea
```

The same IOUSBHost transport also passed the guarded EFUSE dump:

- Report: `/tmp/wfb-remote-macos-efuse-dump.json`
- Raw physical dump: `/tmp/wfb-remote-macos-efuse-raw.bin`
- Decoded logical map: `/tmp/wfb-remote-macos-efuse-logical.bin`
- Decoded packets: 49
- Physical EFUSE used before terminator: 378 bytes
- EFUSE USB identity: `0x0bda:0x8812`
- EFUSE MAC address: `00:c0:ca:ba:bd:9f`
- RFE option byte: `0x03`
- TX power region: 84 bytes, 66 non-`0xff` bytes

## Interpretation

The macOS 26 blocker is not raw USB device visibility. The default control endpoint is reachable through IOUSBHost even when libusb cannot enumerate the radio. The blocker is interface and endpoint materialization: without `IOUSBHostInterface` children or a libusb-visible configuration, the current code has no bulk IN/OUT pipes for RX or TX.

The next useful implementation work is to move guarded control-transfer diagnostics onto IOUSBHost, then investigate an IOUSBHost interface/pipe path or a DriverKit transport for bulk endpoints.
