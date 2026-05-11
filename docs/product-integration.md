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
     -> Android: userspace AWUS036ACH runtime with Android USB host transport
```

## Current Backend Choices

| Need | Backend | Status |
| --- | --- | --- |
| Managed macOS IP tunnel | `MacosWfbTunnelBackend` | Product-facing and smoke-tested for tunnel-only runs. Supervises radio runtime, WFB-NG helpers, and `wfb-tun-macos`. |
| Managed raw application multi-streams, optionally with an IP tunnel | `ManagedWfbStreamsBackend` | Product-facing implementation. Supervises one WFB-NG helper per configured stream, can also supervise one `wfb-tun-macos` tunnel on separate radio ports, and exposes named raw UDP endpoints with health. |
| Userspace WFB distributor datagram streams | `UserspaceRadioBackend` | Product-facing for callers that already speak WFB-NG distributor/aggregator UDP. Current checked-in live transport is macOS IOUSBHost; Android uses the same contract with Android USBHost direct JNI control/bulk transfers. |
| Linux | Native WFB-NG backend implementing the same trait | Design contract only in this repo today. Do not port the userspace USB bridge to Linux. |
| Android | Android SDK AAR over Android USBHost transport | Rust direct-JNI control/bulk bridge implemented; local AAR packages Java USB handoff/config/result classes, `libwfb_android.so`, and optional WFB-NG helper executables. The smoke harness has passed register, init, RX descriptor, Android-to-Linux TX, Linux-to-Android RX frame validation, and managed-stream short-range validation when the adapter is enumerated. |

The important distinction is `payload_kind`:

- `WfbDistributorDatagram`: local UDP carries WFB-NG distributor or aggregator datagrams. The product or helper process owns WFB framing, encryption, FEC, and radio-port selection.
- `RawApplicationDatagram`: local UDP carries product payload bytes. A backend or helper must supervise WFB-NG `wfb_tx`/`wfb_rx` or provide an equivalent codec layer.

Today, `UserspaceRadioBackend` consumes and emits `WfbDistributorDatagram`.
It rejects `RawApplicationDatagram` endpoints before startup because it does
not supervise WFB codec processes itself.
`ManagedWfbStreamsBackend` exposes `RawApplicationDatagram` streams by starting
WFB-NG `wfb_tx`/`wfb_rx` helpers around the direct radio runtime.
When configured with `ManagedWfbTunnelConfig`, it also starts a tunnel
`wfb_tx`/`wfb_rx` pair and `wfb-tun-macos` in the same radio session.
`MacosWfbTunnelBackend` exposes raw IP tunnel endpoints because it starts the
WFB-NG helper processes and `wfb-tun-macos` bridge for that specific tunnel use.

## Cargo Dependency

Inside this workspace:

```toml
[dependencies]
wfb-link = { path = "crates/wfb-link" }
```

From another repo, use the final Git location and revision policy chosen for the
product. For the first alpha integration, pin the Git tag:

```toml
[dependencies]
wfb-link = { git = "https://github.com/arc-edge/wfb-link.git", tag = "v0.1.0-alpha.4" }
```

For fully reproducible product releases, Cargo.lock will record the resolved
commit. A product can also pin the exact release commit explicitly with `rev`
after the tag has been validated in its own CI. Keep the service TOML and helper
binaries under product release management; do not depend on `wfb-radio-diag`.

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

## Managed Raw Application Streams

Use this path when the product wants named raw UDP streams, such as video
downlink, telemetry downlink, and sparse control uplink, and does not want to
spawn WFB-NG helpers itself. The service TOML is still the radio/runtime base;
the managed raw streams are configured through the Rust builder so product code
can own product port assignments explicitly.

```rust
use std::{net::SocketAddr, time::Duration};
use wfb_link::{
    LinkBackend, LinkConfig, ManagedWfbStreamConfig, ManagedWfbStreamsBackend,
    ManagedWfbStreamsConfig, ManagedWfbTxProfile, UserspaceRadioConfig,
};

