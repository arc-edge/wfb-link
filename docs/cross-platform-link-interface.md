# Cross-Platform WFB Link Interface

The product binary should use one WFB link interface across macOS, Linux, and
Android. The interface owns lifecycle, endpoint discovery, health, and
shutdown. Each platform backend owns the radio implementation that makes sense
on that OS.

```text
Product binary
  -> LinkBackend
     -> macOS: WFB-NG UDP codec + wfb-radio-runtime + AWUS036ACH USB
     -> Linux: WFB-NG native tools + wfb0 monitor interface + rtl88xxau
     -> Android: wfb-radio-runtime + Android USB host transport
```

## Product Boundary

The product boundary should be WFB stream or tunnel semantics, not raw RF. The
initial Rust facade lives in `crates/wfb-link`.

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
    pub direction: LinkDirection,
    pub local_udp: SocketAddr,
    pub payload_kind: PayloadKind,
    pub criticality: StreamCriticality,
    pub stream: Option<WfbStreamId>,
}

pub struct WfbStreamId {
    pub link_id: Option<u32>,
    pub radio_port: u8,
}

pub enum PayloadKind {
    RawApplicationDatagram,
    WfbDistributorDatagram,
}

pub enum StreamCriticality {
    Required,
    BestEffort,
}
```

Prefer `RawApplicationDatagram` for product-facing streams when the backend
supervises WFB-NG `wfb_tx`/`wfb_rx` helpers. Use `WfbDistributorDatagram` only
when the caller owns WFB-NG codec/session work. Distributor datagram endpoints
may leave `stream` unset because one local UDP ingress can carry multiple WFB
streams.

`UserspaceRadioBackend` is a direct-radio backend, not a codec supervisor. It
therefore rejects `RawApplicationDatagram` streams before startup. Raw
application endpoints are currently valid for `MacosWfbTunnelBackend`'s managed
IP tunnel path, or for future managed codec backends.

Do not treat endpoint metadata as a runtime rewrite mechanism. Runtime sockets
come from `UserspaceRadioConfig` / `wfb-radio-service` resolution. If a
product needs named streams, put them in `[[streams]]` or build the complete
runtime config and endpoint model together.

## Platform Backends

`UserspaceRadioConfig`, `UserspaceRadioBackend`, and
`LinkBackendConfig::UserspaceRadio` are the portable direct-radio contract. They
do not promise a particular USB transport; the runtime config selects the
platform-specific transport underneath. Existing macOS TOML profiles select the
macOS IOUSBHost path with `[macos_usbhost]`. A future Android backend should
reuse the same lifecycle, endpoint, health, and report shapes while providing an
Android USB host transport.

The deprecated `MacosUserspaceRadio*` names are compatibility aliases. New
product code should use `UserspaceRadio*`.

## macOS Backends

The macOS direct-radio path uses this repository's native radio runtime:

```text
raw app UDP or tunnel
  <-> WFB-NG UDP codec helpers
  <-> wfb-radio-runtime / wfb-radio-service
  <-> AWUS036ACH over userspace USB
```

The `wfb-link` macOS layer has two backend shapes:

- `UserspaceRadioConfig::from_service_config_path` resolves an existing
  `wfb-radio-service` TOML profile.
- `UserspaceRadioBackend` starts `run_production_runtime_flow` on a
  thread and exposes WFB distributor datagram endpoints. Use this when the
  caller owns WFB-NG codec/session framing.
- `MacosWfbTunnelBackend` supervises the production radio runtime, stock
  `wfb_tx`/`wfb_rx` helpers, and the Rust `wfb-tun-macos` bridge. Use this when
  a product wants a raw IP tunnel endpoint and a single Rust lifecycle handle.

Both backends use `request_stop` with cooperative runtime shutdown and managed
child termination instead of relying on process-wide signal handlers. Both
return normalized `wait_ready`, `health`, and `join` evidence while keeping
backend-specific reports attached for diagnostics.

The tunnel backend is a process supervisor by design. That keeps WFB-NG codec
compatibility intact for the production cutover and lets a later native Rust
codec/helper replacement fit behind the same `LinkBackend` contract.

## Integration Boundary

`wfb-link` is a generic WFB driver boundary. It exposes operator-named streams
on WFB `radio_port`s and local UDP sockets, plus an optional IP tunnel endpoint.
Those stream names are labels for humans and supervisors; the crate should not
encode application meanings such as "video", "control", or product-specific
port conventions. Higher-level systems own those mappings, build a
`LinkEndpoints` value, and hand it to the selected backend.

Use `LinkEndpointsBuilder` when a product needs to construct a multi-stream
shape directly:

```rust
use wfb_link::{LinkEndpointsBuilder, PayloadKind};

let endpoints = LinkEndpointsBuilder::new()
    .rx_stream("s0", 0, "127.0.0.1:5800")
    .rx_stream("s1", 1, "127.0.0.1:5801")
    .tx_stream_with_payload_kind(
        "s2",
        2,
        "127.0.0.1:5802",
        PayloadKind::WfbDistributorDatagram,
    )
    .with_tunnel("10.5.0.1", "10.5.0.2")
    .build()?;
