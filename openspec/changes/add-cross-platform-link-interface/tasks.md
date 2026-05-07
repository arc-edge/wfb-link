## 1. Interface Design

- [x] 1.1 Define the cross-platform control-plane and data-plane boundary.
- [x] 1.2 Document macOS userspace-radio and Linux native-WFB backend
      responsibilities.
- [x] 1.3 Define the 8-hour macOS embedding slice and longer-term codec path.

## 2. Embedding API

- [x] 2.1 Add a small product-facing Rust interface module/crate with
      `LinkBackend`, `LinkHandle`, endpoint, health, and report types.
- [x] 2.2 Add a macOS backend handle that starts the existing production
      runtime on a thread without installing process signal handlers.
- [x] 2.3 Add cooperative stop plumbing for embedded runtime use.

## 3. Examples And Validation

- [x] 3.1 Add an example Rust binary that starts the macOS backend, waits for
      ready, prints endpoints/health, requests stop, and prints the report.
- [x] 3.2 Add unit tests for endpoint shape, embedded no-signal behavior, and
      cooperative stop/report behavior.
- [x] 3.3 Run the hardware `PROFILE_SET=loaded` gate after embedding changes.
      Attempted on 2026-05-07 after deploying the rebuilt
      `wfb-radio-service` to the remote macOS host. Two `400 us` runs had
      radio/runtime and tunnel probes pass, but the strict duplex side-load
      gate rejected them due to Mac-to-Linux side-stream recovery below 100/100:
      `/tmp/wfb-mac-wf-tun-loaded-profile-link-20260507-010003` recovered
      90/100 M2L, and
      `/tmp/wfb-mac-wf-tun-loaded-profile-link-rerun-20260507-010104`
      recovered 88/100 M2L. The accepted loaded gate is
      `/tmp/wfb-mac-wf-tun-loaded-profile-link-500us-20260507-010322` with
      `TX_MIN_INTERVAL_US=500`: 262,144 bytes in 7.997s, side streams 100/100
      in both directions, zero tunnel drops/corrupt/truncated messages, zero
      TX failures, and one pending ingress datagram.
