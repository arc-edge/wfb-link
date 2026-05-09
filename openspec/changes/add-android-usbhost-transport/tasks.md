## 1. Contract And Config

- [x] 1.1 Add runtime Android USBHost config, backend enum support, production
      config serialization, endpoint validation, and fail-closed open behavior.
- [x] 1.2 Add `[android_usbhost]` service config and CLI overrides.
- [x] 1.3 Add unit tests for endpoint validation, runtime backend mapping,
      service config mapping, and multiple-backend rejection.
- [x] 1.4 Document the Android support boundary in README, integration docs,
      service config reference, runtime boundary, and OpenSpec.

## 2. Native Android Transfer Bridge

- [x] 2.1 Choose the bridge strategy: JNI calls into `UsbDeviceConnection` or a
      native file-descriptor ownership model.
- [x] 2.2 Implement Android control transfers for RTL8812AU vendor register and
      EFUSE access.
- [x] 2.3 Implement Android bulk IN and bulk OUT transfers with timeout and
      short-transfer semantics matching the runtime traits.
- [x] 2.4 Add Android-specific lifecycle tests for ownership, close, timeout,
      and unsupported-device errors.
      Unit coverage verifies fd preflight, caller-owned fd survival after a
      rejected wrap, timeout classification, and unsupported-device open-plan
      errors. Successful close/drop behavior remains part of the Android
      hardware smoke.

## 3. Android Hardware Validation

- [ ] 3.1 Add an Android smoke harness that obtains USB permission, opens the
      AWUS036ACH, and passes the runtime transport handoff into Rust.
- [ ] 3.2 Validate power-on/init RX-only descriptor parsing.
- [ ] 3.3 Validate single TX and bounded bidirectional WFB distributor datagrams
      against the Linux peer.
- [ ] 3.4 Run the production managed-stream profile and compare against the
      macOS bench results before considering Android production-ready.