```

The builder validates duplicate stream names, duplicate local UDP sockets, and
duplicate `(direction, radio_port)` pairs, returning typed
`LinkBuilderError` variants so callers can surface clean operator errors.

The service TOML can express the same shape directly. Existing `[wfb]`
single-stream and tunnel-oriented profiles still work; `[[streams]]` becomes
the higher-level form when present and CLI overrides still win.

```toml
[[streams]]
name = "s0"
direction = "rx"
radio_port = 0
local_udp = "127.0.0.1:5800"
payload_kind = "wfb_distributor_datagram"
criticality = "required"

[[streams]]
name = "s1"
direction = "rx"
radio_port = 1
local_udp = "127.0.0.1:5801"
criticality = "required"

[[streams]]
name = "s2"
direction = "tx"
radio_port = 2
local_udp = "127.0.0.1:5802"
payload_kind = "wfb_distributor_datagram"

[tunnel]
local_ip = "10.5.0.1"
peer_ip = "10.5.0.2"
```

When this shape is handed to `UserspaceRadioBackend`, every stream must use
`wfb_distributor_datagram`, and RX streams must be `required`. Raw application
streams require a backend/helper that owns WFB codec supervision; best-effort RX
needs explicit managed degradation semantics before it is safe to expose.

`LinkHealth` and `LinkReport` include `streams[]` keyed by these names. TX
entries expose submitted frames, failed submissions, dropped datagrams, and
last successful submit time when the runtime owns the TX bind. RX entries
expose forwarded frames/bytes and the last RX-forward time. `degraded_streams`
lists best-effort streams that were skipped or degraded during startup.

For the current userspace radio backend, `BestEffort` is implemented for
TX UDP bind preflight: an unavailable best-effort TX socket is removed from the
runtime bind set and reported as degraded instead of failing the whole link.
Required bind failures still abort. RX forward sockets are runtime-owned
ephemeral sockets today, so RX best-effort currently acts as health metadata;
there is no operator-specified RX bind to skip.

See [Product integration](product-integration.md) for backend selection and
[Service config reference](service-config-reference.md) for field-level TOML
rules.

Example:

```rust
use std::time::Duration;
use wfb_link::{
    LinkBackend, LinkConfig, MacosWfbTunnelBackend, MacosWfbTunnelConfig,
};

let config = MacosWfbTunnelConfig::from_service_config_path(
    "configs/radio-run-video-control-tdd.toml",
    "/path/to/gs.key",
)?;
let mut backend = MacosWfbTunnelBackend::default();
let handle = backend.start(LinkConfig::macos_wfb_tunnel(config))?;
let ready = handle.wait_ready(Duration::from_secs(90))?;
let health = handle.health()?;
handle.request_stop()?;
let report = handle.join()?;
```

The checked-in API smoke is:

```sh
WFB_KEY=/path/to/gs.key \
PEER_IP=10.5.0.2 \
scripts/run-wfb-link-tunnel-smoke.sh
```

It runs the product-facing Rust backend, then probes the tunnel with a 256 KiB
SSH download through `10.5.0.2`. `SSH_KEY` is optional when the drone is
reachable through the default SSH config or agent. `wfb-tun-macos` normally
needs `sudo -n` for `utun`; production hosts should pre-authorize that helper or
run an equivalent privileged service. The checked-in smoke defaults to
`CHANNEL=161` because that is the current Arc tunnel peer channel; override it
only when the Linux peer has been moved.

## macOS Privilege And Install Notes

The radio runtime can run as the product user once macOS grants USB ownership,
but `utun` creation and interface/route configuration are privileged. The
current production cutover uses:

```text
product process
  -> sudo -n wfb-tun-macos
```

Operational requirements:

- Build/install `wfb-tun-macos`, `wfb_tx`, `wfb_rx`, and the product binary in
  stable absolute paths.
- Configure passwordless sudo for the exact `wfb-tun-macos` path only, or run
  the product under a launchd job/account that has the required network
  extension privileges.
- Codesign the installed binaries in the release pipeline. Ad-hoc signing is
  acceptable for bench deploys; production should use the product signing
  identity and notarization process.
- Keep the Python tunnel helper out of adopter docs and packages except as a
  development fallback under `scripts/development/`.

Longer term, a privileged helper or network-extension packaging model is
cleaner than `sudo -n`; the `MacosWfbTunnelBackend` contract does not depend on
which privilege mechanism launches the tunnel bridge.

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
It should not use the userspace USB bridge just to share implementation; Linux
already has a mature native WFB path.

## Android Backend

Android is expected to use the portable userspace radio contract rather than a
native Linux monitor-mode path. The missing platform piece is an Android USB
host transport for RTL8812AU control and bulk transfers. Once that transport can
produce the same runtime USB session behavior as macOS IOUSBHost, Android
product code should be able to start a `UserspaceRadioBackend` with Android
runtime USB config and keep the same `LinkEndpoints`, `LinkHealth`, and
`LinkReport` model.

## Why This Boundary

- Product code gets one lifecycle and endpoint contract across platforms.
- macOS can keep the userspace USB radio path that works today.
- Linux can keep the native monitor-mode path that already exists.
- Android can add a platform USB transport without changing the top-level link
  contract.
- WFB-NG compatibility remains intact.
- A future Rust WFB codec can replace helper processes without changing the
  product-facing link control plane.
