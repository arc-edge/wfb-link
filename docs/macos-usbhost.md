# macOS IOUSBHost Fallback

`wfb-radio-diag` has macOS-only IOUSBHost diagnostics for adapters that IOKit can see but libusb cannot enumerate.

This matters on macOS 26 because the attached RTL8812AU appeared in the IOUSB plane as an `IOUSBHostDevice`, but not as a registered, matched, configured interface tree that libusb could list or claim.

## Commands

```sh
cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-usb-state.json macos-usb-state \
  --vid 0x0bda \
  --pid 0x8812

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-descriptor-smoke.json macos-descriptor-smoke \
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

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-power-on-smoke.json macos-power-on-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-firmware-smoke.json macos-firmware-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --firmware /tmp/rtl8812aefw.bin \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-llt-smoke.json macos-llt-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-queue-dma-smoke.json macos-queue-dma-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --bulk-out-endpoint-count 3 \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-mac-smoke.json macos-mac-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-bb-smoke.json macos-bb-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --bb-source /tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c \
  --i-understand-this-writes-registers

cargo run -p wfb-radio-diag -- --json --report /tmp/wfb-remote-macos-rf-smoke.json macos-rf-smoke \
  --vid 0x0bda \
  --pid 0x8812 \
  --rf-source /tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c \
  --i-understand-this-writes-registers
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

The read-only IOUSBHost descriptor smoke test passed even though the IORegistry tree still lacked interface children:

- Report: `/tmp/wfb-remote-macos-descriptor-smoke.json`
- Control reads: 3
- Device descriptor VID/PID: `0x0bda:0x8812`
- Configurations: 1
- Configuration value: 1
- Interfaces in descriptor: 1
- Total configuration descriptor length: 53 bytes
- Bulk IN endpoints: `0x81`
- Bulk OUT endpoints: `0x02`, `0x03`, `0x04`
- Interrupt IN endpoint: `0x85`
- Max packet size: 512 bytes for all bulk endpoints, 64 bytes for interrupt IN

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

The guarded IOUSBHost power-on smoke test also passed:

- Report: `/tmp/wfb-remote-macos-power-on-smoke.json`
- Steps: 14 passed
- Control reads: 25
- Control writes: 11
- Bulk IN reads: 0
- Bulk OUT writes: 0
- Covered phases: card-emulation-to-active, command-register enable, RF path A/B reset

After a fresh `macos-power-on-smoke`, the guarded IOUSBHost firmware smoke test also passed with a temporary Linux-firmware `rtlwifi/rtl8812aefw.bin` copy:

- Firmware SHA-256: `d40396544ee56c9dab43a458344b8936aa3d878c1582e96a62e9346bdfbdf50f`
- Report: `/tmp/wfb-remote-macos-firmware-smoke.json`
- Firmware payload written: 27,484 bytes
- Firmware control writes: 290
- Checksum poll attempts: 1
- Ready poll attempts: 18
- Final `REG_MCUFWDL`: `0x000607c6`
- Bulk IN reads: 0
- Bulk OUT writes: 0

After a fresh `macos-power-on-smoke`, the guarded IOUSBHost LLT smoke test also passed:

- Report: `/tmp/wfb-remote-macos-llt-smoke.json`
- Entries written: 256
- Max poll attempts observed: 1
- Control reads: 257
- Control writes: 256
- Bulk IN reads: 0
- Bulk OUT writes: 0

After fresh power-on, firmware, and LLT smoke stages, the guarded IOUSBHost queue/DMA smoke test passed with the AWUS036ACH's known three-bulk-OUT queue layout supplied explicitly:

- Report: `/tmp/wfb-remote-macos-queue-dma-smoke.json`
- Bulk OUT endpoint count: 3
- Queue select mask: `0x07`
- HPQ pages: `0x10`
- LPQ pages: `0x10`
- NPQ pages: `0x00`
- Public queue pages: `0xd8`
- `REG_RQPN`: `0x80d81010`
- Control reads: 22
- Control writes: 10
- Bulk IN reads: 0
- Bulk OUT writes: 0

After fresh power-on, firmware, LLT, and queue/DMA smoke stages, the guarded IOUSBHost MAC/WMAC smoke test also passed:

- Report: `/tmp/wfb-remote-macos-mac-smoke.json`
- Receive configuration: `0x740060ce`
- Retry limit: `0x3030`
- Control reads: 50
- Control writes: 24
- Bulk IN reads: 0
- Bulk OUT writes: 0

After fresh power-on, firmware, LLT, queue/DMA, and MAC/WMAC smoke stages, the guarded IOUSBHost BB smoke test also passed using `aircrack-ng/rtl8812au` reference source commit `7344855`:

- Report: `/tmp/wfb-remote-macos-bb-smoke.json`
- `PHY_REG` writes applied: 215
- `AGC_TAB` writes applied: 132
- Delays applied: 0
- Control reads: 12
- Control writes: 352
- Bulk IN reads: 0
- Bulk OUT writes: 0

After BB smoke, the guarded IOUSBHost RF smoke test also passed:

- Report: `/tmp/wfb-remote-macos-rf-smoke.json`
- `radioA` writes applied: 206
- `radioB` writes applied: 193
- Delays applied: 4
- Control reads: 3
- Control writes: 399
- Bulk IN reads: 0
- Bulk OUT writes: 0

## Interpretation

The macOS 26 blocker is not raw USB device visibility, descriptor access, or default-control access. The default control endpoint is reachable through IOUSBHost even when libusb cannot enumerate the radio, standard USB descriptors can be read, and guarded register-write sequences can execute there through BB/RF programming. The blocker is pipe access: the descriptor advertises bulk endpoints, but without `IOUSBHostInterface` children, a libusb-visible configuration, or another pipe-opening mechanism, the current code still has no bulk IN/OUT pipes for RX or TX.

The next useful implementation work is to investigate an IOUSBHost interface/pipe path or a DriverKit transport for those descriptor-confirmed bulk endpoints.

## SDK Notes

The macOS 26.4 Command Line Tools IOUSBHost headers confirm the public object split:

- `IOUSBHostDevice` exposes default-control requests, `configureWithValue:matchInterfaces:error:`, the current `configurationDescriptor`, and reset.
- `IOUSBHostInterface` exposes `copyPipeWithAddress:error:` for endpoint pipes.
- `IOUSBHostPipe` exposes synchronous `sendIORequestWithData:bytesTransferred:completionTimeout:error:` for bulk/interrupt transfers and async enqueue APIs.

So the next proof target is not endpoint discovery; that is done through descriptors. The target is obtaining an `IOUSBHostInterface` service for interface 0, or replacing that with a DriverKit path that can own the interface and create pipes for `0x81`, `0x02`, `0x03`, and `0x04`.
