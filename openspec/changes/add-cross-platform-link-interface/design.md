## Overview

The product binary should depend on a platform-neutral WFB link interface, not
on a specific radio implementation. The common interface owns the link control
plane and exposes local data-plane endpoints. macOS and Linux backends then
implement that contract with their native radio paths.

```text
Product binary
  -> cross-platform LinkBackend
     -> macOS backend: WFB-NG UDP codec + wfb-radio-runtime + AWUS036ACH USB
     -> Linux backend: WFB-NG native tools + wfb0 monitor interface
```

The common product boundary is WFB stream/tunnel semantics. It is not raw
RTL8812AU USB, not Linux monitor injection, and not direct 802.11 descriptor
construction.

## Goals

- Give the product binary one Rust control-plane API for macOS and Linux.
- Preserve Linux's native WFB/aircrack path instead of porting this Mac bridge
  to Linux.
- Keep the 8-hour implementation path small: embed/supervise the existing
  macOS production runtime and use UDP stream endpoints.
- Allow later replacement of WFB-NG helper processes with an in-process Rust
  codec without changing the product-facing control-plane shape.

## Non-Goals

- Do not make the product binary call raw RF packet or RTL8812AU descriptor
  APIs.
- Do not require Linux to use the macOS userspace USB bridge.
- Do not port WFB-NG crypto/FEC/session framing into Rust for the first
  production integration.
- Do not hide platform-specific operational evidence; health/report snapshots
  should preserve backend-specific diagnostics under normalized top-level
  fields.

## Control Plane

The product-facing interface should be blocking and runtime-agnostic first.
Tokio wrappers can be layered on top. Avoid `async_trait` in the core interface
unless the product integration requires it.

```rust
pub trait LinkBackend: Send {
    fn start(&mut self, config: LinkConfig) -> Result<Box<dyn LinkHandle>>;
}

pub trait LinkHandle: Send {
    fn endpoints(&self) -> &LinkEndpoints;
    fn wait_ready(&self, timeout: Duration) -> Result<LinkReady>;
    fn health(&self) -> Result<LinkHealth>;
    fn request_stop(&self) -> Result<()>;
    fn join(self: Box<Self>) -> Result<LinkReport>;
}
```

`wait_ready` means the backend has claimed/configured the radio path and local
data-plane endpoints are usable. It does not mean the remote peer is healthy;
peer/session evidence belongs in `LinkHealth` and profile gates.

`request_stop` must be cooperative. A product binary must be able to shut down
the link without sending process-wide signals. Signal handlers are acceptable
for standalone binaries, but embedded mode should not install them by default.

## Data Plane

The shared data plane is local endpoint discovery:

```rust
pub struct LinkEndpoints {
    pub streams: Vec<LinkStreamEndpoint>,
    pub tunnel: Option<LinkTunnelEndpoint>,
}

pub struct LinkStreamEndpoint {
    pub name: String,
    pub radio_port: u8,
    pub direction: LinkDirection,
    pub local_udp: SocketAddr,
    pub payload_kind: PayloadKind,
}

pub enum PayloadKind {
    RawApplicationDatagram,
    WfbDistributorDatagram,
}
```

For the product-facing API, prefer `RawApplicationDatagram` streams wherever
the backend supervises WFB-NG `wfb_tx`/`wfb_rx` codec helpers. This lets the
same product logic work on macOS and Linux.

Use `WfbDistributorDatagram` only for advanced integrations where the product
binary owns WFB-NG codec/session work itself. This is a useful lower-level
escape hatch, but it should not be the default cross-platform product boundary.

Tunnel mode exposes an IP interface or routed endpoint:

```rust
pub struct LinkTunnelEndpoint {
    pub local_ip: IpAddr,
    pub peer_ip: IpAddr,
    pub interface_name: Option<String>,
}
```

On macOS this may be an owned `utun` helper. On Linux this may be stock
`wfb_tun` or a system-managed tunnel interface.

## Backend Responsibilities

### macOS Backend

The macOS backend should use this repository's production runtime for radio
ownership:

