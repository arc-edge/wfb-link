# Service Config Reference

`wfb-radio-service` reads one TOML file and resolves it into a
`ProductionRuntimeFlowConfig`. `wfb-link` also reads the resolved stream and
tunnel metadata from the same file when using
`UserspaceRadioConfig::from_service_config_path`. Managed raw application
streams use this file as the radio/runtime base and define product-facing raw
UDP streams through `ManagedWfbStreamsConfig`.

The stream, WFB, tunnel, artifact, and calibration sections are platform-neutral
contracts. `[macos_usbhost]` is intentionally macOS-only transport config; a
future Android transport should add its own transport section while preserving
the resolved `UserspaceRadioConfig` and `LinkEndpoints` shape.

The checked-in examples are:

- `configs/radio-run-video-control-tdd.toml`: current short-range video/control
  TDD profile using the legacy single-stream `[wfb]` shape.
- `configs/radio-run-multi-stream-example.toml`: named multi-stream profile for
  products that own WFB-NG distributor datagrams.

## Precedence

Resolution order is intentionally conservative:

1. CLI flags win.
2. `[[streams]]` drives TX binds and RX forwards when present and when no
   conflicting CLI override was supplied.
3. Legacy `[wfb]` fields remain the fallback and keep existing profiles valid.
4. Built-in defaults fill only fields that are safe to default.

If any of `--wfb-link-id`, `--wfb-radio-port`, `--rx-aggregator`, or
`--rx-forward` is supplied, RX stream resolution falls back to the explicit CLI
path for that run.

## Minimum macOS Runtime Sections

A production macOS run normally needs:

```toml
[adapter]
vid = 3034
pid = 34834

[macos_usbhost]
enabled = true

[radio]
channel = 36
bandwidth_mhz = 20
firmware = "/tmp/rtl8812aefw.bin"
```

`firmware` is resolved before USB init. The default examples assume
`/tmp/rtl8812aefw.bin`; product packaging should install the firmware at a
stable path or override the field.

`[authorization].transmit` is no longer required. `[authorization]` is retained
only for `live_register_writes = true`, which is still required for runtime
calibration profiles that write BB/RF registers.

## Stream Schema

Use `[[streams]]` for operator-named local UDP endpoints.

```toml
[[streams]]
name = "downlink-primary"
direction = "rx"
radio_port = 4
local_udp = "127.0.0.1:5804"
link_id = 1
payload_kind = "wfb_distributor_datagram"
criticality = "required"
```

Fields:

| Field | Required | Meaning |
| --- | --- | --- |
| `name` | yes | Operator label surfaced in `LinkEndpoints`, `LinkHealth`, and `LinkReport`. Must be unique. |
| `direction` | yes | `"rx"` or `"tx"`. |
| `radio_port` | yes | WFB radio port used for RX filtering and health attribution. |
| `local_udp` | yes | Local UDP socket. TX streams bind this address. RX streams forward matching WFB payloads to this address. |
| `link_id` | no | WFB link ID for this stream. Defaults to `[wfb].link_id` when present. |
| `payload_kind` | no | `"raw_application_datagram"` or `"wfb_distributor_datagram"`. Service `[[streams]]` default to `"wfb_distributor_datagram"` because `UserspaceRadioBackend` is the direct-radio stream backend. Use `"raw_application_datagram"` only with a managed backend/helper layer that supervises WFB codec processes. |
| `criticality` | no | `"required"` or `"best_effort"`. Defaults to `"required"`. |

Validation:

- Duplicate stream names are rejected by the `wfb-link` endpoint builder.
- Duplicate local UDP sockets are rejected by the `wfb-link` endpoint builder.
- Duplicate `(direction, radio_port)` pairs are rejected by the `wfb-link`
  endpoint builder.
- Empty stream names are rejected during service config resolution.

## Runtime Mapping

TX streams resolve into the runtime bind set:

- First TX stream becomes `bind_addr`.
- Additional TX streams become `tx_binds`.
- If CLI `--bind` or `--tx-bind` is supplied, CLI bind values win.
- If no TX stream exists, legacy `[wfb].bind` and `[wfb].tx_binds` are used.

RX streams resolve into runtime forwarding:

- First RX stream becomes `primary_rx_forward`.
- Additional RX streams become `rx_forwards`.
- Per-stream `link_id` overrides `[wfb].link_id`; otherwise `[wfb].link_id` is
  used.
- If no RX stream exists, legacy `[wfb].radio_port`, `[wfb].rx_aggregator`, and
  `[wfb].rx_forwards` are used.

Example:

```toml
[wfb]
link_id = 1
rx_wlan_idx = 0
rx_mcs_index = 2

[[streams]]
name = "downlink-primary"
direction = "rx"
radio_port = 4
local_udp = "127.0.0.1:5804"
payload_kind = "wfb_distributor_datagram"

[[streams]]
name = "downlink-aux"
direction = "rx"
radio_port = 5
local_udp = "127.0.0.1:5805"
payload_kind = "wfb_distributor_datagram"
criticality = "required"

[[streams]]
name = "uplink-control"
direction = "tx"
radio_port = 6
local_udp = "0.0.0.0:5606"
payload_kind = "wfb_distributor_datagram"
```

