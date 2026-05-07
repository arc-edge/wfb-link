//! Product-facing WFB link facade.
//!
//! This crate keeps the product boundary at link lifecycle and local
//! stream/tunnel endpoints. Platform backends own the radio-specific path:
//! macOS embeds this repository's userspace RTL8812AU runtime, while Linux is
//! expected to use the native monitor-mode WFB stack.

use std::{
    collections::HashSet,
    fs::{self, File},
    net::{IpAddr, SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use wfb_radio_runtime::{
    run_production_runtime_flow, ProductionRuntimeFlowConfig, ProductionRuntimeFlowExecutionInputs,
    ProductionRuntimeFlowReport, ProductionRuntimeFlowResult, RuntimeRadioError,
};
use wfb_radio_service::{
    resolve_service_run, service_runtime_config_from_resolved,
    service_runtime_inputs_from_resolved, ResolvedServiceRun, ResolvedServiceStream, ServiceCli,
    ServiceStreamCriticality, ServiceStreamDirection, ServiceStreamPayloadKind,
};

static NEXT_ARTIFACT_ID: AtomicU64 = AtomicU64::new(0);

pub type Result<T> = std::result::Result<T, LinkError>;

#[derive(Debug, Error)]
pub enum LinkError {
    #[error("{code}: {message}")]
    Runtime { code: &'static str, message: String },
    #[error("invalid link endpoints: {0}")]
    InvalidEndpoints(#[from] LinkBuilderError),
    #[error("unsupported backend config: {0}")]
    UnsupportedBackend(&'static str),
    #[error("backend exited before ready")]
    BackendExitedBeforeReady,
    #[error("timeout waiting for ready marker after {0:?}")]
    ReadyTimeout(Duration),
    #[error("failed to join backend thread")]
    JoinFailed,
    #[error("missing {label}: {path}")]
    MissingPath { label: &'static str, path: PathBuf },
    #[error("failed to spawn {label}: {source}")]
    Spawn {
        label: &'static str,
        source: std::io::Error,
    },
    #[error("{label} exited before ready with status {status}")]
    ProcessExitedBeforeReady { label: String, status: String },
    #[error("child process lock poisoned")]
    ChildLockPoisoned,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl From<RuntimeRadioError> for LinkError {
    fn from(error: RuntimeRadioError) -> Self {
        Self::Runtime {
            code: error.code,
            message: error.message,
        }
    }
}

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

#[derive(Debug, Clone)]
pub struct LinkConfig {
    pub backend: LinkBackendConfig,
}

impl LinkConfig {
    pub fn userspace_radio(config: UserspaceRadioConfig) -> Self {
        Self {
            backend: LinkBackendConfig::UserspaceRadio(config),
        }
    }

    #[deprecated(note = "use LinkConfig::userspace_radio")]
    pub fn macos_userspace_radio(config: UserspaceRadioConfig) -> Self {
        Self::userspace_radio(config)
    }

    pub fn macos_wfb_tunnel(config: MacosWfbTunnelConfig) -> Self {
        Self {
            backend: LinkBackendConfig::MacosWfbTunnel(config),
        }
    }

    pub fn linux_native_wfb(config: LinuxNativeWfbConfig) -> Self {
        Self {
            backend: LinkBackendConfig::LinuxNativeWfb(config),
        }
    }
}

#[derive(Debug, Clone)]
pub enum LinkBackendConfig {
    UserspaceRadio(UserspaceRadioConfig),
    #[deprecated(note = "use LinkBackendConfig::UserspaceRadio")]
    MacosUserspaceRadio(UserspaceRadioConfig),
    MacosWfbTunnel(MacosWfbTunnelConfig),
    LinuxNativeWfb(LinuxNativeWfbConfig),
}

#[derive(Debug, Clone)]
pub struct UserspaceRadioConfig {
    pub runtime_config: ProductionRuntimeFlowConfig,
    pub execution_inputs: ProductionRuntimeFlowExecutionInputs,
    pub endpoints: LinkEndpoints,
    pub ready_poll_interval: Duration,
}

impl UserspaceRadioConfig {
    pub fn from_service_config_path(config: impl AsRef<Path>) -> Result<Self> {
        let cli = ServiceCli::config_only(config.as_ref().to_path_buf());
        let resolved = resolve_service_run(&cli)?;
        let runtime_config = service_runtime_config_from_resolved(&resolved)?;
        let execution_inputs =
            service_runtime_inputs_from_resolved(&resolved, runtime_config.channel)?;
        let mut config = Self::from_runtime_parts(runtime_config, execution_inputs);
        if resolved.tunnel.is_some() || !resolved.streams.is_empty() {
            config.endpoints = link_endpoints_from_service_resolved(&resolved)?;
        }
        Ok(config)
    }

    pub fn from_runtime_parts(
        runtime_config: ProductionRuntimeFlowConfig,
        mut execution_inputs: ProductionRuntimeFlowExecutionInputs,
    ) -> Self {
        execution_inputs.process_signal_stop = false;
        execution_inputs.external_stop_requested = None;
        let endpoints = userspace_radio_endpoints(&runtime_config);
        Self {
            runtime_config,
            execution_inputs,
            endpoints,
            ready_poll_interval: Duration::from_millis(25),
        }
    }

    pub fn with_ready_poll_interval(mut self, interval: Duration) -> Self {
        self.ready_poll_interval = interval;
        self
    }
}

#[deprecated(note = "use UserspaceRadioConfig")]
pub type MacosUserspaceRadioConfig = UserspaceRadioConfig;

#[derive(Debug, Clone)]
pub struct LinuxNativeWfbConfig {
    pub interface_name: String,
    pub channel: u8,
    pub bandwidth_mhz: u16,
    pub key_path: Option<PathBuf>,
    pub endpoints: LinkEndpoints,
}

#[derive(Debug, Clone)]
pub struct MacosWfbTunnelConfig {
    pub radio: UserspaceRadioConfig,
    pub wfb_key: PathBuf,
    pub wfb_tx_bin: PathBuf,
    pub wfb_rx_bin: PathBuf,
    pub tun_bin: PathBuf,
    pub artifact_dir: PathBuf,
    pub link_id: u32,
    pub tunnel_rx_radio_port: u8,
    pub tunnel_tx_radio_port: u8,
    pub tunnel_rx_aggregator: SocketAddr,
    pub tunnel_tx_radio_bind: SocketAddr,
    pub tunnel_tx_udp: SocketAddr,
    pub tunnel_rx_udp: SocketAddr,
    pub local_ip: IpAddr,
    pub peer_ip: IpAddr,
    pub prefix_len: u8,
    pub tun_mtu: usize,
    pub radio_mtu: usize,
    pub agg_timeout_ms: f64,
    pub bandwidth_mhz: u16,
    pub mcs: u8,
    pub fec_k: u8,
    pub fec_n: u8,
    pub use_sudo_for_tun: bool,
    pub startup_settle: Duration,
    pub ready_poll_interval: Duration,
    pub endpoints: LinkEndpoints,
}

impl MacosWfbTunnelConfig {
    pub fn from_radio_config(radio: UserspaceRadioConfig, wfb_key: impl Into<PathBuf>) -> Self {
        let service_tunnel = radio.endpoints.tunnel.clone();
        let tunnel_rx_aggregator = radio
            .runtime_config
            .primary_rx_forward
            .aggregator
            .unwrap_or_else(|| "127.0.0.1:5801".parse().expect("default aggregator"));
        let tunnel_rx_radio_port = radio
            .runtime_config
            .primary_rx_forward
            .radio_port
            .unwrap_or(3);
        let tunnel_tx_radio_bind = radio.runtime_config.bind_addr;
        let link_id = radio.runtime_config.primary_rx_forward.link_id.unwrap_or(0);
        let bandwidth_mhz = radio.runtime_config.bandwidth.mhz();
        let artifact_dir = std::env::temp_dir().join(format!(
            "wfb-link-tunnel-{}",
            NEXT_ARTIFACT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut config = Self {
            radio,
            wfb_key: wfb_key.into(),
            wfb_tx_bin: PathBuf::from("target/wfb-ng-macos/bin/wfb_tx"),
            wfb_rx_bin: PathBuf::from("target/wfb-ng-macos/bin/wfb_rx"),
            tun_bin: PathBuf::from("target/debug/wfb-tun-macos"),
            artifact_dir,
            link_id,
            tunnel_rx_radio_port,
            tunnel_tx_radio_port: 4,
            tunnel_rx_aggregator,
            tunnel_tx_radio_bind,
            tunnel_tx_udp: "127.0.0.1:56020".parse().expect("default tunnel tx UDP"),
            tunnel_rx_udp: "127.0.0.1:56021".parse().expect("default tunnel rx UDP"),
            local_ip: service_tunnel
                .as_ref()
                .map(|tunnel| tunnel.local_ip)
                .unwrap_or_else(|| "10.5.0.1".parse().expect("default local tunnel IP")),
            peer_ip: service_tunnel
                .as_ref()
                .map(|tunnel| tunnel.peer_ip)
                .unwrap_or_else(|| "10.5.0.2".parse().expect("default peer tunnel IP")),
            prefix_len: 24,
            tun_mtu: 1400,
            radio_mtu: 1445,
            agg_timeout_ms: 5.0,
            bandwidth_mhz,
            mcs: 1,
            fec_k: 2,
            fec_n: 4,
            use_sudo_for_tun: true,
            startup_settle: Duration::from_millis(500),
            ready_poll_interval: Duration::from_millis(50),
            endpoints: LinkEndpoints::empty(),
        };
        config.refresh_endpoints();
        config
    }

    pub fn from_service_config_path(
        config: impl AsRef<Path>,
        wfb_key: impl Into<PathBuf>,
    ) -> Result<Self> {
        let radio = UserspaceRadioConfig::from_service_config_path(config)?;
        Ok(Self::from_radio_config(radio, wfb_key))
    }

    pub fn with_bins(
        mut self,
        wfb_tx_bin: impl Into<PathBuf>,
        wfb_rx_bin: impl Into<PathBuf>,
        tun_bin: impl Into<PathBuf>,
    ) -> Self {
        self.wfb_tx_bin = wfb_tx_bin.into();
        self.wfb_rx_bin = wfb_rx_bin.into();
        self.tun_bin = tun_bin.into();
        self
    }

    pub fn with_artifact_dir(mut self, artifact_dir: impl Into<PathBuf>) -> Self {
        self.artifact_dir = artifact_dir.into();
        self
    }

    pub fn with_tunnel_streams(
        mut self,
        link_id: u32,
        rx_radio_port: u8,
        tx_radio_port: u8,
    ) -> Self {
        self.link_id = link_id;
        self.tunnel_rx_radio_port = rx_radio_port;
        self.tunnel_tx_radio_port = tx_radio_port;
        self.radio.runtime_config.primary_rx_forward.link_id = Some(link_id);
        self.radio.runtime_config.primary_rx_forward.radio_port = Some(rx_radio_port);
        self.refresh_endpoints();
        self
    }

    pub fn with_tunnel_ips(mut self, local_ip: IpAddr, peer_ip: IpAddr) -> Self {
        self.local_ip = local_ip;
        self.peer_ip = peer_ip;
        self.refresh_endpoints();
        self
    }

    pub fn with_tx_profile(mut self, bandwidth_mhz: u16, mcs: u8, fec_k: u8, fec_n: u8) -> Self {
        self.bandwidth_mhz = bandwidth_mhz;
        self.mcs = mcs;
        self.fec_k = fec_k;
        self.fec_n = fec_n;
        self
    }

    pub fn with_sudo_for_tun(mut self, enabled: bool) -> Self {
        self.use_sudo_for_tun = enabled;
        self
    }

    pub fn refresh_endpoints(&mut self) {
        let mut endpoints = userspace_radio_endpoints(&self.radio.runtime_config);
        endpoints.streams.push(LinkStreamEndpoint {
            name: "tunnel-tx".to_string(),
            direction: LinkDirection::Tx,
            local_udp: self.tunnel_tx_udp,
            payload_kind: PayloadKind::RawApplicationDatagram,
            criticality: StreamCriticality::Required,
            stream: Some(WfbStreamId {
                link_id: Some(self.link_id),
                radio_port: self.tunnel_tx_radio_port,
            }),
        });
        endpoints.streams.push(LinkStreamEndpoint {
            name: "tunnel-rx".to_string(),
            direction: LinkDirection::Rx,
            local_udp: self.tunnel_rx_udp,
            payload_kind: PayloadKind::RawApplicationDatagram,
            criticality: StreamCriticality::Required,
            stream: Some(WfbStreamId {
                link_id: Some(self.link_id),
                radio_port: self.tunnel_rx_radio_port,
            }),
        });
        endpoints.tunnel = Some(LinkTunnelEndpoint {
            local_ip: self.local_ip,
            peer_ip: self.peer_ip,
            interface_name: None,
        });
        self.endpoints = endpoints;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkEndpoints {
    pub streams: Vec<LinkStreamEndpoint>,
    pub tunnel: Option<LinkTunnelEndpoint>,
}

impl LinkEndpoints {
    pub fn empty() -> Self {
        Self {
            streams: Vec::new(),
            tunnel: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinkEndpointsBuilder {
    streams: Vec<LinkStreamEndpointDraft>,
    tunnel: Option<LinkTunnelEndpointDraft>,
}

impl LinkEndpointsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rx_stream(
        self,
        name: impl Into<String>,
        radio_port: u8,
        local_udp: impl ToString,
    ) -> Self {
        self.stream(
            name,
            LinkDirection::Rx,
            radio_port,
            local_udp,
            PayloadKind::RawApplicationDatagram,
        )
    }

    pub fn tx_stream(
        self,
        name: impl Into<String>,
        radio_port: u8,
        local_udp: impl ToString,
    ) -> Self {
        self.stream(
            name,
            LinkDirection::Tx,
            radio_port,
            local_udp,
            PayloadKind::RawApplicationDatagram,
        )
    }

    pub fn rx_stream_with_payload_kind(
        self,
        name: impl Into<String>,
        radio_port: u8,
        local_udp: impl ToString,
        payload_kind: PayloadKind,
    ) -> Self {
        self.stream(name, LinkDirection::Rx, radio_port, local_udp, payload_kind)
    }

    pub fn tx_stream_with_payload_kind(
        self,
        name: impl Into<String>,
        radio_port: u8,
        local_udp: impl ToString,
        payload_kind: PayloadKind,
    ) -> Self {
        self.stream(name, LinkDirection::Tx, radio_port, local_udp, payload_kind)
    }

    pub fn rx_stream_with_criticality(
        self,
        name: impl Into<String>,
        radio_port: u8,
        local_udp: impl ToString,
        payload_kind: PayloadKind,
        criticality: StreamCriticality,
    ) -> Self {
        self.stream_with_criticality(
            name,
            LinkDirection::Rx,
            radio_port,
            local_udp,
            payload_kind,
            criticality,
        )
    }

    pub fn tx_stream_with_criticality(
        self,
        name: impl Into<String>,
        radio_port: u8,
        local_udp: impl ToString,
        payload_kind: PayloadKind,
        criticality: StreamCriticality,
    ) -> Self {
        self.stream_with_criticality(
            name,
            LinkDirection::Tx,
            radio_port,
            local_udp,
            payload_kind,
            criticality,
        )
    }

    pub fn with_tunnel(self, local_ip: impl ToString, peer_ip: impl ToString) -> Self {
        Self {
            tunnel: Some(LinkTunnelEndpointDraft {
                local_ip: local_ip.to_string(),
                peer_ip: peer_ip.to_string(),
            }),
            ..self
        }
    }

    pub fn build(self) -> std::result::Result<LinkEndpoints, LinkBuilderError> {
        let mut names = HashSet::new();
        let mut sockets = HashSet::new();
        let mut stream_ports = HashSet::new();
        let mut streams = Vec::with_capacity(self.streams.len());

        for draft in self.streams {
            if !names.insert(draft.name.clone()) {
                return Err(LinkBuilderError::DuplicateStreamName { name: draft.name });
            }
            let local_udp = parse_socket_addr(&draft.name, &draft.local_udp)?;
            if !sockets.insert(local_udp) {
                return Err(LinkBuilderError::DuplicateLocalUdp { local_udp });
            }
            if !stream_ports.insert((draft.direction, draft.radio_port)) {
                return Err(LinkBuilderError::DuplicateDirectionRadioPort {
                    direction: draft.direction,
                    radio_port: draft.radio_port,
                });
            }
            streams.push(LinkStreamEndpoint {
                name: draft.name,
                direction: draft.direction,
                local_udp,
                payload_kind: draft.payload_kind,
                criticality: draft.criticality,
                stream: Some(WfbStreamId {
                    link_id: None,
                    radio_port: draft.radio_port,
                }),
            });
        }

        let tunnel = self.tunnel.map(|draft| draft.parse()).transpose()?;

        Ok(LinkEndpoints { streams, tunnel })
    }

    fn stream(
        mut self,
        name: impl Into<String>,
        direction: LinkDirection,
        radio_port: u8,
        local_udp: impl ToString,
        payload_kind: PayloadKind,
    ) -> Self {
        self = self.stream_with_criticality(
            name,
            direction,
            radio_port,
            local_udp,
            payload_kind,
            StreamCriticality::Required,
        );
        self
    }

    fn stream_with_criticality(
        mut self,
        name: impl Into<String>,
        direction: LinkDirection,
        radio_port: u8,
        local_udp: impl ToString,
        payload_kind: PayloadKind,
        criticality: StreamCriticality,
    ) -> Self {
        self.streams.push(LinkStreamEndpointDraft {
            name: name.into(),
            direction,
            radio_port,
            local_udp: local_udp.to_string(),
            payload_kind,
            criticality,
        });
        self
    }
}

#[derive(Debug, Clone)]
struct LinkStreamEndpointDraft {
    name: String,
    direction: LinkDirection,
    radio_port: u8,
    local_udp: String,
    payload_kind: PayloadKind,
    criticality: StreamCriticality,
}

#[derive(Debug, Clone)]
struct LinkTunnelEndpointDraft {
    local_ip: String,
    peer_ip: String,
}

impl LinkTunnelEndpointDraft {
    fn parse(self) -> std::result::Result<LinkTunnelEndpoint, LinkBuilderError> {
        Ok(LinkTunnelEndpoint {
            local_ip: self.local_ip.parse::<IpAddr>().map_err(|error| {
                LinkBuilderError::InvalidTunnelIp {
                    field: "local_ip",
                    value: self.local_ip.clone(),
                    message: error.to_string(),
                }
            })?,
            peer_ip: self.peer_ip.parse::<IpAddr>().map_err(|error| {
                LinkBuilderError::InvalidTunnelIp {
                    field: "peer_ip",
                    value: self.peer_ip.clone(),
                    message: error.to_string(),
                }
            })?,
            interface_name: None,
        })
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum LinkBuilderError {
    #[error("invalid local UDP socket for stream {name}: {value}: {message}")]
    InvalidLocalUdp {
        name: String,
        value: String,
        message: String,
    },
    #[error("invalid tunnel {field}: {value}: {message}")]
    InvalidTunnelIp {
        field: &'static str,
        value: String,
        message: String,
    },
    #[error("duplicate stream name: {name}")]
    DuplicateStreamName { name: String },
    #[error("duplicate local UDP socket: {local_udp}")]
    DuplicateLocalUdp { local_udp: SocketAddr },
    #[error("duplicate {direction:?} stream radio port: {radio_port}")]
    DuplicateDirectionRadioPort {
        direction: LinkDirection,
        radio_port: u8,
    },
}

fn parse_socket_addr(name: &str, value: &str) -> std::result::Result<SocketAddr, LinkBuilderError> {
    value.parse().map_err(
        |error: std::net::AddrParseError| LinkBuilderError::InvalidLocalUdp {
            name: name.to_string(),
            value: value.to_string(),
            message: error.to_string(),
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkStreamEndpoint {
    pub name: String,
    pub direction: LinkDirection,
    pub local_udp: SocketAddr,
    pub payload_kind: PayloadKind,
    pub criticality: StreamCriticality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<WfbStreamId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamCriticality {
    Required,
    BestEffort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WfbStreamId {
    pub link_id: Option<u32>,
    pub radio_port: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkDirection {
    Tx,
    Rx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKind {
    RawApplicationDatagram,
    WfbDistributorDatagram,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkTunnelEndpoint {
    pub local_ip: IpAddr,
    pub peer_ip: IpAddr,
    pub interface_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkReady {
    pub endpoints: LinkEndpoints,
    pub ready_file: PathBuf,
    pub ready_at_unix_ms: Option<u64>,
    pub backend: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkHealth {
    pub lifecycle: LinkLifecycle,
    pub ready: bool,
    pub endpoints: LinkEndpoints,
    pub tx: LinkTxHealth,
    pub rx: LinkRxHealth,
    pub streams: Vec<LinkStreamHealth>,
    pub degraded_streams: Vec<String>,
    pub backend: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkLifecycle {
    Starting,
    Ready,
    Degraded,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkTxHealth {
    pub ingress_datagrams_received: u64,
    pub ingress_pending_datagrams: u64,
    pub datagrams_received: u64,
    pub submitted_frames: u64,
    pub failed_submissions: u64,
    pub dropped_datagrams: u64,
    pub bytes_written: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkRxHealth {
    pub buffers_read: u64,
    pub parsed_frames: u64,
    pub forwarded_payloads: u64,
    pub dropped_packets: u64,
    pub rssi_average_dbm: Option<i64>,
    pub snr_average_db: Option<i64>,
    pub noise_average_dbm: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkStreamHealth {
    pub name: String,
    pub direction: LinkDirection,
    pub local_udp: SocketAddr,
    pub payload_kind: PayloadKind,
    pub criticality: StreamCriticality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<WfbStreamId>,
    pub degraded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx: Option<LinkStreamTxHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx: Option<LinkStreamRxHealth>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkStreamTxHealth {
    pub submitted_frames: u64,
    pub failed_submissions: u64,
    pub dropped_datagrams: u64,
    pub last_submit_unix_ms: Option<u64>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkStreamRxHealth {
    pub forwarded_frames: u64,
    pub forwarded_bytes: u64,
    pub last_rx_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkStreamDegradation {
    name: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkReport {
    pub lifecycle: LinkLifecycle,
    pub endpoints: LinkEndpoints,
    pub streams: Vec<LinkStreamHealth>,
    pub degraded_streams: Vec<String>,
    pub backend: LinkBackendReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkBackendReport {
    UserspaceRadio(ProductionRuntimeFlowReport),
    #[deprecated(note = "use LinkBackendReport::UserspaceRadio")]
    MacosUserspaceRadio(ProductionRuntimeFlowReport),
    MacosWfbTunnel(MacosWfbTunnelReport),
    LinuxNativeWfb(Value),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MacosWfbTunnelReport {
    pub radio: ProductionRuntimeFlowReport,
    pub tunnel_summary: Option<Value>,
    pub artifacts_dir: PathBuf,
    pub children: Vec<ChildProcessReport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChildProcessReport {
    pub name: String,
    pub pid: u32,
    pub status: Option<String>,
    pub stdout_log: PathBuf,
    pub stderr_log: PathBuf,
}

#[derive(Debug, Default)]
pub struct UserspaceRadioBackend;

#[deprecated(note = "use UserspaceRadioBackend")]
pub type MacosUserspaceRadioBackend = UserspaceRadioBackend;

#[derive(Debug, Default)]
pub struct MacosWfbTunnelBackend;

#[allow(deprecated)]
impl LinkBackend for UserspaceRadioBackend {
    fn start(&mut self, config: LinkConfig) -> Result<Box<dyn LinkHandle>> {
        let config = match config.backend {
            LinkBackendConfig::UserspaceRadio(config)
            | LinkBackendConfig::MacosUserspaceRadio(config) => config,
            _ => return Err(LinkError::UnsupportedBackend("expected userspace_radio")),
        };
        let handle = UserspaceRadioHandle::start(config)?;
        Ok(Box::new(handle))
    }
}

impl LinkBackend for MacosWfbTunnelBackend {
    fn start(&mut self, config: LinkConfig) -> Result<Box<dyn LinkHandle>> {
        let LinkBackendConfig::MacosWfbTunnel(config) = config.backend else {
            return Err(LinkError::UnsupportedBackend("expected macos_wfb_tunnel"));
        };
        let handle = MacosWfbTunnelHandle::start(config)?;
        Ok(Box::new(handle))
    }
}

impl UserspaceRadioHandle {
    fn start(config: UserspaceRadioConfig) -> Result<Self> {
        let UserspaceRadioConfig {
            mut runtime_config,
            mut execution_inputs,
            endpoints,
            ready_poll_interval,
        } = config;
        let startup_degraded_streams =
            apply_best_effort_tx_bind_preflight(&mut runtime_config, &endpoints);

        let ready_file = runtime_config
            .ready_file
            .get_or_insert_with(|| unique_runtime_artifact_path("ready"))
            .clone();
        let health_file = runtime_config
            .health_file
            .get_or_insert_with(|| unique_runtime_artifact_path("health"))
            .clone();
        remove_file_if_exists(&ready_file)?;
        remove_file_if_exists(&health_file)?;

        let stop_requested = Arc::new(AtomicBool::new(false));
        execution_inputs.process_signal_stop = false;
        execution_inputs.external_stop_requested = Some(Arc::clone(&stop_requested));

        let join_handle =
            thread::spawn(move || run_production_runtime_flow(runtime_config, execution_inputs));

        Ok(Self {
            endpoints,
            startup_degraded_streams,
            stop_requested,
            join_handle,
            ready_file,
            health_file,
            ready_poll_interval,
        })
    }
}

fn apply_best_effort_tx_bind_preflight(
    runtime_config: &mut ProductionRuntimeFlowConfig,
    endpoints: &LinkEndpoints,
) -> Vec<LinkStreamDegradation> {
    let mut degraded = Vec::new();
    for stream in endpoints.streams.iter().filter(|stream| {
        stream.direction == LinkDirection::Tx && stream.criticality == StreamCriticality::BestEffort
    }) {
        if !runtime_tx_bind_addrs(runtime_config).contains(&stream.local_udp) {
            continue;
        }
        match UdpSocket::bind(stream.local_udp) {
            Ok(socket) => drop(socket),
            Err(error) => {
                remove_runtime_tx_bind(runtime_config, stream.local_udp);
                degraded.push(LinkStreamDegradation {
                    name: stream.name.clone(),
                    reason: format!(
                        "best-effort TX bind {} unavailable: {error}",
                        stream.local_udp
                    ),
                });
            }
        }
    }
    degraded
}

fn runtime_tx_bind_addrs(config: &ProductionRuntimeFlowConfig) -> Vec<SocketAddr> {
    std::iter::once(config.bind_addr)
        .chain(config.tx_binds.iter().copied())
        .collect()
}

fn remove_runtime_tx_bind(config: &mut ProductionRuntimeFlowConfig, local_udp: SocketAddr) {
    if config.bind_addr == local_udp {
        if let Some(promoted) = config.tx_binds.first().copied() {
            config.bind_addr = promoted;
            config.tx_binds.remove(0);
        } else {
            config.bind_addr = "127.0.0.1:0"
                .parse()
                .expect("fallback loopback wildcard bind");
        }
    } else {
        config.tx_binds.retain(|bind| *bind != local_udp);
    }
}

#[derive(Debug)]
pub struct UserspaceRadioHandle {
    endpoints: LinkEndpoints,
    startup_degraded_streams: Vec<LinkStreamDegradation>,
    stop_requested: Arc<AtomicBool>,
    join_handle: JoinHandle<ProductionRuntimeFlowReport>,
    ready_file: PathBuf,
    health_file: PathBuf,
    ready_poll_interval: Duration,
}

impl LinkHandle for UserspaceRadioHandle {
    fn endpoints(&self) -> &LinkEndpoints {
        &self.endpoints
    }

    fn wait_ready(&self, timeout: Duration) -> Result<LinkReady> {
        let started = Instant::now();
        loop {
            if self.ready_file.exists() {
                match read_json_file(&self.ready_file) {
                    Ok(ready) => {
                        return Ok(LinkReady {
                            endpoints: self.endpoints.clone(),
                            ready_file: self.ready_file.clone(),
                            ready_at_unix_ms: ready
                                .get("ready_at_unix_ms")
                                .and_then(serde_json::Value::as_u64),
                            backend: ready,
                        })
                    }
                    Err(error @ LinkError::Io { .. }) => return Err(error),
                    Err(error @ LinkError::Json { .. }) => {
                        if started.elapsed() >= timeout {
                            return Err(error);
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            if self.join_handle.is_finished() {
                return Err(LinkError::BackendExitedBeforeReady);
            }
            if started.elapsed() >= timeout {
                return Err(LinkError::ReadyTimeout(timeout));
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            let sleep_for = self.ready_poll_interval.min(remaining);
            if sleep_for.is_zero() {
                return Err(LinkError::ReadyTimeout(timeout));
            }
            thread::sleep(sleep_for);
        }
    }

    fn health(&self) -> Result<LinkHealth> {
        if self.health_file.exists() {
            let health = read_json_file(&self.health_file)?;
            let streams = link_stream_health_from_backend_json(
                &self.endpoints,
                &health,
                &self.startup_degraded_streams,
            );
            let degraded_streams = degraded_stream_names(&streams);
            return Ok(LinkHealth {
                lifecycle: link_lifecycle_from_health_json(&health),
                ready: matches!(
                    link_lifecycle_from_health_json(&health),
                    LinkLifecycle::Ready | LinkLifecycle::Degraded | LinkLifecycle::Stopped
                ) || self.ready_file.exists(),
                endpoints: self.endpoints.clone(),
                tx: link_tx_health_from_json(health.get("tx")),
                rx: link_rx_health_from_json(health.get("rx")),
                streams,
                degraded_streams,
                backend: health,
            });
        }

        if self.ready_file.exists() {
            let ready = read_json_file(&self.ready_file)?;
            let streams = link_stream_health_from_backend_json(
                &self.endpoints,
                &ready,
                &self.startup_degraded_streams,
            );
            let degraded_streams = degraded_stream_names(&streams);
            return Ok(LinkHealth {
                lifecycle: LinkLifecycle::Ready,
                ready: true,
                endpoints: self.endpoints.clone(),
                tx: LinkTxHealth::default(),
                rx: LinkRxHealth::default(),
                streams,
                degraded_streams,
                backend: ready,
            });
        }

        let streams = link_stream_health_from_backend_json(
            &self.endpoints,
            &Value::Null,
            &self.startup_degraded_streams,
        );
        let degraded_streams = degraded_stream_names(&streams);
        Ok(LinkHealth {
            lifecycle: if self.join_handle.is_finished() {
                LinkLifecycle::Failed
            } else {
                LinkLifecycle::Starting
            },
            ready: false,
            endpoints: self.endpoints.clone(),
            tx: LinkTxHealth::default(),
            rx: LinkRxHealth::default(),
            streams,
            degraded_streams,
            backend: Value::Null,
        })
    }

    fn request_stop(&self) -> Result<()> {
        self.stop_requested.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn join(self: Box<Self>) -> Result<LinkReport> {
        let UserspaceRadioHandle {
            endpoints,
            startup_degraded_streams,
            join_handle,
            ..
        } = *self;
        let report = join_handle.join().map_err(|_| LinkError::JoinFailed)?;
        let lifecycle = match report.result {
            ProductionRuntimeFlowResult::Pass => LinkLifecycle::Stopped,
            ProductionRuntimeFlowResult::Fail => LinkLifecycle::Failed,
        };
        let backend_json = serde_json::to_value(&report).unwrap_or(Value::Null);
        let streams = link_stream_health_from_backend_json(
            &endpoints,
            &backend_json,
            &startup_degraded_streams,
        );
        let degraded_streams = degraded_stream_names(&streams);
        Ok(LinkReport {
            lifecycle,
            endpoints,
            streams,
            degraded_streams,
            backend: LinkBackendReport::UserspaceRadio(report),
        })
    }
}

#[deprecated(note = "use UserspaceRadioHandle")]
pub type MacosUserspaceRadioHandle = UserspaceRadioHandle;

#[derive(Debug)]
pub struct MacosWfbTunnelHandle {
    endpoints: LinkEndpoints,
    radio_handle: UserspaceRadioHandle,
    children: Mutex<Vec<ManagedChild>>,
    tun_summary_file: PathBuf,
    artifact_dir: PathBuf,
    startup_settle: Duration,
}

impl MacosWfbTunnelHandle {
    fn start(mut config: MacosWfbTunnelConfig) -> Result<Self> {
        config.refresh_endpoints();
        require_existing_path(&config.wfb_key, "WFB key")?;
        require_existing_path(&config.wfb_tx_bin, "wfb_tx binary")?;
        require_existing_path(&config.wfb_rx_bin, "wfb_rx binary")?;
        require_existing_path(&config.tun_bin, "wfb-tun-macos binary")?;
        fs::create_dir_all(&config.artifact_dir).map_err(|source| LinkError::Io {
            path: config.artifact_dir.clone(),
            source,
        })?;

        let tun_summary_file = config.artifact_dir.join("wf-tun-summary.json");
        remove_file_if_exists(&tun_summary_file)?;

        let mut radio = config.radio.clone();
        radio.runtime_config.primary_rx_forward.aggregator = Some(config.tunnel_rx_aggregator);
        radio.runtime_config.primary_rx_forward.link_id = Some(config.link_id);
        radio.runtime_config.primary_rx_forward.radio_port = Some(config.tunnel_rx_radio_port);
        radio.runtime_config.bind_addr = config.tunnel_tx_radio_bind;
        let radio_handle = UserspaceRadioHandle::start(radio)?;

        let mut children = Vec::new();
        children.push(spawn_logged(
            "wfb-rx",
            wfb_rx_command(&config),
            &config.artifact_dir,
        )?);
        children.push(spawn_logged(
            "wfb-tx",
            wfb_tx_command(&config),
            &config.artifact_dir,
        )?);
        children.push(spawn_logged(
            "wfb-tun",
            wfb_tun_command(&config, &tun_summary_file),
            &config.artifact_dir,
        )?);

        Ok(Self {
            endpoints: config.endpoints,
            radio_handle,
            children: Mutex::new(children),
            tun_summary_file,
            artifact_dir: config.artifact_dir,
            startup_settle: config.startup_settle,
        })
    }

    fn child_reports(&self) -> Result<Vec<ChildProcessReport>> {
        let mut children = self
            .children
            .lock()
            .map_err(|_| LinkError::ChildLockPoisoned)?;
        children.iter_mut().map(ManagedChild::report).collect()
    }

    fn check_children_alive(&self) -> Result<()> {
        let mut children = self
            .children
            .lock()
            .map_err(|_| LinkError::ChildLockPoisoned)?;
        for child in children.iter_mut() {
            if let Some(status) = child.try_wait()? {
                return Err(LinkError::ProcessExitedBeforeReady {
                    label: child.name.clone(),
                    status: exit_status_label(status),
                });
            }
        }
        Ok(())
    }

    fn terminate_children(&self) -> Result<Vec<ChildProcessReport>> {
        let mut children = self
            .children
            .lock()
            .map_err(|_| LinkError::ChildLockPoisoned)?;
        terminate_child_processes(&mut children);
        children.iter_mut().map(ManagedChild::report).collect()
    }
}

impl LinkHandle for MacosWfbTunnelHandle {
    fn endpoints(&self) -> &LinkEndpoints {
        &self.endpoints
    }

    fn wait_ready(&self, timeout: Duration) -> Result<LinkReady> {
        let started = Instant::now();
        loop {
            self.check_children_alive()?;
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(LinkError::ReadyTimeout(timeout));
            }
            match self.radio_handle.wait_ready(remaining) {
                Ok(ready) => {
                    if !self.startup_settle.is_zero() {
                        thread::sleep(self.startup_settle);
                    }
                    self.check_children_alive()?;
                    return Ok(LinkReady {
                        endpoints: self.endpoints.clone(),
                        ready_file: ready.ready_file,
                        ready_at_unix_ms: ready.ready_at_unix_ms,
                        backend: serde_json::json!({
                            "kind": "macos_wfb_tunnel",
                            "radio": ready.backend,
                            "artifacts_dir": self.artifact_dir,
                            "tun_summary_file": self.tun_summary_file,
                            "children": self.child_reports()?,
                        }),
                    });
                }
                Err(LinkError::ReadyTimeout(_)) => return Err(LinkError::ReadyTimeout(timeout)),
                Err(error) => return Err(error),
            }
        }
    }

    fn health(&self) -> Result<LinkHealth> {
        let radio_health = self.radio_handle.health()?;
        let children = self.child_reports()?;
        let child_failed = children.iter().any(|child| {
            child
                .status
                .as_deref()
                .is_some_and(|status| !status.starts_with("running"))
        });
        let lifecycle = if child_failed {
            LinkLifecycle::Degraded
        } else {
            radio_health.lifecycle
        };
        let streams =
            link_stream_health_from_backend_json(&self.endpoints, &radio_health.backend, &[]);
        let degraded_streams = degraded_stream_names(&streams);
        Ok(LinkHealth {
            lifecycle,
            ready: radio_health.ready && !child_failed,
            endpoints: self.endpoints.clone(),
            tx: radio_health.tx,
            rx: radio_health.rx,
            streams,
            degraded_streams,
            backend: serde_json::json!({
                "kind": "macos_wfb_tunnel",
                "radio": radio_health.backend,
                "artifacts_dir": self.artifact_dir,
                "tun_summary_file": self.tun_summary_file,
                "children": children,
                "tunnel_summary": read_json_file(&self.tun_summary_file).ok(),
            }),
        })
    }

    fn request_stop(&self) -> Result<()> {
        self.radio_handle.request_stop()?;
        let _ = self.terminate_children()?;
        Ok(())
    }

    fn join(self: Box<Self>) -> Result<LinkReport> {
        let MacosWfbTunnelHandle {
            endpoints,
            radio_handle,
            children,
            tun_summary_file,
            artifact_dir,
            ..
        } = *self;
        radio_handle.request_stop()?;
        let mut children = children
            .into_inner()
            .map_err(|_| LinkError::ChildLockPoisoned)?;
        terminate_child_processes(&mut children);
        let child_reports = children
            .iter_mut()
            .map(ManagedChild::report)
            .collect::<Result<Vec<_>>>()?;
        let radio_report = Box::new(radio_handle).join()?;
        let LinkBackendReport::UserspaceRadio(radio) = radio_report.backend else {
            unreachable!("macOS tunnel owns a userspace radio handle");
        };
        let tunnel_summary = read_json_file(&tun_summary_file).ok();
        let child_failed = child_reports.iter().any(|child| {
            child
                .status
                .as_deref()
                .is_some_and(|status| !status.starts_with("exit:0") && status != "signal")
        });
        let lifecycle = if radio_report.lifecycle == LinkLifecycle::Stopped && !child_failed {
            LinkLifecycle::Stopped
        } else {
            LinkLifecycle::Failed
        };
        let backend_json = serde_json::to_value(&radio).unwrap_or(Value::Null);
        let streams = link_stream_health_from_backend_json(&endpoints, &backend_json, &[]);
        let degraded_streams = degraded_stream_names(&streams);
        Ok(LinkReport {
            lifecycle,
            endpoints,
            streams,
            degraded_streams,
            backend: LinkBackendReport::MacosWfbTunnel(MacosWfbTunnelReport {
                radio,
                tunnel_summary,
                artifacts_dir: artifact_dir,
                children: child_reports,
            }),
        })
    }
}

pub fn userspace_radio_endpoints(config: &ProductionRuntimeFlowConfig) -> LinkEndpoints {
    let mut streams = Vec::with_capacity(2 + config.tx_binds.len() + config.rx_forwards.len());
    for (index, local_udp) in std::iter::once(config.bind_addr)
        .chain(config.tx_binds.iter().copied())
        .enumerate()
    {
        streams.push(LinkStreamEndpoint {
            name: format!("tx{index}"),
            direction: LinkDirection::Tx,
            local_udp,
            payload_kind: PayloadKind::WfbDistributorDatagram,
            criticality: StreamCriticality::Required,
            stream: None,
        });
    }

    if let (Some(local_udp), Some(radio_port)) = (
        config.primary_rx_forward.aggregator,
        config.primary_rx_forward.radio_port,
    ) {
        streams.push(LinkStreamEndpoint {
            name: "rx-primary".to_string(),
            direction: LinkDirection::Rx,
            local_udp,
            payload_kind: PayloadKind::WfbDistributorDatagram,
            criticality: StreamCriticality::Required,
            stream: Some(WfbStreamId {
                link_id: config.primary_rx_forward.link_id,
                radio_port,
            }),
        });
    }

    for (index, forward) in config.rx_forwards.iter().enumerate() {
        let Some(local_udp) = forward.aggregator else {
            continue;
        };
        streams.push(LinkStreamEndpoint {
            name: format!("rx{index}"),
            direction: LinkDirection::Rx,
            local_udp,
            payload_kind: PayloadKind::WfbDistributorDatagram,
            criticality: StreamCriticality::Required,
            stream: Some(WfbStreamId {
                link_id: forward.link_id,
                radio_port: forward.radio_port,
            }),
        });
    }

    LinkEndpoints {
        streams,
        tunnel: None,
    }
}

#[deprecated(note = "use userspace_radio_endpoints")]
pub fn macos_userspace_radio_endpoints(config: &ProductionRuntimeFlowConfig) -> LinkEndpoints {
    userspace_radio_endpoints(config)
}

fn link_endpoints_from_service_resolved(
    resolved: &ResolvedServiceRun,
) -> std::result::Result<LinkEndpoints, LinkBuilderError> {
    let streams = resolved
        .streams
        .iter()
        .map(link_stream_endpoint_from_service_stream)
        .collect();
    let tunnel = resolved.tunnel.as_ref().map(|tunnel| LinkTunnelEndpoint {
        local_ip: tunnel.local_ip,
        peer_ip: tunnel.peer_ip,
        interface_name: tunnel.interface_name.clone(),
    });
    let endpoints = LinkEndpoints { streams, tunnel };
    validate_link_endpoints(&endpoints)?;
    Ok(endpoints)
}

fn link_stream_endpoint_from_service_stream(stream: &ResolvedServiceStream) -> LinkStreamEndpoint {
    LinkStreamEndpoint {
        name: stream.name.clone(),
        direction: LinkDirection::from(stream.direction),
        local_udp: stream.local_udp,
        payload_kind: PayloadKind::from(stream.payload_kind),
        criticality: StreamCriticality::from(stream.criticality),
        stream: Some(WfbStreamId {
            link_id: stream.link_id,
            radio_port: stream.radio_port,
        }),
    }
}

fn validate_link_endpoints(endpoints: &LinkEndpoints) -> std::result::Result<(), LinkBuilderError> {
    let mut names = HashSet::new();
    let mut sockets = HashSet::new();
    let mut stream_ports = HashSet::new();

    for stream in &endpoints.streams {
        if !names.insert(stream.name.clone()) {
            return Err(LinkBuilderError::DuplicateStreamName {
                name: stream.name.clone(),
            });
        }
        if !sockets.insert(stream.local_udp) {
            return Err(LinkBuilderError::DuplicateLocalUdp {
                local_udp: stream.local_udp,
            });
        }
        if let Some(wfb_stream) = stream.stream {
            if !stream_ports.insert((stream.direction, wfb_stream.radio_port)) {
                return Err(LinkBuilderError::DuplicateDirectionRadioPort {
                    direction: stream.direction,
                    radio_port: wfb_stream.radio_port,
                });
            }
        }
    }
    Ok(())
}

impl From<ServiceStreamDirection> for LinkDirection {
    fn from(direction: ServiceStreamDirection) -> Self {
        match direction {
            ServiceStreamDirection::Tx => Self::Tx,
            ServiceStreamDirection::Rx => Self::Rx,
        }
    }
}

impl From<ServiceStreamPayloadKind> for PayloadKind {
    fn from(payload_kind: ServiceStreamPayloadKind) -> Self {
        match payload_kind {
            ServiceStreamPayloadKind::RawApplicationDatagram => Self::RawApplicationDatagram,
            ServiceStreamPayloadKind::WfbDistributorDatagram => Self::WfbDistributorDatagram,
        }
    }
}

impl From<ServiceStreamCriticality> for StreamCriticality {
    fn from(criticality: ServiceStreamCriticality) -> Self {
        match criticality {
            ServiceStreamCriticality::Required => Self::Required,
            ServiceStreamCriticality::BestEffort => Self::BestEffort,
        }
    }
}

#[derive(Debug)]
struct ManagedChild {
    name: String,
    child: Child,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
    status: Option<ExitStatus>,
}

impl ManagedChild {
    fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let status = self.child.try_wait().map_err(|source| LinkError::Spawn {
            label: "poll child process",
            source,
        })?;
        if let Some(status) = status {
            self.status = Some(status);
        }
        Ok(status)
    }

    fn report(&mut self) -> Result<ChildProcessReport> {
        let status = self.try_wait()?.map(exit_status_label);
        Ok(ChildProcessReport {
            name: self.name.clone(),
            pid: self.child.id(),
            status: status.or_else(|| Some("running".to_string())),
            stdout_log: self.stdout_log.clone(),
            stderr_log: self.stderr_log.clone(),
        })
    }
}

fn require_existing_path(path: &Path, label: &'static str) -> Result<()> {
    if path.exists() {
        Ok(())
    } else {
        Err(LinkError::MissingPath {
            label,
            path: path.to_path_buf(),
        })
    }
}

fn spawn_logged(
    label: &'static str,
    mut command: Command,
    artifact_dir: &Path,
) -> Result<ManagedChild> {
    let stdout_log = artifact_dir.join(format!("{label}.stdout.log"));
    let stderr_log = artifact_dir.join(format!("{label}.stderr.log"));
    let stdout = File::create(&stdout_log).map_err(|source| LinkError::Io {
        path: stdout_log.clone(),
        source,
    })?;
    let stderr = File::create(&stderr_log).map_err(|source| LinkError::Io {
        path: stderr_log.clone(),
        source,
    })?;
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|source| LinkError::Spawn { label, source })?;
    Ok(ManagedChild {
        name: label.to_string(),
        child,
        stdout_log,
        stderr_log,
        status: None,
    })
}

fn wfb_rx_command(config: &MacosWfbTunnelConfig) -> Command {
    let mut command = Command::new(&config.wfb_rx_bin);
    command
        .arg("-a")
        .arg(config.tunnel_rx_aggregator.port().to_string())
        .arg("-K")
        .arg(&config.wfb_key)
        .arg("-i")
        .arg(config.link_id.to_string())
        .arg("-p")
        .arg(config.tunnel_rx_radio_port.to_string())
        .arg("-c")
        .arg(config.tunnel_rx_udp.ip().to_string())
        .arg("-u")
        .arg(config.tunnel_rx_udp.port().to_string());
    command
}

fn wfb_tx_command(config: &MacosWfbTunnelConfig) -> Command {
    let mut command = Command::new(&config.wfb_tx_bin);
    command
        .arg("-d")
        .arg("-K")
        .arg(&config.wfb_key)
        .arg("-i")
        .arg(config.link_id.to_string())
        .arg("-p")
        .arg(config.tunnel_tx_radio_port.to_string())
        .arg("-B")
        .arg(config.bandwidth_mhz.to_string())
        .arg("-M")
        .arg(config.mcs.to_string())
        .arg("-k")
        .arg(config.fec_k.to_string())
        .arg("-n")
        .arg(config.fec_n.to_string())
        .arg("-u")
        .arg(config.tunnel_tx_udp.port().to_string())
        .arg(config.tunnel_tx_radio_bind.to_string());
    command
}

fn wfb_tun_command(config: &MacosWfbTunnelConfig, summary_file: &Path) -> Command {
    let mut command = if config.use_sudo_for_tun {
        let mut sudo = Command::new("sudo");
        sudo.arg("-n").arg(&config.tun_bin);
        sudo
    } else {
        Command::new(&config.tun_bin)
    };
    command
        .arg("--local-ip")
        .arg(config.local_ip.to_string())
        .arg("--peer-ip")
        .arg(config.peer_ip.to_string())
        .arg("--prefix-len")
        .arg(config.prefix_len.to_string())
        .arg("--tun-mtu")
        .arg(config.tun_mtu.to_string())
        .arg("--radio-mtu")
        .arg(config.radio_mtu.to_string())
        .arg("--agg-timeout-ms")
        .arg(config.agg_timeout_ms.to_string())
        .arg("--tx-peer")
        .arg(config.tunnel_tx_udp.to_string())
        .arg("--rx-bind")
        .arg(config.tunnel_rx_udp.to_string())
        .arg("--summary-file")
        .arg(summary_file);
    command
}

fn terminate_child_processes(children: &mut [ManagedChild]) {
    for child in children.iter_mut() {
        if child.status.is_some() {
            continue;
        }
        send_sigterm(child.child.id());
    }

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut all_done = true;
        for child in children.iter_mut() {
            if child.status.is_some() {
                continue;
            }
            match child.child.try_wait() {
                Ok(Some(status)) => child.status = Some(status),
                Ok(None) => all_done = false,
                Err(_) => all_done = true,
            }
        }
        if all_done || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    for child in children.iter_mut() {
        if child.status.is_some() {
            continue;
        }
        let _ = child.child.kill();
        if let Ok(status) = child.child.wait() {
            child.status = Some(status);
        }
    }
}

fn send_sigterm(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

fn exit_status_label(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit:{code}"),
        None => "signal".to_string(),
    }
}

fn unique_runtime_artifact_path(kind: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let id = NEXT_ARTIFACT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "wfb-link-{kind}-{}-{nanos}-{id}.json",
        std::process::id()
    ))
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(LinkError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read_json_file(path: &Path) -> Result<Value> {
    let input = fs::read_to_string(path).map_err(|source| LinkError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&input).map_err(|source| LinkError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn link_lifecycle_from_health_json(health: &Value) -> LinkLifecycle {
    match health
        .get("lifecycle")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
    {
        "starting" | "validating" | "initializing" => LinkLifecycle::Starting,
        "ready" | "running" => LinkLifecycle::Ready,
        "stopping" => LinkLifecycle::Stopping,
        "exited_pass" => LinkLifecycle::Stopped,
        "exited_fail" => LinkLifecycle::Failed,
        _ => LinkLifecycle::Degraded,
    }
}

fn link_tx_health_from_json(tx: Option<&Value>) -> LinkTxHealth {
    let Some(tx) = tx else {
        return LinkTxHealth::default();
    };
    LinkTxHealth {
        ingress_datagrams_received: json_u64(tx, "ingress_datagrams_received"),
        ingress_pending_datagrams: json_u64(tx, "ingress_pending_datagrams"),
        datagrams_received: json_u64(tx, "datagrams_received"),
        submitted_frames: json_u64(tx, "submitted_frames"),
        failed_submissions: json_u64(tx, "failed_submissions"),
        dropped_datagrams: json_u64(tx, "dropped_datagrams"),
        bytes_written: json_u64(tx, "bytes_written"),
    }
}

fn link_rx_health_from_json(rx: Option<&Value>) -> LinkRxHealth {
    let Some(rx) = rx else {
        return LinkRxHealth::default();
    };
    LinkRxHealth {
        buffers_read: json_u64(rx, "buffers_read"),
        parsed_frames: json_u64(rx, "parsed_frames"),
        forwarded_payloads: json_u64(rx, "forwarded_payloads"),
        dropped_packets: json_u64(rx, "dropped_packets"),
        rssi_average_dbm: rx_signal_average(rx, "rssi_dbm"),
        snr_average_db: rx_signal_average(rx, "snr_db"),
        noise_average_dbm: rx_signal_average(rx, "noise_dbm"),
    }
}

fn link_stream_health_from_backend_json(
    endpoints: &LinkEndpoints,
    backend: &Value,
    startup_degraded_streams: &[LinkStreamDegradation],
) -> Vec<LinkStreamHealth> {
    let tx_stream_count = endpoints
        .streams
        .iter()
        .filter(|stream| stream.direction == LinkDirection::Tx)
        .count();
    endpoints
        .streams
        .iter()
        .map(|stream| {
            let startup_degradation = startup_degraded_streams
                .iter()
                .find(|degradation| degradation.name == stream.name);
            let degradation_reason =
                startup_degradation.map(|degradation| degradation.reason.clone());
            let degraded = degradation_reason.is_some();
            LinkStreamHealth {
                name: stream.name.clone(),
                direction: stream.direction,
                local_udp: stream.local_udp,
                payload_kind: stream.payload_kind,
                criticality: stream.criticality,
                stream: stream.stream,
                degraded,
                degradation_reason,
                tx: (stream.direction == LinkDirection::Tx).then(|| {
                    link_stream_tx_health_from_json(backend.get("tx"), stream, tx_stream_count)
                }),
                rx: (stream.direction == LinkDirection::Rx)
                    .then(|| link_stream_rx_health_from_json(backend.get("rx"), stream)),
            }
        })
        .collect()
}

fn degraded_stream_names(streams: &[LinkStreamHealth]) -> Vec<String> {
    streams
        .iter()
        .filter(|stream| stream.degraded)
        .map(|stream| stream.name.clone())
        .collect()
}

fn link_stream_tx_health_from_json(
    tx: Option<&Value>,
    stream: &LinkStreamEndpoint,
    tx_stream_count: usize,
) -> LinkStreamTxHealth {
    let Some(tx) = tx else {
        return LinkStreamTxHealth::default();
    };
    if let Some(bind) = tx
        .get("tx_binds")
        .and_then(Value::as_array)
        .and_then(|binds| {
            binds.iter().find(|bind| {
                bind.get("bind_addr")
                    .and_then(Value::as_str)
                    .and_then(|value| value.parse::<SocketAddr>().ok())
                    == Some(stream.local_udp)
            })
        })
    {
        return LinkStreamTxHealth {
            submitted_frames: json_u64(bind, "submitted_frames"),
            failed_submissions: json_u64(bind, "failed_submissions"),
            dropped_datagrams: json_u64(bind, "dropped_datagrams"),
            last_submit_unix_ms: bind.get("last_submit_unix_ms").and_then(Value::as_u64),
        };
    }

    let submitted_frames = stream
        .stream
        .map(|wfb_stream| {
            link_wfb_observation_count(tx.get("wfb_channel_observations"), wfb_stream)
        })
        .filter(|count| *count > 0)
        .unwrap_or_else(|| {
            if tx_stream_count <= 1 {
                json_u64(tx, "submitted_frames")
            } else {
                0
            }
        });
    LinkStreamTxHealth {
        submitted_frames,
        failed_submissions: if tx_stream_count <= 1 {
            json_u64(tx, "failed_submissions")
        } else {
            0
        },
        dropped_datagrams: if tx_stream_count <= 1 {
            json_u64(tx, "dropped_datagrams")
        } else {
            0
        },
        last_submit_unix_ms: tx.get("last_submit_unix_ms").and_then(Value::as_u64),
    }
}

fn link_stream_rx_health_from_json(
    rx: Option<&Value>,
    stream: &LinkStreamEndpoint,
) -> LinkStreamRxHealth {
    let Some(rx) = rx else {
        return LinkStreamRxHealth::default();
    };
    let Some(wfb_stream) = stream.stream else {
        return LinkStreamRxHealth::default();
    };
    let Some(forward) = rx
        .get("rx_forwards")
        .and_then(Value::as_array)
        .and_then(|forwards| {
            forwards.iter().find(|forward| {
                let channel_id = forward
                    .get("config")
                    .and_then(|config| config.get("channel_id"));
                let link_id_matches = match wfb_stream.link_id {
                    Some(link_id) => {
                        channel_id
                            .and_then(|channel_id| channel_id.get("link_id"))
                            .and_then(Value::as_u64)
                            == Some(u64::from(link_id))
                    }
                    None => true,
                };
                link_id_matches
                    && channel_id
                        .and_then(|channel_id| channel_id.get("radio_port"))
                        .and_then(Value::as_u64)
                        == Some(u64::from(wfb_stream.radio_port))
            })
        })
    else {
        return LinkStreamRxHealth::default();
    };
    LinkStreamRxHealth {
        forwarded_frames: forward
            .get("counters")
            .map(|counters| json_u64(counters, "forwarded"))
            .unwrap_or(0),
        forwarded_bytes: json_u64(forward, "forwarded_bytes"),
        last_rx_unix_ms: forward.get("last_rx_unix_ms").and_then(Value::as_u64),
    }
}

fn link_wfb_observation_count(observations: Option<&Value>, stream: WfbStreamId) -> u64 {
    observations
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|observation| wfb_observation_matches_stream(observation, stream))
        .map(|observation| json_u64(observation, "count"))
        .sum()
}

fn wfb_observation_matches_stream(observation: &Value, stream: WfbStreamId) -> bool {
    let source_matches = wfb_observation_side_matches_stream(observation, "source", stream);
    let destination_matches =
        wfb_observation_side_matches_stream(observation, "destination", stream);
    source_matches || destination_matches
}

fn wfb_observation_side_matches_stream(
    observation: &Value,
    side: &str,
    stream: WfbStreamId,
) -> bool {
    let radio_key = format!("{side}_radio_port");
    if observation.get(&radio_key).and_then(Value::as_u64) != Some(u64::from(stream.radio_port)) {
        return false;
    }
    let Some(link_id) = stream.link_id else {
        return true;
    };
    let link_key = format!("{side}_link_id");
    observation.get(&link_key).and_then(Value::as_u64) == Some(u64::from(link_id))
}

fn rx_signal_average(rx: &Value, metric: &str) -> Option<i64> {
    rx.get("signal")?.get(metric)?.get("average")?.as_i64()
}

fn json_u64(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use radio_core::{Bandwidth, Channel, DeviceSelector};
    use wfb_radio_runtime::{
        ProductionRuntimeAirtimeSchedule, ProductionRuntimePrimaryRxForwardConfig,
        ProductionRuntimeRxForwardConfig, ProductionRuntimeUsbConfig, TxCalibrationProfile,
    };

    fn fixture_runtime_config() -> ProductionRuntimeFlowConfig {
        ProductionRuntimeFlowConfig {
            usb: ProductionRuntimeUsbConfig::libusb(DeviceSelector::default()),
            channel: Channel::from_number(161).expect("channel 161"),
            bandwidth: Bandwidth::Mhz20,
            firmware: None,
            bind_addr: "127.0.0.1:5600".parse().expect("bind addr"),
            tx_binds: vec!["127.0.0.1:5601".parse().expect("tx bind")],
            duration_ms: 0,
            rx_timeout_ms: 20,
            tx_burst_limit: 8,
            tx_min_interval_us: 400,
            max_datagrams: 0,
            airtime_schedule: ProductionRuntimeAirtimeSchedule::continuous(),
            ready_file: None,
            health_file: None,
            tx_authorized: true,
            live_register_write_authorized: false,
            calibration_profile: TxCalibrationProfile::CurrentDefault,
            captured_tail_applied: true,
            primary_rx_forward: ProductionRuntimePrimaryRxForwardConfig {
                link_id: Some(0x2f389),
                radio_port: Some(0),
                aggregator: Some("127.0.0.1:5700".parse().expect("primary aggregator")),
            },
            rx_forwards: vec![ProductionRuntimeRxForwardConfig {
                link_id: Some(0x2f389),
                radio_port: 4,
                aggregator: Some("127.0.0.1:5704".parse().expect("rx forward")),
            }],
            rx_wlan_idx: 0,
            rx_mcs_index: 1,
        }
    }

    #[test]
    fn endpoint_shape_uses_wfb_distributor_datagrams() {
        let endpoints = userspace_radio_endpoints(&fixture_runtime_config());

        assert_eq!(endpoints.tunnel, None);
        assert_eq!(endpoints.streams.len(), 4);
        assert_eq!(endpoints.streams[0].name, "tx0");
        assert_eq!(endpoints.streams[0].direction, LinkDirection::Tx);
        assert_eq!(
            endpoints.streams[0].payload_kind,
            PayloadKind::WfbDistributorDatagram
        );
        assert_eq!(endpoints.streams[0].stream, None);
        assert_eq!(endpoints.streams[2].name, "rx-primary");
        assert_eq!(endpoints.streams[2].direction, LinkDirection::Rx);
        assert_eq!(
            endpoints.streams[2].stream,
            Some(WfbStreamId {
                link_id: Some(0x2f389),
                radio_port: 0,
            })
        );
    }

    #[test]
    fn endpoint_builder_constructs_named_streams_and_tunnel() {
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
            .build()
            .expect("endpoints");

        assert_eq!(endpoints.streams.len(), 3);
        assert_eq!(endpoints.streams[0].name, "s0");
        assert_eq!(endpoints.streams[0].direction, LinkDirection::Rx);
        assert_eq!(
            endpoints.streams[0].payload_kind,
            PayloadKind::RawApplicationDatagram
        );
        assert_eq!(
            endpoints.streams[0].criticality,
            StreamCriticality::Required
        );
        assert_eq!(
            endpoints.streams[0].stream,
            Some(WfbStreamId {
                link_id: None,
                radio_port: 0,
            })
        );
        assert_eq!(endpoints.streams[2].direction, LinkDirection::Tx);
        assert_eq!(
            endpoints.streams[2].payload_kind,
            PayloadKind::WfbDistributorDatagram
        );
        let tunnel = endpoints.tunnel.expect("tunnel");
        assert_eq!(tunnel.local_ip, "10.5.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(tunnel.peer_ip, "10.5.0.2".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn endpoint_builder_accepts_best_effort_streams() {
        let endpoints = LinkEndpointsBuilder::new()
            .tx_stream_with_criticality(
                "opportunistic",
                7,
                "127.0.0.1:5807",
                PayloadKind::RawApplicationDatagram,
                StreamCriticality::BestEffort,
            )
            .build()
            .expect("endpoints");

        assert_eq!(
            endpoints.streams[0].criticality,
            StreamCriticality::BestEffort
        );
    }

    #[test]
    fn stream_health_maps_runtime_counters_by_named_endpoint() {
        let endpoints = LinkEndpoints {
            tunnel: None,
            streams: vec![
                LinkStreamEndpoint {
                    name: "control-up".to_string(),
                    direction: LinkDirection::Tx,
                    local_udp: "127.0.0.1:5606".parse().unwrap(),
                    payload_kind: PayloadKind::WfbDistributorDatagram,
                    criticality: StreamCriticality::Required,
                    stream: Some(WfbStreamId {
                        link_id: Some(1),
                        radio_port: 6,
                    }),
                },
                LinkStreamEndpoint {
                    name: "video-down".to_string(),
                    direction: LinkDirection::Rx,
                    local_udp: "127.0.0.1:5804".parse().unwrap(),
                    payload_kind: PayloadKind::RawApplicationDatagram,
                    criticality: StreamCriticality::BestEffort,
                    stream: Some(WfbStreamId {
                        link_id: Some(1),
                        radio_port: 4,
                    }),
                },
            ],
        };
        let backend = serde_json::json!({
            "tx": {
                "tx_binds": [{
                    "report_index": 0,
                    "bind_addr": "127.0.0.1:5606",
                    "datagrams_received": 3,
                    "submitted_frames": 2,
                    "failed_submissions": 1,
                    "dropped_datagrams": 1,
                    "last_submit_unix_ms": 1234
                }]
            },
            "rx": {
                "rx_forwards": [{
                    "config": {
                        "channel_id": {
                            "link_id": 1,
                            "radio_port": 4
                        }
                    },
                    "forwarded_bytes": 4096,
                    "last_rx_unix_ms": 5678,
                    "counters": {
                        "forwarded": 8
                    }
                }]
            }
        });

        let streams = link_stream_health_from_backend_json(
            &endpoints,
            &backend,
            &[LinkStreamDegradation {
                name: "video-down".to_string(),
                reason: "optional aggregator unavailable".to_string(),
            }],
        );

        assert_eq!(
            streams[0].tx,
            Some(LinkStreamTxHealth {
                submitted_frames: 2,
                failed_submissions: 1,
                dropped_datagrams: 1,
                last_submit_unix_ms: Some(1234),
            })
        );
        assert_eq!(
            streams[1].rx,
            Some(LinkStreamRxHealth {
                forwarded_frames: 8,
                forwarded_bytes: 4096,
                last_rx_unix_ms: Some(5678),
            })
        );
        assert!(streams[1].degraded);
        assert_eq!(degraded_stream_names(&streams), vec!["video-down"]);
    }

    #[test]
    fn endpoint_builder_rejects_duplicate_stream_names() {
        let error = LinkEndpointsBuilder::new()
            .rx_stream("s0", 0, "127.0.0.1:5800")
            .tx_stream("s0", 1, "127.0.0.1:5801")
            .build()
            .expect_err("duplicate name");

        assert_eq!(
            error,
            LinkBuilderError::DuplicateStreamName {
                name: "s0".to_string(),
            }
        );
    }

    #[test]
    fn endpoint_builder_rejects_duplicate_sockets() {
        let error = LinkEndpointsBuilder::new()
            .rx_stream("s0", 0, "127.0.0.1:5800")
            .tx_stream("s1", 1, "127.0.0.1:5800")
            .build()
            .expect_err("duplicate socket");

        assert_eq!(
            error,
            LinkBuilderError::DuplicateLocalUdp {
                local_udp: "127.0.0.1:5800".parse().unwrap(),
            }
        );
    }

    #[test]
    fn endpoint_builder_rejects_duplicate_direction_and_radio_port() {
        let error = LinkEndpointsBuilder::new()
            .rx_stream("s0", 0, "127.0.0.1:5800")
            .rx_stream("s1", 0, "127.0.0.1:5801")
            .build()
            .expect_err("duplicate direction radio port");

        assert_eq!(
            error,
            LinkBuilderError::DuplicateDirectionRadioPort {
                direction: LinkDirection::Rx,
                radio_port: 0,
            }
        );
    }

    #[test]
    fn endpoint_builder_rejects_invalid_addresses() {
        let error = LinkEndpointsBuilder::new()
            .rx_stream("s0", 0, "not-a-socket")
            .build()
            .expect_err("invalid socket");
        assert!(matches!(error, LinkBuilderError::InvalidLocalUdp { .. }));

        let error = LinkEndpointsBuilder::new()
            .with_tunnel("not-an-ip", "10.5.0.2")
            .build()
            .expect_err("invalid tunnel ip");
        assert!(matches!(
            error,
            LinkBuilderError::InvalidTunnelIp {
                field: "local_ip",
                ..
            }
        ));
    }

    #[test]
    fn userspace_radio_config_from_runtime_parts_disables_process_signal_stop() {
        let mut inputs = ProductionRuntimeFlowExecutionInputs::default();
        inputs.process_signal_stop = true;
        inputs.external_stop_requested = Some(Arc::new(AtomicBool::new(false)));

        let config = UserspaceRadioConfig::from_runtime_parts(fixture_runtime_config(), inputs);

        assert!(!config.execution_inputs.process_signal_stop);
        assert!(config.execution_inputs.external_stop_requested.is_none());
    }

    #[test]
    fn tunnel_config_exposes_ip_tunnel_and_internal_streams() {
        let radio =
            UserspaceRadioConfig::from_runtime_parts(fixture_runtime_config(), Default::default());
        let config = MacosWfbTunnelConfig::from_radio_config(radio, "/tmp/gs.key")
            .with_tunnel_streams(0, 3, 4);

        let tunnel = config.endpoints.tunnel.expect("tunnel endpoint");
        assert_eq!(tunnel.local_ip, "10.5.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(tunnel.peer_ip, "10.5.0.2".parse::<IpAddr>().unwrap());
        assert!(config
            .endpoints
            .streams
            .iter()
            .any(|stream| stream.name == "tunnel-tx"
                && stream.direction == LinkDirection::Tx
                && stream.payload_kind == PayloadKind::RawApplicationDatagram
                && stream.stream
                    == Some(WfbStreamId {
                        link_id: Some(0),
                        radio_port: 4,
                    })));
        assert!(config
            .endpoints
            .streams
            .iter()
            .any(|stream| stream.name == "tunnel-rx"
                && stream.direction == LinkDirection::Rx
                && stream.payload_kind == PayloadKind::RawApplicationDatagram
                && stream.stream
                    == Some(WfbStreamId {
                        link_id: Some(0),
                        radio_port: 3,
                    })));
    }

    #[test]
    fn userspace_radio_handle_request_stop_sets_cooperative_flag_and_join_reports() {
        let runtime_config = fixture_runtime_config();
        let endpoints = userspace_radio_endpoints(&runtime_config);
        let stop_requested = Arc::new(AtomicBool::new(false));
        let stop_for_thread = Arc::clone(&stop_requested);
        let join_handle = thread::spawn(move || {
            while !stop_for_thread.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
            ProductionRuntimeFlowReport::not_started(
                &runtime_config,
                RuntimeRadioError::new("test_stop", "stopped by test"),
            )
        });
        let handle = UserspaceRadioHandle {
            endpoints: endpoints.clone(),
            startup_degraded_streams: Vec::new(),
            stop_requested: Arc::clone(&stop_requested),
            join_handle,
            ready_file: unique_runtime_artifact_path("test-ready"),
            health_file: unique_runtime_artifact_path("test-health"),
            ready_poll_interval: Duration::from_millis(1),
        };

        assert!(!stop_requested.load(Ordering::SeqCst));
        handle.request_stop().expect("request stop");
        assert!(stop_requested.load(Ordering::SeqCst));

        let report = Box::new(handle).join().expect("join report");
        assert_eq!(report.lifecycle, LinkLifecycle::Failed);
        assert_eq!(report.endpoints, endpoints);
        let report_json = serde_json::to_value(&report).expect("report json");
        assert!(report_json
            .get("backend")
            .and_then(|backend| backend.get("userspace_radio"))
            .is_some());
        let LinkBackendReport::UserspaceRadio(runtime_report) = report.backend else {
            panic!("expected userspace radio runtime report");
        };
        assert_eq!(runtime_report.stop_reason, "not_started");
        assert_eq!(
            runtime_report.error.as_ref().map(|error| error.code),
            Some("test_stop")
        );
    }
}
