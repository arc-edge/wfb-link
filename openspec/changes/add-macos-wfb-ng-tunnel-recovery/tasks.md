## 1. Tunnel Shim

- [x] 1.1 Add a macOS `utun` helper that creates/configures a GS tunnel
      interface.
- [x] 1.2 Encode outgoing IP packets as WFB-NG tunnel records and aggregate up
      to the radio MTU.
- [x] 1.3 Decode incoming WFB-NG tunnel records, ignore empty keepalives, and
      write recovered IP packets to `utun`.
- [x] 1.4 Add a self-test for tunnel record parsing, utun address-family
      framing, and aggregation.

## 2. WFB-NG Codec Build

- [x] 2.1 Add minimal macOS compatibility headers for building WFB-NG
      distributor/aggregator binaries.
- [x] 2.2 Add a build wrapper that produces `wfb_tx`, `wfb_rx`, and
      `wfb_keygen` under `target/wfb-ng-macos/bin`.
- [x] 2.3 Prefer static `libsodium.a` when available so the binaries can be
      copied to the hardware Mac.

## 3. Recovery Orchestration

- [x] 3.1 Add a recovery runner that starts `wfb-radio-service`,
      WFB-NG `wfb_rx`, WFB-NG `wfb_tx`, and the macOS tunnel shim.
- [x] 3.2 Use Arc GS tunnel defaults: link ID `0x000000`, RX stream `3`, TX
      stream `4`, FEC `2/4`, GS `10.5.0.1`, drone `10.5.0.2`.
- [x] 3.3 Fail fast when the required WFB-NG keypair is missing.
- [x] 3.4 Record observed WFB channel IDs in production RX telemetry for
      tunnel recovery diagnostics.

## 4. Verification

- [x] 4.1 Local tunnel helper self-test passes.
- [x] 4.2 Local WFB-NG codec build succeeds and links without a dynamic
      Homebrew libsodium dependency.
- [x] 4.3 Hardware Mac tunnel helper self-test passes.
- [x] 4.4 Hardware Mac production service crate builds.
- [x] 4.5 Paired-key active RF probes submit frames cleanly on the hardware Mac
      and report zero WFB-prefixed drone responses across the tested Arc tuples.
- [ ] 4.6 End-to-end tunnel SSH over RF succeeds after the drone-side
      `wfb-link`/`wfb_tun` stack is confirmed running and keyed.