fn run_streams() -> Result<(), Box<dyn std::error::Error>> {
    let radio = UserspaceRadioConfig::from_service_config_path(
        "configs/radio-run-video-control-tdd.toml",
    )?;
    let config = ManagedWfbStreamsConfig::from_radio_config(radio, "/path/to/gs.key")
        .with_stream(
            ManagedWfbStreamConfig::rx(
                "video-down",
                4,
                "127.0.0.1:5804".parse::<SocketAddr>()?,
            )
            .with_link_id(1),
        )
        .with_stream(
            ManagedWfbStreamConfig::rx(
                "telemetry-down",
                5,
                "127.0.0.1:5805".parse::<SocketAddr>()?,
            )
            .with_link_id(1),
        )
        .with_stream(
            ManagedWfbStreamConfig::tx(
                "control-up",
                6,
                "127.0.0.1:5606".parse::<SocketAddr>()?,
            )
            .with_link_id(1)
            .with_tx_profile(ManagedWfbTxProfile {
                bandwidth_mhz: 20,
                mcs: 0,
                fec_k: 2,
                fec_n: 16,
            }),
        );

    let mut backend = ManagedWfbStreamsBackend::default();
    let handle = backend.start(LinkConfig::managed_wfb_streams(config))?;
    handle.wait_ready(Duration::from_secs(90))?;

    for stream in &handle.endpoints().streams {
        println!("{} {:?} raw UDP at {}", stream.name, stream.direction, stream.local_udp);
    }

    handle.request_stop()?;
    let _report = handle.join()?;
    Ok(())
}
```

For RX, `wfb-link` starts `wfb_rx`, listens for matching WFB aggregator
datagrams from the radio runtime, decrypts/decodes them through WFB-NG, and
forwards raw payload bytes to the stream's `app_udp`. For TX, `wfb-link` starts
`wfb_tx`, listens for raw payload bytes on `app_udp`, encodes them with the
stream's FEC/MCS profile, and sends WFB distributor datagrams into the internal
radio endpoint.

### Adding an IP tunnel to managed streams

Use `ManagedWfbTunnelConfig` when the same backend should also expose an IP
tunnel for SSH or control-plane protocols. The tunnel consumes two additional
WFB radio ports and two local UDP sockets. Port collisions with managed streams
are rejected before startup.

```rust
use std::net::SocketAddr;
use wfb_link::{
    ManagedWfbStreamConfig, ManagedWfbStreamsConfig, ManagedWfbTunnelConfig,
    ManagedWfbTxProfile,
};

let config = ManagedWfbStreamsConfig::from_radio_config(radio, "/path/to/gs.key")
    .with_stream(
        ManagedWfbStreamConfig::rx(
            "video-down",
            4,
            "127.0.0.1:5804".parse::<SocketAddr>()?,
        )
        .with_link_id(1),
    )
    .with_stream(
        ManagedWfbStreamConfig::tx(
            "control-up",
            6,
            "127.0.0.1:5606".parse::<SocketAddr>()?,
        )
        .with_link_id(1)
        .with_tx_profile(ManagedWfbTxProfile {
            bandwidth_mhz: 20,
            mcs: 0,
            fec_k: 2,
            fec_n: 16,
        }),
    )
    .with_tunnel(
        ManagedWfbTunnelConfig::try_new("10.5.0.1", "10.5.0.2")?
            .with_link_id(1)
            .with_radio_ports(8, 7) // TX/out, then RX/in
            .with_mtu(1400)
            .with_aggregation_timeout_ms(5)
            .with_tun_bin("/usr/local/bin/wfb-tun-macos"),
    );