```text
raw app UDP or tunnel
  <-> WFB-NG UDP codec helpers
  <-> wfb-radio-runtime / wfb-radio-service embedded backend
  <-> AWUS036ACH over userspace USB
```

Responsibilities:

- Build/resolve `ProductionRuntimeFlowConfig`.
- Start the runtime without process signal handlers in embedded mode.
- Apply production defaults such as `TX_MIN_INTERVAL_US=400` for loaded tunnel
  profiles when selected.
- Optionally supervise `wfb_tx`, `wfb_rx`, and `wf_tun` helpers.
- Report runtime health, radio telemetry, TX ingress metrics, RX signal
  summaries, and helper process status.

The first 8-hour slice can expose the lower-level WFB distributor datagram
endpoints if needed, but the interface should be shaped so a higher-level codec
supervisor can be added without breaking product code.

### Linux Backend

The Linux backend should not use this Mac USB bridge. It should use native WFB
operation:

```text
raw app UDP or tunnel
  <-> stock WFB-NG tools
  <-> wfb0 monitor interface
  <-> aircrack/rtl88xxau driver
```

Responsibilities:

- Verify required tools (`iw`, `ip`, `wfb_tx`, `wfb_rx`, optional `wfb_tun`).
- Verify and configure monitor/radiotap interface state.
- Pin channel/bandwidth before starting traffic.
- Start/stop stock WFB-NG processes with bounded logs and PID tracking.
- Expose the same stream/tunnel endpoints and normalized health/report fields.

Linux-specific channel setup failures should surface as backend health/config
errors, not as generic product-level failures.

## Config Shape

The product-facing config should separate common link intent from platform
backend details:

```toml
[link]
role = "gs"
link_id = 0
channel = 161
bandwidth_mhz = 20
key = "/path/to/gs.key"

[[streams]]
name = "video"
direction = "rx"
radio_port = 0
local_udp = "127.0.0.1:5800"

[[streams]]
name = "command"
direction = "tx"
radio_port = 4
local_udp = "127.0.0.1:5601"
fec_k = 2
fec_n = 4
mcs = 1

[macos]
backend = "userspace-usb"
firmware = "/path/to/rtl8812aefw.bin"
tx_min_interval_us = 400
usbhost = true

[linux]
backend = "native-wfb"
interface = "wfb0"
driver = "rtl88xxau"
```

The backend may accept richer platform settings, but common stream identity and
endpoint semantics should stay stable.

## Health And Reports

Normalize the top-level status:

- lifecycle: `starting | ready | degraded | stopping | stopped | failed`
- ready state and ready timestamp
- endpoint list
- peer/session observations when available
- TX counters: ingress, processed, submitted, failed, pending
- RX counters: frames, forwarded payloads, drops, signal summary
- backend-specific evidence under `backend`

The macOS backend can embed `ProductionRuntimeFlowReport`. The Linux backend
can embed process logs, channel evidence, and WFB-NG receiver counters.

## 8-Hour Slice

The smallest useful implementation is:

1. Add an embedded macOS backend facade that starts
   `run_production_runtime_flow` on a thread and does not install signal
   handlers.
2. Return local WFB distributor/aggregator UDP endpoints and ready/health/report
   handles.
3. Add an example Rust binary showing how the product binary starts, waits,
   sends/receives over UDP, stops, and reads the final report.
4. Keep Linux backend as a trait-compatible design/stub or product-side
   implementation for this slice.
5. Validate the macOS backend with the existing `PROFILE_SET=loaded` hardware
   gate.

This gets product integration unblocked while leaving the higher-level WFB-NG
codec supervisor and Linux backend implementation to follow without changing
the product-facing control plane.

## Longer-Term Upgrade Path

1. Keep the backend split: macOS userspace USB radio, Linux native monitor WFB.
2. Add a common process supervisor for stock WFB-NG tools on both platforms.
3. Move WFB-NG codec/session logic into a Rust crate when stability and
   observability justify removing helper processes.
4. Add direct in-process stream APIs only after WFB stream UDP semantics are
   stable in production.
