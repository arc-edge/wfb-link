//! Product-facing WFB link facade.
//!
//! This crate keeps the product boundary at link lifecycle and local
//! stream/tunnel endpoints. Platform backends own the radio-specific path:
//! macOS embeds this repository's userspace RTL8812AU runtime, while Linux is
//! expected to use the native monitor-mode WFB stack.

use std::{
    fs,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
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
    service_runtime_inputs_from_resolved, ServiceCli,
};

static NEXT_ARTIFACT_ID: AtomicU64 = AtomicU64::new(0);

pub type Result<T> = std::result::Result<T, LinkError>;

#[derive(Debug, Error)]
pub enum LinkError {
    #[error("{code}: {message}")]
    Runtime { code: &'static str, message: String },
    #[error("unsupported backend config: {0}")]
    UnsupportedBackend(&'static str),
    #[error("backend exited before ready")]
    BackendExitedBeforeReady,
    #[error("timeout waiting for ready marker after {0:?}")]
    ReadyTimeout(Duration),
    #[error("failed to join backend thread")]
    JoinFailed,
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
    pub fn macos_userspace_radio(config: MacosUserspaceRadioConfig) -> Self {
        Self {
            backend: LinkBackendConfig::MacosUserspaceRadio(config),
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
    MacosUserspaceRadio(MacosUserspaceRadioConfig),
    LinuxNativeWfb(LinuxNativeWfbConfig),
}

#[derive(Debug, Clone)]
pub struct MacosUserspaceRadioConfig {
    pub runtime_config: ProductionRuntimeFlowConfig,
    pub execution_inputs: ProductionRuntimeFlowExecutionInputs,
    pub endpoints: LinkEndpoints,
    pub ready_poll_interval: Duration,
}

impl MacosUserspaceRadioConfig {
    pub fn from_service_config_path(config: impl AsRef<Path>) -> Result<Self> {
        let cli = ServiceCli::config_only(config.as_ref().to_path_buf());
        let resolved = resolve_service_run(&cli)?;
        let runtime_config = service_runtime_config_from_resolved(&resolved)?;
        let execution_inputs =
            service_runtime_inputs_from_resolved(&resolved, runtime_config.channel)?;
        Ok(Self::from_runtime_parts(runtime_config, execution_inputs))
    }

    pub fn from_runtime_parts(
        runtime_config: ProductionRuntimeFlowConfig,
        mut execution_inputs: ProductionRuntimeFlowExecutionInputs,
    ) -> Self {
        execution_inputs.process_signal_stop = false;
        execution_inputs.external_stop_requested = None;
        let endpoints = macos_userspace_radio_endpoints(&runtime_config);
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

#[derive(Debug, Clone)]
pub struct LinuxNativeWfbConfig {
    pub interface_name: String,
    pub channel: u8,
    pub bandwidth_mhz: u16,
    pub key_path: Option<PathBuf>,
    pub endpoints: LinkEndpoints,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkStreamEndpoint {
    pub name: String,
    pub direction: LinkDirection,
    pub local_udp: SocketAddr,
    pub payload_kind: PayloadKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<WfbStreamId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WfbStreamId {
    pub link_id: Option<u32>,
    pub radio_port: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkReport {
    pub lifecycle: LinkLifecycle,
    pub endpoints: LinkEndpoints,
    pub backend: LinkBackendReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkBackendReport {
    MacosUserspaceRadio(ProductionRuntimeFlowReport),
    LinuxNativeWfb(Value),
}

#[derive(Debug, Default)]
pub struct MacosUserspaceRadioBackend;

impl LinkBackend for MacosUserspaceRadioBackend {
    fn start(&mut self, config: LinkConfig) -> Result<Box<dyn LinkHandle>> {
        let LinkBackendConfig::MacosUserspaceRadio(config) = config.backend else {
            return Err(LinkError::UnsupportedBackend(
                "expected macos_userspace_radio",
            ));
        };
        let handle = MacosUserspaceRadioHandle::start(config)?;
        Ok(Box::new(handle))
    }
}

impl MacosUserspaceRadioHandle {
    fn start(config: MacosUserspaceRadioConfig) -> Result<Self> {
        let MacosUserspaceRadioConfig {
            mut runtime_config,
            mut execution_inputs,
            endpoints,
            ready_poll_interval,
        } = config;

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
            stop_requested,
            join_handle,
            ready_file,
            health_file,
            ready_poll_interval,
        })
    }
}

#[derive(Debug)]
pub struct MacosUserspaceRadioHandle {
    endpoints: LinkEndpoints,
    stop_requested: Arc<AtomicBool>,
    join_handle: JoinHandle<ProductionRuntimeFlowReport>,
    ready_file: PathBuf,
    health_file: PathBuf,
    ready_poll_interval: Duration,
}

impl LinkHandle for MacosUserspaceRadioHandle {
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
            return Ok(LinkHealth {
                lifecycle: link_lifecycle_from_health_json(&health),
                ready: matches!(
                    link_lifecycle_from_health_json(&health),
                    LinkLifecycle::Ready | LinkLifecycle::Degraded | LinkLifecycle::Stopped
                ) || self.ready_file.exists(),
                endpoints: self.endpoints.clone(),
                tx: link_tx_health_from_json(health.get("tx")),
                rx: link_rx_health_from_json(health.get("rx")),
                backend: health,
            });
        }

        if self.ready_file.exists() {
            let ready = read_json_file(&self.ready_file)?;
            return Ok(LinkHealth {
                lifecycle: LinkLifecycle::Ready,
                ready: true,
                endpoints: self.endpoints.clone(),
                tx: LinkTxHealth::default(),
                rx: LinkRxHealth::default(),
                backend: ready,
            });
        }

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
            backend: Value::Null,
        })
    }

    fn request_stop(&self) -> Result<()> {
        self.stop_requested.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn join(self: Box<Self>) -> Result<LinkReport> {
        let MacosUserspaceRadioHandle {
            endpoints,
            join_handle,
            ..
        } = *self;
        let report = join_handle.join().map_err(|_| LinkError::JoinFailed)?;
        let lifecycle = match report.result {
            ProductionRuntimeFlowResult::Pass => LinkLifecycle::Stopped,
            ProductionRuntimeFlowResult::Fail => LinkLifecycle::Failed,
        };
        Ok(LinkReport {
            lifecycle,
            endpoints,
            backend: LinkBackendReport::MacosUserspaceRadio(report),
        })
    }
}

pub fn macos_userspace_radio_endpoints(config: &ProductionRuntimeFlowConfig) -> LinkEndpoints {
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
        let endpoints = macos_userspace_radio_endpoints(&fixture_runtime_config());

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
    fn macos_config_from_runtime_parts_disables_process_signal_stop() {
        let mut inputs = ProductionRuntimeFlowExecutionInputs::default();
        inputs.process_signal_stop = true;
        inputs.external_stop_requested = Some(Arc::new(AtomicBool::new(false)));

        let config =
            MacosUserspaceRadioConfig::from_runtime_parts(fixture_runtime_config(), inputs);

        assert!(!config.execution_inputs.process_signal_stop);
        assert!(config.execution_inputs.external_stop_requested.is_none());
    }

    #[test]
    fn macos_handle_request_stop_sets_cooperative_flag_and_join_reports() {
        let runtime_config = fixture_runtime_config();
        let endpoints = macos_userspace_radio_endpoints(&runtime_config);
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
        let handle = MacosUserspaceRadioHandle {
            endpoints: endpoints.clone(),
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
        let LinkBackendReport::MacosUserspaceRadio(runtime_report) = report.backend else {
            panic!("expected macOS runtime report");
        };
        assert_eq!(runtime_report.stop_reason, "not_started");
        assert_eq!(
            runtime_report.error.as_ref().map(|error| error.code),
            Some("test_stop")
        );
    }
}