```

`LinkReady.endpoints.tunnel` is populated when the backend starts. The backend
report includes a `tunnel` section with the tunnel radio ports, internal UDP
mapping, `wfb-tun-macos` summary path, and any parsed tunnel summary. Tunnel
criticality defaults to `Required`; a `BestEffort` tunnel startup/helper
failure keeps required streams running and adds `__tunnel` to
`degraded_streams`.

Current managed-stream limitations:

- Required managed helper exits fail readiness. Best-effort managed helper exits
  degrade only the named stream and appear in `degraded_streams`.
- A combined managed-stream plus tunnel bench gate still needs to be added.
  Until then, validate each product profile against a receiver before field use.
- Helper binaries and the `gs.key` are product release artifacts.
- The API owns stream supervision, not stream semantics. Product code still
  decides which UDP port is video, telemetry, or control.
- Receiver-backed adoption gates should be run before depending on a new
  stream profile in production.

The current receiver-backed adoption gate is:

```sh
WFB_KEY=/path/to/gs.key \
LINUX_HOST=pi@drone-2f389.local \
scripts/run-wfb-link-managed-streams-smoke.sh
```

It starts `ManagedWfbStreamsBackend` locally, configures the Linux peer on
`wfb0`, supervises matching Linux WFB-NG helpers, sends marked raw UDP payloads
on video/telemetry/control streams, and writes `summary.json` under `OUT_DIR`.
Use `PREPARE_LINUX_PEER=0` only when the Linux peer is already in monitor mode
on the configured channel.

## Userspace Distributor Streams

Use this path when the product already owns WFB-NG datagrams or supervises its
own helper processes. On macOS, the service TOML selects `[macos_usbhost]`;
Android selects `[android_usbhost]` without changing this top-level link code.
The Android app layer obtains USB permission, opens and claims
`UsbDeviceConnection`, resolves the selected endpoint objects, and keeps those
Java objects alive while Rust uses direct JNI control/bulk transfers.

```rust
use std::time::Duration;
use wfb_link::{
    LinkBackend, LinkConfig, LinkDirection, UserspaceRadioBackend,
    UserspaceRadioConfig, PayloadKind,
};

fn run_radio() -> Result<(), Box<dyn std::error::Error>> {
    let radio = UserspaceRadioConfig::from_service_config_path(
        "configs/radio-run-multi-stream-example.toml",
    )?;

    let mut backend = UserspaceRadioBackend::default();
    let handle = backend.start(LinkConfig::userspace_radio(radio))?;
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

## Android App Integration

Android apps should consume the local SDK AAR rather than depend directly on the
smoke harness package. Build and compile-smoke the artifact with:

```sh
INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-sdk-aar.sh
scripts/build-android-sdk-consumer-smoke.sh
```

The app owns USB permission, opens and claims the device, resolves endpoint
objects, provisions `gs.key` plus firmware/table assets, and runs
`WfbLinkManager.runManagedStreamsBlocking` on a worker thread or foreground
service. The SDK returns `WfbManagedStreamsResult` JSON-derived counters and
typed `WfbLinkException` codes for Java-side validation failures. Native
runtime failures return structured result codes instead of smoke integer return
values.

See [Android SDK integration](android-sdk.md) for the exact Java API and
packaging limitations.

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

Current userspace radio behavior:

- Required TX bind failures abort startup.
- Best-effort TX bind failures are preflighted, skipped, and reported degraded.
- Best-effort RX streams are rejected before startup with
  `userspace_radio_rx_best_effort_unsupported`. UDP forward-target reachability
  is not reliably knowable at startup, so pretending RX had TX-like degradation
  semantics is too easy to misuse.
- `ManagedWfbStreamsBackend` owns per-stream WFB helper processes, so
  best-effort managed stream failures are attributed to the affected stream and
  reported as degraded instead of failing the whole link.

## Platform Selection Shape

The product should target the same trait on every platform:

```rust
let mut backend: Box<dyn LinkBackend> = if cfg!(target_os = "macos") {
    Box::new(MacosWfbTunnelBackend::default())
} else if cfg!(target_os = "android") {
    Box::new(UserspaceRadioBackend::default())
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

For raw application multi-stream adoption, also run:

```sh
WFB_KEY=/path/to/gs.key \
LINUX_HOST=pi@drone-2f389.local \
scripts/run-wfb-link-managed-streams-smoke.sh
```
