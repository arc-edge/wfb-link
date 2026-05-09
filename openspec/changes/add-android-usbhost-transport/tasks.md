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

- [x] 3.1 Add an Android smoke harness that obtains USB permission, opens the
      AWUS036ACH, and passes the runtime transport handoff into Rust.
      Added an Android harness, `wfb-android-smoke` JNI crate, and direct
      SDK/NDK debug APK packaging script. Product Gradle/instrumentation
      packaging remains follow-up work.
- [x] 3.2 Validate power-on/init RX-only descriptor parsing.
      Pixel 7 Pro smoke now passes permission, Java control transfer, Rust JNI
      register read, and full production init (14 phases, 3103 control writes).
      Latest APK smoke parsed one frame before init and one frame after full
      init through the runtime RX descriptor parser.
- [ ] 3.3 Validate single TX and bounded bidirectional WFB distributor datagrams
      against the Linux peer.
      Android init+TX smoke now submits 3/3 descriptor-prefixed frames through
      bulk OUT with 192 USB bytes, 0 failed writes, and 0 short writes. The
      smoke harness now also sends three synthetic WFB distributor datagrams
      through the production bridge TX path and host tests verify their packet
      shape parses through `wfb-bridge`. Pixel 7 Pro live smoke on 2026-05-09
      saw the RTL8812AU over Android USBHost, completed production init, and
      reported `submitted=6/6`, `wfb_incoming=3`, `wfb_injected=3`,
      `wfb_malformed=0`. Live receiver-backed WFB validation is still pending;
      the reachable `10.42.0.1` peer is `drone-60ee4` and currently lacks
      `tcpdump`, `wfb_rx`, and `wfb_tx`.
- [ ] 3.4 Run the production managed-stream profile and compare against the
      macOS bench results before considering Android production-ready.
