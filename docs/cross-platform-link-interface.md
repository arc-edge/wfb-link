# Cross-Platform WFB Link Interface

The product binary should use one WFB link interface on macOS and Linux. The
interface owns lifecycle, endpoint discovery, health, and shutdown. Each
platform backend owns the radio implementation that makes sense on that OS.

```text
Product binary
  -> LinkBackend
     -> macOS: WFB-NG UDP codec + wfb-radio-runtime + AWUS036ACH USB
     -> Linux: WFB-NG native tools + wfb0 monitor interface + rtl88xxau
```

## Product Boundary

The product boundary should be WFB stream or tunnel semantics, not raw RF.

The common control plane is:

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

The common data plane is local endpoint discovery:

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

Prefer `RawApplicationDatagram` for product-facing streams when the backend
supervises WFB-NG `wfb_tx`/`wfb_rx` helpers. Use `WfbDistributorDatagram` only
when the caller owns WFB-NG codec/session work.

## macOS Backend

The macOS backend uses this repository's native radio runtime:

```text
raw app UDP or tunnel
  <-> WFB-NG UDP codec helpers
  <-> wfb-radio-runtime / wfb-radio-service
  <-> AWUS036ACH over userspace USB
```

For the 8-hour integration path, the backend can first expose WFB distributor
datagram endpoints and embed `run_production_runtime_flow` without installing
process signal handlers. The next step is supervising `wfb_tx`, `wfb_rx`, and
optional `wf_tun` helpers so the product only sees raw stream/tunnel endpoints.

## Linux Backend

The Linux backend should use native Linux WFB:

```text
raw app UDP or tunnel
  <-> stock WFB-NG tools
  <-> wfb0 monitor interface
  <-> aircrack/rtl88xxau driver
```

It should verify tools, configure `wfb0` channel/bandwidth, supervise stock
WFB-NG processes, and report normalized health plus Linux-specific evidence.
It should not use the macOS userspace USB bridge.

## Why This Boundary

- Product code gets one lifecycle and endpoint contract on both platforms.
- macOS can keep the userspace USB radio path that works today.
- Linux can keep the native monitor-mode path that already exists.
- WFB-NG compatibility remains intact.
- A future Rust WFB codec can replace helper processes without changing the
  product-facing link control plane.
