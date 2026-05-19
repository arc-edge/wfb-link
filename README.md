# wfb-link

`wfb-link` is a cross-platform WFB link stack and product-facing Rust facade.
Its current direct-radio implementation is a native macOS userspace backend for
WFB-NG traffic on RTL8812AU USB adapters, tested with the ALFA AWUS036ACH.

The project lets a Mac treat the adapter as a USB radio peripheral: initialize
the chip, submit raw 802.11 WFB frames through bulk OUT, receive frames through
bulk IN, and bridge those frames to WFB-NG's existing UDP distributor and
aggregator protocols. It also contains a small Rust facade crate so a product
binary can use one link lifecycle API across macOS, Linux, and future Android
support while each platform uses the right radio path.

## Current Shape

```text
Product binary
  -> wfb-link
     -> macOS: wfb-radio-runtime + AWUS036ACH userspace USB
     -> Linux: native WFB-NG + wfb0 monitor mode + rtl88xxau
     -> Android: wfb-radio-runtime + Android USB host transport contract
```

Main crates:

- `wfb-link`: product-facing `LinkBackend` / `LinkHandle` facade.
- `wfb-radio-service`: production-oriented macOS service binary.
- `wfb-radio-runtime`: runtime library for USB ownership, init, RX/TX, health,
  and reports.
- `wfb-tun`: Rust macOS `utun` bridge for WFB-NG tunnel UDP messages.
- `wfb-radio-diag`: diagnostic and bring-up binary. This is intentionally
  broad; production code should not depend on it.
- `radio-core` and `wfb-bridge`: lower-level USB, RTL8812AU, and WFB datagram
  helpers.

Canonical repository:

```sh
git remote set-url origin git@github.com:arc-edge/wfb-link.git
```

Older clones may still point at the pre-rename `llamadrone/wfb-mac-radio`
remote. Update them before adding Cargo git dependencies or release automation.

## What Works Now

- macOS userspace control of an AWUS036ACH through libusb or direct IOUSBHost.
- RTL8812AU init, firmware download, MAC/BB/RF setup, channel setup, RX bulk
  reads, TX bulk writes, and WFB distributor/aggregator bridging.
- A production service entry point driven by a reviewed TOML config.
- A product-facing Rust facade that can start the macOS production runtime,
  wait for readiness, read health, request cooperative stop, and join for a
  final report. The facade also includes a macOS tunnel supervisor that manages
  the radio runtime, WFB-NG codec helpers, and `wfb-tun-macos`.
- A managed macOS raw-application multi-stream backend that supervises one
  `wfb_tx`/`wfb_rx` helper per stream while exposing product-facing UDP
  endpoints and named per-stream health.
- Android USBHost runtime and service config selection, endpoint validation,
  plus a JNI smoke transport that drives app-owned `UsbDeviceConnection`
  control/bulk transfers directly.
- A local Android SDK AAR path with Java USB handoff/config/session/result
  classes, named stream config, product-facing JNI symbol names, optional
  packaged WFB-NG helper executables, and direct plus Gradle-style consumer
  compile checks. See
  [Android Consumer Quickstart](docs/android-consumer-quickstart.md) for the
  handoff guide used by downstream Android apps.
- A checked-in short-range TDD radio profile for video downlink plus sparse
  control uplink.
- Short-range loaded tunnel validation using `PROFILE_SET=loaded` with a 700 us
  TX pacing default.

## Quick Start

Build the production service and facade example:

```sh
cargo build -p wfb-radio-service -p wfb-link --examples
cargo build -p wfb-tun --bin wfb-tun-macos
```

Run the production macOS radio service:

```sh
cargo run -p wfb-radio-service -- \
  --config configs/radio-run-video-control-tdd.toml \
  --report /tmp/wfb-radio-service.json \
  --ready-file /tmp/wfb-radio-service-ready.json \
  --health-file /tmp/wfb-radio-service-health.json
```

The recommended checked-in config is the short-range video/control TDD profile.
It expects an RTL8812A firmware image at `/tmp/rtl8812aefw.bin`; override
`--firmware` or edit the config when your firmware lives elsewhere.

Run the embedded-link example:

```sh
cargo run -p wfb-link --example embed-radio-service -- \
  configs/radio-run-video-control-tdd.toml
```

That runtime profile uses channel 36 HT20 with TDD airtime windows validated
against Linux WFB peer traffic shaped as L2M `4/12` MCS2 at 5 ms and sparse M2L
`2/16` MCS0 at 100 ms.

That example demonstrates the lifecycle API: start, wait-ready, print health,
request stop, and print the final report. It is not a full application data
plane by itself.

Run the multi-stream distributor example:

```sh
cargo run -p wfb-link --example multi-stream-link -- \
  configs/radio-run-multi-stream-example.toml
```