Resolved runtime shape:

```text
bind_addr = 0.0.0.0:5606
primary_rx_forward = link_id 1, radio_port 4, aggregator 127.0.0.1:5804
rx_forwards = [link_id 1, radio_port 5, aggregator 127.0.0.1:5805]
```

## Tunnel Section

`[tunnel]` describes the product-facing IP tunnel endpoint:

```toml
[tunnel]
local_ip = "10.5.0.1"
peer_ip = "10.5.0.2"
interface_name = "utun-wfb"
```

Fields:

| Field | Required | Meaning |
| --- | --- | --- |
| `local_ip` | yes | Local tunnel IP advertised in `LinkEndpoints`. |
| `peer_ip` | yes | Peer tunnel IP advertised in `LinkEndpoints`. |
| `interface_name` | no | Operator-facing name metadata. macOS `utun` allocation may still choose the concrete interface name. |

Important backend behavior:

- `UserspaceRadioBackend` preserves `[tunnel]` as endpoint metadata but
  does not start `wfb_tx`, `wfb_rx`, or `wfb-tun-macos`.
- `MacosWfbTunnelBackend` is the managed IP tunnel path. It starts the helper
  processes for the tunnel use case.
- Combining managed tunnel traffic with extra side streams is possible only
  when the side streams are represented as distributor datagrams and the tunnel
  radio port remains the primary tunnel path. Treat that as an advanced profile
  until it has its own receiver-backed gate.

## Legacy WFB Section

Legacy single-stream configs remain valid:

```toml
[wfb]
bind = "0.0.0.0:5611"
tx_binds = ["0.0.0.0:5612"]
link_id = 1
radio_port = 1
rx_aggregator = "127.0.0.1:5801"
rx_forwards = ["1:5=127.0.0.1:5805"]
rx_wlan_idx = 0
rx_mcs_index = 2
```

Use `[wfb]` for existing profiles and simple tunnel runs. Use `[[streams]]`
when a product wants stable stream names and per-stream health/report counters.

## Payload Kind Rules

Use this decision table:

| Product sends/receives | Config value | Backend |
| --- | --- | --- |
| Raw IP tunnel packets | `raw_application_datagram` endpoint exposed by `MacosWfbTunnelBackend` | Managed tunnel backend |
| WFB-NG distributor datagrams | `wfb_distributor_datagram` | `UserspaceRadioBackend` |
| Raw app UDP for arbitrary streams | `raw_application_datagram` endpoints exposed by `ManagedWfbStreamsBackend` | Managed stream backend |

If a product sends raw payload bytes to a `WfbDistributorDatagram` endpoint, the
runtime will treat the bytes as malformed WFB distributor input and drop them.
`UserspaceRadioBackend` rejects `raw_application_datagram` streams before start
so this mistake fails during integration instead of becoming a silent data-plane
failure.

`ManagedWfbStreamsBackend` does not currently populate streams from service
`[[streams]]`; product code constructs `ManagedWfbStreamConfig` entries in Rust.
That keeps application port ownership in the product while allowing the same
service TOML to remain the radio/channel/calibration base.

## Criticality

`required` streams should make the link fail if they cannot start.

`best_effort` streams should be exposed in health as degraded when they cannot
start. Current macOS behavior skips unavailable best-effort TX binds during
startup and reports the stream name in `degraded_streams`. RX forwards use
runtime-owned ephemeral send sockets, and UDP forward-target reachability is not
reliably knowable at startup. For that reason `UserspaceRadioBackend` currently
rejects best-effort RX streams with
`userspace_radio_rx_best_effort_unsupported`; model RX streams as `required`
until managed RX degradation semantics exist.

`ManagedWfbStreamsBackend` currently rejects all best-effort streams with
`managed_wfb_best_effort_unsupported` because helper child-process degradation
needs explicit stream-level semantics before production use.

## Artifacts

The service can write ready and health files:

```toml
[artifacts]
ready_file = "/tmp/wfb-radio-ready.json"
health_file = "/tmp/wfb-radio-health.json"
```

`wfb-link` reads these files behind `wait_ready()` and `health()`. Products
normally should use the Rust handle methods rather than reading the files
directly.

## Runnable Checks

Validate config parsing and examples without hardware:

```sh
cargo test -p wfb-radio-service
cargo build -p wfb-link --examples
```

Run hardware-backed radio smoke when an AWUS036ACH and peer are available:

```sh
scripts/run-wfb-link-radio-smoke.sh
```

Run the managed raw-application stream gate when validating product-facing raw
UDP streams:

```sh
WFB_KEY=/path/to/gs.key \
LINUX_HOST=pi@drone-2f389.local \
scripts/run-wfb-link-managed-streams-smoke.sh
```
