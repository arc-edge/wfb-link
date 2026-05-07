# Product Integration Guide

This guide is for a Rust product binary that wants to own link lifecycle without
depending on diagnostic commands or RF bring-up internals.

Use `wfb-link` as the product boundary. Product code starts a backend, waits for
readiness, discovers local UDP endpoints, monitors health, requests stop, and
collects a final report. Platform-specific radio work stays behind the backend.

```text
Product code
  -> wfb_link::LinkBackend
     -> macOS: userspace AWUS036ACH runtime from this repo
     -> Linux: native WFB-NG/wfb0 supervisor, using the same trait contract
```

## Current Backend Choices

| Need | Backend | Status |
| --- | --- | --- |
| Managed macOS IP tunnel | `MacosWfbTunnelBackend` | Product-facing and smoke-tested. Supervises radio runtime, WFB-NG helpers, and `wfb-tun-macos`. |
| macOS WFB distributor datagram streams | `MacosUserspaceRadioBackend` | Product-facing for callers that already speak WFB-NG distributor/aggregator UDP. |
| Generic raw application multi-streams | Product/helper layer above `MacosUserspaceRadioBackend` | Not built into `wfb-link` yet. Convert raw app UDP to WFB distributor datagrams before handing it to the radio backend. |
| Linux | Native WFB-NG backend implementing the same trait | Design contract only in this repo today. Do not port the macOS USB bridge to Linux. |

The important distinction is `payload_kind`:

- `WfbDistributorDatagram`: local UDP carries WFB-NG distributor or aggregator datagrams. The product or helper process owns WFB framing, encryption, FEC, and radio-port selection.
- `RawApplicationDatagram`: local UDP carries product payload bytes. A backend or helper must supervise WFB-NG `wfb_tx`/`wfb_rx` or provide an equivalent codec layer.

Today, `MacosUserspaceRadioBackend` consumes and emits `WfbDistributorDatagram`.
`MacosWfbTunnelBackend` exposes raw IP tunnel endpoints because it starts the
WFB-NG helper processes and `wfb-tun-macos` bridge for that specific tunnel use.

## Cargo Dependency

Inside this workspace:

```toml
[dependencies]
wfb-link = { path = "crates/wfb-link" }
```

From another repo, use the final Git location and revision policy chosen for the
product. Keep the service TOML and helper binaries under product release
management; do not depend on `wfb-radio-diag`.

## Managed macOS Tunnel

Use this path when the product wants an IP tunnel and does not want to manage
WFB-NG helper processes itself.

```rust
use std::time::Duration;
use wfb_link::{
    LinkBackend, LinkConfig, MacosWfbTunnelBackend, MacosWfbTunnelConfig,
};

fn run_link() -> Result<(), Box<dyn std::error::Error>> {
    let config = MacosWfbTunnelConfig::from_service_config_path(
        "configs/radio-run-video-control-tdd.toml",
        "/path/to/gs.key",
    )?
    .with_artifact_dir("/tmp/wfb-link-artifacts");

    let mut backend = MacosWfbTunnelBackend::default();
    let handle = backend.start(LinkConfig::macos_wfb_tunnel(config))?;

    let ready = handle.wait_ready(Duration::from_secs(90))?;
    println!("ready endpoints: {:?}", ready.endpoints);

    let health = handle.health()?;
    println!("link health: {:?}", health.lifecycle);

    handle.request_stop()?;
    let report = handle.join()?;
    println!("final lifecycle: {:?}", report.lifecycle);
    Ok(())
}
```

Operational requirements:

- `wfb_tx` and `wfb_rx` from WFB-NG must be installed at the configured paths.
- `wfb-tun-macos` must be installed and normally needs passwordless `sudo -n`
  or an equivalent privileged launch model for `utun`.
- The `gs.key` must match the Linux peer.
- The Linux peer must be on the same channel, bandwidth, link ID, and tunnel
  radio ports.

## macOS Distributor Streams

Use this path when the product already owns WFB-NG datagrams or supervises its
own helper processes.

