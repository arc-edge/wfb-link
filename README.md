# wfb-mac-radio

`wfb-mac-radio` is a native macOS userspace radio backend for WFB-NG traffic on
RTL8812AU USB adapters, currently tested with the ALFA AWUS036ACH.

The project lets a Mac treat the adapter as a USB radio peripheral: initialize
the chip, submit raw 802.11 WFB frames through bulk OUT, receive frames through
bulk IN, and bridge those frames to WFB-NG's existing UDP distributor and
aggregator protocols. It also contains a small Rust facade crate so a product
binary can use one link lifecycle API on macOS and Linux while each platform
uses the right native radio path.

## Current Shape

```text
Product binary
  -> wfb-link
     -> macOS: wfb-radio-runtime + AWUS036ACH userspace USB
     -> Linux: native WFB-NG + wfb0 monitor mode + rtl88xxau
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

## What Works Now

- macOS userspace control of an AWUS036ACH through libusb or direct IOUSBHost.
- RTL8812AU init, firmware download, MAC/BB/RF setup, channel setup, RX bulk
  reads, TX bulk writes, and WFB distributor/aggregator bridging.
- A production service entry point driven by a reviewed TOML config.
- A product-facing Rust facade that can start the macOS production runtime,
  wait for readiness, read health, request cooperative stop, and join for a
  final report.
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
  --config configs/radio-run-robust-short-range.toml \
  --report /tmp/wfb-radio-service.json \
  --ready-file /tmp/wfb-radio-service-ready.json \
  --health-file /tmp/wfb-radio-service-health.json \
  --i-understand-this-transmits
```

The checked-in config is a short-range service profile. It expects an
RTL8812A firmware image at `/tmp/rtl8812aefw.bin`; override `--firmware` or edit
the config when your firmware lives elsewhere.

Run the embedded-link example:

```sh
cargo run -p wfb-link --example embed-radio-service -- \
  configs/radio-run-robust-short-range.toml
```

That example demonstrates the lifecycle API: start, wait-ready, print health,
request stop, and print the final report. It is not a full application data
plane by itself.

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

## Product Integration

For a Rust product binary, depend on `wfb-link` and construct a backend:

```rust
use std::time::Duration;
use wfb_link::{
    LinkBackend, LinkConfig, MacosUserspaceRadioBackend, MacosUserspaceRadioConfig,
};

fn start_link() -> Result<(), Box<dyn std::error::Error>> {
    let radio = MacosUserspaceRadioConfig::from_service_config_path(
        "configs/radio-run-robust-short-range.toml",
    )?;
    let mut backend = MacosUserspaceRadioBackend::default();
    let handle = backend.start(LinkConfig::macos_userspace_radio(radio))?;
    let ready = handle.wait_ready(Duration::from_secs(60))?;
    let health = handle.health()?;
    handle.request_stop()?;
    let report = handle.join()?;
    Ok(())
}
```

The macOS backend currently exposes WFB distributor datagram endpoints. A
product can use those directly if it owns WFB-NG codec/session framing, or it
can supervise stock WFB-NG helper processes around those endpoints. The
higher-level raw application stream/tunnel supervisor is the next production
integration layer.

On Linux, the intended backend is native WFB-NG over `wfb0` monitor mode with
the aircrack/rtl88xxau driver. Do not port the macOS USB bridge to Linux just
to share implementation; share the `wfb-link` lifecycle and endpoint contract.

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
- The current macOS product-facing data endpoint is still
  `WfbDistributorDatagram`, not raw application datagrams.
- Tunnel helpers may need elevated privileges for macOS `utun` creation.
- The old Python `utun` helper is kept only under `scripts/development/` as a
  bring-up fallback; the default tunnel path is the Rust `wfb-tun-macos`
  binary.
- This software can transmit RF. Operators are responsible for local radio
  regulations, antenna setup, channel choice, and bench isolation.

## Documentation

- [Cross-platform link interface](docs/cross-platform-link-interface.md)
- [Runtime boundary](docs/runtime-boundary.md)
- [Tunnel recovery and loaded profile](docs/wfb-ng-tunnel-recovery.md)
- [RF quality and range work](docs/rf-quality-and-range.md)
- [Development and bring-up notes](docs/development/README.md)

## Development

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Equivalent `make` and `just` targets are available: `fmt`, `clippy`, `test`,
`check`, and `verify`.