That profile exposes named WFB distributor/aggregator datagram streams. It is
for products that already own WFB-NG datagrams or supervise helper processes;
raw application UDP streams need a codec/helper layer above the radio backend.

Run the managed raw-application multi-stream example:

```sh
WFB_KEY=/path/to/gs.key \
cargo run -p wfb-link --example managed-streams-link -- \
  configs/radio-run-video-control-tdd.toml
```

That example starts the radio runtime plus WFB-NG helper processes for separate
raw UDP streams such as video downlink, telemetry downlink, and sparse control
uplink. Override `VIDEO_DOWN_UDP`, `TELEMETRY_DOWN_UDP`, `CONTROL_UP_UDP`,
`WFB_TX_BIN`, `WFB_RX_BIN`, and `LINK_ID` as needed for the product.

Run the receiver-backed managed-stream smoke on a prepared Mac plus Linux peer:

```sh
WFB_KEY=/path/to/gs.key \
LINUX_HOST=pi@drone-2f389.local \
scripts/run-wfb-link-managed-streams-smoke.sh
```

That gate verifies raw UDP recovery on three managed streams: Linux-to-Mac
video, Linux-to-Mac telemetry, and Mac-to-Linux control. It writes a
`summary.json` with per-stream payload counters, WFB helper logs, and the final
`ManagedWfbStreamsBackend` report. The default smoke profile is intentionally
conservative for product-adoption checks; use explicit MCS, FEC, interval, and
payload-count overrides for throughput or range profiling.

Run the product-facing radio API smoke on a prepared Mac with an attached
AWUS036ACH:

```sh
scripts/run-wfb-link-radio-smoke.sh
```

The smoke uses `UserspaceRadioBackend`, holds the runtime long enough to
exercise the TDD airtime gate, injects synthetic WFB distributor datagrams into
the exposed TX endpoint, and checks the final TX/RX counters and cooperative
stop report.

Run the product-facing tunnel smoke on a prepared bench:

```sh
WFB_KEY=/path/to/gs.key \
PEER_IP=10.5.0.2 \
scripts/run-wfb-link-tunnel-smoke.sh
```

That path uses `MacosWfbTunnelBackend`, not the legacy shell orchestration, and
probes the resulting `utun` link with a 256 KiB SSH download. Set `SSH_KEY` only
when the drone is not reachable through the default SSH config or agent. The
tunnel smoke preflights `sudo -n` by default because `wfb-tun-macos` usually
needs privilege to create `utun`; set `TUN_USE_SUDO=0` only on hosts that can
open `utun` without sudo. The tunnel smoke defaults to the current Arc tunnel
peer channel, `CHANNEL=161`; set `CHANNEL=36` only when the Linux tunnel peer is
also on the video/control bench channel.

Run the current loaded tunnel gate on a prepared bench:

```sh
PROFILE_SET=loaded \
WFB_KEY=/path/to/gs.key \
SSH_KEY=/path/to/drone_ssh_key \
PEER_IP=10.5.0.2 \
scripts/run-mac-wf-tun-profile-matrix.sh
```

By default, `PROFILE_SET=loaded` uses duplex side traffic, exact 100/100 side
payload acceptance in both directions, and `TX_MIN_INTERVAL_US=700`.

Run the production readiness wrapper locally:

```sh
scripts/run-production-readiness-gate.sh
```

Set `RUN_API_RADIO_SMOKE=1`, `RUN_API_TUNNEL_SMOKE=1`,
`RUN_MANAGED_STREAMS_SMOKE=1`, `RUN_LOADED_TUNNEL_GATE=1`,
`RUN_VIDEO_CONTROL_RADIO_GATE=1`, `RUN_RF_CLOSE_RANGE=1`, or
`RUN_CALIBRATION_REGRESSION=1` to include hardware and RF gates when the bench
is set up. For the managed raw-stream adoption gate, set
`MANAGED_STREAMS_SMOKE_REPEATS=N` to require repeated clean receiver-backed
runs before accepting a build.

Run the receiver-backed video/control radio gate:

```sh
PROFILE_SET=video-control-tdd \
LOCAL_HW=1 \
LINUX_HOST=pi@drone-2f389.local \
MAC_LAN_IP=192.168.122.98 \
LINUX_LAN_IP=192.168.122.95 \
scripts/run-radio-run-profile-matrix.sh
```

`PROFILE_SET=video-control-tdd` selects
`configs/radio-run-video-control-tdd.toml`, requires two clean repeats, and
uses the accepted TDD timing profile.

## Product Integration

For a Rust product binary, depend on `wfb-link` and construct a backend. The
short version for a managed macOS tunnel is:

```rust
use std::time::Duration;
use wfb_link::{
    LinkBackend, LinkConfig, MacosWfbTunnelBackend, MacosWfbTunnelConfig,
};

fn start_link() -> Result<(), Box<dyn std::error::Error>> {
    let link = MacosWfbTunnelConfig::from_service_config_path(
        "configs/radio-run-video-control-tdd.toml",
        "/path/to/gs.key",
    )?;
    let mut backend = MacosWfbTunnelBackend::default();
    let handle = backend.start(LinkConfig::macos_wfb_tunnel(link))?;
    let ready = handle.wait_ready(Duration::from_secs(90))?;
    let health = handle.health()?;
    handle.request_stop()?;
    let report = handle.join()?;
    Ok(())
}
```

Use `UserspaceRadioBackend` instead only when the product owns WFB-NG
codec/session framing and wants direct WFB distributor datagram endpoints.
Use `ManagedWfbStreamsBackend` when the product wants ordinary raw UDP streams
and wants `wfb-link` to supervise one WFB-NG codec helper per stream. It
can also supervise one optional IP tunnel in the same radio session with
`ManagedWfbTunnelConfig`.
The old `MacosUserspaceRadio*` names are deprecated aliases; new integration
code should use `UserspaceRadio*` for the portable direct-radio contract.

On Linux, the intended backend is native WFB-NG over `wfb0` monitor mode with
the aircrack/rtl88xxau driver. Android reuses the userspace radio contract
with an Android USB host transport section. The first Android hardware path uses
JNI calls into an app-owned `UsbDeviceConnection`; the libusb fd-wrap approach
failed on Pixel 7 Pro with `Input/Output Error`. Do not port the userspace USB
bridge to Linux just to share implementation; share the `wfb-link` lifecycle
and endpoint contract.

For the full integration contract, backend selection rules, payload-kind
semantics, and health/report shape, read
[Product integration](docs/product-integration.md).

For Android app integration, build the local AAR with:

```sh
INCLUDE_ANDROID_WFB_HELPERS=1 scripts/build-android-sdk-aar.sh
scripts/build-android-sdk-consumer-smoke.sh
```

See [Android SDK integration](docs/android-sdk.md) for manifest, USB handoff,
asset/key provisioning, threading, and current Android limitations.

For the first alpha integration from another Rust repository:

```toml
wfb-link = { git = "https://github.com/arc-edge/wfb-link.git", tag = "v0.1.0-alpha.4" }
```

`v0.1.0-alpha.4` includes the combined managed raw application stream plus
optional tunnel backend path, along with the alpha.3 managed-stream smoke gate
and best-effort helper degradation semantics.

## Current Limitations

- Hardware scope is RTL8812AU/AWUS036ACH class adapters.
- macOS 26 can require the IOUSBHost path because libusb enumeration is not
  reliable there.
- The accepted tunnel profile is short-range validation, not long-distance RF
  acceptance.
- RF calibration is not yet full Linux parity across all conditions. Runtime
  LCK/IQK/EFUSE TX-power work exists, but production profiles still need more
  receiver-backed validation.
- The `wfb-link` Linux backend is a contract/design stub, not an implemented
  native Linux supervisor.
- The Android USBHost smoke harness can obtain permission, read registers, run
  production init, apply monitor opmode, parse RX frames, submit TX frames, and
  run an intent-gated managed raw-application stream smoke with packaged
  Android `wfb_tx`/`wfb_rx` helpers. Short-range Pixel 7 Pro validation has
  passed against the Linux peer. Android still needs NDK CI and product app
  packaging before it should be treated as release-grade.
- `ManagedWfbStreamsBackend` is the first managed raw-application multi-stream
  path. It can now include one optional managed tunnel. Required helper exits
  fail startup; best-effort helper exits degrade only the named stream, or
  `__tunnel` for a best-effort tunnel. Receiver-backed adoption gates are
  available for the current macOS plus Linux-peer bench; the combined stream
  plus tunnel gate still needs to be added.
- `UserspaceRadioBackend` accepts WFB distributor/aggregator datagrams only.
- Tunnel helpers may need elevated privileges for macOS `utun` creation.
- The old Python `utun` helper is kept only under `scripts/development/` as a
  bring-up fallback; the default tunnel path is the Rust `wfb-tun-macos`
  binary.
- This software can transmit RF. Operators are responsible for local radio
  regulations, antenna setup, channel choice, and bench isolation.

## Documentation

- [Cross-platform link interface](docs/cross-platform-link-interface.md)
- [Product integration](docs/product-integration.md)
- [Service config reference](docs/service-config-reference.md)
- [Android USBHost backend](docs/android-usbhost.md)
- [Runtime boundary](docs/runtime-boundary.md)
- [Tunnel recovery and loaded profile](docs/wfb-ng-tunnel-recovery.md)
- [RF quality and range work](docs/rf-quality-and-range.md)
- [Long-range tailnet field test](docs/long-range-tailnet-field-test.md)
- [Development and bring-up notes](docs/development/README.md)

## Development

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Equivalent `make` and `just` targets are available: `fmt`, `clippy`, `test`,
`check`, and `verify`.