```rust
use std::time::Duration;
use wfb_link::{
    LinkBackend, LinkConfig, LinkDirection, MacosUserspaceRadioBackend,
    MacosUserspaceRadioConfig, PayloadKind,
};

fn run_radio() -> Result<(), Box<dyn std::error::Error>> {
    let radio = MacosUserspaceRadioConfig::from_service_config_path(
        "configs/radio-run-multi-stream-example.toml",
    )?;

    let mut backend = MacosUserspaceRadioBackend::default();
    let handle = backend.start(LinkConfig::macos_userspace_radio(radio))?;
    handle.wait_ready(Duration::from_secs(90))?;

    for stream in &handle.endpoints().streams {
        if stream.direction == LinkDirection::Tx
            && stream.payload_kind == PayloadKind::WfbDistributorDatagram
        {
            println!("send WFB distributor datagrams to {}", stream.local_udp);
        }
    }

    handle.request_stop()?;
    let _report = handle.join()?;
    Ok(())
}
```

For TX, the datagram sent to `stream.local_udp` must already be a WFB-NG
distributor datagram. For RX, the datagram received from `stream.local_udp` is a
WFB-NG aggregator datagram. Raw application bytes will not work on this backend
unless your product adds the WFB codec/helper layer.

## Endpoints

`wait_ready()` and `handle.endpoints()` return the normalized endpoint model:

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
```

Treat `name` as an operator label. Product-specific meaning belongs in product
configuration, not inside `wfb-link`. The `stream` field is WFB metadata:
`link_id` plus `radio_port`. For distributor datagram TX sockets, the actual
radio port is still encoded by the WFB datagram itself; `stream` is used for
health/report attribution.

## Health And Reports

`LinkHealth` is for supervisors while the link is running. `LinkReport` is the
final evidence bundle after `join()`.

Both expose aggregate counters and named per-stream counters:

```json
{
  "lifecycle": "ready",
  "ready": true,
  "degraded_streams": ["downlink-aux"],
  "streams": [
    {
      "name": "uplink-control",
      "direction": "tx",
      "local_udp": "0.0.0.0:5606",
      "payload_kind": "wfb_distributor_datagram",
      "criticality": "required",
      "stream": { "link_id": 1, "radio_port": 6 },
      "degraded": false,
      "tx": {
        "submitted_frames": 120,
        "failed_submissions": 0,
        "dropped_datagrams": 0,
        "last_submit_unix_ms": 1778179200123
      }
    },
    {
      "name": "downlink-primary",
      "direction": "rx",
      "local_udp": "127.0.0.1:5804",
      "payload_kind": "wfb_distributor_datagram",
      "criticality": "required",
      "stream": { "link_id": 1, "radio_port": 4 },
      "degraded": false,
      "rx": {
        "forwarded_frames": 118,
        "forwarded_bytes": 196824,
        "last_rx_unix_ms": 1778179200456
      }
    }
  ]
}
```

Counters are best-effort observability, not a delivery guarantee. End-to-end
application acceptance should still be measured at the receiver.

## Criticality

`StreamCriticality::Required` means start-time failure should fail the link.
`StreamCriticality::BestEffort` means the link should start if possible and
surface the stream in `degraded_streams`.

Current macOS behavior:

- Required TX bind failures abort startup.
- Best-effort TX bind failures are preflighted, skipped, and reported degraded.
- RX forwarding uses runtime-owned ephemeral sockets; RX best-effort is
  currently health metadata rather than a separate bind-skip path.

## Linux Shape

The product should target the same trait on Linux:

```rust
let mut backend: Box<dyn LinkBackend> = if cfg!(target_os = "macos") {
    Box::new(MacosWfbTunnelBackend::default())
} else {
    Box::new(ProductLinuxWfbBackend::new())
};
```

The Linux backend should use native WFB-NG tools and the monitor-mode `wfb0`
interface with the Linux rtl88xxau/aircrack driver. It should normalize its
local UDP endpoints, health, and final report into the same `wfb-link` model.

## Validation Before Adoption

Minimum checks before wiring this into a product binary:

```sh
cargo build -p wfb-link --examples
cargo test --workspace
scripts/run-wfb-link-radio-smoke.sh
```

On a tunnel bench, also run:

```sh
WFB_KEY=/path/to/gs.key \
PEER_IP=10.5.0.2 \
scripts/run-wfb-link-tunnel-smoke.sh
```
