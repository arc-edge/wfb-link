//! Runtime-facing policy for the native WFB radio backend.
//!
//! This crate owns stable decisions and live transport abstractions that a
//! production runtime, diagnostic harness, or future daemon must agree on
//! without depending on `wfb-radio-diag`.

use std::{
    cell::RefCell,
    error::Error,
    fmt, fs, io,
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use radio_core::{
    build_tx_packet, frame_type, list_usb_devices, parse_rx_packet,
    rtl8812au::{Rtl8812auUsbTransport, TxQueue, TX_DESC_SIZE},
    submit_tx_frame, Band, Bandwidth, Channel, ClaimedUsbDevice, DeviceSelector, EndpointInfo,
    FirmwareImage, FrameType, InterfaceInfo, ParsedRxPacket, RealtekTableActionKind,
    RealtekTableKind, RealtekTablePlan, Rtl8812auRegisterAccess, Rtl8812auRegisterError,
    Rtl8812auTxSubmitError, RxFrame, RxParseOutcome, TxOptions, TxRate, TxSubmitCounters,
    UsbBulkTransfer, UsbDeviceInfo, UsbEndpoints, UsbError,
};
use serde::Serialize;
use wfb_bridge::{
    build_rx_forward_datagram, parse_tx_datagram, RadiotapError, RxCounters, RxForwardConfig,
    TxCounters, TxDatagramError, WfbChannelId,
};

#[cfg(target_os = "macos")]
pub mod macos_usbhost;

pub mod led_heartbeat;
pub use led_heartbeat::{
    LedHeartbeat, LedHeartbeatConfig, LedHeartbeatConfigError, LedHeartbeatCounters,
    DEFAULT_HEARTBEAT_HALF_PERIOD_MS, MAX_HEARTBEAT_HALF_PERIOD_MS, MIN_HEARTBEAT_HALF_PERIOD_MS,
};

mod tx_power;
pub use tx_power::{
    plan_rtl8812au_efuse_tx_power, rtl8812au_tx_power_agc_registers, rtl8812au_tx_power_agc_value,
    run_rtl8812au_efuse_tx_power, run_rtl8812au_manual_tx_power, Rtl8812auTxPowerAgcRegister,
    Rtl8812auTxPowerChannelGroupReport, Rtl8812auTxPowerControlMode, Rtl8812auTxPowerControlReport,
    Rtl8812auTxPowerDerivedLaneReport, Rtl8812auTxPowerDerivedWriteReport,
    Rtl8812auTxPowerEfusePlanReport, Rtl8812auTxPowerEfuseSourceReport,
    Rtl8812auTxPowerSafetyProfile,
};

pub const PRODUCTION_TX_SOCKET_RCVBUF_BYTES: usize = 4 * 1024 * 1024;
pub const PRODUCTION_TX_RECEIVE_TIMEOUT: Duration = Duration::from_millis(100);
const WFB_OBSERVED_HEADER_LEN: usize = 24;
const WFB_OBSERVED_MAC_PREFIX: [u8; 2] = [0x57, 0x42];
const WFB_OBSERVED_SRC_MAC_PREFIX_OFFSET: usize = 10;
const WFB_OBSERVED_SRC_CHANNEL_ID_OFFSET: usize = 12;
const WFB_OBSERVED_DST_MAC_PREFIX_OFFSET: usize = 16;
const WFB_OBSERVED_DST_CHANNEL_ID_OFFSET: usize = 18;
const WFB_MAX_CHANNEL_OBSERVATIONS: usize = 32;
const PRODUCTION_RX_STARTUP_KICK_FRAME: [u8; 24] = [
    0x48, 0x00, // data null, no flags
    0x00, 0x00, // duration
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // addr1: broadcast
    0x02, 0x00, 0x5e, 0x00, 0x00, 0x01, // addr2: local non-WFB source
    0x02, 0x00, 0x5e, 0x00, 0x00, 0x01, // addr3: local non-WFB BSSID
    0x00, 0x00, // seq control
];

pub enum RuntimeUsbTransport {
    Libusb(Box<ClaimedUsbDevice>),
    #[cfg(target_os = "macos")]
    Macos(macos_usbhost::MacosUsbHostSession),
}

impl Rtl8812auUsbTransport for RuntimeUsbTransport {
    fn read_vendor(
        &self,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> std::result::Result<usize, UsbError> {
        match self {
            RuntimeUsbTransport::Libusb(claimed) => {
                claimed.as_ref().read_vendor(value, index, data, timeout)
            }
            #[cfg(target_os = "macos")]
            RuntimeUsbTransport::Macos(session) => session.read_vendor(value, index, data, timeout),
        }
    }

    fn write_vendor(
        &self,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> std::result::Result<usize, UsbError> {
        match self {
            RuntimeUsbTransport::Libusb(claimed) => {
                claimed.as_ref().write_vendor(value, index, data, timeout)
            }
            #[cfg(target_os = "macos")]
            RuntimeUsbTransport::Macos(session) => {
                session.write_vendor(value, index, data, timeout)
            }
        }
    }
}

impl Rtl8812auUsbTransport for &RuntimeUsbTransport {
    fn read_vendor(
        &self,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> std::result::Result<usize, UsbError> {
        <RuntimeUsbTransport as Rtl8812auUsbTransport>::read_vendor(
            *self, value, index, data, timeout,
        )
    }

    fn write_vendor(
        &self,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> std::result::Result<usize, UsbError> {
        <RuntimeUsbTransport as Rtl8812auUsbTransport>::write_vendor(
            *self, value, index, data, timeout,
        )
    }
}

impl UsbBulkTransfer for RuntimeUsbTransport {
    fn read_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &mut [u8],
        timeout: Duration,
    ) -> std::result::Result<usize, UsbError> {
        match self {
            RuntimeUsbTransport::Libusb(claimed) => {
                claimed.as_mut().read_bulk_transfer(endpoint, data, timeout)
            }
            #[cfg(target_os = "macos")]
            RuntimeUsbTransport::Macos(session) => {
                session.read_bulk_transfer(endpoint, data, timeout)
            }
        }
    }

    fn write_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: Duration,
    ) -> std::result::Result<usize, UsbError> {
        match self {
            RuntimeUsbTransport::Libusb(claimed) => claimed
                .as_mut()
                .write_bulk_transfer(endpoint, data, timeout),
            #[cfg(target_os = "macos")]
            RuntimeUsbTransport::Macos(session) => {
                session.write_bulk_transfer(endpoint, data, timeout)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacosUsbHostConfig {
    pub configuration_value: u8,
    pub interface_number: u8,
    pub bulk_in_endpoint: u8,
    pub bulk_out_endpoint: u8,
    pub bulk_out_endpoint_count: usize,
    pub poll_attempts: u32,
    pub poll_delay: Duration,
}

impl Default for MacosUsbHostConfig {
    fn default() -> Self {
        Self {
            configuration_value: 1,
            interface_number: 0,
            bulk_in_endpoint: 0x81,
            bulk_out_endpoint: 0x02,
            bulk_out_endpoint_count: 3,
            poll_attempts: 25,
            poll_delay: Duration::from_millis(100),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTransportError {
    pub code: &'static str,
    pub message: String,
}

impl RuntimeTransportError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for RuntimeTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Error for RuntimeTransportError {}

pub struct RuntimeUsbTransportOpen {
    pub transport: RuntimeUsbTransport,
    pub adapter: UsbDeviceInfo,
    pub endpoints: UsbEndpoints,
    pub initial_usb_control_writes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeUsbBackend {
    Libusb,
    MacosUsbHost(MacosUsbHostConfig),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeUsbOpenConfig {
    pub selector: DeviceSelector,
    pub backend: RuntimeUsbBackend,
}

impl RuntimeUsbOpenConfig {
    pub fn libusb(selector: DeviceSelector) -> Self {
        Self {
            selector,
            backend: RuntimeUsbBackend::Libusb,
        }
    }

    pub fn macos_usbhost(selector: DeviceSelector, config: MacosUsbHostConfig) -> Self {
        Self {
            selector,
            backend: RuntimeUsbBackend::MacosUsbHost(config),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeUsbBackend {
    Libusb,
    MacosUsbHost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionMacosUsbHostConfig {
    pub configuration_value: u8,
    pub interface_number: u8,
    pub bulk_in_endpoint: u8,
    pub bulk_out_endpoint: u8,
    pub bulk_out_endpoint_count: usize,
    pub poll_attempts: u32,
    pub poll_delay_ms: u64,
}

impl From<MacosUsbHostConfig> for ProductionMacosUsbHostConfig {
    fn from(config: MacosUsbHostConfig) -> Self {
        Self {
            configuration_value: config.configuration_value,
            interface_number: config.interface_number,
            bulk_in_endpoint: config.bulk_in_endpoint,
            bulk_out_endpoint: config.bulk_out_endpoint,
            bulk_out_endpoint_count: config.bulk_out_endpoint_count,
            poll_attempts: config.poll_attempts,
            poll_delay_ms: u64::try_from(config.poll_delay.as_millis()).unwrap_or(u64::MAX),
        }
    }
}

impl From<ProductionMacosUsbHostConfig> for MacosUsbHostConfig {
    fn from(config: ProductionMacosUsbHostConfig) -> Self {
        Self {
            configuration_value: config.configuration_value,
            interface_number: config.interface_number,
            bulk_in_endpoint: config.bulk_in_endpoint,
            bulk_out_endpoint: config.bulk_out_endpoint,
            bulk_out_endpoint_count: config.bulk_out_endpoint_count,
            poll_attempts: config.poll_attempts,
            poll_delay: Duration::from_millis(config.poll_delay_ms),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeUsbConfig {
    pub selector: DeviceSelector,
    pub backend: ProductionRuntimeUsbBackend,
    pub macos_usbhost: Option<ProductionMacosUsbHostConfig>,
}

impl ProductionRuntimeUsbConfig {
    pub fn libusb(selector: DeviceSelector) -> Self {
        Self {
            selector,
            backend: ProductionRuntimeUsbBackend::Libusb,
            macos_usbhost: None,
        }
    }

    pub fn macos_usbhost(selector: DeviceSelector, config: MacosUsbHostConfig) -> Self {
        Self {
            selector,
            backend: ProductionRuntimeUsbBackend::MacosUsbHost,
            macos_usbhost: Some(config.into()),
        }
    }

    pub fn to_runtime_open_config(self) -> RuntimeUsbOpenConfig {
        match self.backend {
            ProductionRuntimeUsbBackend::Libusb => RuntimeUsbOpenConfig::libusb(self.selector),
            ProductionRuntimeUsbBackend::MacosUsbHost => RuntimeUsbOpenConfig::macos_usbhost(
                self.selector,
                self.macos_usbhost
                    .map(MacosUsbHostConfig::from)
                    .unwrap_or_default(),
            ),
        }
    }
}

pub fn select_libusb_supported_adapter(
    selector: DeviceSelector,
) -> Result<UsbDeviceInfo, RuntimeTransportError> {
    let devices = list_usb_devices(false)
        .map_err(|error| RuntimeTransportError::new("usb_list_failed", error.to_string()))?;
    devices
        .into_iter()
        .find(|device| selector.matches(device))
        .ok_or_else(|| {
            RuntimeTransportError::new(
                "no_supported_adapter",
                if selector.is_empty() {
                    "no supported RTL8812AU adapter found"
                } else {
                    "no supported RTL8812AU adapter matched selector"
                },
            )
        })
}

pub fn open_libusb_transport(
    selector: DeviceSelector,
) -> Result<RuntimeUsbTransportOpen, RuntimeTransportError> {
    let selected = select_libusb_supported_adapter(selector)?;
    let claimed = radio_core::usb::claim_usb_device(&selected)
        .map_err(|error| RuntimeTransportError::new("usb_claim_failed", error.to_string()))?;
    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    Ok(RuntimeUsbTransportOpen {
        transport: RuntimeUsbTransport::Libusb(Box::new(claimed)),
        adapter,
        endpoints,
        initial_usb_control_writes: 0,
    })
}

pub fn open_runtime_usb_transport(
    config: RuntimeUsbOpenConfig,
) -> Result<RuntimeUsbTransportOpen, RuntimeTransportError> {
    match config.backend {
        RuntimeUsbBackend::Libusb => open_libusb_transport(config.selector),
        RuntimeUsbBackend::MacosUsbHost(macos_config) => {
            open_macos_usbhost_transport(&macos_config, config.selector)
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRadioCounters {
    pub usb_control_reads: u64,
    pub usb_control_writes: u64,
    pub usb_bulk_in_reads: u64,
    pub usb_bulk_out_writes: u64,
    pub rx_frames: u64,
    pub tx_frames: u64,
    pub dropped_frames: u64,
}

impl RuntimeRadioCounters {
    pub fn saturating_sub(self, before: Self) -> Self {
        Self {
            usb_control_reads: self
                .usb_control_reads
                .saturating_sub(before.usb_control_reads),
            usb_control_writes: self
                .usb_control_writes
                .saturating_sub(before.usb_control_writes),
            usb_bulk_in_reads: self
                .usb_bulk_in_reads
                .saturating_sub(before.usb_bulk_in_reads),
            usb_bulk_out_writes: self
                .usb_bulk_out_writes
                .saturating_sub(before.usb_bulk_out_writes),
            rx_frames: self.rx_frames.saturating_sub(before.rx_frames),
            tx_frames: self.tx_frames.saturating_sub(before.tx_frames),
            dropped_frames: self.dropped_frames.saturating_sub(before.dropped_frames),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRadioError {
    pub code: &'static str,
    pub message: String,
    pub timeout: bool,
}

impl RuntimeRadioError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            timeout: false,
        }
    }

    fn new_timeout(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            timeout: true,
        }
    }

    fn register_read(
        register_name: &'static str,
        phase: &'static str,
        error: Rtl8812auRegisterError,
    ) -> Self {
        Self::new(
            "register_read_failed",
            format!("{register_name} {phase} read failed: {error}"),
        )
    }

    fn register_write(
        register_name: &'static str,
        phase: &'static str,
        error: Rtl8812auRegisterError,
    ) -> Self {
        Self::new(
            "register_write_failed",
            format!("{register_name} {phase} write failed: {error}"),
        )
    }
}

impl fmt::Display for RuntimeRadioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Error for RuntimeRadioError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeFlowErrorReport {
    pub code: &'static str,
    pub message: String,
    pub timeout: bool,
}

impl From<RuntimeRadioError> for ProductionRuntimeFlowErrorReport {
    fn from(error: RuntimeRadioError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            timeout: error.timeout,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRxRead {
    pub endpoint: u8,
    pub bytes_read: usize,
    pub packets: Vec<ParsedRxPacket>,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeFlowRxTelemetry {
    pub buffers_read: u64,
    pub read_timeouts: u64,
    pub parsed_frames: u64,
    pub phy_status_frames: u64,
    pub rssi_valid_frames: u64,
    pub snr_frames: u64,
    pub noise_frames: u64,
    pub signal: RuntimeRxSignalSummary,
    pub forwarded_payloads: u64,
    pub rx_forwards: Vec<ProductionRuntimeRxForwardSnapshot>,
    pub dropped_packets: u64,
    pub need_more_data: u64,
    pub management_frames: u64,
    pub control_frames: u64,
    pub data_frames: u64,
    pub extension_frames: u64,
    pub wfb_channel_observations: Vec<RuntimeWfbChannelObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWfbChannelObservation {
    pub source_channel_id: u32,
    pub source_channel_id_hex: String,
    pub source_link_id: u32,
    pub source_radio_port: u8,
    pub destination_channel_id: u32,
    pub destination_channel_id_hex: String,
    pub destination_link_id: u32,
    pub destination_radio_port: u8,
    pub count: u64,
}

fn format_wfb_observed_channel_id(raw: u32) -> String {
    format!("0x{raw:08x}")
}

fn split_wfb_observed_channel_id(raw: u32) -> (u32, u8) {
    (raw >> 8, (raw & 0xff) as u8)
}

fn observe_runtime_wfb_channel_id(
    observations: &mut Vec<RuntimeWfbChannelObservation>,
    frame: &[u8],
) {
    if frame.len() < WFB_OBSERVED_HEADER_LEN {
        return;
    }
    if frame[WFB_OBSERVED_SRC_MAC_PREFIX_OFFSET..WFB_OBSERVED_SRC_MAC_PREFIX_OFFSET + 2]
        != WFB_OBSERVED_MAC_PREFIX
        || frame[WFB_OBSERVED_DST_MAC_PREFIX_OFFSET..WFB_OBSERVED_DST_MAC_PREFIX_OFFSET + 2]
            != WFB_OBSERVED_MAC_PREFIX
    {
        return;
    }

    let source_channel_id = u32::from_be_bytes(
        frame[WFB_OBSERVED_SRC_CHANNEL_ID_OFFSET..WFB_OBSERVED_SRC_CHANNEL_ID_OFFSET + 4]
            .try_into()
            .expect("slice length checked by frame length"),
    );
    let destination_channel_id = u32::from_be_bytes(
        frame[WFB_OBSERVED_DST_CHANNEL_ID_OFFSET..WFB_OBSERVED_DST_CHANNEL_ID_OFFSET + 4]
            .try_into()
            .expect("slice length checked by frame length"),
    );

    if let Some(existing) = observations.iter_mut().find(|observation| {
        observation.source_channel_id == source_channel_id
            && observation.destination_channel_id == destination_channel_id
    }) {
        existing.count = existing.count.saturating_add(1);
        return;
    }
    if observations.len() >= WFB_MAX_CHANNEL_OBSERVATIONS {
        return;
    }

    let (source_link_id, source_radio_port) = split_wfb_observed_channel_id(source_channel_id);
    let (destination_link_id, destination_radio_port) =
        split_wfb_observed_channel_id(destination_channel_id);
    observations.push(RuntimeWfbChannelObservation {
        source_channel_id,
        source_channel_id_hex: format_wfb_observed_channel_id(source_channel_id),
        source_link_id,
        source_radio_port,
        destination_channel_id,
        destination_channel_id_hex: format_wfb_observed_channel_id(destination_channel_id),
        destination_link_id,
        destination_radio_port,
        count: 1,
    });
}

fn merge_runtime_wfb_channel_observations(
    target: &mut Vec<RuntimeWfbChannelObservation>,
    source: Vec<RuntimeWfbChannelObservation>,
) {
    for observation in source {
        if let Some(existing) = target.iter_mut().find(|existing| {
            existing.source_channel_id == observation.source_channel_id
                && existing.destination_channel_id == observation.destination_channel_id
        }) {
            existing.count = existing.count.saturating_add(observation.count);
            continue;
        }
        if target.len() < WFB_MAX_CHANNEL_OBSERVATIONS {
            target.push(observation);
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRxSignalMetric {
    pub sample_count: u64,
    pub min: Option<i8>,
    pub max: Option<i8>,
    pub average: Option<i64>,
    #[serde(skip)]
    sum: i64,
}

impl RuntimeRxSignalMetric {
    pub fn observe(&mut self, value: i8) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.sum = self.sum.saturating_add(i64::from(value));
        self.min = Some(self.min.map_or(value, |current| current.min(value)));
        self.max = Some(self.max.map_or(value, |current| current.max(value)));
        self.average = Some(rounded_average_i64(self.sum, self.sample_count));
    }

    pub fn merge(&mut self, other: &Self) {
        if other.sample_count == 0 {
            return;
        }
        self.sample_count = self.sample_count.saturating_add(other.sample_count);
        self.sum = self.sum.saturating_add(other.sum);
        self.min = match (self.min, other.min) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (None, Some(right)) => Some(right),
            (left, None) => left,
        };
        self.max = match (self.max, other.max) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (None, Some(right)) => Some(right),
            (left, None) => left,
        };
        self.average = Some(rounded_average_i64(self.sum, self.sample_count));
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRxSignalSummary {
    pub rssi_dbm: RuntimeRxSignalMetric,
    pub snr_db: RuntimeRxSignalMetric,
    pub noise_dbm: RuntimeRxSignalMetric,
}

impl RuntimeRxSignalSummary {
    pub fn observe_frame(&mut self, frame: &RxFrame) {
        if frame.rssi_dbm_valid {
            self.rssi_dbm.observe(frame.rssi_dbm);
        }
        if let Some(snr_db) = frame.snr_db {
            self.snr_db.observe(snr_db);
        }
        if let Some(noise_dbm) = frame.noise_dbm {
            self.noise_dbm.observe(noise_dbm);
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.rssi_dbm.merge(&other.rssi_dbm);
        self.snr_db.merge(&other.snr_db);
        self.noise_dbm.merge(&other.noise_dbm);
    }
}

fn rounded_average_i64(sum: i64, count: u64) -> i64 {
    let count = i64::try_from(count).unwrap_or(i64::MAX).max(1);
    if sum >= 0 {
        (sum + count / 2) / count
    } else {
        -((-sum + count / 2) / count)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeFlowTxTelemetry {
    pub datagrams_received: u64,
    pub submitted_frames: u64,
    pub failed_submissions: u64,
    pub dropped_datagrams: u64,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeRxForwardConfig {
    pub link_id: Option<u32>,
    pub radio_port: u8,
    pub aggregator: Option<SocketAddr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimePrimaryRxForwardConfig {
    pub link_id: Option<u32>,
    pub radio_port: Option<u8>,
    pub aggregator: Option<SocketAddr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeAirtimeMode {
    Continuous,
    Tdd,
}

impl Default for ProductionRuntimeAirtimeMode {
    fn default() -> Self {
        Self::Continuous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeTddWindow {
    Rx,
    Tx,
}

impl Default for ProductionRuntimeTddWindow {
    fn default() -> Self {
        Self::Rx
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeAirtimeSchedule {
    pub mode: ProductionRuntimeAirtimeMode,
    pub tdd_first_window: ProductionRuntimeTddWindow,
    pub tdd_rx_window_ms: u64,
    pub tdd_tx_window_ms: u64,
    pub tdd_guard_ms: u64,
    pub tdd_start_delay_ms: u64,
}

impl Default for ProductionRuntimeAirtimeSchedule {
    fn default() -> Self {
        Self {
            mode: ProductionRuntimeAirtimeMode::Continuous,
            tdd_first_window: ProductionRuntimeTddWindow::Rx,
            tdd_rx_window_ms: 1_000,
            tdd_tx_window_ms: 1_000,
            tdd_guard_ms: 0,
            tdd_start_delay_ms: 0,
        }
    }
}

impl ProductionRuntimeAirtimeSchedule {
    pub fn continuous() -> Self {
        Self::default()
    }

    pub fn tdd(
        tdd_first_window: ProductionRuntimeTddWindow,
        tdd_rx_window_ms: u64,
        tdd_tx_window_ms: u64,
        tdd_guard_ms: u64,
        tdd_start_delay_ms: u64,
    ) -> Self {
        Self {
            mode: ProductionRuntimeAirtimeMode::Tdd,
            tdd_first_window,
            tdd_rx_window_ms,
            tdd_tx_window_ms,
            tdd_guard_ms,
            tdd_start_delay_ms,
        }
    }

    pub fn validate(self) -> Result<(), RuntimeRadioError> {
        match self.mode {
            ProductionRuntimeAirtimeMode::Continuous => Ok(()),
            ProductionRuntimeAirtimeMode::Tdd => {
                if self.tdd_rx_window_ms == 0 {
                    return Err(RuntimeRadioError::new(
                        "invalid_airtime_tdd_rx_window",
                        "TDD airtime schedule requires tdd_rx_window_ms greater than zero",
                    ));
                }
                if self.tdd_tx_window_ms == 0 {
                    return Err(RuntimeRadioError::new(
                        "invalid_airtime_tdd_tx_window",
                        "TDD airtime schedule requires tdd_tx_window_ms greater than zero",
                    ));
                }
                Ok(())
            }
        }
    }

    pub fn tx_allowed_at_elapsed(self, elapsed: Duration) -> bool {
        match self.mode {
            ProductionRuntimeAirtimeMode::Continuous => true,
            ProductionRuntimeAirtimeMode::Tdd => self.tdd_tx_allowed_at_elapsed(elapsed),
        }
    }

    fn tdd_tx_allowed_at_elapsed(self, elapsed: Duration) -> bool {
        let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        if elapsed_ms < self.tdd_start_delay_ms {
            return false;
        }
        let rx = self.tdd_rx_window_ms;
        let tx = self.tdd_tx_window_ms;
        let guard = self.tdd_guard_ms;
        let cycle = rx
            .saturating_add(tx)
            .saturating_add(guard.saturating_mul(2));
        if cycle == 0 {
            return false;
        }
        let offset = elapsed_ms.saturating_sub(self.tdd_start_delay_ms) % cycle;
        match self.tdd_first_window {
            ProductionRuntimeTddWindow::Rx => {
                let tx_start = rx.saturating_add(guard);
                offset >= tx_start && offset < tx_start.saturating_add(tx)
            }
            ProductionRuntimeTddWindow::Tx => offset < tx,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeAirtimeReport {
    pub schedule: ProductionRuntimeAirtimeSchedule,
    pub tx_allowed_iterations: u64,
    pub tx_gated_iterations: u64,
}

impl ProductionRuntimeAirtimeReport {
    fn idle(schedule: ProductionRuntimeAirtimeSchedule) -> Self {
        Self {
            schedule,
            tx_allowed_iterations: 0,
            tx_gated_iterations: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeRxForwardPlan {
    pub config: RxForwardConfig,
    pub aggregator: Option<SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeWfbLoopConfig {
    pub bind_addr: SocketAddr,
    pub tx_binds: Vec<SocketAddr>,
    pub rx_timeout_ms: u64,
    pub tx_burst_limit: u32,
    pub max_datagrams: u32,
    pub airtime_schedule: ProductionRuntimeAirtimeSchedule,
    pub bandwidth: Bandwidth,
    pub primary_rx_forward: ProductionRuntimePrimaryRxForwardConfig,
    pub rx_forwards: Vec<ProductionRuntimeRxForwardConfig>,
    pub rx_wlan_idx: u8,
    pub rx_mcs_index: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeWfbLoopPlan {
    pub tx_bind_addrs: Vec<SocketAddr>,
    pub rx_forwards: Vec<ProductionRuntimeRxForwardPlan>,
    pub rx_timeout_ms: u64,
    pub tx_burst_limit: u32,
    pub max_datagrams: u32,
    pub airtime_schedule: ProductionRuntimeAirtimeSchedule,
    pub rx_wlan_idx: u8,
    pub rx_mcs_index: u8,
    pub bandwidth_mhz: u8,
}

pub fn plan_production_wfb_loop(
    config: &ProductionRuntimeWfbLoopConfig,
) -> Result<ProductionRuntimeWfbLoopPlan, RuntimeRadioError> {
    if config.rx_timeout_ms == 0 {
        return Err(RuntimeRadioError::new(
            "invalid_rx_timeout",
            "production WFB loop requires rx_timeout_ms greater than zero",
        ));
    }
    if config.tx_burst_limit == 0 {
        return Err(RuntimeRadioError::new(
            "invalid_tx_burst_limit",
            "production WFB loop requires tx_burst_limit greater than zero",
        ));
    }
    config.airtime_schedule.validate()?;

    let mut tx_bind_addrs = Vec::with_capacity(config.tx_binds.len() + 1);
    tx_bind_addrs.push(config.bind_addr);
    tx_bind_addrs.extend(config.tx_binds.iter().copied());

    let mut rx_forwards = Vec::with_capacity(config.rx_forwards.len() + 1);
    match (
        config.primary_rx_forward.link_id,
        config.primary_rx_forward.radio_port,
        config.primary_rx_forward.aggregator,
    ) {
        (None, None, None) => {}
        (None, None, Some(_)) => {
            return Err(RuntimeRadioError::new(
                "missing_wfb_rx_filter",
                "production RX aggregator requires WFB link ID and radio port",
            ));
        }
        (Some(_), None, _) | (None, Some(_), _) => {
            return Err(RuntimeRadioError::new(
                "incomplete_wfb_rx_filter",
                "production RX forwarding requires WFB link ID and radio port together",
            ));
        }
        (Some(link_id), Some(radio_port), aggregator) => {
            let channel_id = WfbChannelId::new(link_id, radio_port).map_err(|error| {
                RuntimeRadioError::new("invalid_wfb_rx_channel_id", error.to_string())
            })?;
            rx_forwards.push(ProductionRuntimeRxForwardPlan {
                config: RxForwardConfig {
                    channel_id,
                    wlan_idx: config.rx_wlan_idx,
                    mcs_index: config.rx_mcs_index,
                    bandwidth_mhz: config.bandwidth.mhz() as u8,
                },
                aggregator,
            });
        }
    }
    for forward in &config.rx_forwards {
        let link_id = forward.link_id.ok_or_else(|| {
            RuntimeRadioError::new(
                "missing_wfb_rx_forward_link_id",
                "production RX forward target requires a WFB link ID",
            )
        })?;
        let channel_id = WfbChannelId::new(link_id, forward.radio_port).map_err(|error| {
            RuntimeRadioError::new("invalid_wfb_rx_channel_id", error.to_string())
        })?;
        rx_forwards.push(ProductionRuntimeRxForwardPlan {
            config: RxForwardConfig {
                channel_id,
                wlan_idx: config.rx_wlan_idx,
                mcs_index: config.rx_mcs_index,
                bandwidth_mhz: config.bandwidth.mhz() as u8,
            },
            aggregator: forward.aggregator,
        });
    }

    Ok(ProductionRuntimeWfbLoopPlan {
        tx_bind_addrs,
        rx_forwards,
        rx_timeout_ms: config.rx_timeout_ms,
        tx_burst_limit: config.tx_burst_limit,
        max_datagrams: config.max_datagrams,
        airtime_schedule: config.airtime_schedule,
        rx_wlan_idx: config.rx_wlan_idx,
        rx_mcs_index: config.rx_mcs_index,
        bandwidth_mhz: config.bandwidth.mhz() as u8,
    })
}

#[derive(Debug)]
pub struct ProductionRuntimeRxForwardRuntime {
    pub config: RxForwardConfig,
    pub aggregator: Option<SocketAddr>,
    socket: Option<UdpSocket>,
    pub forwarded_bytes: u64,
    pub counters: RxCounters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeRxForwardSnapshot {
    pub config: RxForwardConfig,
    pub aggregator: Option<SocketAddr>,
    pub forwarded_bytes: u64,
    pub counters: RxCounters,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeRxPacketTelemetry {
    pub parsed_frames: u64,
    pub phy_status_frames: u64,
    pub rssi_valid_frames: u64,
    pub snr_frames: u64,
    pub noise_frames: u64,
    pub signal: RuntimeRxSignalSummary,
    pub dropped_packets: u64,
    pub need_more_data: u64,
    pub management_frames: u64,
    pub control_frames: u64,
    pub data_frames: u64,
    pub extension_frames: u64,
    pub wfb_channel_observations: Vec<RuntimeWfbChannelObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeRxPacketOutcome {
    pub telemetry: ProductionRuntimeRxPacketTelemetry,
    pub rx_forwards: Vec<ProductionRuntimeRxForwardSnapshot>,
}

pub fn create_production_rx_forward_runtimes(
    plans: &[ProductionRuntimeRxForwardPlan],
) -> Result<Vec<ProductionRuntimeRxForwardRuntime>, RuntimeRadioError> {
    plans
        .iter()
        .map(|plan| {
            let socket = match plan.aggregator {
                Some(_) => Some(UdpSocket::bind("0.0.0.0:0").map_err(|error| {
                    RuntimeRadioError::new(
                        "rx_forward_socket_bind_failed",
                        format!("failed to bind WFB RX forwarding UDP socket: {error}"),
                    )
                })?),
                None => None,
            };
            Ok(ProductionRuntimeRxForwardRuntime {
                config: plan.config,
                aggregator: plan.aggregator,
                socket,
                forwarded_bytes: 0,
                counters: RxCounters::default(),
            })
        })
        .collect()
}

pub fn production_rx_forward_snapshots(
    runtimes: &[ProductionRuntimeRxForwardRuntime],
) -> Vec<ProductionRuntimeRxForwardSnapshot> {
    runtimes
        .iter()
        .map(|runtime| ProductionRuntimeRxForwardSnapshot {
            config: runtime.config,
            aggregator: runtime.aggregator,
            forwarded_bytes: runtime.forwarded_bytes,
            counters: runtime.counters.clone(),
        })
        .collect()
}

pub fn process_production_rx_packet_outcomes(
    packets: &[ParsedRxPacket],
    rx_forwards: &mut [ProductionRuntimeRxForwardRuntime],
) -> Result<ProductionRuntimeRxPacketOutcome, RuntimeRadioError> {
    let mut telemetry = ProductionRuntimeRxPacketTelemetry::default();
    for parsed in packets {
        match parsed.outcome {
            RxParseOutcome::Frame => {
                let frame = parsed.frame.as_ref().expect("frame outcome includes frame");
                telemetry.parsed_frames = telemetry.parsed_frames.saturating_add(1);
                count_production_rx_metadata(&mut telemetry, frame);
                count_production_rx_frame_type(&mut telemetry, &frame.data);
                observe_runtime_wfb_channel_id(
                    &mut telemetry.wfb_channel_observations,
                    &frame.data,
                );
                process_production_wfb_rx_forwards(rx_forwards, frame)?;
            }
            RxParseOutcome::Drop => {
                telemetry.dropped_packets = telemetry.dropped_packets.saturating_add(1);
            }
            RxParseOutcome::NeedMoreData => {
                telemetry.need_more_data = telemetry.need_more_data.saturating_add(1);
            }
        }
    }
    Ok(ProductionRuntimeRxPacketOutcome {
        telemetry,
        rx_forwards: production_rx_forward_snapshots(rx_forwards),
    })
}

fn count_production_rx_metadata(
    telemetry: &mut ProductionRuntimeRxPacketTelemetry,
    frame: &RxFrame,
) {
    if frame.phy_status {
        telemetry.phy_status_frames = telemetry.phy_status_frames.saturating_add(1);
    }
    if frame.rssi_dbm_valid {
        telemetry.rssi_valid_frames = telemetry.rssi_valid_frames.saturating_add(1);
    }
    if frame.snr_db.is_some() {
        telemetry.snr_frames = telemetry.snr_frames.saturating_add(1);
    }
    if frame.noise_dbm.is_some() {
        telemetry.noise_frames = telemetry.noise_frames.saturating_add(1);
    }
    telemetry.signal.observe_frame(frame);
}

fn count_production_rx_frame_type(
    telemetry: &mut ProductionRuntimeRxPacketTelemetry,
    frame: &[u8],
) {
    match frame_type(frame) {
        Ok(FrameType::Management) => {
            telemetry.management_frames = telemetry.management_frames.saturating_add(1);
        }
        Ok(FrameType::Control) => {
            telemetry.control_frames = telemetry.control_frames.saturating_add(1);
        }
        Ok(FrameType::Data) => {
            telemetry.data_frames = telemetry.data_frames.saturating_add(1);
        }
        Ok(FrameType::Extension) => {
            telemetry.extension_frames = telemetry.extension_frames.saturating_add(1);
        }
        Err(_) => {
            telemetry.dropped_packets = telemetry.dropped_packets.saturating_add(1);
        }
    }
}

fn process_production_wfb_rx_forwards(
    rx_forwards: &mut [ProductionRuntimeRxForwardRuntime],
    frame: &RxFrame,
) -> Result<(), RuntimeRadioError> {
    for runtime in rx_forwards {
        process_production_wfb_rx_forward(runtime, frame)?;
    }
    Ok(())
}

fn process_production_wfb_rx_forward(
    runtime: &mut ProductionRuntimeRxForwardRuntime,
    frame: &RxFrame,
) -> Result<(), RuntimeRadioError> {
    let Some(packet) = build_rx_forward_datagram(frame, runtime.config, &mut runtime.counters)
    else {
        return Ok(());
    };
    if let (Some(socket), Some(aggregator)) = (runtime.socket.as_ref(), runtime.aggregator) {
        let bytes = socket.send_to(&packet, aggregator).map_err(|error| {
            runtime.counters.send_failed = runtime.counters.send_failed.saturating_add(1);
            RuntimeRadioError::new(
                "rx_forward_send_failed",
                format!("failed to send WFB RX datagram to {aggregator}: {error}"),
            )
        })?;
        runtime.counters.forwarded = runtime.counters.forwarded.saturating_add(1);
        runtime.forwarded_bytes = runtime.forwarded_bytes.saturating_add(bytes as u64);
    }
    Ok(())
}

#[derive(Debug)]
pub struct ProductionRuntimeTxIngressSocket {
    pub bind_addr: SocketAddr,
    pub socket: UdpSocket,
    pub report_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionRuntimeQueuedDatagram {
    pub report_index: usize,
    pub peer: SocketAddr,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct ProductionRuntimeTxIngressReceiver {
    pub receiver: mpsc::Receiver<ProductionRuntimeQueuedDatagram>,
    stop: Arc<AtomicBool>,
    handles: Vec<thread::JoinHandle<()>>,
}

impl Drop for ProductionRuntimeTxIngressReceiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

pub fn bind_production_tx_ingress_sockets(
    bind_addrs: &[SocketAddr],
    receive_buffer_bytes: usize,
) -> Result<Vec<ProductionRuntimeTxIngressSocket>, RuntimeRadioError> {
    let mut sockets = Vec::with_capacity(bind_addrs.len());
    for (report_index, bind_addr) in bind_addrs.iter().copied().enumerate() {
        let socket = UdpSocket::bind(bind_addr).map_err(|error| {
            RuntimeRadioError::new(
                "udp_bind_failed",
                format!("failed to bind TX ingress UDP socket {bind_addr}: {error}"),
            )
        })?;
        configure_production_tx_ingress_socket(&socket, bind_addr, receive_buffer_bytes)?;
        sockets.push(ProductionRuntimeTxIngressSocket {
            bind_addr,
            socket,
            report_index,
        });
    }
    Ok(sockets)
}

pub fn configure_production_tx_ingress_socket(
    socket: &UdpSocket,
    bind_addr: SocketAddr,
    receive_buffer_bytes: usize,
) -> Result<(), RuntimeRadioError> {
    set_udp_receive_buffer(socket, receive_buffer_bytes).map_err(|error| {
        RuntimeRadioError::new(
            "udp_rcvbuf_config_failed",
            format!(
                "failed to configure {bind_addr} receive buffer to {receive_buffer_bytes} bytes: {error}"
            ),
        )
    })
}

pub fn spawn_production_tx_ingress_receivers(
    sockets: Vec<ProductionRuntimeTxIngressSocket>,
    receive_timeout: Duration,
) -> Result<ProductionRuntimeTxIngressReceiver, RuntimeRadioError> {
    let (sender, receiver) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::with_capacity(sockets.len());

    for tx_socket in sockets {
        let bind_addr = tx_socket.bind_addr;
        tx_socket
            .socket
            .set_read_timeout(Some(receive_timeout))
            .map_err(|error| {
                RuntimeRadioError::new(
                    "udp_timeout_config_failed",
                    format!("failed to configure {bind_addr} receive timeout: {error}"),
                )
            })?;
        let sender = sender.clone();
        let stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let mut buf = vec![0u8; u16::MAX as usize];
            while !stop.load(Ordering::Relaxed) {
                match tx_socket.socket.recv_from(&mut buf) {
                    Ok((len, peer)) => {
                        let queued = ProductionRuntimeQueuedDatagram {
                            report_index: tx_socket.report_index,
                            peer,
                            data: buf[..len].to_vec(),
                        };
                        if sender.send(queued).is_err() {
                            break;
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue;
                    }
                    Err(_) => break,
                }
            }
        });
        handles.push(handle);
    }
    drop(sender);

    Ok(ProductionRuntimeTxIngressReceiver {
        receiver,
        stop,
        handles,
    })
}

#[cfg(unix)]
fn set_udp_receive_buffer(socket: &UdpSocket, bytes: usize) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    let value: libc::c_int = bytes.try_into().unwrap_or(libc::c_int::MAX);
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            (&value as *const libc::c_int).cast(),
            std::mem::size_of_val(&value) as libc::socklen_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn set_udp_receive_buffer(_socket: &UdpSocket, _bytes: usize) -> io::Result<()> {
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeBridgeLoopRunConfig {
    pub duration: Option<Duration>,
    pub rx_timeout: Duration,
    pub tx_burst_limit: u32,
    pub max_datagrams: u64,
    pub airtime_schedule: ProductionRuntimeAirtimeSchedule,
}

impl ProductionRuntimeBridgeLoopRunConfig {
    pub fn from_bounds(
        duration_ms: u64,
        rx_timeout_ms: u64,
        tx_burst_limit: u32,
        max_datagrams: u64,
    ) -> Self {
        Self {
            duration: (duration_ms != 0).then(|| Duration::from_millis(duration_ms)),
            rx_timeout: Duration::from_millis(rx_timeout_ms),
            tx_burst_limit,
            max_datagrams,
            airtime_schedule: ProductionRuntimeAirtimeSchedule::continuous(),
        }
    }

    pub fn with_airtime_schedule(
        mut self,
        airtime_schedule: ProductionRuntimeAirtimeSchedule,
    ) -> Self {
        self.airtime_schedule = airtime_schedule;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeBridgeLoopStopReason {
    Signal,
    DurationElapsed,
    TxDatagramLimit,
}

impl ProductionRuntimeBridgeLoopStopReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Signal => "signal",
            Self::DurationElapsed => "duration_elapsed",
            Self::TxDatagramLimit => "tx_datagram_limit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionRuntimeBridgeLoopStep {
    TryTx,
    ReadRx { timeout: Duration },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionRuntimeBridgeLoopStepOutcome {
    TxProcessed,
    TxEmpty,
    TxDisconnected,
    RxRead,
    RxTimeout,
    Stop(ProductionRuntimeBridgeLoopStopReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeBridgeLoopOutcome {
    pub stop_reason: ProductionRuntimeBridgeLoopStopReason,
    pub tx_datagrams_processed: u64,
    pub iterations: u64,
    pub tx_polls: u64,
    pub rx_polls: u64,
    pub airtime_tx_allowed_iterations: u64,
    pub airtime_tx_gated_iterations: u64,
}

/// Run the production bridge loop with a per-iteration tick hook.
///
/// `on_iteration_start` is invoked once at the top of each outer iteration
/// after the stop/deadline checks pass and before TX burst/RX poll work.
/// It receives the same `Instant::now()` value the loop will reuse for
/// scheduling, so a consumer can drive periodic state (LED heartbeat,
/// watchdog kicks, throttle pacing) without taking its own clock reading.
pub fn run_production_bridge_loop<E, OnIterationStart, StopRequested, HandleStep>(
    config: ProductionRuntimeBridgeLoopRunConfig,
    mut on_iteration_start: OnIterationStart,
    mut stop_requested: StopRequested,
    mut handle_step: HandleStep,
) -> Result<ProductionRuntimeBridgeLoopOutcome, E>
where
    OnIterationStart: FnMut(std::time::Instant),
    StopRequested: FnMut() -> bool,
    HandleStep:
        FnMut(ProductionRuntimeBridgeLoopStep) -> Result<ProductionRuntimeBridgeLoopStepOutcome, E>,
{
    let started = std::time::Instant::now();
    let deadline = config.duration.map(|duration| started + duration);
    let unlimited_datagrams = config.max_datagrams == 0;
    let mut tx_datagrams_processed = 0u64;
    let mut iterations = 0u64;
    let mut tx_polls = 0u64;
    let mut rx_polls = 0u64;
    let mut airtime_tx_allowed_iterations = 0u64;
    let mut airtime_tx_gated_iterations = 0u64;

    loop {
        iterations = iterations.saturating_add(1);
        if stop_requested() {
            return Ok(ProductionRuntimeBridgeLoopOutcome {
                stop_reason: ProductionRuntimeBridgeLoopStopReason::Signal,
                tx_datagrams_processed,
                iterations,
                tx_polls,
                rx_polls,
                airtime_tx_allowed_iterations,
                airtime_tx_gated_iterations,
            });
        }
        if let Some(deadline) = deadline {
            if std::time::Instant::now() >= deadline {
                return Ok(ProductionRuntimeBridgeLoopOutcome {
                    stop_reason: ProductionRuntimeBridgeLoopStopReason::DurationElapsed,
                    tx_datagrams_processed,
                    iterations,
                    tx_polls,
                    rx_polls,
                    airtime_tx_allowed_iterations,
                    airtime_tx_gated_iterations,
                });
            }
        } else if !unlimited_datagrams && tx_datagrams_processed >= config.max_datagrams {
            return Ok(ProductionRuntimeBridgeLoopOutcome {
                stop_reason: ProductionRuntimeBridgeLoopStopReason::TxDatagramLimit,
                tx_datagrams_processed,
                iterations,
                tx_polls,
                rx_polls,
                airtime_tx_allowed_iterations,
                airtime_tx_gated_iterations,
            });
        }

        let now = std::time::Instant::now();
        on_iteration_start(now);

        if config
            .airtime_schedule
            .tx_allowed_at_elapsed(now.saturating_duration_since(started))
        {
            airtime_tx_allowed_iterations = airtime_tx_allowed_iterations.saturating_add(1);
            let mut tx_burst_count = 0u32;
            while (unlimited_datagrams || tx_datagrams_processed < config.max_datagrams)
                && tx_burst_count < config.tx_burst_limit
            {
                tx_polls = tx_polls.saturating_add(1);
                match handle_step(ProductionRuntimeBridgeLoopStep::TryTx)? {
                    ProductionRuntimeBridgeLoopStepOutcome::TxProcessed => {
                        tx_datagrams_processed = tx_datagrams_processed.saturating_add(1);
                        tx_burst_count = tx_burst_count.saturating_add(1);
                    }
                    ProductionRuntimeBridgeLoopStepOutcome::TxEmpty
                    | ProductionRuntimeBridgeLoopStepOutcome::TxDisconnected => break,
                    ProductionRuntimeBridgeLoopStepOutcome::Stop(stop_reason) => {
                        return Ok(ProductionRuntimeBridgeLoopOutcome {
                            stop_reason,
                            tx_datagrams_processed,
                            iterations,
                            tx_polls,
                            rx_polls,
                            airtime_tx_allowed_iterations,
                            airtime_tx_gated_iterations,
                        });
                    }
                    ProductionRuntimeBridgeLoopStepOutcome::RxRead
                    | ProductionRuntimeBridgeLoopStepOutcome::RxTimeout => break,
                }
            }
        } else {
            airtime_tx_gated_iterations = airtime_tx_gated_iterations.saturating_add(1);
        }

        let timeout = match deadline {
            Some(deadline) => {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    return Ok(ProductionRuntimeBridgeLoopOutcome {
                        stop_reason: ProductionRuntimeBridgeLoopStopReason::DurationElapsed,
                        tx_datagrams_processed,
                        iterations,
                        tx_polls,
                        rx_polls,
                        airtime_tx_allowed_iterations,
                        airtime_tx_gated_iterations,
                    });
                }
                config.rx_timeout.min(remaining)
            }
            None => config.rx_timeout,
        };
        rx_polls = rx_polls.saturating_add(1);
        match handle_step(ProductionRuntimeBridgeLoopStep::ReadRx { timeout })? {
            ProductionRuntimeBridgeLoopStepOutcome::RxRead
            | ProductionRuntimeBridgeLoopStepOutcome::RxTimeout => {}
            ProductionRuntimeBridgeLoopStepOutcome::Stop(stop_reason) => {
                return Ok(ProductionRuntimeBridgeLoopOutcome {
                    stop_reason,
                    tx_datagrams_processed,
                    iterations,
                    tx_polls,
                    rx_polls,
                    airtime_tx_allowed_iterations,
                    airtime_tx_gated_iterations,
                });
            }
            ProductionRuntimeBridgeLoopStepOutcome::TxProcessed
            | ProductionRuntimeBridgeLoopStepOutcome::TxEmpty
            | ProductionRuntimeBridgeLoopStepOutcome::TxDisconnected => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeBridgeTxProfile {
    LinuxMonitor,
    RadiotapDirect,
}

impl Default for ProductionRuntimeBridgeTxProfile {
    fn default() -> Self {
        Self::LinuxMonitor
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProductionRuntimeBridgeTxOverrides {
    pub tx_profile: ProductionRuntimeBridgeTxProfile,
    pub tx_rate: Option<radio_core::TxRate>,
    pub tx_bandwidth: Option<Bandwidth>,
    pub tx_channel_bandwidth: Option<Bandwidth>,
    pub tx_queue: Option<TxQueue>,
    pub mac_id: Option<u8>,
    pub tx_rate_id: Option<u8>,
    pub tx_retries: Option<u8>,
    pub tx_fallback_limit: Option<u8>,
    pub enable_rate_fallback: bool,
    pub no_agg_break: bool,
}

impl Default for ProductionRuntimeBridgeTxOverrides {
    fn default() -> Self {
        Self {
            tx_profile: ProductionRuntimeBridgeTxProfile::LinuxMonitor,
            tx_rate: None,
            tx_bandwidth: None,
            tx_channel_bandwidth: None,
            tx_queue: None,
            mac_id: None,
            tx_rate_id: None,
            tx_retries: None,
            tx_fallback_limit: None,
            enable_rate_fallback: false,
            no_agg_break: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProductionRuntimeBridgeTxConfig {
    pub channel: Channel,
    pub channel_bandwidth: Bandwidth,
    pub overrides: ProductionRuntimeBridgeTxOverrides,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductionRuntimeBridgeTxDatagramMetadata {
    pub peer: SocketAddr,
    pub datagram_len: usize,
    pub fwmark: u32,
    pub radiotap_len: usize,
    pub frame_len: usize,
    pub packet_len: usize,
    pub tx_descriptor_preview_hex: String,
    pub tx_profile: ProductionRuntimeBridgeTxProfile,
    pub tx_options: TxOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductionRuntimeBridgeTxOutcome {
    pub metadata: Option<ProductionRuntimeBridgeTxDatagramMetadata>,
    pub datagram_bytes: u64,
    pub frame_bytes: u64,
    pub bridge_counters: TxCounters,
    pub submit_counters: TxSubmitCounters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductionRuntimeBridgeTxError {
    pub code: &'static str,
    pub message: String,
    pub metadata: Option<ProductionRuntimeBridgeTxDatagramMetadata>,
    pub datagram_bytes: u64,
    pub frame_bytes: u64,
    pub bridge_counters: TxCounters,
    pub submit_counters: TxSubmitCounters,
}

impl fmt::Display for ProductionRuntimeBridgeTxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Error for ProductionRuntimeBridgeTxError {}

pub fn apply_production_bridge_tx_overrides(
    overrides: ProductionRuntimeBridgeTxOverrides,
    channel_bandwidth: Bandwidth,
    mut options: TxOptions,
) -> TxOptions {
    options.channel_bandwidth = Some(overrides.tx_channel_bandwidth.unwrap_or(channel_bandwidth));
    if let Some(rate) = overrides.tx_rate {
        options.rate = rate;
    }
    if let Some(bandwidth) = overrides.tx_bandwidth {
        options.bandwidth = bandwidth;
    }
    options = match overrides.tx_profile {
        ProductionRuntimeBridgeTxProfile::LinuxMonitor => {
            apply_production_wfb_monitor_tx_defaults(options)
        }
        ProductionRuntimeBridgeTxProfile::RadiotapDirect => options,
    };
    if let Some(queue) = overrides.tx_queue {
        options.queue = queue;
    }
    if let Some(mac_id) = overrides.mac_id {
        options.mac_id = mac_id;
    }
    if let Some(rate_id) = overrides.tx_rate_id {
        options.rate_id = Some(rate_id);
    }
    if let Some(retries) = overrides.tx_retries {
        options.retries = retries;
    }
    if let Some(fallback_limit) = overrides.tx_fallback_limit {
        options.rate_fallback_limit = fallback_limit;
    }
    if overrides.enable_rate_fallback {
        options.disable_rate_fallback = false;
    }
    if overrides.no_agg_break {
        options.aggregate_break = false;
    }
    options
}

fn apply_production_wfb_monitor_tx_defaults(mut options: TxOptions) -> TxOptions {
    if matches!(
        options.rate,
        radio_core::TxRate::Mcs(_) | radio_core::TxRate::Vht { .. }
    ) {
        options.queue = TxQueue::Mgnt;
        options.mac_id = 1;
        if matches!(options.rate, radio_core::TxRate::Mcs(_)) {
            options.rate_id = Some(7);
        }
        options.disable_rate_fallback = false;
        options.rate_fallback_limit = 0;
        options.aggregate_break = false;
        if options.no_retry {
            options.retries = 0;
        }
    }

    options
}

pub fn handle_production_bridge_tx_datagram<T>(
    session: &mut RuntimeRadioSession<T>,
    queued: &ProductionRuntimeQueuedDatagram,
    config: ProductionRuntimeBridgeTxConfig,
    bridge_counters: &mut TxCounters,
    submit_counters: &mut TxSubmitCounters,
) -> Result<ProductionRuntimeBridgeTxOutcome, ProductionRuntimeBridgeTxError>
where
    T: UsbBulkTransfer,
{
    let datagram = queued.data.as_slice();
    let datagram_bytes = datagram.len() as u64;
    let parsed = match parse_tx_datagram(datagram) {
        Ok(parsed) => parsed,
        Err(error) => {
            bridge_counters.incoming = bridge_counters.incoming.saturating_add(1);
            bridge_counters.dropped = bridge_counters.dropped.saturating_add(1);
            bridge_counters.malformed = bridge_counters.malformed.saturating_add(1);
            bridge_counters.unsupported_radiotap = bridge_counters
                .unsupported_radiotap
                .saturating_add(u64::from(is_unsupported_runtime_radiotap(&error)));
            return Ok(ProductionRuntimeBridgeTxOutcome {
                metadata: None,
                datagram_bytes,
                frame_bytes: 0,
                bridge_counters: bridge_counters.clone(),
                submit_counters: submit_counters.clone(),
            });
        }
    };

    let tx_options = apply_production_bridge_tx_overrides(
        config.overrides,
        config.channel_bandwidth,
        parsed.tx_options,
    );
    let frame_bytes = parsed.ieee80211_frame.len() as u64;
    let packet = match build_tx_packet(parsed.ieee80211_frame, config.channel, tx_options) {
        Ok(packet) => packet,
        Err(_) => {
            bridge_counters.incoming = bridge_counters.incoming.saturating_add(1);
            bridge_counters.dropped = bridge_counters.dropped.saturating_add(1);
            bridge_counters.malformed = bridge_counters.malformed.saturating_add(1);
            return Ok(ProductionRuntimeBridgeTxOutcome {
                metadata: None,
                datagram_bytes,
                frame_bytes,
                bridge_counters: bridge_counters.clone(),
                submit_counters: submit_counters.clone(),
            });
        }
    };
    let metadata = ProductionRuntimeBridgeTxDatagramMetadata {
        peer: queued.peer,
        datagram_len: datagram.len(),
        fwmark: parsed.fwmark,
        radiotap_len: parsed.radiotap_len,
        frame_len: parsed.ieee80211_frame.len(),
        packet_len: packet.len(),
        tx_descriptor_preview_hex: encode_runtime_hex(&packet[..TX_DESC_SIZE.min(packet.len())]),
        tx_profile: config.overrides.tx_profile,
        tx_options,
    };

    bridge_counters.incoming = bridge_counters.incoming.saturating_add(1);
    match session.submit_80211_frame(
        parsed.ieee80211_frame,
        config.channel,
        tx_options,
        submit_counters,
    ) {
        Ok(_) => {
            bridge_counters.injected = bridge_counters.injected.saturating_add(1);
            Ok(ProductionRuntimeBridgeTxOutcome {
                metadata: Some(metadata),
                datagram_bytes,
                frame_bytes,
                bridge_counters: bridge_counters.clone(),
                submit_counters: submit_counters.clone(),
            })
        }
        Err(error) => {
            bridge_counters.dropped = bridge_counters.dropped.saturating_add(1);
            Err(ProductionRuntimeBridgeTxError {
                code: "tx_submit_failed",
                message: format!("radio TX failed: {error}"),
                metadata: Some(metadata),
                datagram_bytes,
                frame_bytes,
                bridge_counters: bridge_counters.clone(),
                submit_counters: submit_counters.clone(),
            })
        }
    }
}

fn is_unsupported_runtime_radiotap(error: &TxDatagramError) -> bool {
    matches!(
        error,
        TxDatagramError::Radiotap(
            RadiotapError::UnsupportedPresentFlags { .. }
                | RadiotapError::UnsupportedHtBandwidth { .. }
                | RadiotapError::UnsupportedVhtBandwidth { .. }
        )
    )
}

fn encode_runtime_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeFlowConfig {
    pub usb: ProductionRuntimeUsbConfig,
    pub channel: Channel,
    pub bandwidth: Bandwidth,
    pub firmware: Option<PathBuf>,
    pub bind_addr: SocketAddr,
    pub tx_binds: Vec<SocketAddr>,
    pub duration_ms: u64,
    pub rx_timeout_ms: u64,
    pub tx_burst_limit: u32,
    pub max_datagrams: u32,
    pub airtime_schedule: ProductionRuntimeAirtimeSchedule,
    pub ready_file: Option<PathBuf>,
    pub health_file: Option<PathBuf>,
    pub tx_authorized: bool,
    pub live_register_write_authorized: bool,
    pub calibration_profile: TxCalibrationProfile,
    pub captured_tail_applied: bool,
    pub primary_rx_forward: ProductionRuntimePrimaryRxForwardConfig,
    pub rx_forwards: Vec<ProductionRuntimeRxForwardConfig>,
    pub rx_wlan_idx: u8,
    pub rx_mcs_index: u8,
}

#[derive(Debug, Clone)]
pub struct ProductionRuntimeRtl8812auInitInputs {
    pub firmware_image: FirmwareImage,
    pub mac_plan: RealtekTablePlan,
    pub phy_plan: RealtekTablePlan,
    pub agc_plan: RealtekTablePlan,
    pub radioa_plan: RealtekTablePlan,
    pub radiob_plan: RealtekTablePlan,
    pub init_order: Rtl8812auInitOrder,
    pub rfe_type: u8,
    pub init_timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum ProductionRuntimeTxPowerControlInput {
    None,
    ManualIndex {
        path: Rtl8812auRfPath,
        index: u8,
    },
    EfuseDerived {
        source: Rtl8812auTxPowerEfuseSourceReport,
        plan: Rtl8812auTxPowerEfusePlanReport,
    },
}

impl Default for ProductionRuntimeTxPowerControlInput {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProductionRuntimeFlowExecutionInputs {
    pub rtl8812au_init: Option<ProductionRuntimeRtl8812auInitInputs>,
    pub tx_power_control: ProductionRuntimeTxPowerControlInput,
    pub heartbeat_led: LedHeartbeatConfig,
    pub process_signal_stop: bool,
}

impl ProductionRuntimeFlowExecutionInputs {
    pub fn validate(&self) -> Result<(), RuntimeRadioError> {
        if self.rtl8812au_init.is_none() {
            return Err(RuntimeRadioError::new(
                "missing_runtime_init_assets",
                "production radio run requires parsed firmware and RTL8812AU table plans",
            ));
        }
        self.heartbeat_led.validate().map_err(|error| {
            RuntimeRadioError::new("invalid_heartbeat_led_config", error.to_string())
        })
    }
}

static PRODUCTION_RUNTIME_STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn production_runtime_signal_handler(_signal: libc::c_int) {
    PRODUCTION_RUNTIME_STOP_REQUESTED.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
fn install_production_runtime_signal_handlers() -> io::Result<()> {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = production_runtime_signal_handler as *const () as libc::sighandler_t;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        for signal in [libc::SIGINT, libc::SIGTERM] {
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn install_production_runtime_signal_handlers() -> io::Result<()> {
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct ProductionRuntimePreLoopReports {
    monitor_opmode_applied: bool,
    tx_power_control: Option<Rtl8812auTxPowerControlReport>,
    tx_calibration_profile: Option<Rtl8812auTxCalibrationProfileReport>,
    rx_startup_kick: Option<ProductionRuntimeRxStartupKickReport>,
}

impl ProductionRuntimeFlowConfig {
    pub fn validate(&self) -> Result<ProductionRuntimeFlowValidation, RuntimeRadioError> {
        validate_production_runtime_flow_config(self)
    }

    pub fn wfb_loop_config(&self) -> ProductionRuntimeWfbLoopConfig {
        ProductionRuntimeWfbLoopConfig {
            bind_addr: self.bind_addr,
            tx_binds: self.tx_binds.clone(),
            rx_timeout_ms: self.rx_timeout_ms,
            tx_burst_limit: self.tx_burst_limit,
            max_datagrams: self.max_datagrams,
            airtime_schedule: self.airtime_schedule,
            bandwidth: self.bandwidth,
            primary_rx_forward: self.primary_rx_forward,
            rx_forwards: self.rx_forwards.clone(),
            rx_wlan_idx: self.rx_wlan_idx,
            rx_mcs_index: self.rx_mcs_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeFlowValidation {
    pub calibration: RuntimeTxCalibrationDecision,
    pub wfb_loop: ProductionRuntimeWfbLoopPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeReadyMarker {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_at_unix_ms: Option<u64>,
    pub bind_addr: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bind_addrs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_frequency_mhz: Option<u16>,
    pub bandwidth_mhz: u16,
    pub max_datagrams: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_burst_limit: Option<u32>,
    pub airtime_schedule: ProductionRuntimeAirtimeSchedule,
    pub init_before_tx: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_session_init_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_opmode_applied: Option<bool>,
    pub tx_power_control_applied: bool,
    pub tx_calibration_profile_applied: bool,
    pub rx_startup_kick_applied: bool,
}

pub fn write_production_runtime_ready_marker(
    path: Option<&Path>,
    mut marker: ProductionRuntimeReadyMarker,
) -> Result<(), RuntimeRadioError> {
    let Some(path) = path else {
        return Ok(());
    };
    marker.ready_at_unix_ms = Some(runtime_unix_ms());
    let mut bytes = serde_json::to_vec_pretty(&marker).map_err(|error| {
        RuntimeRadioError::new("runtime_ready_marker_serialize_failed", error.to_string())
    })?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| {
        RuntimeRadioError::new(
            "runtime_ready_marker_write_failed",
            format!("{}: {error}", path.display()),
        )
    })
}

fn runtime_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeInitReadiness {
    NotStarted,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeInitTelemetry {
    pub readiness: ProductionRuntimeInitReadiness,
    pub phase_count: usize,
    pub completed_phase_count: usize,
}

impl Default for ProductionRuntimeInitTelemetry {
    fn default() -> Self {
        Self {
            readiness: ProductionRuntimeInitReadiness::NotStarted,
            phase_count: 0,
            completed_phase_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeFlowResult {
    Pass,
    Fail,
}

impl ProductionRuntimeFlowResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeHeartbeatLedReport {
    pub enabled: bool,
    pub half_period_ms: u64,
    pub toggles_attempted: u64,
    pub toggles_succeeded: u64,
    pub toggles_failed: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeRxStartupKickReport {
    pub semantics: &'static str,
    pub frame_len: usize,
    pub tx_options: TxOptions,
    pub attempted: u64,
    pub submitted: u64,
    pub failed: u64,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeFlowExecutionReport {
    pub selector: DeviceSelector,
    pub adapter: Option<UsbDeviceInfo>,
    pub endpoints: Option<UsbEndpoints>,
    pub channel: Option<Channel>,
    pub bandwidth: Bandwidth,
    pub duration_ms: u64,
    pub ready_file: Option<PathBuf>,
    pub stop_reason: &'static str,
    pub bulk_in_endpoint: Option<u8>,
    pub bulk_out_endpoint: Option<u8>,
    pub calibration_profile: TxCalibrationProfile,
    pub calibration_class: TxCalibrationClass,
    pub tx_power_control: Option<Rtl8812auTxPowerControlReport>,
    pub tx_calibration_profile: Option<Rtl8812auTxCalibrationProfileReport>,
    pub rx_startup_kick: Option<ProductionRuntimeRxStartupKickReport>,
    pub heartbeat_led: Option<ProductionRuntimeHeartbeatLedReport>,
    pub airtime: ProductionRuntimeAirtimeReport,
    pub receiver_backed_validation_required: bool,
    pub init: ProductionRuntimeInitTelemetry,
    pub rx: RuntimeFlowRxTelemetry,
    pub tx: RuntimeFlowTxTelemetry,
    pub counters: RuntimeRadioCounters,
    pub result: ProductionRuntimeFlowResult,
    pub error: Option<ProductionRuntimeFlowErrorReport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeFlowReport {
    pub schema_version: u8,
    pub command: &'static str,
    pub selector: DeviceSelector,
    pub adapter: Option<UsbDeviceInfo>,
    pub endpoints: Option<UsbEndpoints>,
    pub channel: Option<Channel>,
    pub bandwidth: Bandwidth,
    pub duration_ms: u64,
    pub ready_file: Option<PathBuf>,
    pub stop_reason: &'static str,
    pub bulk_in_endpoint: Option<u8>,
    pub bulk_out_endpoint: Option<u8>,
    pub calibration_profile: TxCalibrationProfile,
    pub calibration_class: TxCalibrationClass,
    pub calibration_evidence_source: RuntimeTxCalibrationEvidenceSource,
    pub tx_power_control: Option<Rtl8812auTxPowerControlReport>,
    pub tx_calibration_profile: Option<Rtl8812auTxCalibrationProfileReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_startup_kick: Option<ProductionRuntimeRxStartupKickReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_led: Option<ProductionRuntimeHeartbeatLedReport>,
    pub airtime: ProductionRuntimeAirtimeReport,
    pub receiver_backed_validation_required: bool,
    pub init: ProductionRuntimeInitTelemetry,
    pub rx: RuntimeFlowRxTelemetry,
    pub tx: RuntimeFlowTxTelemetry,
    pub counters: RuntimeRadioCounters,
    pub result: ProductionRuntimeFlowResult,
    pub error: Option<ProductionRuntimeFlowErrorReport>,
}

impl ProductionRuntimeFlowReport {
    pub fn from_execution(
        config: &ProductionRuntimeFlowConfig,
        execution: ProductionRuntimeFlowExecutionReport,
    ) -> Self {
        Self {
            schema_version: 1,
            command: "radio-run",
            selector: execution.selector,
            adapter: execution.adapter,
            endpoints: execution.endpoints,
            channel: execution.channel,
            bandwidth: execution.bandwidth,
            duration_ms: execution.duration_ms,
            ready_file: execution.ready_file,
            stop_reason: execution.stop_reason,
            bulk_in_endpoint: execution.bulk_in_endpoint,
            bulk_out_endpoint: execution.bulk_out_endpoint,
            calibration_profile: execution.calibration_profile,
            calibration_class: execution.calibration_class,
            calibration_evidence_source: config
                .calibration_profile
                .evidence_source(config.captured_tail_applied),
            tx_power_control: execution.tx_power_control,
            tx_calibration_profile: execution.tx_calibration_profile,
            rx_startup_kick: execution.rx_startup_kick,
            heartbeat_led: execution.heartbeat_led,
            airtime: execution.airtime,
            receiver_backed_validation_required: execution.receiver_backed_validation_required,
            init: execution.init,
            rx: execution.rx,
            tx: execution.tx,
            counters: execution.counters,
            result: execution.result,
            error: execution.error,
        }
    }

    pub fn not_started(config: &ProductionRuntimeFlowConfig, error: RuntimeRadioError) -> Self {
        let calibration_class = config
            .calibration_profile
            .before_tx_class(config.captured_tail_applied);
        let calibration_evidence_source = config
            .calibration_profile
            .evidence_source(config.captured_tail_applied);
        Self {
            schema_version: 1,
            command: "radio-run",
            selector: config.usb.selector,
            adapter: None,
            endpoints: None,
            channel: Some(config.channel),
            bandwidth: config.bandwidth,
            duration_ms: config.duration_ms,
            ready_file: config.ready_file.clone(),
            stop_reason: "not_started",
            bulk_in_endpoint: None,
            bulk_out_endpoint: None,
            calibration_profile: config.calibration_profile,
            calibration_class,
            calibration_evidence_source,
            tx_power_control: None,
            tx_calibration_profile: None,
            rx_startup_kick: None,
            heartbeat_led: None,
            airtime: ProductionRuntimeAirtimeReport::idle(config.airtime_schedule),
            receiver_backed_validation_required: !config.calibration_profile.is_default(),
            init: ProductionRuntimeInitTelemetry::default(),
            rx: RuntimeFlowRxTelemetry::default(),
            tx: RuntimeFlowTxTelemetry::default(),
            counters: RuntimeRadioCounters::default(),
            result: ProductionRuntimeFlowResult::Fail,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeServiceLifecycle {
    Starting,
    Validating,
    Initializing,
    Ready,
    Running,
    Stopping,
    ExitedPass,
    ExitedFail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionRuntimeServiceOperatorAction {
    Wait,
    Monitor,
    Investigate,
    Restart,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProductionRuntimeServiceHealth {
    pub schema_version: u8,
    pub command: &'static str,
    pub updated_at_unix_ms: u64,
    pub lifecycle: ProductionRuntimeServiceLifecycle,
    pub operator_action: ProductionRuntimeServiceOperatorAction,
    pub selector: DeviceSelector,
    pub adapter: Option<UsbDeviceInfo>,
    pub endpoints: Option<UsbEndpoints>,
    pub channel: Option<Channel>,
    pub bandwidth: Bandwidth,
    pub duration_ms: u64,
    pub ready_file: Option<PathBuf>,
    pub health_file: Option<PathBuf>,
    pub report_file: Option<PathBuf>,
    pub stop_reason: Option<String>,
    pub result: Option<ProductionRuntimeFlowResult>,
    pub init: ProductionRuntimeInitTelemetry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_led: Option<ProductionRuntimeHeartbeatLedReport>,
    pub rx: RuntimeFlowRxTelemetry,
    pub tx: RuntimeFlowTxTelemetry,
    pub counters: RuntimeRadioCounters,
    pub error: Option<ProductionRuntimeFlowErrorReport>,
}

impl ProductionRuntimeServiceHealth {
    pub fn from_config(
        config: &ProductionRuntimeFlowConfig,
        lifecycle: ProductionRuntimeServiceLifecycle,
        report_file: Option<PathBuf>,
        error: Option<RuntimeRadioError>,
    ) -> Self {
        let error_report = error.map(ProductionRuntimeFlowErrorReport::from);
        Self {
            schema_version: 1,
            command: "radio-run",
            updated_at_unix_ms: runtime_unix_ms(),
            lifecycle,
            operator_action: production_service_operator_action(
                lifecycle,
                None,
                None,
                &RuntimeFlowRxTelemetry::default(),
                &RuntimeFlowTxTelemetry::default(),
                error_report.as_ref(),
            ),
            selector: config.usb.selector,
            adapter: None,
            endpoints: None,
            channel: Some(config.channel),
            bandwidth: config.bandwidth,
            duration_ms: config.duration_ms,
            ready_file: config.ready_file.clone(),
            health_file: config.health_file.clone(),
            report_file,
            stop_reason: None,
            result: None,
            init: ProductionRuntimeInitTelemetry::default(),
            heartbeat_led: None,
            rx: RuntimeFlowRxTelemetry::default(),
            tx: RuntimeFlowTxTelemetry::default(),
            counters: RuntimeRadioCounters::default(),
            error: error_report,
        }
    }

    pub fn from_report(
        report: &ProductionRuntimeFlowReport,
        lifecycle: ProductionRuntimeServiceLifecycle,
        health_file: Option<PathBuf>,
        report_file: Option<PathBuf>,
    ) -> Self {
        let stop_reason = Some(report.stop_reason.to_string());
        Self {
            schema_version: 1,
            command: "radio-run",
            updated_at_unix_ms: runtime_unix_ms(),
            lifecycle,
            operator_action: production_service_operator_action(
                lifecycle,
                Some(report.result),
                stop_reason.as_deref(),
                &report.rx,
                &report.tx,
                report.error.as_ref(),
            ),
            selector: report.selector,
            adapter: report.adapter.clone(),
            endpoints: report.endpoints.clone(),
            channel: report.channel,
            bandwidth: report.bandwidth,
            duration_ms: report.duration_ms,
            ready_file: report.ready_file.clone(),
            health_file,
            report_file,
            stop_reason,
            result: Some(report.result),
            init: report.init,
            heartbeat_led: report.heartbeat_led,
            rx: report.rx.clone(),
            tx: report.tx,
            counters: report.counters,
            error: report.error.clone(),
        }
    }
}

pub fn write_production_runtime_service_health(
    path: Option<&Path>,
    health: &ProductionRuntimeServiceHealth,
) -> Result<(), RuntimeRadioError> {
    let Some(path) = path else {
        return Ok(());
    };
    let mut bytes = serde_json::to_vec_pretty(health).map_err(|error| {
        RuntimeRadioError::new("runtime_health_serialize_failed", error.to_string())
    })?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| {
        RuntimeRadioError::new(
            "runtime_health_write_failed",
            format!("{}: {error}", path.display()),
        )
    })
}

fn production_service_operator_action(
    lifecycle: ProductionRuntimeServiceLifecycle,
    result: Option<ProductionRuntimeFlowResult>,
    stop_reason: Option<&str>,
    rx: &RuntimeFlowRxTelemetry,
    tx: &RuntimeFlowTxTelemetry,
    error: Option<&ProductionRuntimeFlowErrorReport>,
) -> ProductionRuntimeServiceOperatorAction {
    match lifecycle {
        ProductionRuntimeServiceLifecycle::Starting
        | ProductionRuntimeServiceLifecycle::Validating
        | ProductionRuntimeServiceLifecycle::Initializing => {
            return ProductionRuntimeServiceOperatorAction::Wait
        }
        ProductionRuntimeServiceLifecycle::Stopping => {
            return ProductionRuntimeServiceOperatorAction::Stopped
        }
        ProductionRuntimeServiceLifecycle::ExitedFail => {
            return ProductionRuntimeServiceOperatorAction::Restart
        }
        ProductionRuntimeServiceLifecycle::Ready
        | ProductionRuntimeServiceLifecycle::Running
        | ProductionRuntimeServiceLifecycle::ExitedPass => {}
    }

    if error.is_some() || matches!(result, Some(ProductionRuntimeFlowResult::Fail)) {
        return ProductionRuntimeServiceOperatorAction::Restart;
    }
    if stop_reason == Some("signal") {
        return ProductionRuntimeServiceOperatorAction::Stopped;
    }

    let rx_forward_send_failures = rx
        .rx_forwards
        .iter()
        .map(|forward| forward.counters.send_failed)
        .sum::<u64>();
    if tx.failed_submissions > 0
        || tx.dropped_datagrams > 0
        || rx.dropped_packets > 0
        || rx_forward_send_failures > 0
    {
        return ProductionRuntimeServiceOperatorAction::Investigate;
    }

    ProductionRuntimeServiceOperatorAction::Monitor
}

pub fn validate_production_runtime_flow_config(
    config: &ProductionRuntimeFlowConfig,
) -> Result<ProductionRuntimeFlowValidation, RuntimeRadioError> {
    let supported_channel = Channel::from_number(config.channel.number).map_err(|error| {
        RuntimeRadioError::new(
            "invalid_channel",
            format!("invalid runtime channel: {error}"),
        )
    })?;
    if supported_channel != config.channel {
        return Err(RuntimeRadioError::new(
            "invalid_channel",
            format!(
                "channel {} metadata does not match supported channel table",
                config.channel.number
            ),
        ));
    }
    if !config.channel.supports_bandwidth(config.bandwidth) {
        return Err(RuntimeRadioError::new(
            "unsupported_bandwidth",
            format!(
                "channel {} does not support {} MHz bandwidth",
                config.channel.number,
                config.bandwidth.mhz()
            ),
        ));
    }
    let wfb_loop = plan_production_wfb_loop(&config.wfb_loop_config())?;
    if config.firmware.is_none() {
        return Err(RuntimeRadioError::new(
            "missing_firmware",
            "production radio run requires an RTL8812A firmware image path",
        ));
    }
    if !config.tx_authorized {
        return Err(RuntimeRadioError::new(
            "missing_tx_authorization",
            "production radio run requires explicit RF transmit authorization",
        ));
    }

    let calibration = config.calibration_profile.calibration_decision(
        config.captured_tail_applied,
        config.live_register_write_authorized,
    )?;
    Ok(ProductionRuntimeFlowValidation {
        calibration,
        wfb_loop,
    })
}

pub fn run_production_runtime_flow(
    config: ProductionRuntimeFlowConfig,
    inputs: ProductionRuntimeFlowExecutionInputs,
) -> ProductionRuntimeFlowReport {
    if let Err(error) = write_production_runtime_service_health(
        config.health_file.as_deref(),
        &ProductionRuntimeServiceHealth::from_config(
            &config,
            ProductionRuntimeServiceLifecycle::Validating,
            None,
            None,
        ),
    ) {
        return ProductionRuntimeFlowReport::not_started(&config, error);
    }
    if let Err(error) = config.validate() {
        return production_runtime_report_with_health(
            &config,
            ProductionRuntimeFlowReport::not_started(&config, error),
            ProductionRuntimeServiceLifecycle::ExitedFail,
        );
    }
    if let Err(error) = inputs.validate() {
        return production_runtime_report_with_health(
            &config,
            ProductionRuntimeFlowReport::not_started(&config, error),
            ProductionRuntimeServiceLifecycle::ExitedFail,
        );
    }
    if inputs.process_signal_stop {
        PRODUCTION_RUNTIME_STOP_REQUESTED.store(false, Ordering::SeqCst);
        if let Err(error) = install_production_runtime_signal_handlers() {
            return production_runtime_report_with_health(
                &config,
                ProductionRuntimeFlowReport::not_started(
                    &config,
                    RuntimeRadioError::new(
                        "runtime_signal_handler_failed",
                        format!("failed to install runtime SIGINT/SIGTERM handler: {error}"),
                    ),
                ),
                ProductionRuntimeServiceLifecycle::ExitedFail,
            );
        }
    }
    if let Err(error) = write_production_runtime_service_health(
        config.health_file.as_deref(),
        &ProductionRuntimeServiceHealth::from_config(
            &config,
            ProductionRuntimeServiceLifecycle::Initializing,
            None,
            None,
        ),
    ) {
        return ProductionRuntimeFlowReport::not_started(&config, error);
    }

    let mut session = match RuntimeRadioSession::open(config.usb.to_runtime_open_config()) {
        Ok(session) => session,
        Err(error) => {
            return production_runtime_report_with_health(
                &config,
                ProductionRuntimeFlowReport::not_started(
                    &config,
                    RuntimeRadioError::new(error.code, error.message),
                ),
                ProductionRuntimeServiceLifecycle::ExitedFail,
            )
        }
    };
    let init_inputs = inputs
        .rtl8812au_init
        .clone()
        .expect("validated production runtime inputs include init assets");
    let mut init_state =
        Rtl8812auProductionInitState::new(init_inputs, config.channel, config.bandwidth);

    run_production_runtime_flow_with_session(config, inputs, &mut session, |session, phase| {
        run_rtl8812au_production_init_phase(session, phase, &mut init_state)
    })
}

struct Rtl8812auProductionInitState {
    inputs: ProductionRuntimeRtl8812auInitInputs,
    channel: Channel,
    bandwidth: Bandwidth,
    firmware_payload_offset: usize,
    firmware_signature: Option<u16>,
    firmware_payload_bytes: Vec<u8>,
    llt_entries_written: u64,
    queue_layout: Option<ProductionQueueLayout>,
    phy_writes_applied: usize,
    agc_writes_applied: usize,
    radioa_writes_applied: usize,
    radiob_writes_applied: usize,
    table_delays_applied: usize,
}

impl Rtl8812auProductionInitState {
    fn new(
        inputs: ProductionRuntimeRtl8812auInitInputs,
        channel: Channel,
        bandwidth: Bandwidth,
    ) -> Self {
        let (firmware_payload_offset, firmware_signature, firmware_payload_bytes) = {
            let payload = inputs.firmware_image.realtek_download_payload();
            (payload.offset, payload.signature, payload.bytes.to_vec())
        };
        Self {
            inputs,
            channel,
            bandwidth,
            firmware_payload_offset,
            firmware_signature,
            firmware_payload_bytes,
            llt_entries_written: 0,
            queue_layout: None,
            phy_writes_applied: 0,
            agc_writes_applied: 0,
            radioa_writes_applied: 0,
            radiob_writes_applied: 0,
            table_delays_applied: 0,
        }
    }
}

fn run_rtl8812au_production_init_phase<T>(
    session: &mut RuntimeRadioSession<T>,
    phase: Rtl8812auInitPhase,
    state: &mut Rtl8812auProductionInitState,
) -> Result<RuntimeSameSessionInitPhaseSummary, RuntimeSameSessionInitPhaseFailure>
where
    for<'a> &'a T: Rtl8812auUsbTransport,
{
    let registers =
        Rtl8812auRegisterAccess::new(&session.transport).with_timeout(state.inputs.init_timeout);
    let before = session.counters;

    let result = match phase {
        Rtl8812auInitPhase::PowerOn => production_power_on_sequence(
            &registers,
            &mut session.counters,
            state.inputs.init_timeout,
        )
        .map(|writes| (format!("completed {writes} power-on/RF-reset writes"), Some(writes))),
        Rtl8812auInitPhase::Firmware => production_firmware_sequence(
            &registers,
            &mut session.counters,
            &state.firmware_payload_bytes,
            state.inputs.init_timeout,
        )
        .map(|stats| {
            (
                format!(
                    "downloaded {} payload bytes from offset {} signature {} in {} control writes; checksum ready after {} polls, firmware ready after {} polls",
                    stats.bytes_written,
                    state.firmware_payload_offset,
                    state
                        .firmware_signature
                        .map(|value| format_register_value(value, 4))
                        .unwrap_or_else(|| "n/a".to_string()),
                    stats.control_writes,
                    stats.checksum_poll_attempts,
                    stats.ready_poll_attempts
                ),
                Some(usize::try_from(stats.control_writes).unwrap_or(usize::MAX)),
            )
        }),
        Rtl8812auInitPhase::Llt => production_llt_sequence(
            &registers,
            &mut session.counters,
            state.inputs.init_timeout,
        )
        .map(|entries| {
            state.llt_entries_written = entries;
            (
                format!("wrote {entries} LLT entries"),
                Some(usize::try_from(entries).unwrap_or(usize::MAX)),
            )
        }),
        Rtl8812auInitPhase::MacTable => {
            production_apply_mac_table_plan(&registers, &mut session.counters, &state.inputs.mac_plan)
                .map(|writes| {
                    (
                        format!("applied {writes} generated MAC register writes"),
                        Some(writes),
                    )
                })
        }
        Rtl8812auInitPhase::QueueDma => {
            match production_queue_layout_from_endpoints(&session.endpoints) {
                Ok(layout) => {
                    state.queue_layout = Some(layout);
                    production_queue_dma_sequence(&registers, &mut session.counters, layout).map(
                        |writes| {
                            (
                                format!(
                                    "programmed queue/DMA layout for {} bulk OUT endpoints (HPQ={}, LPQ={}, NPQ={}, PUBQ={})",
                                    layout.bulk_out_endpoint_count,
                                    layout.hpq,
                                    layout.lpq,
                                    layout.npq,
                                    layout.pubq
                                ),
                                Some(writes),
                            )
                        },
                    )
                }
                Err(error) => Err(error),
            }
        }
        Rtl8812auInitPhase::Mac => {
            production_mac_sequence(&registers, &mut session.counters).map(|writes| {
                (
                    format!("completed {writes} MAC/WMAC setup writes"),
                    Some(writes),
                )
            })
        }
        Rtl8812auInitPhase::MacAddr => {
            production_program_efuse_macid(&registers, &mut session.counters).map(|detail| (detail, None))
        }
        Rtl8812auInitPhase::Bb => {
            production_bb_sequence(&registers, &mut session.counters, state).map(|writes| {
                (
                    format!(
                        "applied {} PHY writes, {} AGC writes, and {} delays",
                        state.phy_writes_applied, state.agc_writes_applied, state.table_delays_applied
                    ),
                    Some(writes),
                )
            })
        }
        Rtl8812auInitPhase::Rf => {
            production_rf_sequence(&registers, &mut session.counters, state).map(|writes| {
                (
                    format!(
                        "applied {} radioA writes, {} radioB writes",
                        state.radioa_writes_applied, state.radiob_writes_applied
                    ),
                    Some(writes),
                )
            })
        }
        Rtl8812auInitPhase::Channel => production_channel_sequence(
            &registers,
            &mut session.counters,
            state.channel,
            state.bandwidth,
            state.inputs.rfe_type,
            &state.inputs.radioa_plan,
            &state.inputs.radiob_plan,
        )
        .map(|writes| {
            (
                format!(
                    "programmed channel {} ({} MHz, {} MHz bandwidth, RFE type {}) in {writes} writes",
                    state.channel.number,
                    state.channel.frequency_mhz,
                    state.bandwidth.mhz(),
                    format_register_value(state.inputs.rfe_type, 2)
                ),
                Some(writes),
            )
        }),
        Rtl8812auInitPhase::TxSchedulerTail => {
            run_rtl8812au_tx_scheduler_tail(&registers, &mut session.counters).map(|execution| {
                (
                    format!(
                        "applied {} late Linux USB TX scheduler writes",
                        execution.register_writes
                    ),
                    Some(execution.register_writes),
                )
            })
        }
        Rtl8812auInitPhase::RfCalibrationBeforeChannel
        | Rtl8812auInitPhase::RfCalibrationAfterChannel
        | Rtl8812auInitPhase::RfCalibrationBeforeTx => Ok((
            "skipped diagnostic-only RF calibration probe in direct production runtime path"
                .to_string(),
            Some(0),
        )),
    };

    match result {
        Ok((detail, Some(writes))) => {
            Ok(RuntimeSameSessionInitPhaseSummary::completed_with_writes(
                phase,
                detail,
                writes,
                before,
                session.counters,
            ))
        }
        Ok((detail, None)) => Ok(RuntimeSameSessionInitPhaseSummary::completed(
            phase,
            detail,
            before,
            session.counters,
        )),
        Err(error) => Err(RuntimeSameSessionInitPhaseFailure::new(
            RuntimeSameSessionInitPhaseSummary::blocked(
                phase,
                error.message.clone(),
                before,
                session.counters,
            ),
            error,
        )),
    }
}

#[derive(Debug, Clone, Copy)]
struct ProductionFirmwareStats {
    bytes_written: u64,
    control_writes: u64,
    checksum_poll_attempts: u32,
    ready_poll_attempts: u32,
}

#[derive(Debug, Clone, Copy)]
struct ProductionQueueLayout {
    bulk_out_endpoint_count: usize,
    hpq: u8,
    lpq: u8,
    npq: u8,
    pubq: u8,
    rqpn_npq: u8,
    rqpn: u32,
    queue_map: u16,
}

fn production_readback_error(
    register_name: &'static str,
    mask: impl Into<u64>,
    expected: impl Into<u64>,
    actual: impl Into<u64>,
    digits: usize,
) -> RuntimeRadioError {
    RuntimeRadioError::new(
        "register_readback_mismatch",
        format!(
            "{register_name} expected mask {} to equal {}, got {}",
            format_register_value(mask, digits),
            format_register_value(expected, digits),
            format_register_value(actual, digits)
        ),
    )
}

const PRODUCTION_READBACK_VERIFY_ATTEMPTS: u32 = 5;
const PRODUCTION_READBACK_VERIFY_DELAY: Duration = Duration::from_millis(1);

fn production_verify8_readback<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    mask: u8,
    expected: u8,
    phase: &'static str,
) -> Result<u8, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let expected = expected & mask;
    let mut actual = 0;
    for attempt in 1..=PRODUCTION_READBACK_VERIFY_ATTEMPTS {
        let after = read8_with_counter(registers, counters, address, register_name, phase)?;
        actual = after & mask;
        if actual == expected {
            return Ok(after);
        }
        if attempt < PRODUCTION_READBACK_VERIFY_ATTEMPTS {
            thread::sleep(PRODUCTION_READBACK_VERIFY_DELAY);
        }
    }
    Err(production_readback_error(
        register_name,
        mask,
        expected,
        actual,
        2,
    ))
}

fn production_verify16_readback<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    mask: u16,
    expected: u16,
    phase: &'static str,
) -> Result<u16, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let expected = expected & mask;
    let mut actual = 0;
    for attempt in 1..=PRODUCTION_READBACK_VERIFY_ATTEMPTS {
        let after = read16_with_counter(registers, counters, address, register_name, phase)?;
        actual = after & mask;
        if actual == expected {
            return Ok(after);
        }
        if attempt < PRODUCTION_READBACK_VERIFY_ATTEMPTS {
            thread::sleep(PRODUCTION_READBACK_VERIFY_DELAY);
        }
    }
    Err(production_readback_error(
        register_name,
        mask,
        expected,
        actual,
        4,
    ))
}

fn production_verify32_readback<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    mask: u32,
    expected: u32,
    phase: &'static str,
) -> Result<u32, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let expected = expected & mask;
    let mut actual = 0;
    for attempt in 1..=PRODUCTION_READBACK_VERIFY_ATTEMPTS {
        let after = read32_with_counter(registers, counters, address, register_name, phase)?;
        actual = after & mask;
        if actual == expected {
            return Ok(after);
        }
        if attempt < PRODUCTION_READBACK_VERIFY_ATTEMPTS {
            thread::sleep(PRODUCTION_READBACK_VERIFY_DELAY);
        }
    }
    Err(production_readback_error(
        register_name,
        mask,
        expected,
        actual,
        8,
    ))
}

fn production_rmw8<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    mask: u8,
    value: u8,
    verify_readback: bool,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before = read8_with_counter(registers, counters, address, register_name, phase)?;
    let written = (before & !mask) | (value & mask);
    write8_with_counter(registers, counters, address, written, register_name, phase)?;
    if verify_readback {
        production_verify8_readback(
            registers,
            counters,
            register_name,
            address,
            mask,
            value,
            phase,
        )?;
    }
    Ok(())
}

fn production_rmw8_preserve<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    preserve_mask: u8,
    value_mask: u8,
    value: u8,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before = read8_with_counter(registers, counters, address, register_name, phase)?;
    let written = (before & preserve_mask) | (value & value_mask);
    write8_with_counter(registers, counters, address, written, register_name, phase)?;
    production_verify8_readback(
        registers,
        counters,
        register_name,
        address,
        value_mask,
        value,
        phase,
    )?;
    Ok(())
}

fn production_rmw16_preserve<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    preserve_mask: u16,
    value_mask: u16,
    value: u16,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before = read16_with_counter(registers, counters, address, register_name, phase)?;
    let written = (before & preserve_mask) | (value & value_mask);
    write16_with_counter(registers, counters, address, written, register_name, phase)?;
    production_verify16_readback(
        registers,
        counters,
        register_name,
        address,
        value_mask,
        value,
        phase,
    )?;
    Ok(())
}

fn production_rmw32_preserve<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    preserve_mask: u32,
    value_mask: u32,
    value: u32,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before = read32_with_counter(registers, counters, address, register_name, phase)?;
    let written = (before & preserve_mask) | (value & value_mask);
    write32_with_counter(registers, counters, address, written, register_name, phase)?;
    production_verify32_readback(
        registers,
        counters,
        register_name,
        address,
        value_mask,
        value,
        phase,
    )?;
    Ok(())
}

fn production_write8_verify<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    value: u8,
    verify_mask: u8,
    verify_value: u8,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    write8_with_counter(registers, counters, address, value, register_name, phase)?;
    if verify_mask != 0 {
        production_verify8_readback(
            registers,
            counters,
            register_name,
            address,
            verify_mask,
            verify_value,
            phase,
        )?;
    }
    Ok(())
}

fn production_write16_verify<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    value: u16,
    verify_mask: u16,
    verify_value: u16,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    write16_with_counter(registers, counters, address, value, register_name, phase)?;
    if verify_mask != 0 {
        production_verify16_readback(
            registers,
            counters,
            register_name,
            address,
            verify_mask,
            verify_value,
            phase,
        )?;
    }
    Ok(())
}

fn production_write32_verify<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    value: u32,
    verify_mask: u32,
    verify_value: u32,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    write32_with_counter(registers, counters, address, value, register_name, phase)?;
    if verify_mask != 0 {
        production_verify32_readback(
            registers,
            counters,
            register_name,
            address,
            verify_mask,
            verify_value,
            phase,
        )?;
    }
    Ok(())
}

fn production_poll8<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    mask: u8,
    expected: u8,
    attempts: u32,
    delay: Duration,
) -> Result<u32, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut last = 0u8;
    for attempt in 1..=attempts {
        last = read8_with_counter(registers, counters, address, register_name, "poll8")?;
        if (last & mask) == (expected & mask) {
            return Ok(attempt);
        }
        if !delay.is_zero() {
            thread::sleep(delay);
        }
    }
    Err(RuntimeRadioError::new(
        "register_poll_timeout",
        format!(
            "{register_name} expected mask {} to equal {}, last value {} after {attempts} attempts",
            format_register_value(mask, 2),
            format_register_value(expected & mask, 2),
            format_register_value(last, 2)
        ),
    ))
}

fn production_poll32_min<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    mask: u32,
    expected: u32,
    min_attempts: u32,
    timeout: Duration,
    delay: Duration,
) -> Result<(u32, u32), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let started = std::time::Instant::now();
    let mut attempts = 0u32;
    loop {
        attempts = attempts.saturating_add(1);
        let value = read32_with_counter(registers, counters, address, register_name, "poll32")?;
        if (value & mask) == (expected & mask) {
            return Ok((attempts, value));
        }
        if started.elapsed() >= timeout && attempts >= min_attempts {
            return Err(RuntimeRadioError::new(
                "register_poll_timeout",
                format!(
                    "{register_name} expected mask {} to equal {}, last value {} after {attempts} attempts",
                    format_register_value(mask, 8),
                    format_register_value(expected & mask, 8),
                    format_register_value(value, 8)
                ),
            ));
        }
        if !delay.is_zero() {
            thread::sleep(delay);
        }
    }
}

fn production_power_on_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    _timeout: Duration,
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before = counters.usb_control_writes;
    production_rmw8(
        registers,
        counters,
        "REG_APS_FSMCO + 1",
        REG_APS_FSMCO_PLUS_1,
        BIT2,
        0,
        true,
        "power-on",
    )?;
    production_poll8(
        registers,
        counters,
        "REG_APS_FSMCO + 2",
        REG_APS_FSMCO_PLUS_2,
        BIT1,
        BIT1,
        200,
        Duration::from_micros(10),
    )?;
    production_rmw8(
        registers,
        counters,
        "REG_APS_FSMCO + 1",
        REG_APS_FSMCO_PLUS_1,
        BIT3,
        0,
        true,
        "power-on",
    )?;
    production_rmw8(
        registers,
        counters,
        "REG_APS_FSMCO + 1",
        REG_APS_FSMCO_PLUS_1,
        BIT0,
        BIT0,
        false,
        "power-on",
    )?;
    production_poll8(
        registers,
        counters,
        "REG_APS_FSMCO + 1",
        REG_APS_FSMCO_PLUS_1,
        BIT0,
        0,
        200,
        Duration::from_micros(10),
    )?;
    production_rmw8(
        registers,
        counters,
        "REG_AFE_XTAL_CTRL",
        REG_AFE_XTAL_CTRL,
        BIT1,
        0,
        true,
        "power-on",
    )?;
    production_rmw8(
        registers,
        counters,
        "REG_AFE_PLL_CTRL",
        REG_AFE_PLL_CTRL,
        BIT3,
        0,
        true,
        "power-on",
    )?;
    production_write16_verify(
        registers,
        counters,
        "REG_CR",
        REG_CR,
        0,
        u16::MAX,
        0,
        "power-on",
    )?;
    let cr = read16_with_counter(registers, counters, REG_CR, "REG_CR", "power-on")?;
    production_write16_verify(
        registers,
        counters,
        "REG_CR",
        REG_CR,
        cr | CR_ENABLE_BITS,
        CR_ENABLE_BITS,
        CR_ENABLE_BITS,
        "power-on",
    )?;
    for (register_name, address, value) in [
        ("REG_RF_CTRL", REG_RF_CTRL, 0x05),
        ("REG_RF_CTRL", REG_RF_CTRL, 0x07),
        ("REG_RF_B_CTRL_8812", REG_RF_B_CTRL_8812, 0x05),
        ("REG_RF_B_CTRL_8812", REG_RF_B_CTRL_8812, 0x07),
    ] {
        write8_with_counter(
            registers,
            counters,
            address,
            value,
            register_name,
            "power-on",
        )?;
    }
    Ok(usize::try_from(counters.usb_control_writes.saturating_sub(before)).unwrap_or(usize::MAX))
}

fn production_firmware_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    firmware_payload: &[u8],
    _timeout: Duration,
) -> Result<ProductionFirmwareStats, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_writes = counters.usb_control_writes;
    let mut bytes_written = 0u64;

    production_firmware_preflight_reset_loaded_code(registers, counters)?;
    production_rmw8(
        registers,
        counters,
        "REG_MCUFWDL",
        REG_MCUFWDL,
        MCUFWDL_EN,
        MCUFWDL_EN,
        true,
        "firmware-download-enable",
    )?;
    production_rmw8(
        registers,
        counters,
        "REG_MCUFWDL + 2",
        REG_MCUFWDL_PLUS_2,
        BIT3,
        0,
        true,
        "firmware-download-enable",
    )?;

    let mut checksum_poll_attempts = 0;
    let mut download_result = Err(RuntimeRadioError::new(
        "firmware_download_not_started",
        "firmware download loop did not run",
    ));
    for attempt in 1..=3 {
        production_rmw8(
            registers,
            counters,
            "REG_MCUFWDL",
            REG_MCUFWDL,
            FWDL_CHKSUM_RPT_U8,
            FWDL_CHKSUM_RPT_U8,
            false,
            "firmware-download",
        )?;
        bytes_written = bytes_written.saturating_add(production_write_firmware_image(
            registers,
            counters,
            firmware_payload,
            attempt,
        )?);
        match production_poll32_min(
            registers,
            counters,
            "REG_MCUFWDL",
            REG_MCUFWDL,
            FWDL_CHKSUM_RPT_U32,
            FWDL_CHKSUM_RPT_U32,
            5,
            Duration::from_millis(50),
            Duration::from_micros(1000),
        ) {
            Ok((attempts, _value)) => {
                checksum_poll_attempts = attempts;
                download_result = Ok(());
                break;
            }
            Err(error) => {
                download_result = Err(error);
            }
        }
    }

    let disable_result = production_rmw8(
        registers,
        counters,
        "REG_MCUFWDL",
        REG_MCUFWDL,
        MCUFWDL_EN,
        0,
        true,
        "firmware-download-disable",
    );
    download_result?;
    disable_result?;

    let before = read32_with_counter(
        registers,
        counters,
        REG_MCUFWDL,
        "REG_MCUFWDL",
        "firmware-ready",
    )?;
    let written = (before | MCUFWDL_RDY) & !WINTINI_RDY;
    production_write32_verify(
        registers,
        counters,
        "REG_MCUFWDL",
        REG_MCUFWDL,
        written,
        0,
        0,
        "firmware-ready",
    )?;
    production_firmware_8051_reset_8812(registers, counters)?;
    let (ready_poll_attempts, _value) = production_poll32_min(
        registers,
        counters,
        "REG_MCUFWDL",
        REG_MCUFWDL,
        WINTINI_RDY,
        WINTINI_RDY,
        10,
        Duration::from_millis(200),
        Duration::from_micros(1000),
    )?;

    Ok(ProductionFirmwareStats {
        bytes_written,
        control_writes: counters.usb_control_writes.saturating_sub(before_writes),
        checksum_poll_attempts,
        ready_poll_attempts,
    })
}

fn production_firmware_preflight_reset_loaded_code<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let value = read8_with_counter(
        registers,
        counters,
        REG_MCUFWDL,
        "REG_MCUFWDL",
        "firmware-preflight",
    )?;
    if value & RAM_DL_SEL == 0 {
        return Ok(());
    }
    production_write8_verify(
        registers,
        counters,
        "REG_MCUFWDL",
        REG_MCUFWDL,
        0,
        RAM_DL_SEL,
        0,
        "firmware-preflight",
    )?;
    production_firmware_8051_reset_8812(registers, counters)
}

fn production_write_firmware_image<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    firmware_payload: &[u8],
    _attempt: u32,
) -> Result<u64, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut bytes_written = 0u64;
    for (page, chunk) in firmware_payload.chunks(MAX_DLFW_PAGE_SIZE).enumerate() {
        production_rmw8(
            registers,
            counters,
            "REG_MCUFWDL + 2",
            REG_MCUFWDL_PLUS_2,
            0x07,
            (page as u8) & 0x07,
            true,
            "firmware-download",
        )?;
        bytes_written = bytes_written.saturating_add(production_write_firmware_page(
            registers, counters, page, chunk,
        )?);
    }
    Ok(bytes_written)
}

fn production_write_firmware_page<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    _page: usize,
    bytes: &[u8],
) -> Result<u64, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut bytes_written = 0u64;
    let full_block_len = bytes.len() / MAX_REG_BLOCK_SIZE * MAX_REG_BLOCK_SIZE;
    for (index, block) in bytes[..full_block_len]
        .chunks(MAX_REG_BLOCK_SIZE)
        .enumerate()
    {
        let page_offset = index * MAX_REG_BLOCK_SIZE;
        production_firmware_write_data(registers, counters, page_offset, block)?;
        bytes_written = bytes_written.saturating_add(block.len() as u64);
    }

    let remainder = &bytes[full_block_len..];
    let remainder_block_len =
        remainder.len() / FIRMWARE_REMAINDER_BLOCK_SIZE * FIRMWARE_REMAINDER_BLOCK_SIZE;
    for (index, block) in remainder[..remainder_block_len]
        .chunks(FIRMWARE_REMAINDER_BLOCK_SIZE)
        .enumerate()
    {
        let page_offset = full_block_len + index * FIRMWARE_REMAINDER_BLOCK_SIZE;
        production_firmware_write_data(registers, counters, page_offset, block)?;
        bytes_written = bytes_written.saturating_add(block.len() as u64);
    }
    for (index, byte) in remainder[remainder_block_len..].iter().enumerate() {
        let page_offset = full_block_len + remainder_block_len + index;
        production_firmware_write_data(
            registers,
            counters,
            page_offset,
            std::slice::from_ref(byte),
        )?;
        bytes_written = bytes_written.saturating_add(1);
    }
    Ok(bytes_written)
}

fn production_firmware_write_data<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    page_offset: usize,
    data: &[u8],
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let address = production_firmware_page_address(page_offset)?;
    if data.len() == 1 {
        write8_with_counter(
            registers,
            counters,
            address,
            data[0],
            "FW_START_ADDRESS + page_offset",
            "firmware-download",
        )
    } else {
        registers.write_block(address, data).map_err(|error| {
            RuntimeRadioError::new(
                "firmware_write_failed",
                format!(
                    "firmware write offset={page_offset} len={} addr={} failed: {error}",
                    data.len(),
                    format_register_address(address)
                ),
            )
        })?;
        counters.usb_control_writes = counters.usb_control_writes.saturating_add(1);
        Ok(())
    }
}

fn production_firmware_page_address(page_offset: usize) -> Result<u16, RuntimeRadioError> {
    let offset = u16::try_from(page_offset).map_err(|_| {
        RuntimeRadioError::new(
            "firmware_offset_too_large",
            format!("firmware page offset {page_offset} does not fit in a register address"),
        )
    })?;
    FW_START_ADDRESS.checked_add(offset).ok_or_else(|| {
        RuntimeRadioError::new(
            "firmware_address_overflow",
            format!(
                "firmware address overflow: start={} offset={page_offset}",
                format_register_address(FW_START_ADDRESS)
            ),
        )
    })
}

fn production_firmware_8051_reset_8812<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_rmw8(
        registers,
        counters,
        "REG_RSV_CTRL",
        REG_RSV_CTRL,
        BIT1,
        0,
        true,
        "firmware-8051-reset",
    )?;
    production_rmw8(
        registers,
        counters,
        "REG_RSV_CTRL + 1",
        REG_RSV_CTRL + 1,
        BIT3,
        0,
        true,
        "firmware-8051-reset",
    )?;
    let sys_func = read8_with_counter(
        registers,
        counters,
        REG_SYS_FUNC_EN_PLUS_1,
        "REG_SYS_FUNC_EN + 1",
        "firmware-8051-reset",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_SYS_FUNC_EN + 1",
        REG_SYS_FUNC_EN_PLUS_1,
        sys_func & !BIT2,
        BIT2,
        0,
        "firmware-8051-reset",
    )?;
    production_rmw8(
        registers,
        counters,
        "REG_RSV_CTRL",
        REG_RSV_CTRL,
        BIT1,
        0,
        true,
        "firmware-8051-reset",
    )?;
    production_rmw8(
        registers,
        counters,
        "REG_RSV_CTRL + 1",
        REG_RSV_CTRL + 1,
        BIT3,
        BIT3,
        true,
        "firmware-8051-reset",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_SYS_FUNC_EN + 1",
        REG_SYS_FUNC_EN_PLUS_1,
        sys_func | BIT2,
        BIT2,
        BIT2,
        "firmware-8051-reset",
    )
}

fn production_llt_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    _timeout: Duration,
) -> Result<u64, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_preflight_cr(registers, counters, "LLT programming")?;
    let mut entries = 0u64;
    for address in 0..(TX_PAGE_BOUNDARY_8812 - 1) {
        production_llt_write(registers, counters, address, address + 1)?;
        entries = entries.saturating_add(1);
    }
    production_llt_write(registers, counters, TX_PAGE_BOUNDARY_8812 - 1, 0xff)?;
    entries = entries.saturating_add(1);
    for address in TX_PAGE_BOUNDARY_8812..LAST_ENTRY_OF_TX_PKT_BUFFER_8812 {
        production_llt_write(registers, counters, address, address + 1)?;
        entries = entries.saturating_add(1);
    }
    production_llt_write(
        registers,
        counters,
        LAST_ENTRY_OF_TX_PKT_BUFFER_8812,
        TX_PAGE_BOUNDARY_8812,
    )?;
    entries = entries.saturating_add(1);
    Ok(entries)
}

fn production_llt_write<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    llt_address: u8,
    llt_data: u8,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let written = encode_llt_write(llt_address, llt_data);
    write32_with_counter(
        registers,
        counters,
        REG_LLT_INIT,
        written,
        "REG_LLT_INIT",
        "llt",
    )?;
    let mut last = 0u32;
    for _attempt in 1..=25 {
        last = read32_with_counter(registers, counters, REG_LLT_INIT, "REG_LLT_INIT", "llt")?;
        if llt_op_value(last) == LLT_NO_ACTIVE {
            return Ok(());
        }
        thread::sleep(Duration::from_micros(10));
    }
    Err(RuntimeRadioError::new(
        "llt_poll_timeout",
        format!(
            "LLT write addr=0x{llt_address:02x} data=0x{llt_data:02x} did not become idle, last {}",
            format_register_value(last, 8)
        ),
    ))
}

fn encode_llt_write(address: u8, data: u8) -> u32 {
    (u32::from(address) << 8) | u32::from(data) | (LLT_WRITE_ACCESS << LLT_OP_SHIFT)
}

fn llt_op_value(value: u32) -> u32 {
    (value >> LLT_OP_SHIFT) & LLT_OP_MASK
}

fn production_apply_mac_table_plan<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    plan: &RealtekTablePlan,
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    if plan.kind != RealtekTableKind::Mac {
        return Err(RuntimeRadioError::new(
            "invalid_mac_table_kind",
            format!("{} has non-MAC table kind {:?}", plan.array_name, plan.kind),
        ));
    }
    let mut writes = 0usize;
    for action in &plan.actions {
        match action.kind {
            RealtekTableActionKind::Delay => {
                if let Some(delay_us) = action.delay_us {
                    if delay_us > 0 {
                        thread::sleep(Duration::from_micros(delay_us));
                    }
                }
            }
            RealtekTableActionKind::Write => {
                let address = u16::try_from(action.address).map_err(|_| {
                    RuntimeRadioError::new(
                        "mac_table_address_out_of_range",
                        format!(
                            "{} pair {} address {} does not fit a USB register address",
                            plan.array_name, action.pair_index, action.address_hex
                        ),
                    )
                })?;
                let data = action.data.ok_or_else(|| {
                    RuntimeRadioError::new(
                        "mac_table_write_missing_data",
                        format!(
                            "{} pair {} is a write action with no data",
                            plan.array_name, action.pair_index
                        ),
                    )
                })?;
                let value = u8::try_from(data).map_err(|_| {
                    RuntimeRadioError::new(
                        "mac_table_write_data_out_of_range",
                        format!(
                            "{} pair {} data {} does not fit an 8-bit MAC table write",
                            plan.array_name,
                            action.pair_index,
                            action.data_hex.as_deref().unwrap_or("<missing>")
                        ),
                    )
                })?;
                write8_with_counter(
                    registers,
                    counters,
                    address,
                    value,
                    "array_mp_8812a_mac_reg",
                    "mac-table",
                )?;
                writes += 1;
            }
        }
    }
    Ok(writes)
}

fn production_queue_layout_from_endpoints(
    endpoints: &UsbEndpoints,
) -> Result<ProductionQueueLayout, RuntimeRadioError> {
    production_queue_layout_from_bulk_out_endpoint_count(endpoints.bulk_out_all.len())
}

fn production_queue_layout_from_bulk_out_endpoint_count(
    bulk_out_endpoint_count: usize,
) -> Result<ProductionQueueLayout, RuntimeRadioError> {
    let (use_hpq, use_lpq, use_npq) = match bulk_out_endpoint_count {
        2 => (true, false, true),
        3 | 4 => (true, true, true),
        other => {
            return Err(RuntimeRadioError::new(
                "unsupported_bulk_out_endpoint_count",
                format!("queue/DMA setup supports 2, 3, or 4 bulk OUT endpoints, found {other}"),
            ))
        }
    };
    let hpq = if use_hpq { NORMAL_PAGE_NUM_HPQ_8812 } else { 0 };
    let lpq = if use_lpq { NORMAL_PAGE_NUM_LPQ_8812 } else { 0 };
    let npq = if use_npq { NORMAL_PAGE_NUM_NPQ_8812 } else { 0 };
    let pubq = TX_TOTAL_PAGE_NUMBER_8812
        .checked_sub(hpq)
        .and_then(|value| value.checked_sub(lpq))
        .and_then(|value| value.checked_sub(npq))
        .ok_or_else(|| {
            RuntimeRadioError::new(
                "invalid_queue_page_layout",
                "queue reserved-page layout underflowed public queue pages",
            )
        })?;
    let rqpn_npq = npq;
    let rqpn = u32::from(hpq) | (u32::from(lpq) << 8) | (u32::from(pubq) << 16) | LD_RQPN;
    let queue_map = production_queue_map_for_endpoint_count(bulk_out_endpoint_count);
    Ok(ProductionQueueLayout {
        bulk_out_endpoint_count,
        hpq,
        lpq,
        npq,
        pubq,
        rqpn_npq,
        rqpn,
        queue_map,
    })
}

fn production_queue_map_for_endpoint_count(bulk_out_endpoint_count: usize) -> u16 {
    match bulk_out_endpoint_count {
        2 => production_queue_map(
            QUEUE_NORMAL,
            QUEUE_NORMAL,
            QUEUE_HIGH,
            QUEUE_HIGH,
            QUEUE_HIGH,
            QUEUE_HIGH,
        ),
        3 => production_queue_map(
            QUEUE_LOW,
            QUEUE_LOW,
            QUEUE_NORMAL,
            QUEUE_HIGH,
            QUEUE_HIGH,
            QUEUE_HIGH,
        ),
        4 => production_queue_map(
            QUEUE_LOW,
            QUEUE_LOW,
            QUEUE_NORMAL,
            QUEUE_NORMAL,
            QUEUE_EXTRA,
            QUEUE_HIGH,
        ),
        _ => 0,
    }
}

fn production_queue_map(beq: u16, bkq: u16, viq: u16, voq: u16, mgq: u16, hiq: u16) -> u16 {
    ((hiq & 0x3) << 14)
        | ((mgq & 0x3) << 12)
        | ((bkq & 0x3) << 10)
        | ((beq & 0x3) << 8)
        | ((viq & 0x3) << 6)
        | ((voq & 0x3) << 4)
}

fn production_queue_dma_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    layout: ProductionQueueLayout,
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_preflight_cr(registers, counters, "queue/DMA programming")?;
    let before_writes = counters.usb_control_writes;
    production_write8_verify(
        registers,
        counters,
        "REG_RQPN_NPQ",
        REG_RQPN_NPQ,
        layout.rqpn_npq,
        u8::MAX,
        layout.rqpn_npq,
        "queue-dma",
    )?;
    production_write32_verify(
        registers,
        counters,
        "REG_RQPN",
        REG_RQPN,
        layout.rqpn,
        RQPN_PAGE_MASK,
        layout.rqpn,
        "queue-dma",
    )?;
    for (register_name, address) in [
        ("REG_BCNQ_BDNY", REG_BCNQ_BDNY),
        ("REG_MGQ_BDNY", REG_MGQ_BDNY),
        ("REG_WMAC_LBK_BF_HD", REG_WMAC_LBK_BF_HD),
        ("REG_TRXFF_BNDY", REG_TRXFF_BNDY),
        ("REG_TDECTRL + 1", REG_TDECTRL + 1),
    ] {
        production_write8_verify(
            registers,
            counters,
            register_name,
            address,
            TX_PAGE_BOUNDARY_8812,
            u8::MAX,
            TX_PAGE_BOUNDARY_8812,
            "queue-dma",
        )?;
    }
    production_rmw16_preserve(
        registers,
        counters,
        "REG_TRXDMA_CTRL",
        REG_TRXDMA_CTRL,
        0x0007,
        TXDMA_QUEUE_MAP_MASK,
        layout.queue_map,
        "queue-dma",
    )?;
    production_write16_verify(
        registers,
        counters,
        "REG_TRXFF_BNDY + 2",
        REG_TRXFF_BNDY + 2,
        RX_DMA_BOUNDARY_8812,
        u16::MAX,
        RX_DMA_BOUNDARY_8812,
        "queue-dma",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_PBP",
        REG_PBP,
        PSTX_PBP_512,
        u8::MAX,
        PSTX_PBP_512,
        "queue-dma",
    )?;
    Ok(
        usize::try_from(counters.usb_control_writes.saturating_sub(before_writes))
            .unwrap_or(usize::MAX),
    )
}

fn production_preflight_cr<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    label: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let cr = read16_with_counter(registers, counters, REG_CR, "REG_CR", "preflight")?;
    if (cr & CR_ENABLE_BITS) != CR_ENABLE_BITS {
        return Err(RuntimeRadioError::new(
            "mac_not_powered_on",
            format!(
                "REG_CR expected block-enable mask {} to be set before {label}, got {}",
                format_register_value(CR_ENABLE_BITS, 4),
                format_register_value(cr, 4)
            ),
        ));
    }
    Ok(())
}

fn production_preflight_firmware<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    label: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_preflight_cr(registers, counters, label)?;
    let mcu = read8_with_counter(registers, counters, REG_MCUFWDL, "REG_MCUFWDL", "preflight")?;
    let firmware_ready_mask = RAM_DL_SEL | BIT6 | BIT1;
    if (mcu & firmware_ready_mask) != firmware_ready_mask {
        return Err(RuntimeRadioError::new(
            "firmware_not_ready",
            format!(
                "REG_MCUFWDL expected firmware-ready mask {} before {label}, got {}",
                format_register_value(firmware_ready_mask, 2),
                format_register_value(mcu, 2)
            ),
        ));
    }
    Ok(())
}

fn production_mac_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_preflight_firmware(registers, counters, "MAC programming")?;
    let before_writes = counters.usb_control_writes;

    production_write8_verify(
        registers,
        counters,
        "REG_RX_DRVINFO_SZ",
        REG_RX_DRVINFO_SZ,
        DRVINFO_SZ,
        u8::MAX,
        DRVINFO_SZ,
        "mac",
    )?;
    production_rmw32_preserve(
        registers,
        counters,
        "REG_CR",
        REG_CR,
        !MASK_NETTYPE,
        MASK_NETTYPE,
        NETTYPE_LINK_AP,
        "mac",
    )?;
    production_write32_verify(
        registers,
        counters,
        "REG_RCR",
        REG_RCR,
        MAC_RECEIVE_CONFIG,
        u32::MAX,
        MAC_RECEIVE_CONFIG,
        "mac",
    )?;
    for (register_name, address) in [("REG_MAR", REG_MAR), ("REG_MAR + 4", REG_MAR + 4)] {
        production_write32_verify(
            registers,
            counters,
            register_name,
            address,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            "mac",
        )?;
    }
    production_write16_verify(
        registers,
        counters,
        "REG_RXFLTMAP1",
        REG_RXFLTMAP1,
        1 << 10,
        u16::MAX,
        1 << 10,
        "mac",
    )?;
    production_rmw32_preserve(
        registers,
        counters,
        "REG_RRSR",
        REG_RRSR,
        !RATE_BITMAP_ALL,
        RATE_BITMAP_ALL,
        RATE_RRSR_CCK_ONLY_1M,
        "mac",
    )?;
    production_write16_verify(
        registers,
        counters,
        "REG_SPEC_SIFS",
        REG_SPEC_SIFS,
        0x1010,
        u16::MAX,
        0x1010,
        "mac",
    )?;
    production_write16_verify(
        registers,
        counters,
        "REG_RETRY_LIMIT",
        REG_RETRY_LIMIT,
        RETRY_LIMIT_STA,
        u16::MAX,
        RETRY_LIMIT_STA,
        "mac",
    )?;
    for (register_name, address) in [
        ("REG_SPEC_SIFS", REG_SPEC_SIFS),
        ("REG_MAC_SPEC_SIFS", REG_MAC_SPEC_SIFS),
        ("REG_SIFS_CTX", REG_SIFS_CTX),
        ("REG_SIFS_TRX", REG_SIFS_TRX),
    ] {
        production_write16_verify(
            registers,
            counters,
            register_name,
            address,
            0x100a,
            u16::MAX,
            0x100a,
            "mac",
        )?;
    }
    for (register_name, address, value) in [
        ("REG_EDCA_BE_PARAM", REG_EDCA_BE_PARAM, 0x005e_a42b),
        ("REG_EDCA_BK_PARAM", REG_EDCA_BK_PARAM, 0x0000_a44f),
        ("REG_EDCA_VI_PARAM", REG_EDCA_VI_PARAM, 0x005e_a324),
        ("REG_EDCA_VO_PARAM", REG_EDCA_VO_PARAM, 0x002f_a226),
    ] {
        production_write32_verify(
            registers,
            counters,
            register_name,
            address,
            value,
            u32::MAX,
            value,
            "mac",
        )?;
    }
    production_write8_verify(
        registers,
        counters,
        "REG_TXPAUSE",
        REG_TXPAUSE,
        0,
        u8::MAX,
        0,
        "mac",
    )?;
    for (register_name, address) in [
        ("REG_USTIME_TSF", REG_USTIME_TSF),
        ("REG_USTIME_EDCA", REG_USTIME_EDCA),
    ] {
        production_write8_verify(
            registers,
            counters,
            register_name,
            address,
            0x50,
            u8::MAX,
            0x50,
            "mac",
        )?;
    }
    production_rmw8_preserve(
        registers,
        counters,
        "REG_FWHW_TXQ_CTRL",
        REG_FWHW_TXQ_CTRL,
        !EN_AMPDU_RTY_NEW,
        EN_AMPDU_RTY_NEW,
        EN_AMPDU_RTY_NEW,
        "mac",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_ACKTO",
        REG_ACKTO,
        0x80,
        u8::MAX,
        0x80,
        "mac",
    )?;
    production_beacon_parameters_sequence(registers, counters)?;
    production_usb_burst_packet_sequence(registers, counters)?;
    production_write8_verify(
        registers,
        counters,
        "REG_HWSEQ_CTRL",
        REG_HWSEQ_CTRL,
        0xff,
        u8::MAX,
        0xff,
        "mac",
    )?;
    production_write32_verify(
        registers,
        counters,
        "REG_BAR_MODE_CTRL",
        REG_BAR_MODE_CTRL,
        BAR_MODE_CTRL_VALUE,
        BAR_MODE_CTRL_READBACK_MASK,
        BAR_MODE_CTRL_VALUE,
        "mac",
    )?;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_CR",
        REG_CR,
        !MAC_TX_RX_ENABLE_MASK,
        MAC_TX_RX_ENABLE_MASK,
        MAC_TX_RX_ENABLE_MASK,
        "mac",
    )?;

    Ok(
        usize::try_from(counters.usb_control_writes.saturating_sub(before_writes))
            .unwrap_or(usize::MAX),
    )
}

fn production_beacon_parameters_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let bcn_ctrl = u16::from(DIS_TSF_UDT) | (u16::from(DIS_TSF_UDT) << 8);
    production_write16_verify(
        registers,
        counters,
        "REG_BCN_CTRL",
        REG_BCN_CTRL,
        bcn_ctrl,
        u16::MAX,
        bcn_ctrl,
        "beacon-parameters",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_TBTT_PROHIBIT",
        REG_TBTT_PROHIBIT,
        TBTT_PROHIBIT_SETUP_TIME,
        u8::MAX,
        TBTT_PROHIBIT_SETUP_TIME,
        "beacon-parameters",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_TBTT_PROHIBIT + 1",
        REG_TBTT_PROHIBIT + 1,
        (TBTT_PROHIBIT_HOLD_TIME_STOP_BCN & 0x00ff) as u8,
        u8::MAX,
        (TBTT_PROHIBIT_HOLD_TIME_STOP_BCN & 0x00ff) as u8,
        "beacon-parameters",
    )?;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_TBTT_PROHIBIT + 2",
        REG_TBTT_PROHIBIT + 2,
        0xf0,
        0x0f,
        (TBTT_PROHIBIT_HOLD_TIME_STOP_BCN >> 8) as u8,
        "beacon-parameters",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_DRVERLYINT",
        REG_DRVERLYINT,
        DRIVER_EARLY_INT_TIME_8812,
        u8::MAX,
        DRIVER_EARLY_INT_TIME_8812,
        "beacon-parameters",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_BCNDMATIM",
        REG_BCNDMATIM,
        BCN_DMA_ATIME_INT_TIME_8812,
        u8::MAX,
        BCN_DMA_ATIME_INT_TIME_8812,
        "beacon-parameters",
    )?;
    production_write16_verify(
        registers,
        counters,
        "REG_BCNTCFG",
        REG_BCNTCFG,
        0x4413,
        u16::MAX,
        0x4413,
        "beacon-parameters",
    )
}

fn production_usb_burst_packet_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_write8_verify(
        registers,
        counters,
        "0xf050",
        0xf050,
        0x01,
        0,
        0,
        "usb-burst",
    )?;
    production_write16_verify(
        registers,
        counters,
        "REG_RXDMA_STATUS",
        REG_RXDMA_STATUS,
        0x7400,
        0,
        0,
        "usb-burst",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_RXDMA_STATUS + 1",
        REG_RXDMA_STATUS + 1,
        0xf5,
        0,
        0,
        "usb-burst",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_AMPDU_MAX_TIME_8812",
        REG_AMPDU_MAX_TIME_8812,
        0x70,
        u8::MAX,
        0x70,
        "usb-burst",
    )?;
    production_write32_verify(
        registers,
        counters,
        "REG_AMPDU_MAX_LENGTH_8812",
        REG_AMPDU_MAX_LENGTH_8812,
        u32::MAX,
        u32::MAX,
        u32::MAX,
        "usb-burst",
    )?;

    let speed_value =
        read8_with_counter(registers, counters, 0x00ff, "USB speed probe", "usb-burst")?;
    let rxdma_pro = read8_with_counter(
        registers,
        counters,
        REG_RXDMA_PRO_8812,
        "REG_RXDMA_PRO_8812",
        "usb-burst",
    )?;
    let rxdma_pro_value = if speed_value & BIT7 != 0 {
        let phy_speed =
            read8_with_counter(registers, counters, 0xfe17, "USB PHY speed", "usb-burst")?;
        if ((phy_speed >> 4) & 0x03) == 0 {
            (rxdma_pro | BIT4 | BIT3 | BIT2 | BIT1) & !BIT5
        } else {
            (rxdma_pro | BIT5 | BIT3 | BIT2 | BIT1) & !BIT4
        }
    } else {
        (rxdma_pro | BIT3 | BIT2 | BIT1) & !(BIT5 | BIT4)
    };
    production_write8_verify(
        registers,
        counters,
        "REG_RXDMA_PRO_8812",
        REG_RXDMA_PRO_8812,
        rxdma_pro_value,
        u8::MAX,
        rxdma_pro_value,
        "usb-burst",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_TDECTRL",
        REG_TDECTRL,
        0x10,
        u8::MAX,
        0x10,
        "usb-burst",
    )?;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_HT_SINGLE_AMPDU_8812",
        REG_HT_SINGLE_AMPDU_8812,
        !BIT7,
        BIT7,
        BIT7,
        "usb-burst",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_RX_PKT_LIMIT",
        REG_RX_PKT_LIMIT,
        0x18,
        u8::MAX,
        0x18,
        "usb-burst",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_PIFS",
        REG_PIFS,
        0,
        u8::MAX,
        0,
        "usb-burst",
    )?;
    production_write16_verify(
        registers,
        counters,
        "REG_MAX_AGGR_NUM",
        REG_MAX_AGGR_NUM,
        0x1f1f,
        u16::MAX,
        0x1f1f,
        "usb-burst",
    )?;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_FWHW_TXQ_CTRL",
        REG_FWHW_TXQ_CTRL,
        !EN_AMPDU_RTY_NEW,
        EN_AMPDU_RTY_NEW,
        0,
        "usb-burst",
    )?;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_RSV_CTRL",
        REG_RSV_CTRL,
        !(BIT5 | BIT6),
        BIT5 | BIT6,
        BIT5 | BIT6,
        "usb-burst",
    )?;

    for (register_name, address, low, high) in [
        ("REG_ARFR0_8812", REG_ARFR0_8812, 0x0000_0010, 0xffff_f000),
        ("REG_ARFR1_8812", REG_ARFR1_8812, 0x0000_0010, 0x003f_f000),
        ("REG_ARFR2_8812", REG_ARFR2_8812, 0x0000_0015, 0x003f_f000),
        ("REG_ARFR3_8812", REG_ARFR3_8812, 0x0000_0015, 0xffcf_f000),
    ] {
        production_write32_verify(
            registers,
            counters,
            register_name,
            address,
            low,
            u32::MAX,
            low,
            "arfr-table",
        )?;
        production_write32_verify(
            registers,
            counters,
            register_name,
            address + 4,
            high,
            u32::MAX,
            high,
            "arfr-table",
        )?;
    }
    Ok(())
}

fn production_program_efuse_macid<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<String, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    match read_rtl8812au_efuse_mac_address(registers, counters)? {
        Some(mac) => {
            let report = program_rtl8812au_local_mac(registers, mac, counters)?;
            Ok(format!(
                "programmed REG_MACID from EFUSE MAC {} (was {})",
                format_mac_address(report.written),
                format_mac_address(report.before)
            ))
        }
        None => Ok(
            "EFUSE did not contain a programmed MAC address; REG_MACID left unchanged".to_string(),
        ),
    }
}

fn format_mac_address(mac: [u8; 6]) -> String {
    mac.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn production_bb_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    state: &mut Rtl8812auProductionInitState,
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_preflight_firmware(registers, counters, "BB programming")?;
    let before_writes = counters.usb_control_writes;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_SYS_FUNC_EN",
        REG_SYS_FUNC_EN,
        !FEN_USBA,
        FEN_USBA,
        FEN_USBA,
        "bb",
    )?;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_SYS_FUNC_EN",
        REG_SYS_FUNC_EN,
        !(FEN_USBA | FEN_BB_GLB_RSTN | FEN_BBRSTB),
        FEN_USBA | FEN_BB_GLB_RSTN | FEN_BBRSTB,
        FEN_USBA | FEN_BB_GLB_RSTN | FEN_BBRSTB,
        "bb",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_RF_CTRL",
        REG_RF_CTRL,
        0x07,
        u8::MAX,
        0x07,
        "bb",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_RF_B_CTRL_8812",
        REG_RF_B_CTRL_8812,
        0x07,
        u8::MAX,
        0x07,
        "bb",
    )?;

    let (phy_writes, phy_delays) =
        production_run_bb_table_plan(registers, counters, &state.inputs.phy_plan)?;
    let (agc_writes, agc_delays) =
        production_run_bb_table_plan(registers, counters, &state.inputs.agc_plan)?;
    state.phy_writes_applied = phy_writes;
    state.agc_writes_applied = agc_writes;
    state.table_delays_applied = state
        .table_delays_applied
        .saturating_add(phy_delays)
        .saturating_add(agc_delays);
    production_bb_masked_write32(
        registers,
        counters,
        "REG_MAC_PHY_CTRL",
        REG_MAC_PHY_CTRL,
        RTL8812_CRYSTAL_CAP_MASK,
        0x20u32 | (0x20u32 << 6),
        "bb-crystal-cap",
    )?;
    Ok(
        usize::try_from(counters.usb_control_writes.saturating_sub(before_writes))
            .unwrap_or(usize::MAX),
    )
}

fn production_run_bb_table_plan<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    plan: &RealtekTablePlan,
) -> Result<(usize, usize), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut writes = 0usize;
    let mut delays = 0usize;
    for action in &plan.actions {
        match action.kind {
            RealtekTableActionKind::Delay => {
                let delay_us = action.delay_us.unwrap_or_default();
                if delay_us > 0 {
                    thread::sleep(Duration::from_micros(delay_us));
                }
                delays += 1;
            }
            RealtekTableActionKind::Write => {
                let address = u16::try_from(action.address).map_err(|_| {
                    RuntimeRadioError::new(
                        "bb_table_address_out_of_range",
                        format!(
                            "{} pair {} address {} does not fit a USB register address",
                            plan.array_name, action.pair_index, action.address_hex
                        ),
                    )
                })?;
                let bitmask = action.bitmask.unwrap_or(u32::MAX);
                let data = action.data.ok_or_else(|| {
                    RuntimeRadioError::new(
                        "bb_table_write_missing_data",
                        format!(
                            "{} pair {} is a write action with no data",
                            plan.array_name, action.pair_index
                        ),
                    )
                })?;
                bb_set_bb_reg(
                    registers,
                    counters,
                    address,
                    bitmask,
                    data,
                    "realtek-bb-table",
                )
                .map_err(|error| {
                    RuntimeRadioError::new(
                        "bb_table_write_failed",
                        format!(
                            "{} pair {} write addr={} data={} failed: {}",
                            plan.array_name,
                            action.pair_index,
                            action.address_hex,
                            action.data_hex.as_deref().unwrap_or("<missing>"),
                            error.message
                        ),
                    )
                })?;
                writes += 1;
                thread::sleep(Duration::from_micros(1));
            }
        }
    }
    Ok((writes, delays))
}

fn production_bb_masked_write32<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    bitmask: u32,
    data: u32,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    bb_set_bb_reg(registers, counters, address, bitmask, data, register_name)?;
    if bitmask != 0 {
        let after = read32_with_counter(registers, counters, address, register_name, phase)?;
        let expected = (data << bitmask.trailing_zeros()) & bitmask;
        if (after & bitmask) != expected {
            return Err(production_readback_error(
                register_name,
                bitmask,
                expected,
                after & bitmask,
                8,
            ));
        }
    }
    Ok(())
}

fn production_rf_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    state: &mut Rtl8812auProductionInitState,
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_rf_preflight(registers, counters)?;
    let before_writes = counters.usb_control_writes;
    let (radioa_writes, radioa_delays) =
        production_run_rf_table_plan(registers, counters, &state.inputs.radioa_plan)?;
    let (radiob_writes, radiob_delays) =
        production_run_rf_table_plan(registers, counters, &state.inputs.radiob_plan)?;
    state.radioa_writes_applied = radioa_writes;
    state.radiob_writes_applied = radiob_writes;
    state.table_delays_applied = state
        .table_delays_applied
        .saturating_add(radioa_delays)
        .saturating_add(radiob_delays);
    Ok(
        usize::try_from(counters.usb_control_writes.saturating_sub(before_writes))
            .unwrap_or(usize::MAX),
    )
}

fn production_rf_preflight<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_preflight_firmware(registers, counters, "RF programming")?;
    let sys = read8_with_counter(
        registers,
        counters,
        REG_SYS_FUNC_EN,
        "REG_SYS_FUNC_EN",
        "rf-preflight",
    )?;
    let expected = FEN_USBA | FEN_BB_GLB_RSTN | FEN_BBRSTB;
    if (sys & expected) != expected {
        return Err(RuntimeRadioError::new(
            "bb_not_ready",
            format!(
                "REG_SYS_FUNC_EN expected BB-ready mask {} before RF programming, got {}",
                format_register_value(expected, 2),
                format_register_value(sys, 2)
            ),
        ));
    }
    Ok(())
}

fn production_run_rf_table_plan<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    plan: &RealtekTablePlan,
) -> Result<(usize, usize), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let path = match plan.kind {
        RealtekTableKind::RfRadioA => Rtl8812auRfPath::A,
        RealtekTableKind::RfRadioB => Rtl8812auRfPath::B,
        other => {
            return Err(RuntimeRadioError::new(
                "invalid_rf_table_kind",
                format!("{} has non-RF table kind {other:?}", plan.array_name),
            ))
        }
    };
    let mut writes = 0usize;
    let mut delays = 0usize;
    for action in &plan.actions {
        match action.kind {
            RealtekTableActionKind::Delay => {
                let delay_us = action.delay_us.unwrap_or_default();
                if delay_us > 0 {
                    thread::sleep(Duration::from_micros(delay_us));
                }
                delays += 1;
            }
            RealtekTableActionKind::Write => {
                let data = action.data.ok_or_else(|| {
                    RuntimeRadioError::new(
                        "rf_table_write_missing_data",
                        format!(
                            "{} pair {} is a write action with no data",
                            plan.array_name, action.pair_index
                        ),
                    )
                })?;
                rf_serial_write_single_path(registers, path, action.address, data, counters)
                    .map_err(|error| {
                        RuntimeRadioError::new(
                            "rf_table_write_failed",
                            format!(
                                "{} pair {} RF addr={} data={} failed: {}",
                                plan.array_name,
                                action.pair_index,
                                action.address_hex,
                                action.data_hex.as_deref().unwrap_or("<missing>"),
                                error.message
                            ),
                        )
                    })?;
                writes += 1;
                thread::sleep(Duration::from_micros(1));
            }
        }
    }
    Ok((writes, delays))
}

fn production_channel_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    channel: Channel,
    bandwidth: Bandwidth,
    rfe_type: u8,
    radioa_plan: &RealtekTablePlan,
    radiob_plan: &RealtekTablePlan,
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_writes = counters.usb_control_writes;
    let programming_channel_number = production_channel_programming_number(channel, bandwidth)?;
    let data_sc = production_data_secondary_channel_setting(channel, bandwidth)?;
    production_rf_preflight(registers, counters)?;
    let mut rf_path_a = production_last_rf_register_data(radioa_plan, RF_CHNLBW_JAGUAR)?;
    let mut rf_path_b = production_last_rf_register_data(radiob_plan, RF_CHNLBW_JAGUAR)?;

    production_switch_wireless_band_8812(registers, counters, channel.band, rfe_type)?;
    production_bb_masked_write32(
        registers,
        counters,
        "rOFDMCCKEN_Jaguar",
        REG_OFDMCCKEN_JAGUAR,
        0x3000_0000,
        0x03,
        "band-switch",
    )?;
    production_bb_masked_write32(
        registers,
        counters,
        "rFc_area_Jaguar",
        REG_FC_AREA_JAGUAR,
        0x1ffe_0000,
        production_fc_area_data(programming_channel_number),
        "channel",
    )?;

    rf_path_a = apply_rf_mask(
        rf_path_a,
        RF_CHNLBW_MOD_AG_MASK,
        production_rf_mod_ag_data(programming_channel_number),
    );
    rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::A,
        RF_CHNLBW_JAGUAR,
        rf_path_a,
        counters,
    )?;
    production_fix_spur_8812a(registers, counters, programming_channel_number, bandwidth)?;
    rf_path_a = apply_rf_mask(
        rf_path_a,
        RF_CHNLBW_CHANNEL_MASK,
        u32::from(programming_channel_number),
    );
    rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::A,
        RF_CHNLBW_JAGUAR,
        rf_path_a,
        counters,
    )?;

    rf_path_b = apply_rf_mask(
        rf_path_b,
        RF_CHNLBW_MOD_AG_MASK,
        production_rf_mod_ag_data(programming_channel_number),
    );
    rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::B,
        RF_CHNLBW_JAGUAR,
        rf_path_b,
        counters,
    )?;
    production_fix_spur_8812a(registers, counters, programming_channel_number, bandwidth)?;
    rf_path_b = apply_rf_mask(
        rf_path_b,
        RF_CHNLBW_CHANNEL_MASK,
        u32::from(programming_channel_number),
    );
    rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::B,
        RF_CHNLBW_JAGUAR,
        rf_path_b,
        counters,
    )?;

    let wmac_bandwidth_bits = match bandwidth {
        Bandwidth::Mhz20 => 0x0000,
        Bandwidth::Mhz40 => 0x0080,
        Bandwidth::Mhz80 => 0x0100,
    };
    production_rmw16_preserve(
        registers,
        counters,
        "REG_WMAC_TRXPTCL_CTL",
        REG_WMAC_TRXPTCL_CTL,
        0xfe7f,
        0x0180,
        wmac_bandwidth_bits,
        "bandwidth",
    )?;
    production_write8_verify(
        registers,
        counters,
        "REG_DATA_SC_8812",
        REG_DATA_SC_8812,
        data_sc,
        u8::MAX,
        data_sc,
        "bandwidth",
    )?;
    let bw_indication = read8_with_counter(
        registers,
        counters,
        REG_BW_INDICATION_JAGUAR + 3,
        "rBWIndication_Jaguar + 3",
        "bandwidth",
    )?;
    let rf_mode_data = match bandwidth {
        Bandwidth::Mhz20 => 0x0030_0200,
        Bandwidth::Mhz40 => 0x0030_0201,
        Bandwidth::Mhz80 => 0x0030_0202,
    };
    production_bb_masked_write32(
        registers,
        counters,
        "rRFMOD_Jaguar",
        REG_RF_MOD_JAGUAR,
        0x0030_03c3,
        rf_mode_data,
        "bandwidth",
    )?;
    production_bb_masked_write32(
        registers,
        counters,
        "rADC_Buf_Clk_Jaguar",
        REG_ADC_BUF_CLK_JAGUAR,
        1 << 30,
        if bandwidth == Bandwidth::Mhz80 { 1 } else { 0 },
        "bandwidth",
    )?;
    if matches!(bandwidth, Bandwidth::Mhz40 | Bandwidth::Mhz80) {
        production_bb_masked_write32(
            registers,
            counters,
            "rRFMOD_Jaguar",
            REG_RF_MOD_JAGUAR,
            0x0000_003c,
            u32::from(data_sc),
            "bandwidth",
        )?;
        production_bb_masked_write32(
            registers,
            counters,
            "rCCAonSec_Jaguar",
            REG_CCA_ON_SEC_JAGUAR,
            0xf000_0000,
            u32::from(data_sc),
            "bandwidth",
        )?;
    }

    let l1_peak = match bandwidth {
        Bandwidth::Mhz20 => 7,
        Bandwidth::Mhz40 if (bw_indication & BIT2) != 0 => 6,
        Bandwidth::Mhz40 => 7,
        Bandwidth::Mhz80 if (bw_indication & BIT2) != 0 => 5,
        Bandwidth::Mhz80 => 6,
    };
    production_bb_masked_write32(
        registers,
        counters,
        "rL1PeakTH_Jaguar",
        REG_L1_PEAK_TH_JAGUAR,
        0x03c0_0000,
        l1_peak,
        "bandwidth",
    )?;
    if bandwidth == Bandwidth::Mhz40 {
        production_bb_masked_write32(
            registers,
            counters,
            "rCCK_System_Jaguar",
            REG_CCK_SYSTEM_JAGUAR,
            0x10,
            if data_sc == VHT_DATA_SC_20_UPPER_OF_80MHZ {
                1
            } else {
                0
            },
            "bandwidth",
        )?;
    }
    production_fix_spur_8812a(registers, counters, programming_channel_number, bandwidth)?;

    let rf_bandwidth_bits = match bandwidth {
        Bandwidth::Mhz20 => 3,
        Bandwidth::Mhz40 => 1,
        Bandwidth::Mhz80 => 0,
    };
    rf_path_a = apply_rf_mask(rf_path_a, RF_CHNLBW_BW_MASK, rf_bandwidth_bits);
    rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::A,
        RF_CHNLBW_JAGUAR,
        rf_path_a,
        counters,
    )?;
    rf_path_b = apply_rf_mask(rf_path_b, RF_CHNLBW_BW_MASK, rf_bandwidth_bits);
    rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::B,
        RF_CHNLBW_JAGUAR,
        rf_path_b,
        counters,
    )?;

    if production_should_apply_captured_tx_bringup_tail(channel, bandwidth) {
        production_apply_captured_tx_bringup_tail(registers, counters, bandwidth)?;
    }

    Ok(
        usize::try_from(counters.usb_control_writes.saturating_sub(before_writes))
            .unwrap_or(usize::MAX),
    )
}

fn production_data_secondary_channel_setting(
    channel: Channel,
    bandwidth: Bandwidth,
) -> Result<u8, RuntimeRadioError> {
    match bandwidth {
        Bandwidth::Mhz20 => Ok(0),
        Bandwidth::Mhz40 => match channel.band {
            Band::Ghz5 if channel.number % 8 == 4 => Ok(VHT_DATA_SC_20_LOWER_OF_80MHZ),
            Band::Ghz5 if channel.number % 8 == 0 => Ok(VHT_DATA_SC_20_UPPER_OF_80MHZ),
            Band::Ghz5 => Err(RuntimeRadioError::new(
                "channel_bandwidth_not_supported",
                format!(
                    "channel {} is not aligned to a 5 GHz 40 MHz channel pair",
                    channel.number
                ),
            )),
            Band::Ghz2 if (1..=7).contains(&channel.number) => Ok(VHT_DATA_SC_20_LOWER_OF_80MHZ),
            Band::Ghz2 if (8..=13).contains(&channel.number) => Ok(VHT_DATA_SC_20_UPPER_OF_80MHZ),
            Band::Ghz2 => Err(RuntimeRadioError::new(
                "channel_bandwidth_not_supported",
                format!(
                    "channel {} is not supported for 2.4 GHz 40 MHz operation",
                    channel.number
                ),
            )),
        },
        Bandwidth::Mhz80 => {
            let (_center, position) = production_eighty_mhz_center_and_position(channel.number)?;
            let (sc40, sc20) = match position {
                0 => (
                    VHT_DATA_SC_40_LOWER_OF_80MHZ,
                    VHT_DATA_SC_20_LOWEST_OF_80MHZ,
                ),
                1 => (VHT_DATA_SC_40_LOWER_OF_80MHZ, VHT_DATA_SC_20_LOWER_OF_80MHZ),
                2 => (VHT_DATA_SC_40_UPPER_OF_80MHZ, VHT_DATA_SC_20_UPPER_OF_80MHZ),
                3 => (
                    VHT_DATA_SC_40_UPPER_OF_80MHZ,
                    VHT_DATA_SC_20_UPPERST_OF_80MHZ,
                ),
                _ => unreachable!("80 MHz position is constrained to four primary channels"),
            };
            Ok((sc40 << 4) | sc20)
        }
    }
}

fn production_channel_programming_number(
    channel: Channel,
    bandwidth: Bandwidth,
) -> Result<u8, RuntimeRadioError> {
    match bandwidth {
        Bandwidth::Mhz20 => Ok(channel.number),
        Bandwidth::Mhz40 => production_forty_mhz_center_channel(channel),
        Bandwidth::Mhz80 => {
            production_eighty_mhz_center_and_position(channel.number).map(|(center, _)| center)
        }
    }
}

fn production_forty_mhz_center_channel(channel: Channel) -> Result<u8, RuntimeRadioError> {
    let data_sc = production_data_secondary_channel_setting(channel, Bandwidth::Mhz40)?;
    match data_sc {
        VHT_DATA_SC_20_LOWER_OF_80MHZ => Ok(channel.number + 2),
        VHT_DATA_SC_20_UPPER_OF_80MHZ => Ok(channel.number - 2),
        _ => Err(RuntimeRadioError::new(
            "channel_bandwidth_not_supported",
            format!(
                "channel {} did not map to a 40 MHz primary side",
                channel.number
            ),
        )),
    }
}

fn production_eighty_mhz_center_and_position(
    primary_channel: u8,
) -> Result<(u8, u8), RuntimeRadioError> {
    match primary_channel {
        36 => Ok((42, 0)),
        40 => Ok((42, 1)),
        44 => Ok((42, 2)),
        48 => Ok((42, 3)),
        52 => Ok((58, 0)),
        56 => Ok((58, 1)),
        60 => Ok((58, 2)),
        64 => Ok((58, 3)),
        100 => Ok((106, 0)),
        104 => Ok((106, 1)),
        108 => Ok((106, 2)),
        112 => Ok((106, 3)),
        116 => Ok((122, 0)),
        120 => Ok((122, 1)),
        124 => Ok((122, 2)),
        128 => Ok((122, 3)),
        132 => Ok((138, 0)),
        136 => Ok((138, 1)),
        140 => Ok((138, 2)),
        144 => Ok((138, 3)),
        149 => Ok((155, 0)),
        153 => Ok((155, 1)),
        157 => Ok((155, 2)),
        161 => Ok((155, 3)),
        _ => Err(RuntimeRadioError::new(
            "channel_bandwidth_not_supported",
            format!("channel {primary_channel} is not aligned to a supported 5 GHz 80 MHz group"),
        )),
    }
}

fn production_switch_wireless_band_8812<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    target_band: Band,
    rfe_type: u8,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let cck_check = read8_with_counter(
        registers,
        counters,
        REG_CCK_CHECK_8812,
        "REG_CCK_CHECK_8812",
        "band-switch",
    )?;
    let current_band = if cck_check & BIT7 != 0 {
        Band::Ghz5
    } else {
        Band::Ghz2
    };
    if current_band == target_band {
        return production_set_rfe_reg_8812(registers, counters, target_band, rfe_type);
    }
    match target_band {
        Band::Ghz2 => production_switch_to_2g_band(registers, counters, rfe_type),
        Band::Ghz5 => production_switch_to_5g_band(registers, counters, rfe_type),
    }
}

fn production_switch_to_2g_band<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    rfe_type: u8,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_bb_masked_write32(
        registers,
        counters,
        "rOFDMCCKEN_Jaguar",
        REG_OFDMCCKEN_JAGUAR,
        0x3000_0000,
        0x03,
        "band-switch",
    )?;
    production_bb_masked_write32(
        registers,
        counters,
        "rBWIndication_Jaguar",
        REG_BW_INDICATION_JAGUAR,
        0x0000_0003,
        0x01,
        "band-switch",
    )?;
    production_set_pwed_thresholds(registers, counters, 0x17)?;
    production_bb_masked_write32(
        registers,
        counters,
        "rAGC_table_Jaguar",
        REG_AGC_TABLE_JAGUAR,
        0x0000_0003,
        0x00,
        "band-switch",
    )?;
    production_set_rfe_reg_8812(registers, counters, Band::Ghz2, rfe_type)?;
    production_bb_masked_write32(
        registers,
        counters,
        "rTxPath_Jaguar",
        REG_TX_PATH_JAGUAR,
        0x0000_00f0,
        0x01,
        "band-switch",
    )?;
    production_bb_masked_write32(
        registers,
        counters,
        "rCCK_RX_Jaguar",
        REG_CCK_RX_JAGUAR,
        0x0f00_0000,
        0x01,
        "band-switch",
    )?;
    production_set_basic_rate(registers, counters, BASIC_RATE_2G)?;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_CCK_CHECK_8812",
        REG_CCK_CHECK_8812,
        !BIT7,
        BIT7,
        0,
        "band-switch",
    )?;
    production_set_bb_swing_default(registers, counters)
}

fn production_switch_to_5g_band<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    rfe_type: u8,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_rmw8_preserve(
        registers,
        counters,
        "REG_CCK_CHECK_8812",
        REG_CCK_CHECK_8812,
        !BIT7,
        BIT7,
        BIT7,
        "band-switch",
    )?;
    production_poll_tx_packet_empty(registers, counters)?;
    production_bb_masked_write32(
        registers,
        counters,
        "rOFDMCCKEN_Jaguar",
        REG_OFDMCCKEN_JAGUAR,
        0x3000_0000,
        0x03,
        "band-switch",
    )?;
    production_bb_masked_write32(
        registers,
        counters,
        "rBWIndication_Jaguar",
        REG_BW_INDICATION_JAGUAR,
        0x0000_0003,
        0x02,
        "band-switch",
    )?;
    production_set_pwed_thresholds(registers, counters, 0x15)?;
    production_bb_masked_write32(
        registers,
        counters,
        "rAGC_table_Jaguar",
        REG_AGC_TABLE_JAGUAR,
        0x0000_0003,
        0x01,
        "band-switch",
    )?;
    production_set_rfe_reg_8812(registers, counters, Band::Ghz5, rfe_type)?;
    production_bb_masked_write32(
        registers,
        counters,
        "rTxPath_Jaguar",
        REG_TX_PATH_JAGUAR,
        0x0000_00f0,
        0x00,
        "band-switch",
    )?;
    production_bb_masked_write32(
        registers,
        counters,
        "rCCK_RX_Jaguar",
        REG_CCK_RX_JAGUAR,
        0x0f00_0000,
        0x0f,
        "band-switch",
    )?;
    production_set_basic_rate(registers, counters, BASIC_RATE_5G)?;
    production_set_bb_swing_default(registers, counters)
}

fn production_set_pwed_thresholds<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    pd_th_20m: u32,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_bb_masked_write32(
        registers,
        counters,
        "rPwed_TH_Jaguar",
        REG_PWED_TH_JAGUAR,
        0x0003_e000,
        pd_th_20m,
        "band-switch",
    )?;
    production_bb_masked_write32(
        registers,
        counters,
        "rPwed_TH_Jaguar",
        REG_PWED_TH_JAGUAR,
        0x0000_000e,
        0x04,
        "band-switch",
    )
}

#[derive(Debug, Clone, Copy)]
struct ProductionRfeRegConfig {
    pinmux: u32,
    inv_mask: u32,
    inv: u32,
    antsel: Option<u32>,
}

fn production_rfe_reg_config_8812(
    band: Band,
    rfe_type: u8,
) -> Result<ProductionRfeRegConfig, RuntimeRadioError> {
    match (band, rfe_type) {
        (Band::Ghz2, 0 | 1 | 2) => Ok(ProductionRfeRegConfig {
            pinmux: 0x7777_7777,
            inv_mask: 0x3ff0_0000,
            inv: 0x000,
            antsel: None,
        }),
        (Band::Ghz2, 3) => Ok(ProductionRfeRegConfig {
            pinmux: 0x5433_7770,
            inv_mask: 0x3ff0_0000,
            inv: 0x010,
            antsel: Some(0x1),
        }),
        (Band::Ghz2, 4) => Ok(ProductionRfeRegConfig {
            pinmux: 0x7777_7777,
            inv_mask: 0x3ff0_0000,
            inv: 0x001,
            antsel: None,
        }),
        (Band::Ghz2, 6) => Ok(ProductionRfeRegConfig {
            pinmux: 0x0777_2770,
            inv_mask: u32::MAX,
            inv: 0x0000_0077,
            antsel: None,
        }),
        (Band::Ghz5, 0) => Ok(ProductionRfeRegConfig {
            pinmux: 0x7733_7717,
            inv_mask: 0x3ff0_0000,
            inv: 0x010,
            antsel: None,
        }),
        (Band::Ghz5, 1) => Ok(ProductionRfeRegConfig {
            pinmux: 0x7733_7717,
            inv_mask: 0x3ff0_0000,
            inv: 0x000,
            antsel: None,
        }),
        (Band::Ghz5, 2 | 4) => Ok(ProductionRfeRegConfig {
            pinmux: 0x7733_7777,
            inv_mask: 0x3ff0_0000,
            inv: 0x010,
            antsel: None,
        }),
        (Band::Ghz5, 3) => Ok(ProductionRfeRegConfig {
            pinmux: 0x5433_7717,
            inv_mask: 0x3ff0_0000,
            inv: 0x010,
            antsel: Some(0x1),
        }),
        (Band::Ghz5, 6) => Ok(ProductionRfeRegConfig {
            pinmux: 0x0773_7717,
            inv_mask: u32::MAX,
            inv: 0x0000_0077,
            antsel: None,
        }),
        (_, 5) => Err(RuntimeRadioError::new(
            "unsupported_rfe_type",
            "RFE type 5 needs byte-level partial pinmux writes that are not ported yet",
        )),
        _ => Err(RuntimeRadioError::new(
            "unsupported_rfe_type",
            format!("RFE type {rfe_type} is not supported by the RTL8812A channel path"),
        )),
    }
}

fn production_set_rfe_reg_8812<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    band: Band,
    rfe_type: u8,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let config = production_rfe_reg_config_8812(band, rfe_type)?;
    for (register_name, address) in [
        ("rA_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_A_JAGUAR),
        ("rB_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_B_JAGUAR),
    ] {
        production_bb_masked_write32(
            registers,
            counters,
            register_name,
            address,
            u32::MAX,
            config.pinmux,
            "band-switch",
        )?;
    }
    for (register_name, address) in [
        ("rA_RFE_Inv_Jaguar", REG_RFE_INV_A_JAGUAR),
        ("rB_RFE_Inv_Jaguar", REG_RFE_INV_B_JAGUAR),
    ] {
        production_bb_masked_write32(
            registers,
            counters,
            register_name,
            address,
            config.inv_mask,
            config.inv,
            "band-switch",
        )?;
    }
    if let Some(antsel) = config.antsel {
        production_bb_masked_write32(
            registers,
            counters,
            "r_ANTSEL_SW_Jaguar",
            REG_ANTSEL_SW_JAGUAR,
            0x0000_0303,
            antsel,
            "band-switch",
        )?;
    }
    Ok(())
}

fn production_set_basic_rate<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    rate_mask: u16,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    production_write16_verify(
        registers,
        counters,
        "REG_RRSR",
        REG_RRSR,
        rate_mask,
        u16::MAX,
        rate_mask,
        "band-switch",
    )?;
    production_rmw8_preserve(
        registers,
        counters,
        "REG_RRSR + 2",
        REG_RRSR + 2,
        0xf0,
        0x0f,
        0,
        "band-switch",
    )
}

fn production_set_bb_swing_default<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    for (register_name, address) in [
        ("rA_TxScale_Jaguar", REG_TX_SCALE_A_JAGUAR),
        ("rB_TxScale_Jaguar", REG_TX_SCALE_B_JAGUAR),
    ] {
        production_bb_masked_write32(
            registers,
            counters,
            register_name,
            address,
            0xffe0_0000,
            0x200,
            "band-switch",
        )?;
    }
    Ok(())
}

fn production_poll_tx_packet_empty<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut observed = 0u16;
    for _attempt in 0..50 {
        observed = read16_with_counter(
            registers,
            counters,
            REG_TXPKT_EMPTY,
            "REG_TXPKT_EMPTY",
            "band-switch",
        )?;
        if observed & 0x0030 == 0x0030 {
            return Ok(());
        }
        thread::sleep(Duration::from_micros(50));
    }
    Err(RuntimeRadioError::new(
        "tx_packet_empty_poll_failed",
        format!(
            "REG_TXPKT_EMPTY did not report mask 0x0030 after 50 reads, got {}",
            format_register_value(observed, 4)
        ),
    ))
}

fn production_fix_spur_8812a<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    channel_number: u8,
    bandwidth: Bandwidth,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    if channel_number > 14 {
        return Ok(());
    }
    let data = if bandwidth == Bandwidth::Mhz20 && matches!(channel_number, 13 | 14) {
        0x03
    } else {
        0x02
    };
    production_bb_masked_write32(
        registers,
        counters,
        "rRFMOD_Jaguar",
        REG_RF_MOD_JAGUAR,
        0x0000_0300,
        data,
        "spur",
    )
}

fn production_last_rf_register_data(
    plan: &RealtekTablePlan,
    rf_offset: u32,
) -> Result<u32, RuntimeRadioError> {
    let mut data = None;
    for action in &plan.actions {
        if action.kind == RealtekTableActionKind::Write && action.address == rf_offset {
            data = action.data;
        }
    }
    data.ok_or_else(|| {
        RuntimeRadioError::new(
            "rf_channel_base_missing",
            format!(
                "{} did not contain a final RF register 0x{rf_offset:02x} write to use as channel base",
                plan.array_name
            ),
        )
    })
}

fn production_fc_area_data(channel: u8) -> u32 {
    match channel {
        36..=48 => 0x494,
        15..=35 => 0x494,
        50..=80 => 0x453,
        82..=116 => 0x452,
        118..=u8::MAX => 0x412,
        _ => 0x96a,
    }
}

fn production_rf_mod_ag_data(channel: u8) -> u32 {
    match channel {
        36..=80 => 0x101,
        15..=35 => 0x101,
        82..=140 => 0x301,
        141..=u8::MAX => 0x501,
        _ => 0x000,
    }
}

fn production_should_apply_captured_tx_bringup_tail(
    channel: Channel,
    bandwidth: Bandwidth,
) -> bool {
    channel.number == 36 && matches!(bandwidth, Bandwidth::Mhz20 | Bandwidth::Mhz40)
}

type ProductionCapturedTxBringupWrite = (&'static str, u16, u32);

const PRODUCTION_CAPTURED_TX_BRINGUP_TAIL_20MHZ: &[ProductionCapturedTxBringupWrite] = &[
    ("rA_TxScale_Jaguar", REG_TX_SCALE_A_JAGUAR, 0x2d40_0003),
    ("rB_TxScale_Jaguar", REG_TX_SCALE_B_JAGUAR, 0x2d40_0003),
    ("rA_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_A_JAGUAR, 0x5433_7717),
    ("rB_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_B_JAGUAR, 0x5433_7717),
    ("rA_RFE_Inv_Jaguar", REG_RFE_INV_A_JAGUAR, 0x0100_0077),
    ("rB_RFE_Inv_Jaguar", REG_RFE_INV_B_JAGUAR, 0x0100_0077),
    ("rA_RFE_Timing_Jaguar", REG_RFE_TIMING_A_JAGUAR, 0x0050_8242),
    ("rB_RFE_Timing_Jaguar", REG_RFE_TIMING_B_JAGUAR, 0x0050_8242),
    ("rA_IQK_Result_Jaguar", REG_OFDM0_XBAGCCORE1, 0x3000_0c1c),
    (
        "rB_IQK_Result_Jaguar",
        REG_OFDM0_XBAGCCORE1 + 0x200,
        0x3000_0c1c,
    ),
    (
        "rA_IQK_Shadow_Jaguar",
        REG_OFDM0_XBAGCCORE1 + 4,
        0x0000_0058,
    ),
    (
        "rB_IQK_Shadow_Jaguar",
        REG_OFDM0_XBAGCCORE1 + 0x204,
        0x0000_0058,
    ),
    ("rA_TxAGC_CCK", REG_TX_AGC_A_CCK_JAGUAR, 0x1515_1515),
    (
        "rA_TxAGC_OFDM18_OFDM6",
        REG_TX_AGC_A_OFDM18_OFDM6_JAGUAR,
        0x2727_2727,
    ),
    (
        "rA_TxAGC_OFDM54_OFDM24",
        REG_TX_AGC_A_OFDM54_OFDM24_JAGUAR,
        0x2727_2727,
    ),
    (
        "rA_TxAGC_MCS3_MCS0",
        REG_TX_AGC_A_MCS3_MCS0_JAGUAR,
        0x2828_2828,
    ),
    (
        "rA_TxAGC_MCS7_MCS4",
        REG_TX_AGC_A_MCS7_MCS4_JAGUAR,
        0x2828_2828,
    ),
    (
        "rA_TxAGC_NSS1_7_NSS1_4",
        REG_TX_AGC_A_NSS1_7_NSS1_4_JAGUAR,
        0x2626_2626,
    ),
    (
        "rA_TxAGC_NSS1_11_NSS1_8",
        REG_TX_AGC_A_NSS1_11_NSS1_8_JAGUAR,
        0x2626_2626,
    ),
    (
        "rA_TxAGC_NSS1_3_NSS1_0",
        REG_TX_AGC_A_NSS1_3_NSS1_0_JAGUAR,
        0x2828_2828,
    ),
    (
        "rA_TxAGC_NSS2_3_NSS2_0",
        REG_TX_AGC_A_NSS2_3_NSS2_0_JAGUAR,
        0x2828_2828,
    ),
    (
        "rA_TxAGC_NSS2_7_NSS2_4",
        REG_TX_AGC_A_NSS2_7_NSS2_4_JAGUAR,
        0x2626_2828,
    ),
    (
        "rA_TxAGC_NSS2_11_NSS2_8",
        REG_TX_AGC_A_NSS2_11_NSS2_8_JAGUAR,
        0x2626_2626,
    ),
    (
        "rA_TxAGC_NSS3_3_NSS3_0",
        REG_TX_AGC_A_NSS3_3_NSS3_0_JAGUAR,
        0x2626_2626,
    ),
    ("rA_TxPowerOffset", REG_TX_PWR_OFFSET_A_JAGUAR, 0x0000_001e),
    ("rA_TxPowerTraining", REG_TX_PWR_TRAIN_A_JAGUAR, 0x0010_161e),
    ("rA_TxBbCtrl", REG_TX_BB_CTRL_A_JAGUAR, 0x0181_7d24),
    ("rB_TxAGC_CCK", REG_TX_AGC_B_CCK_JAGUAR, 0x1818_1818),
    (
        "rB_TxAGC_OFDM18_OFDM6",
        REG_TX_AGC_B_OFDM18_OFDM6_JAGUAR,
        0x2828_2828,
    ),
    (
        "rB_TxAGC_OFDM54_OFDM24",
        REG_TX_AGC_B_OFDM54_OFDM24_JAGUAR,
        0x2828_2828,
    ),
    (
        "rB_TxAGC_MCS3_MCS0",
        REG_TX_AGC_B_MCS3_MCS0_JAGUAR,
        0x2a2a_2a2a,
    ),
    (
        "rB_TxAGC_MCS7_MCS4",
        REG_TX_AGC_B_MCS7_MCS4_JAGUAR,
        0x2a2a_2a2a,
    ),
    (
        "rB_TxAGC_NSS1_7_NSS1_4",
        REG_TX_AGC_B_NSS1_7_NSS1_4_JAGUAR,
        0x2828_2828,
    ),
    (
        "rB_TxAGC_NSS1_11_NSS1_8",
        REG_TX_AGC_B_NSS1_11_NSS1_8_JAGUAR,
        0x2828_2828,
    ),
    (
        "rB_TxAGC_NSS1_3_NSS1_0",
        REG_TX_AGC_B_NSS1_3_NSS1_0_JAGUAR,
        0x2a2a_2a2a,
    ),
    (
        "rB_TxAGC_NSS2_3_NSS2_0",
        REG_TX_AGC_B_NSS2_3_NSS2_0_JAGUAR,
        0x2a2a_2a2a,
    ),
    (
        "rB_TxAGC_NSS2_7_NSS2_4",
        REG_TX_AGC_B_NSS2_7_NSS2_4_JAGUAR,
        0x2828_2a2a,
    ),
    (
        "rB_TxAGC_NSS2_11_NSS2_8",
        REG_TX_AGC_B_NSS2_11_NSS2_8_JAGUAR,
        0x2828_2828,
    ),
    (
        "rB_TxAGC_NSS3_3_NSS3_0",
        REG_TX_AGC_B_NSS3_3_NSS3_0_JAGUAR,
        0x2828_2828,
    ),
    ("rB_TxPowerOffset", REG_TX_PWR_OFFSET_B_JAGUAR, 0x0000_001e),
    ("rB_TxPowerTraining", REG_TX_PWR_TRAIN_B_JAGUAR, 0x0012_1820),
    ("rB_TxBbCtrl", REG_TX_BB_CTRL_B_JAGUAR, 0x0181_7d24),
];

const PRODUCTION_CAPTURED_TX_BRINGUP_TAIL_40MHZ: &[ProductionCapturedTxBringupWrite] = &[
    ("rA_TxScale_Jaguar", REG_TX_SCALE_A_JAGUAR, 0x35e0_0003),
    ("rB_TxScale_Jaguar", REG_TX_SCALE_B_JAGUAR, 0x35e0_0003),
    ("rA_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_A_JAGUAR, 0x5433_7717),
    ("rB_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_B_JAGUAR, 0x5433_7717),
    ("rA_RFE_Inv_Jaguar", REG_RFE_INV_A_JAGUAR, 0x0100_0077),
    ("rB_RFE_Inv_Jaguar", REG_RFE_INV_B_JAGUAR, 0x0100_0077),
    ("rA_RFE_Timing_Jaguar", REG_RFE_TIMING_A_JAGUAR, 0x0050_8242),
    ("rB_RFE_Timing_Jaguar", REG_RFE_TIMING_B_JAGUAR, 0x0050_8242),
    ("rA_IQK_Result_Jaguar", REG_OFDM0_XBAGCCORE1, 0x3000_0c1c),
    (
        "rB_IQK_Result_Jaguar",
        REG_OFDM0_XBAGCCORE1 + 0x200,
        0x3000_0c1c,
    ),
    (
        "rA_IQK_Shadow_Jaguar",
        REG_OFDM0_XBAGCCORE1 + 4,
        0x0000_0058,
    ),
    (
        "rB_IQK_Shadow_Jaguar",
        REG_OFDM0_XBAGCCORE1 + 0x204,
        0x0000_0058,
    ),
    ("rA_TxAGC_CCK", REG_TX_AGC_A_CCK_JAGUAR, 0x1515_1515),
    (
        "rA_TxAGC_OFDM18_OFDM6",
        REG_TX_AGC_A_OFDM18_OFDM6_JAGUAR,
        0x2727_2727,
    ),
    (
        "rA_TxAGC_OFDM54_OFDM24",
        REG_TX_AGC_A_OFDM54_OFDM24_JAGUAR,
        0x2727_2727,
    ),
    (
        "rA_TxAGC_MCS3_MCS0",
        REG_TX_AGC_A_MCS3_MCS0_JAGUAR,
        0x2929_2929,
    ),
    (
        "rA_TxAGC_MCS7_MCS4",
        REG_TX_AGC_A_MCS7_MCS4_JAGUAR,
        0x2929_2929,
    ),
    (
        "rA_TxAGC_NSS1_7_NSS1_4",
        REG_TX_AGC_A_NSS1_7_NSS1_4_JAGUAR,
        0x2727_2727,
    ),
    (
        "rA_TxAGC_NSS1_11_NSS1_8",
        REG_TX_AGC_A_NSS1_11_NSS1_8_JAGUAR,
        0x2727_2727,
    ),
    (
        "rA_TxAGC_NSS1_3_NSS1_0",
        REG_TX_AGC_A_NSS1_3_NSS1_0_JAGUAR,
        0x2929_2929,
    ),
    (
        "rA_TxAGC_NSS2_3_NSS2_0",
        REG_TX_AGC_A_NSS2_3_NSS2_0_JAGUAR,
        0x2929_2929,
    ),
    (
        "rA_TxAGC_NSS2_7_NSS2_4",
        REG_TX_AGC_A_NSS2_7_NSS2_4_JAGUAR,
        0x2727_2929,
    ),
    (
        "rA_TxAGC_NSS2_11_NSS2_8",
        REG_TX_AGC_A_NSS2_11_NSS2_8_JAGUAR,
        0x2727_2727,
    ),
    (
        "rA_TxAGC_NSS3_3_NSS3_0",
        REG_TX_AGC_A_NSS3_3_NSS3_0_JAGUAR,
        0x2727_2727,
    ),
    ("rA_TxPowerOffset", REG_TX_PWR_OFFSET_A_JAGUAR, 0x0000_001c),
    ("rA_TxPowerTraining", REG_TX_PWR_TRAIN_A_JAGUAR, 0x0011_171f),
    ("rA_TxBbCtrl", REG_TX_BB_CTRL_A_JAGUAR, 0x0423_8500),
    ("rB_TxAGC_CCK", REG_TX_AGC_B_CCK_JAGUAR, 0x1818_1818),
    (
        "rB_TxAGC_OFDM18_OFDM6",
        REG_TX_AGC_B_OFDM18_OFDM6_JAGUAR,
        0x2828_2828,
    ),
    (
        "rB_TxAGC_OFDM54_OFDM24",
        REG_TX_AGC_B_OFDM54_OFDM24_JAGUAR,
        0x2828_2828,
    ),
    (
        "rB_TxAGC_MCS3_MCS0",
        REG_TX_AGC_B_MCS3_MCS0_JAGUAR,
        0x2b2b_2b2b,
    ),
    (
        "rB_TxAGC_MCS7_MCS4",
        REG_TX_AGC_B_MCS7_MCS4_JAGUAR,
        0x2b2b_2b2b,
    ),
    (
        "rB_TxAGC_NSS1_7_NSS1_4",
        REG_TX_AGC_B_NSS1_7_NSS1_4_JAGUAR,
        0x2929_2929,
    ),
    (
        "rB_TxAGC_NSS1_11_NSS1_8",
        REG_TX_AGC_B_NSS1_11_NSS1_8_JAGUAR,
        0x2929_2929,
    ),
    (
        "rB_TxAGC_NSS1_3_NSS1_0",
        REG_TX_AGC_B_NSS1_3_NSS1_0_JAGUAR,
        0x2b2b_2b2b,
    ),
    (
        "rB_TxAGC_NSS2_3_NSS2_0",
        REG_TX_AGC_B_NSS2_3_NSS2_0_JAGUAR,
        0x2b2b_2b2b,
    ),
    (
        "rB_TxAGC_NSS2_7_NSS2_4",
        REG_TX_AGC_B_NSS2_7_NSS2_4_JAGUAR,
        0x2929_2b2b,
    ),
    (
        "rB_TxAGC_NSS2_11_NSS2_8",
        REG_TX_AGC_B_NSS2_11_NSS2_8_JAGUAR,
        0x2929_2929,
    ),
    (
        "rB_TxAGC_NSS3_3_NSS3_0",
        REG_TX_AGC_B_NSS3_3_NSS3_0_JAGUAR,
        0x2929_2929,
    ),
    ("rB_TxPowerOffset", REG_TX_PWR_OFFSET_B_JAGUAR, 0x0000_001c),
    ("rB_TxPowerTraining", REG_TX_PWR_TRAIN_B_JAGUAR, 0x0013_1921),
    ("rB_TxBbCtrl", REG_TX_BB_CTRL_B_JAGUAR, 0x0181_7526),
];

fn production_captured_tx_bringup_tail(
    bandwidth: Bandwidth,
) -> Result<&'static [ProductionCapturedTxBringupWrite], RuntimeRadioError> {
    match bandwidth {
        Bandwidth::Mhz20 => Ok(PRODUCTION_CAPTURED_TX_BRINGUP_TAIL_20MHZ),
        Bandwidth::Mhz40 => Ok(PRODUCTION_CAPTURED_TX_BRINGUP_TAIL_40MHZ),
        Bandwidth::Mhz80 => Err(RuntimeRadioError::new(
            "captured_tx_bringup_tail_unsupported",
            "captured TX bring-up tail is only available for 20 and 40 MHz",
        )),
    }
}

fn production_apply_captured_tx_bringup_tail<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    bandwidth: Bandwidth,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    for (register_name, address, value) in production_captured_tx_bringup_tail(bandwidth)? {
        production_write32_verify(
            registers,
            counters,
            register_name,
            *address,
            *value,
            u32::MAX,
            *value,
            "tx-bringup-tail",
        )?;
    }
    Ok(())
}

pub fn run_production_runtime_flow_with_session<T, F>(
    config: ProductionRuntimeFlowConfig,
    inputs: ProductionRuntimeFlowExecutionInputs,
    session: &mut RuntimeRadioSession<T>,
    mut run_phase: F,
) -> ProductionRuntimeFlowReport
where
    T: UsbBulkTransfer,
    for<'a> &'a T: Rtl8812auUsbTransport,
    F: FnMut(
        &mut RuntimeRadioSession<T>,
        Rtl8812auInitPhase,
    ) -> Result<RuntimeSameSessionInitPhaseSummary, RuntimeSameSessionInitPhaseFailure>,
{
    if let Err(error) = write_production_runtime_service_health(
        config.health_file.as_deref(),
        &ProductionRuntimeServiceHealth::from_config(
            &config,
            ProductionRuntimeServiceLifecycle::Validating,
            None,
            None,
        ),
    ) {
        return ProductionRuntimeFlowReport::not_started(&config, error);
    }
    let validation = match config.validate() {
        Ok(validation) => validation,
        Err(error) => {
            return production_runtime_report_with_health(
                &config,
                ProductionRuntimeFlowReport::not_started(&config, error),
                ProductionRuntimeServiceLifecycle::ExitedFail,
            )
        }
    };
    if let Err(error) = inputs.validate() {
        return production_runtime_report_with_health(
            &config,
            ProductionRuntimeFlowReport::not_started(&config, error),
            ProductionRuntimeServiceLifecycle::ExitedFail,
        );
    }
    if let Err(error) = write_production_runtime_service_health(
        config.health_file.as_deref(),
        &ProductionRuntimeServiceHealth::from_config(
            &config,
            ProductionRuntimeServiceLifecycle::Initializing,
            None,
            None,
        ),
    ) {
        return ProductionRuntimeFlowReport::not_started(&config, error);
    }
    let init_inputs = inputs
        .rtl8812au_init
        .as_ref()
        .expect("validated production runtime inputs include init assets");
    let init_config = RuntimeSameSessionInitConfig {
        init_order: init_inputs.init_order,
        channel: config.channel,
        bandwidth: config.bandwidth,
        rfe_type: init_inputs.rfe_type,
        tx_calibration_profile: config.calibration_profile,
        live_write_authorized: config.live_register_write_authorized,
        captured_tail_applied: config.captured_tail_applied,
    };
    let init_result = run_rtl8812au_same_session_init(session, init_config, |session, phase| {
        run_phase(session, phase)
    });

    let init = match init_result {
        Ok(result) => result,
        Err(RuntimeSameSessionInitFailure { result, error }) => {
            return production_runtime_flow_report_from_state_with_health(
                &config,
                session,
                "not_started",
                production_runtime_init_telemetry(&result),
                ProductionRuntimePreLoopReports::default(),
                None,
                RuntimeFlowRxTelemetry::default(),
                RuntimeFlowTxTelemetry::default(),
                ProductionRuntimeFlowResult::Fail,
                Some(error),
            )
        }
    };
    let init_telemetry = production_runtime_init_telemetry(&init);

    let mut pre_loop = ProductionRuntimePreLoopReports::default();
    match apply_production_runtime_monitor_opmode(&session.transport, &mut session.counters) {
        Ok(()) => {
            pre_loop.monitor_opmode_applied = true;
        }
        Err(error) => {
            return production_runtime_flow_report_from_state_with_health(
                &config,
                session,
                "not_started",
                init_telemetry,
                pre_loop,
                None,
                RuntimeFlowRxTelemetry::default(),
                RuntimeFlowTxTelemetry::default(),
                ProductionRuntimeFlowResult::Fail,
                Some(error),
            )
        }
    }
    match apply_production_runtime_tx_power_control(
        &session.transport,
        &mut session.counters,
        &inputs.tx_power_control,
    ) {
        Ok(report) => {
            pre_loop.tx_power_control = report;
        }
        Err(error) => {
            return production_runtime_flow_report_from_state_with_health(
                &config,
                session,
                "not_started",
                init_telemetry,
                pre_loop,
                None,
                RuntimeFlowRxTelemetry::default(),
                RuntimeFlowTxTelemetry::default(),
                ProductionRuntimeFlowResult::Fail,
                Some(error),
            )
        }
    }
    match apply_production_runtime_tx_calibration_profile(
        &session.transport,
        &mut session.counters,
        &config,
        init_inputs.rfe_type,
    ) {
        Ok(report) => {
            pre_loop.tx_calibration_profile = report;
        }
        Err(error) => {
            return production_runtime_flow_report_from_state_with_health(
                &config,
                session,
                "not_started",
                init_telemetry,
                pre_loop,
                None,
                RuntimeFlowRxTelemetry::default(),
                RuntimeFlowTxTelemetry::default(),
                ProductionRuntimeFlowResult::Fail,
                Some(error),
            )
        }
    }
    if pre_loop.tx_power_control.is_some() && pre_loop.tx_calibration_profile.is_some() {
        match apply_production_runtime_tx_power_control(
            &session.transport,
            &mut session.counters,
            &inputs.tx_power_control,
        ) {
            Ok(report) => {
                pre_loop.tx_power_control = report;
            }
            Err(error) => {
                return production_runtime_flow_report_from_state_with_health(
                    &config,
                    session,
                    "not_started",
                    init_telemetry,
                    pre_loop,
                    None,
                    RuntimeFlowRxTelemetry::default(),
                    RuntimeFlowTxTelemetry::default(),
                    ProductionRuntimeFlowResult::Fail,
                    Some(error),
                )
            }
        }
    }
    if !validation.wfb_loop.rx_forwards.is_empty() {
        match apply_production_runtime_rx_startup_kick(session, &config) {
            Ok(report) => {
                pre_loop.rx_startup_kick = Some(report);
            }
            Err(error) => {
                return production_runtime_flow_report_from_state_with_health(
                    &config,
                    session,
                    "not_started",
                    init_telemetry,
                    pre_loop,
                    None,
                    RuntimeFlowRxTelemetry::default(),
                    RuntimeFlowTxTelemetry::default(),
                    ProductionRuntimeFlowResult::Fail,
                    Some(error),
                )
            }
        }
    }

    if let Err(error) = write_production_runtime_ready_marker(
        config.ready_file.as_deref(),
        production_runtime_ready_marker(
            &config,
            &validation,
            &init,
            pre_loop.monitor_opmode_applied,
            pre_loop.tx_power_control.is_some(),
            pre_loop.tx_calibration_profile.is_some(),
            pre_loop.rx_startup_kick.is_some(),
        ),
    ) {
        return production_runtime_flow_report_from_state_with_health(
            &config,
            session,
            "not_started",
            init_telemetry,
            pre_loop,
            None,
            RuntimeFlowRxTelemetry::default(),
            RuntimeFlowTxTelemetry::default(),
            ProductionRuntimeFlowResult::Fail,
            Some(error),
        );
    }
    let ready_report = production_runtime_flow_report_from_state(
        &config,
        session,
        "ready",
        init_telemetry,
        pre_loop.clone(),
        None,
        RuntimeFlowRxTelemetry::default(),
        RuntimeFlowTxTelemetry::default(),
        ProductionRuntimeFlowResult::Pass,
        None,
    );
    if let Err(error) = write_production_runtime_service_health(
        config.health_file.as_deref(),
        &ProductionRuntimeServiceHealth::from_report(
            &ready_report,
            ProductionRuntimeServiceLifecycle::Ready,
            config.health_file.clone(),
            None,
        ),
    ) {
        return production_runtime_flow_report_from_state(
            &config,
            session,
            "not_started",
            init_telemetry,
            pre_loop,
            None,
            RuntimeFlowRxTelemetry::default(),
            RuntimeFlowTxTelemetry::default(),
            ProductionRuntimeFlowResult::Fail,
            Some(error),
        );
    }

    let tx_sockets = match bind_production_tx_ingress_sockets(
        &validation.wfb_loop.tx_bind_addrs,
        PRODUCTION_TX_SOCKET_RCVBUF_BYTES,
    ) {
        Ok(sockets) => sockets,
        Err(error) => {
            return production_runtime_flow_report_from_state_with_health(
                &config,
                session,
                "not_started",
                init_telemetry,
                pre_loop,
                None,
                RuntimeFlowRxTelemetry::default(),
                RuntimeFlowTxTelemetry::default(),
                ProductionRuntimeFlowResult::Fail,
                Some(error),
            )
        }
    };
    let tx_receiver =
        match spawn_production_tx_ingress_receivers(tx_sockets, PRODUCTION_TX_RECEIVE_TIMEOUT) {
            Ok(receiver) => receiver,
            Err(error) => {
                return production_runtime_flow_report_from_state_with_health(
                    &config,
                    session,
                    "not_started",
                    init_telemetry,
                    pre_loop,
                    None,
                    RuntimeFlowRxTelemetry::default(),
                    RuntimeFlowTxTelemetry::default(),
                    ProductionRuntimeFlowResult::Fail,
                    Some(error),
                )
            }
        };
    let mut rx_forwards =
        match create_production_rx_forward_runtimes(&validation.wfb_loop.rx_forwards) {
            Ok(runtimes) => runtimes,
            Err(error) => {
                return production_runtime_flow_report_from_state_with_health(
                    &config,
                    session,
                    "not_started",
                    init_telemetry,
                    pre_loop,
                    None,
                    RuntimeFlowRxTelemetry::default(),
                    RuntimeFlowTxTelemetry::default(),
                    ProductionRuntimeFlowResult::Fail,
                    Some(error),
                )
            }
        };
    let mut rx = RuntimeFlowRxTelemetry::default();
    let mut tx = RuntimeFlowTxTelemetry::default();
    let mut bridge_counters = TxCounters::default();
    let mut submit_counters = TxSubmitCounters::default();
    let mut rx_buf = vec![0u8; 16 * 1024];
    let session_cell = RefCell::new(session);
    let mut heartbeat = LedHeartbeat::new(inputs.heartbeat_led, std::time::Instant::now());
    let loop_outcome = run_production_bridge_loop(
        ProductionRuntimeBridgeLoopRunConfig::from_bounds(
            config.duration_ms,
            config.rx_timeout_ms,
            config.tx_burst_limit,
            u64::from(config.max_datagrams),
        )
        .with_airtime_schedule(config.airtime_schedule),
        |now| {
            heartbeat.maybe_toggle(&&session_cell.borrow().transport, now);
        },
        || inputs.process_signal_stop && PRODUCTION_RUNTIME_STOP_REQUESTED.load(Ordering::SeqCst),
        |step| -> Result<ProductionRuntimeBridgeLoopStepOutcome, RuntimeRadioError> {
            match step {
                ProductionRuntimeBridgeLoopStep::TryTx => match tx_receiver.receiver.try_recv() {
                    Ok(queued) => {
                        tx.datagrams_received = tx.datagrams_received.saturating_add(1);
                        match handle_production_bridge_tx_datagram(
                            &mut **session_cell.borrow_mut(),
                            &queued,
                            ProductionRuntimeBridgeTxConfig {
                                channel: config.channel,
                                channel_bandwidth: config.bandwidth,
                                overrides: ProductionRuntimeBridgeTxOverrides::default(),
                            },
                            &mut bridge_counters,
                            &mut submit_counters,
                        ) {
                            Ok(outcome) => {
                                bridge_counters = outcome.bridge_counters;
                                submit_counters = outcome.submit_counters;
                                apply_production_runtime_tx_telemetry(
                                    &mut tx,
                                    &bridge_counters,
                                    &submit_counters,
                                );
                                Ok(ProductionRuntimeBridgeLoopStepOutcome::TxProcessed)
                            }
                            Err(error) => {
                                bridge_counters = error.bridge_counters;
                                submit_counters = error.submit_counters;
                                apply_production_runtime_tx_telemetry(
                                    &mut tx,
                                    &bridge_counters,
                                    &submit_counters,
                                );
                                Err(RuntimeRadioError::new(
                                    "bridge_tx_submit_failed",
                                    error.message,
                                ))
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxEmpty)
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxDisconnected)
                    }
                },
                ProductionRuntimeBridgeLoopStep::ReadRx { timeout } => match session_cell
                    .borrow_mut()
                    .read_rx_packets(config.channel, &mut rx_buf, timeout)
                {
                    Ok(read) if read.bytes_read == 0 => {
                        rx.buffers_read = rx.buffers_read.saturating_add(1);
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::RxRead)
                    }
                    Ok(read) => {
                        rx.buffers_read = rx.buffers_read.saturating_add(1);
                        match process_production_rx_packet_outcomes(&read.packets, &mut rx_forwards)
                        {
                            Ok(outcome) => {
                                apply_production_runtime_rx_packet_telemetry(
                                    &mut rx,
                                    outcome.telemetry,
                                );
                                rx.rx_forwards = outcome.rx_forwards;
                                rx.forwarded_payloads = rx
                                    .rx_forwards
                                    .iter()
                                    .map(|forward| forward.counters.forwarded)
                                    .sum();
                                Ok(ProductionRuntimeBridgeLoopStepOutcome::RxRead)
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) if error.timeout => {
                        rx.read_timeouts = rx.read_timeouts.saturating_add(1);
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::RxTimeout)
                    }
                    Err(_error)
                        if inputs.process_signal_stop
                            && PRODUCTION_RUNTIME_STOP_REQUESTED.load(Ordering::SeqCst) =>
                    {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::Stop(
                            ProductionRuntimeBridgeLoopStopReason::Signal,
                        ))
                    }
                    Err(error) => Err(error),
                },
            }
        },
    );
    let session = session_cell.into_inner();
    heartbeat.turn_off(&&session.transport);
    let heartbeat_led = Some(ProductionRuntimeHeartbeatLedReport {
        enabled: heartbeat.config().enabled,
        half_period_ms: heartbeat.config().half_period_ms,
        toggles_attempted: heartbeat.counters().toggles_attempted,
        toggles_succeeded: heartbeat.counters().toggles_succeeded,
        toggles_failed: heartbeat.counters().toggles_failed,
    });
    let loop_outcome = match loop_outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            rx.rx_forwards = production_rx_forward_snapshots(&rx_forwards);
            rx.forwarded_payloads = rx
                .rx_forwards
                .iter()
                .map(|forward| forward.counters.forwarded)
                .sum();
            apply_production_runtime_tx_telemetry(&mut tx, &bridge_counters, &submit_counters);
            return production_runtime_flow_report_from_state_with_health(
                &config,
                session,
                "not_started",
                init_telemetry,
                pre_loop,
                heartbeat_led,
                rx,
                tx,
                ProductionRuntimeFlowResult::Fail,
                Some(error),
            );
        }
    };
    rx.rx_forwards = production_rx_forward_snapshots(&rx_forwards);
    rx.forwarded_payloads = rx
        .rx_forwards
        .iter()
        .map(|forward| forward.counters.forwarded)
        .sum();
    apply_production_runtime_tx_telemetry(&mut tx, &bridge_counters, &submit_counters);

    production_runtime_flow_report_from_state_with_airtime_and_health(
        &config,
        session,
        loop_outcome.stop_reason.as_str(),
        init_telemetry,
        pre_loop,
        heartbeat_led,
        rx,
        tx,
        ProductionRuntimeAirtimeReport {
            schedule: config.airtime_schedule,
            tx_allowed_iterations: loop_outcome.airtime_tx_allowed_iterations,
            tx_gated_iterations: loop_outcome.airtime_tx_gated_iterations,
        },
        ProductionRuntimeFlowResult::Pass,
        None,
    )
}

fn production_runtime_ready_marker(
    config: &ProductionRuntimeFlowConfig,
    validation: &ProductionRuntimeFlowValidation,
    init: &RuntimeSameSessionInitResult,
    monitor_opmode_applied: bool,
    tx_power_control_applied: bool,
    tx_calibration_profile_applied: bool,
    rx_startup_kick_applied: bool,
) -> ProductionRuntimeReadyMarker {
    ProductionRuntimeReadyMarker {
        source: "bridge-run".to_string(),
        ready_at_unix_ms: None,
        bind_addr: config.bind_addr.to_string(),
        bind_addrs: validation
            .wfb_loop
            .tx_bind_addrs
            .iter()
            .map(ToString::to_string)
            .collect(),
        channel: Some(config.channel.number),
        channel_frequency_mhz: Some(config.channel.frequency_mhz),
        bandwidth_mhz: config.bandwidth.mhz(),
        max_datagrams: config.max_datagrams,
        duration_ms: Some(config.duration_ms),
        idle_timeout_ms: None,
        rx_timeout_ms: Some(config.rx_timeout_ms),
        tx_burst_limit: Some(config.tx_burst_limit),
        airtime_schedule: config.airtime_schedule,
        init_before_tx: true,
        same_session_init_result: Some(
            match init.readiness {
                RuntimeSameSessionInitReadiness::Ready => "pass",
                RuntimeSameSessionInitReadiness::Failed => "fail",
            }
            .to_string(),
        ),
        monitor_opmode_applied: Some(monitor_opmode_applied),
        tx_power_control_applied,
        tx_calibration_profile_applied,
        rx_startup_kick_applied,
    }
}

fn apply_production_runtime_monitor_opmode<T>(
    transport: &T,
    counters: &mut RuntimeRadioCounters,
) -> Result<(), RuntimeRadioError>
where
    for<'a> &'a T: Rtl8812auUsbTransport,
{
    let registers = Rtl8812auRegisterAccess::new(transport);
    run_rtl8812au_monitor_opmode(&registers, counters).map(|_| ())
}

fn apply_production_runtime_rx_startup_kick<T>(
    session: &mut RuntimeRadioSession<T>,
    config: &ProductionRuntimeFlowConfig,
) -> Result<ProductionRuntimeRxStartupKickReport, RuntimeRadioError>
where
    T: UsbBulkTransfer,
{
    let tx_options = TxOptions {
        rate: TxRate::Ofdm6m,
        bandwidth: Bandwidth::Mhz20,
        channel_bandwidth: Some(config.bandwidth),
        queue: TxQueue::Mgnt,
        retries: 0,
        no_retry: true,
        rate_fallback_limit: 0,
        ..TxOptions::default()
    };
    let mut counters = TxSubmitCounters::default();
    session.submit_80211_frame(
        &PRODUCTION_RX_STARTUP_KICK_FRAME,
        config.channel,
        tx_options,
        &mut counters,
    )?;
    Ok(ProductionRuntimeRxStartupKickReport {
        semantics: "one-shot non-WFB null-data TX submitted after init and before ready marker to wake RTL8812AU RX DMA on macOS; required for RX-only production runs observed on high-band channels",
        frame_len: PRODUCTION_RX_STARTUP_KICK_FRAME.len(),
        tx_options,
        attempted: counters.attempted,
        submitted: counters.submitted,
        failed: counters.failed,
        bytes_written: counters.bytes_written,
    })
}

fn apply_production_runtime_tx_power_control<T>(
    transport: &T,
    counters: &mut RuntimeRadioCounters,
    input: &ProductionRuntimeTxPowerControlInput,
) -> Result<Option<Rtl8812auTxPowerControlReport>, RuntimeRadioError>
where
    for<'a> &'a T: Rtl8812auUsbTransport,
{
    let registers = Rtl8812auRegisterAccess::new(transport);
    match input {
        ProductionRuntimeTxPowerControlInput::None => Ok(None),
        ProductionRuntimeTxPowerControlInput::ManualIndex { path, index } => {
            let value = rtl8812au_tx_power_agc_value(*index);
            let writes = run_rtl8812au_manual_tx_power(&registers, counters, *path, *index)?;
            Ok(Some(Rtl8812auTxPowerControlReport {
                semantics: "explicit runtime TXAGC manual override; writes the selected index to every byte lane of each selected per-rate TX power register after init and before TX",
                mode: Rtl8812auTxPowerControlMode::ManualIndex,
                manual_index: Some(*index),
                manual_index_hex: Some(format_register_value(*index, 2)),
                path: *path,
                register_count: writes.len(),
                repeated_value: Some(value),
                repeated_value_hex: Some(format_register_value(value, 8)),
                efuse_source: None,
                efuse_plan: None,
                writes,
            }))
        }
        ProductionRuntimeTxPowerControlInput::EfuseDerived { source, plan } => {
            let writes = run_rtl8812au_efuse_tx_power(&registers, counters, plan)?;
            Ok(Some(Rtl8812auTxPowerControlReport {
                semantics: "explicit guarded runtime EFUSE-derived TXAGC programming; computes per-path/per-rate indexes from the EFUSE TX-power region, default PHY_REG_PG by-rate offsets, and the selected safety clamp",
                mode: Rtl8812auTxPowerControlMode::EfuseDerived,
                manual_index: None,
                manual_index_hex: None,
                path: plan.selected_path,
                register_count: writes.len(),
                repeated_value: None,
                repeated_value_hex: None,
                efuse_source: Some(source.clone()),
                efuse_plan: Some(plan.clone()),
                writes,
            }))
        }
    }
}

fn apply_production_runtime_tx_calibration_profile<T>(
    transport: &T,
    counters: &mut RuntimeRadioCounters,
    config: &ProductionRuntimeFlowConfig,
    rfe_type: u8,
) -> Result<Option<Rtl8812auTxCalibrationProfileReport>, RuntimeRadioError>
where
    for<'a> &'a T: Rtl8812auUsbTransport,
{
    let registers = Rtl8812auRegisterAccess::new(transport);
    let report = run_rtl8812au_tx_calibration_profile(
        &registers,
        counters,
        config.calibration_profile,
        config.channel,
        config.bandwidth,
        rfe_type,
    )?;
    if let Some(report) = report.as_ref() {
        validate_production_runtime_tx_calibration_profile_ready_for_tx(report)?;
    }
    Ok(report)
}

fn validate_production_runtime_tx_calibration_profile_ready_for_tx(
    profile: &Rtl8812auTxCalibrationProfileReport,
) -> Result<(), RuntimeRadioError> {
    let Some(iqk) = profile.runtime_iqk.as_ref() else {
        return Ok(());
    };
    if iqk.cleanup_status != "restored" {
        return Err(RuntimeRadioError::new(
            "runtime_iqk_cleanup_not_restored",
            format!(
                "runtime IQK cleanup status is {}; refusing live TX",
                iqk.cleanup_status
            ),
        ));
    }
    if iqk.status != "completed" {
        return Err(RuntimeRadioError::new(
            "runtime_iqk_not_completed",
            format!(
                "runtime IQK status is {} after {}/{} sweeps; refusing live TX because fallback IQK has produced receiver decrypt failures on hardware",
                iqk.status, iqk.sweep_count, iqk.max_sweeps
            ),
        ));
    }
    if !iqk.selected_iqc_fill_applied {
        return Err(RuntimeRadioError::new(
            "runtime_iqk_fill_not_applied",
            "runtime IQK completed but selected IQC fill was not applied after cleanup; refusing live TX",
        ));
    }
    Ok(())
}

fn apply_production_runtime_rx_packet_telemetry(
    rx: &mut RuntimeFlowRxTelemetry,
    telemetry: ProductionRuntimeRxPacketTelemetry,
) {
    rx.parsed_frames = rx.parsed_frames.saturating_add(telemetry.parsed_frames);
    rx.phy_status_frames = rx
        .phy_status_frames
        .saturating_add(telemetry.phy_status_frames);
    rx.rssi_valid_frames = rx
        .rssi_valid_frames
        .saturating_add(telemetry.rssi_valid_frames);
    rx.snr_frames = rx.snr_frames.saturating_add(telemetry.snr_frames);
    rx.noise_frames = rx.noise_frames.saturating_add(telemetry.noise_frames);
    rx.signal.merge(&telemetry.signal);
    rx.dropped_packets = rx.dropped_packets.saturating_add(telemetry.dropped_packets);
    rx.need_more_data = rx.need_more_data.saturating_add(telemetry.need_more_data);
    rx.management_frames = rx
        .management_frames
        .saturating_add(telemetry.management_frames);
    rx.control_frames = rx.control_frames.saturating_add(telemetry.control_frames);
    rx.data_frames = rx.data_frames.saturating_add(telemetry.data_frames);
    rx.extension_frames = rx
        .extension_frames
        .saturating_add(telemetry.extension_frames);
    merge_runtime_wfb_channel_observations(
        &mut rx.wfb_channel_observations,
        telemetry.wfb_channel_observations,
    );
}

fn apply_production_runtime_tx_telemetry(
    tx: &mut RuntimeFlowTxTelemetry,
    bridge_counters: &TxCounters,
    submit_counters: &TxSubmitCounters,
) {
    tx.submitted_frames = submit_counters.submitted;
    tx.failed_submissions = submit_counters.failed;
    tx.dropped_datagrams = bridge_counters.dropped;
    tx.bytes_written = submit_counters.bytes_written;
}

fn production_runtime_init_telemetry(
    init: &RuntimeSameSessionInitResult,
) -> ProductionRuntimeInitTelemetry {
    ProductionRuntimeInitTelemetry {
        readiness: match init.readiness {
            RuntimeSameSessionInitReadiness::Ready => ProductionRuntimeInitReadiness::Ready,
            RuntimeSameSessionInitReadiness::Failed => ProductionRuntimeInitReadiness::Failed,
        },
        phase_count: init.phase_summaries.len(),
        completed_phase_count: init
            .phase_summaries
            .iter()
            .filter(|phase| matches!(phase.status, RuntimeSameSessionInitPhaseStatus::Completed))
            .count(),
    }
}

fn production_runtime_report_with_health(
    config: &ProductionRuntimeFlowConfig,
    mut report: ProductionRuntimeFlowReport,
    lifecycle: ProductionRuntimeServiceLifecycle,
) -> ProductionRuntimeFlowReport {
    let health = ProductionRuntimeServiceHealth::from_report(
        &report,
        lifecycle,
        config.health_file.clone(),
        None,
    );
    if let Err(error) =
        write_production_runtime_service_health(config.health_file.as_deref(), &health)
    {
        report.result = ProductionRuntimeFlowResult::Fail;
        report.error = Some(error.into());
    }
    report
}

fn production_runtime_flow_report_from_state_with_health<T>(
    config: &ProductionRuntimeFlowConfig,
    session: &RuntimeRadioSession<T>,
    stop_reason: &'static str,
    init: ProductionRuntimeInitTelemetry,
    pre_loop: ProductionRuntimePreLoopReports,
    heartbeat_led: Option<ProductionRuntimeHeartbeatLedReport>,
    rx: RuntimeFlowRxTelemetry,
    tx: RuntimeFlowTxTelemetry,
    result: ProductionRuntimeFlowResult,
    error: Option<RuntimeRadioError>,
) -> ProductionRuntimeFlowReport {
    production_runtime_flow_report_from_state_with_airtime_and_health(
        config,
        session,
        stop_reason,
        init,
        pre_loop,
        heartbeat_led,
        rx,
        tx,
        ProductionRuntimeAirtimeReport::idle(config.airtime_schedule),
        result,
        error,
    )
}

fn production_runtime_flow_report_from_state_with_airtime_and_health<T>(
    config: &ProductionRuntimeFlowConfig,
    session: &RuntimeRadioSession<T>,
    stop_reason: &'static str,
    init: ProductionRuntimeInitTelemetry,
    pre_loop: ProductionRuntimePreLoopReports,
    heartbeat_led: Option<ProductionRuntimeHeartbeatLedReport>,
    rx: RuntimeFlowRxTelemetry,
    tx: RuntimeFlowTxTelemetry,
    airtime: ProductionRuntimeAirtimeReport,
    result: ProductionRuntimeFlowResult,
    error: Option<RuntimeRadioError>,
) -> ProductionRuntimeFlowReport {
    let lifecycle = match result {
        ProductionRuntimeFlowResult::Pass => ProductionRuntimeServiceLifecycle::ExitedPass,
        ProductionRuntimeFlowResult::Fail => ProductionRuntimeServiceLifecycle::ExitedFail,
    };
    let report = production_runtime_flow_report_from_state_with_airtime(
        config,
        session,
        stop_reason,
        init,
        pre_loop,
        heartbeat_led,
        rx,
        tx,
        airtime,
        result,
        error,
    );
    production_runtime_report_with_health(config, report, lifecycle)
}

fn production_runtime_flow_report_from_state<T>(
    config: &ProductionRuntimeFlowConfig,
    session: &RuntimeRadioSession<T>,
    stop_reason: &'static str,
    init: ProductionRuntimeInitTelemetry,
    pre_loop: ProductionRuntimePreLoopReports,
    heartbeat_led: Option<ProductionRuntimeHeartbeatLedReport>,
    rx: RuntimeFlowRxTelemetry,
    tx: RuntimeFlowTxTelemetry,
    result: ProductionRuntimeFlowResult,
    error: Option<RuntimeRadioError>,
) -> ProductionRuntimeFlowReport {
    production_runtime_flow_report_from_state_with_airtime(
        config,
        session,
        stop_reason,
        init,
        pre_loop,
        heartbeat_led,
        rx,
        tx,
        ProductionRuntimeAirtimeReport::idle(config.airtime_schedule),
        result,
        error,
    )
}

fn production_runtime_flow_report_from_state_with_airtime<T>(
    config: &ProductionRuntimeFlowConfig,
    session: &RuntimeRadioSession<T>,
    stop_reason: &'static str,
    init: ProductionRuntimeInitTelemetry,
    pre_loop: ProductionRuntimePreLoopReports,
    heartbeat_led: Option<ProductionRuntimeHeartbeatLedReport>,
    rx: RuntimeFlowRxTelemetry,
    tx: RuntimeFlowTxTelemetry,
    airtime: ProductionRuntimeAirtimeReport,
    result: ProductionRuntimeFlowResult,
    error: Option<RuntimeRadioError>,
) -> ProductionRuntimeFlowReport {
    let calibration_class = config
        .calibration_profile
        .before_tx_class(config.captured_tail_applied);
    ProductionRuntimeFlowReport::from_execution(
        config,
        ProductionRuntimeFlowExecutionReport {
            selector: config.usb.selector,
            adapter: Some(session.adapter.clone()),
            endpoints: Some(session.endpoints.clone()),
            channel: Some(config.channel),
            bandwidth: config.bandwidth,
            duration_ms: config.duration_ms,
            ready_file: config.ready_file.clone(),
            stop_reason,
            bulk_in_endpoint: session.selected_bulk_in_endpoint(),
            bulk_out_endpoint: session.selected_bulk_out_endpoint(),
            calibration_profile: config.calibration_profile,
            calibration_class,
            tx_power_control: pre_loop.tx_power_control,
            tx_calibration_profile: pre_loop.tx_calibration_profile,
            rx_startup_kick: pre_loop.rx_startup_kick,
            heartbeat_led,
            airtime,
            receiver_backed_validation_required: !config.calibration_profile.is_default(),
            init,
            rx,
            tx,
            counters: session.counters,
            result,
            error: error.map(Into::into),
        },
    )
}

pub struct RuntimeRadioSession<T = RuntimeUsbTransport> {
    pub transport: T,
    pub adapter: UsbDeviceInfo,
    pub endpoints: UsbEndpoints,
    pub counters: RuntimeRadioCounters,
}

impl<T> RuntimeRadioSession<T> {
    pub fn new(
        transport: T,
        adapter: UsbDeviceInfo,
        endpoints: UsbEndpoints,
        counters: RuntimeRadioCounters,
    ) -> Self {
        Self {
            transport,
            adapter,
            endpoints,
            counters,
        }
    }

    pub fn register_access(&self) -> Rtl8812auRegisterAccess<&T>
    where
        for<'a> &'a T: Rtl8812auUsbTransport,
    {
        Rtl8812auRegisterAccess::new(&self.transport)
    }

    pub fn selected_bulk_in_endpoint(&self) -> Option<u8> {
        self.endpoints.bulk_in
    }

    pub fn selected_bulk_out_endpoint(&self) -> Option<u8> {
        self.endpoints.bulk_out
    }

    pub fn add_counters(&mut self, delta: RuntimeRadioCounters) {
        self.counters.usb_control_reads = self
            .counters
            .usb_control_reads
            .saturating_add(delta.usb_control_reads);
        self.counters.usb_control_writes = self
            .counters
            .usb_control_writes
            .saturating_add(delta.usb_control_writes);
        self.counters.usb_bulk_in_reads = self
            .counters
            .usb_bulk_in_reads
            .saturating_add(delta.usb_bulk_in_reads);
        self.counters.usb_bulk_out_writes = self
            .counters
            .usb_bulk_out_writes
            .saturating_add(delta.usb_bulk_out_writes);
        self.counters.rx_frames = self.counters.rx_frames.saturating_add(delta.rx_frames);
        self.counters.tx_frames = self.counters.tx_frames.saturating_add(delta.tx_frames);
        self.counters.dropped_frames = self
            .counters
            .dropped_frames
            .saturating_add(delta.dropped_frames);
    }

    pub fn submit_80211_frame(
        &mut self,
        frame: &[u8],
        channel: Channel,
        options: TxOptions,
        tx_counters: &mut TxSubmitCounters,
    ) -> Result<usize, RuntimeRadioError>
    where
        T: UsbBulkTransfer,
    {
        let bulk_out = self.selected_bulk_out_endpoint().ok_or_else(|| {
            RuntimeRadioError::new(
                "missing_bulk_out_endpoint",
                "runtime radio session has no selected bulk OUT endpoint",
            )
        })?;
        let before = tx_counters.clone();
        let result = submit_tx_frame(
            &mut self.transport,
            bulk_out,
            frame,
            channel,
            options,
            tx_counters,
        );
        self.apply_tx_submit_counter_delta(&before, tx_counters);
        result.map_err(runtime_tx_submit_error)
    }

    pub fn submit_raw_tx_packet(
        &mut self,
        packet: &[u8],
        tx_counters: &mut TxSubmitCounters,
        timeout: Duration,
    ) -> Result<usize, RuntimeRadioError>
    where
        T: UsbBulkTransfer,
    {
        let bulk_out = self.selected_bulk_out_endpoint().ok_or_else(|| {
            RuntimeRadioError::new(
                "missing_bulk_out_endpoint",
                "runtime radio session has no selected bulk OUT endpoint",
            )
        })?;
        tx_counters.attempted = tx_counters.attempted.saturating_add(1);
        match self
            .transport
            .write_bulk_transfer(bulk_out, packet, timeout)
        {
            Ok(written) if written == packet.len() => {
                tx_counters.submitted = tx_counters.submitted.saturating_add(1);
                tx_counters.bytes_written =
                    tx_counters.bytes_written.saturating_add(written as u64);
                self.counters.usb_bulk_out_writes =
                    self.counters.usb_bulk_out_writes.saturating_add(1);
                self.counters.tx_frames = self.counters.tx_frames.saturating_add(1);
                Ok(written)
            }
            Ok(written) => {
                tx_counters.failed = tx_counters.failed.saturating_add(1);
                tx_counters.short_writes = tx_counters.short_writes.saturating_add(1);
                tx_counters.bytes_written =
                    tx_counters.bytes_written.saturating_add(written as u64);
                self.counters.usb_bulk_out_writes =
                    self.counters.usb_bulk_out_writes.saturating_add(1);
                self.counters.dropped_frames = self.counters.dropped_frames.saturating_add(1);
                Err(RuntimeRadioError::new(
                    "raw_tx_packet_short_write",
                    format!(
                        "short bulk OUT write to endpoint 0x{bulk_out:02x}: expected {} bytes, wrote {written}",
                        packet.len()
                    ),
                ))
            }
            Err(error) => {
                tx_counters.failed = tx_counters.failed.saturating_add(1);
                self.counters.usb_bulk_out_writes =
                    self.counters.usb_bulk_out_writes.saturating_add(1);
                self.counters.dropped_frames = self.counters.dropped_frames.saturating_add(1);
                Err(RuntimeRadioError::new(
                    "raw_tx_packet_submit_failed",
                    error.to_string(),
                ))
            }
        }
    }

    pub fn read_rx_packets(
        &mut self,
        channel: Channel,
        buffer: &mut [u8],
        timeout: Duration,
    ) -> Result<RuntimeRxRead, RuntimeRadioError>
    where
        T: UsbBulkTransfer,
    {
        let bulk_in = self.selected_bulk_in_endpoint().ok_or_else(|| {
            RuntimeRadioError::new(
                "missing_bulk_in_endpoint",
                "runtime radio session has no selected bulk IN endpoint",
            )
        })?;
        let before = self.counters;
        let bytes_read = self
            .transport
            .read_bulk_transfer(bulk_in, buffer, timeout)
            .map_err(|error| runtime_bulk_in_error(bulk_in, error))?;
        self.counters.usb_bulk_in_reads = self.counters.usb_bulk_in_reads.saturating_add(1);

        let mut packets = Vec::new();
        let mut offset = 0usize;
        while offset < bytes_read {
            let parsed = parse_rx_packet(&buffer[offset..bytes_read], channel);
            match parsed.outcome {
                RxParseOutcome::Frame => {
                    self.counters.rx_frames = self.counters.rx_frames.saturating_add(1);
                }
                RxParseOutcome::Drop => {
                    self.counters.dropped_frames = self.counters.dropped_frames.saturating_add(1);
                }
                RxParseOutcome::NeedMoreData => {
                    packets.push(parsed);
                    break;
                }
            }

            let consumed = parsed.consumed;
            packets.push(parsed);
            if consumed == 0 {
                break;
            }
            offset = offset.saturating_add(consumed);
        }

        Ok(RuntimeRxRead {
            endpoint: bulk_in,
            bytes_read,
            packets,
            counters: self.counters.saturating_sub(before),
        })
    }

    fn apply_tx_submit_counter_delta(
        &mut self,
        before: &TxSubmitCounters,
        after: &TxSubmitCounters,
    ) {
        let submitted = after.submitted.saturating_sub(before.submitted);
        let failed = after.failed.saturating_sub(before.failed);
        let rejected = after.rejected.saturating_sub(before.rejected);
        self.counters.usb_bulk_out_writes = self
            .counters
            .usb_bulk_out_writes
            .saturating_add(submitted.saturating_add(failed));
        self.counters.tx_frames = self.counters.tx_frames.saturating_add(submitted);
        self.counters.dropped_frames = self
            .counters
            .dropped_frames
            .saturating_add(rejected.saturating_add(failed));
    }
}

fn runtime_tx_submit_error(error: Rtl8812auTxSubmitError) -> RuntimeRadioError {
    RuntimeRadioError::new("tx_submit_failed", error.to_string())
}

fn runtime_bulk_in_error(endpoint: u8, error: UsbError) -> RuntimeRadioError {
    let message = format!("bulk IN read from endpoint 0x{endpoint:02x} failed: {error}");
    if error.is_timeout() {
        RuntimeRadioError::new_timeout("bulk_in_read_timeout", message)
    } else {
        RuntimeRadioError::new("bulk_in_read_failed", message)
    }
}

impl RuntimeRadioSession<RuntimeUsbTransport> {
    pub fn from_open(open: RuntimeUsbTransportOpen) -> Self {
        Self::new(
            open.transport,
            open.adapter,
            open.endpoints,
            RuntimeRadioCounters {
                usb_control_writes: open.initial_usb_control_writes,
                ..RuntimeRadioCounters::default()
            },
        )
    }

    pub fn open(config: RuntimeUsbOpenConfig) -> Result<Self, RuntimeTransportError> {
        open_runtime_usb_transport(config).map(Self::from_open)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rtl8812auInitOrder {
    Default,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rtl8812auInitPhase {
    PowerOn,
    Firmware,
    Llt,
    MacTable,
    QueueDma,
    Mac,
    MacAddr,
    Bb,
    Rf,
    RfCalibrationBeforeChannel,
    Channel,
    RfCalibrationAfterChannel,
    TxSchedulerTail,
    RfCalibrationBeforeTx,
}

impl Rtl8812auInitPhase {
    pub fn id(self) -> &'static str {
        match self {
            Self::PowerOn => "power_on",
            Self::Firmware => "firmware",
            Self::Llt => "llt",
            Self::MacTable => "mac_table",
            Self::QueueDma => "queue_dma",
            Self::Mac => "mac",
            Self::MacAddr => "mac_addr",
            Self::Bb => "bb",
            Self::Rf => "rf",
            Self::RfCalibrationBeforeChannel => "rf_calibration_before_channel",
            Self::Channel => "channel",
            Self::RfCalibrationAfterChannel => "rf_calibration_after_channel",
            Self::TxSchedulerTail => "tx_scheduler_tail",
            Self::RfCalibrationBeforeTx => "rf_calibration_before_tx",
        }
    }
}

const RTL8812AU_DEFAULT_SAME_SESSION_INIT_SEQUENCE: &[Rtl8812auInitPhase] = &[
    Rtl8812auInitPhase::PowerOn,
    Rtl8812auInitPhase::Firmware,
    Rtl8812auInitPhase::Llt,
    Rtl8812auInitPhase::MacTable,
    Rtl8812auInitPhase::QueueDma,
    Rtl8812auInitPhase::Mac,
    Rtl8812auInitPhase::MacAddr,
    Rtl8812auInitPhase::Bb,
    Rtl8812auInitPhase::Rf,
    Rtl8812auInitPhase::RfCalibrationBeforeChannel,
    Rtl8812auInitPhase::Channel,
    Rtl8812auInitPhase::RfCalibrationAfterChannel,
    Rtl8812auInitPhase::TxSchedulerTail,
    Rtl8812auInitPhase::RfCalibrationBeforeTx,
];

const RTL8812AU_LINUX_SAME_SESSION_INIT_SEQUENCE: &[Rtl8812auInitPhase] = &[
    Rtl8812auInitPhase::PowerOn,
    Rtl8812auInitPhase::Llt,
    Rtl8812auInitPhase::Firmware,
    Rtl8812auInitPhase::MacTable,
    Rtl8812auInitPhase::QueueDma,
    Rtl8812auInitPhase::Mac,
    Rtl8812auInitPhase::MacAddr,
    Rtl8812auInitPhase::Bb,
    Rtl8812auInitPhase::Rf,
    Rtl8812auInitPhase::RfCalibrationBeforeChannel,
    Rtl8812auInitPhase::Channel,
    Rtl8812auInitPhase::RfCalibrationAfterChannel,
    Rtl8812auInitPhase::TxSchedulerTail,
    Rtl8812auInitPhase::RfCalibrationBeforeTx,
];

pub fn rtl8812au_same_session_init_sequence(
    order: Rtl8812auInitOrder,
) -> &'static [Rtl8812auInitPhase] {
    match order {
        Rtl8812auInitOrder::Default => RTL8812AU_DEFAULT_SAME_SESSION_INIT_SEQUENCE,
        Rtl8812auInitOrder::Linux => RTL8812AU_LINUX_SAME_SESSION_INIT_SEQUENCE,
    }
}

pub fn rtl8812au_llt_firmware_sequence(order: Rtl8812auInitOrder) -> &'static [Rtl8812auInitPhase] {
    &rtl8812au_same_session_init_sequence(order)[1..=2]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSameSessionInitReadiness {
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSameSessionInitPhaseStatus {
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTxCalibrationEvidenceSource {
    Default,
    CapturedLinuxTail,
    TargetedLinuxParityCapture,
    RuntimeLck,
    ReadOnlyIqkProbe,
    RuntimeIqk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTxCalibrationValidationStatus {
    NotRequired,
    ReceiverBackedValidationRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTxCalibrationDecision {
    pub profile: TxCalibrationProfile,
    pub class: TxCalibrationClass,
    pub evidence_source: RuntimeTxCalibrationEvidenceSource,
    pub requires_live_write_authorization: bool,
    pub authorized: bool,
    pub validation_status: RuntimeTxCalibrationValidationStatus,
    pub production_safe_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSameSessionInitConfig {
    pub init_order: Rtl8812auInitOrder,
    pub channel: Channel,
    pub bandwidth: Bandwidth,
    pub rfe_type: u8,
    pub tx_calibration_profile: TxCalibrationProfile,
    pub live_write_authorized: bool,
    pub captured_tail_applied: bool,
}

impl RuntimeSameSessionInitConfig {
    pub fn new(channel: Channel, bandwidth: Bandwidth) -> Self {
        Self {
            init_order: Rtl8812auInitOrder::Default,
            channel,
            bandwidth,
            rfe_type: 0,
            tx_calibration_profile: TxCalibrationProfile::CurrentDefault,
            live_write_authorized: false,
            captured_tail_applied: false,
        }
    }

    pub fn calibration_decision(self) -> Result<RuntimeTxCalibrationDecision, RuntimeRadioError> {
        self.tx_calibration_profile
            .calibration_decision(self.captured_tail_applied, self.live_write_authorized)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSameSessionInitPhaseSummary {
    pub phase: Rtl8812auInitPhase,
    pub status: RuntimeSameSessionInitPhaseStatus,
    pub detail: String,
    pub register_writes: Option<usize>,
    pub counters: RuntimeRadioCounters,
}

impl RuntimeSameSessionInitPhaseSummary {
    pub fn completed(
        phase: Rtl8812auInitPhase,
        detail: impl Into<String>,
        before: RuntimeRadioCounters,
        after: RuntimeRadioCounters,
    ) -> Self {
        Self {
            phase,
            status: RuntimeSameSessionInitPhaseStatus::Completed,
            detail: detail.into(),
            register_writes: None,
            counters: after.saturating_sub(before),
        }
    }

    pub fn completed_with_writes(
        phase: Rtl8812auInitPhase,
        detail: impl Into<String>,
        register_writes: usize,
        before: RuntimeRadioCounters,
        after: RuntimeRadioCounters,
    ) -> Self {
        Self {
            register_writes: Some(register_writes),
            ..Self::completed(phase, detail, before, after)
        }
    }

    pub fn blocked(
        phase: Rtl8812auInitPhase,
        detail: impl Into<String>,
        before: RuntimeRadioCounters,
        after: RuntimeRadioCounters,
    ) -> Self {
        Self {
            phase,
            status: RuntimeSameSessionInitPhaseStatus::Blocked,
            detail: detail.into(),
            register_writes: None,
            counters: after.saturating_sub(before),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSameSessionInitResult {
    pub config: RuntimeSameSessionInitConfig,
    pub calibration: RuntimeTxCalibrationDecision,
    pub phase_summaries: Vec<RuntimeSameSessionInitPhaseSummary>,
    pub counters: RuntimeRadioCounters,
    pub readiness: RuntimeSameSessionInitReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSameSessionInitFailure {
    pub result: RuntimeSameSessionInitResult,
    pub error: RuntimeRadioError,
}

impl fmt::Display for RuntimeSameSessionInitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "same-session init failed: {}", self.error)
    }
}

impl Error for RuntimeSameSessionInitFailure {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSameSessionInitPhaseFailure {
    pub summary: RuntimeSameSessionInitPhaseSummary,
    pub error: RuntimeRadioError,
}

impl RuntimeSameSessionInitPhaseFailure {
    pub fn new(summary: RuntimeSameSessionInitPhaseSummary, error: RuntimeRadioError) -> Self {
        Self { summary, error }
    }
}

pub fn run_rtl8812au_same_session_init<T, F>(
    session: &mut RuntimeRadioSession<T>,
    config: RuntimeSameSessionInitConfig,
    mut run_phase: F,
) -> Result<RuntimeSameSessionInitResult, RuntimeSameSessionInitFailure>
where
    F: FnMut(
        &mut RuntimeRadioSession<T>,
        Rtl8812auInitPhase,
    ) -> Result<RuntimeSameSessionInitPhaseSummary, RuntimeSameSessionInitPhaseFailure>,
{
    let calibration = match config.calibration_decision() {
        Ok(calibration) => calibration,
        Err(error) => {
            let result = RuntimeSameSessionInitResult {
                config,
                calibration: RuntimeTxCalibrationDecision {
                    profile: config.tx_calibration_profile,
                    class: config
                        .tx_calibration_profile
                        .before_tx_class(config.captured_tail_applied),
                    evidence_source: config
                        .tx_calibration_profile
                        .evidence_source(config.captured_tail_applied),
                    requires_live_write_authorization: config
                        .tx_calibration_profile
                        .requires_register_write_authorization(),
                    authorized: config.live_write_authorized,
                    validation_status: config.tx_calibration_profile.validation_status(),
                    production_safe_default: config.tx_calibration_profile.is_default(),
                },
                phase_summaries: Vec::new(),
                counters: session.counters,
                readiness: RuntimeSameSessionInitReadiness::Failed,
            };
            return Err(RuntimeSameSessionInitFailure { result, error });
        }
    };

    let mut phase_summaries = Vec::new();
    for phase in rtl8812au_same_session_init_sequence(config.init_order) {
        match run_phase(session, *phase) {
            Ok(summary) => phase_summaries.push(summary),
            Err(failure) => {
                phase_summaries.push(failure.summary);
                let result = RuntimeSameSessionInitResult {
                    config,
                    calibration,
                    phase_summaries,
                    counters: session.counters,
                    readiness: RuntimeSameSessionInitReadiness::Failed,
                };
                return Err(RuntimeSameSessionInitFailure {
                    result,
                    error: failure.error,
                });
            }
        }
    }

    Ok(RuntimeSameSessionInitResult {
        config,
        calibration,
        phase_summaries,
        counters: session.counters,
        readiness: RuntimeSameSessionInitReadiness::Ready,
    })
}

const REG_ACLK_MON: u16 = 0x003e;
const REG_EFUSE_CTRL: u16 = 0x0030;
const REG_EFUSE_BURN_GNT_8812: u16 = 0x00cf;
const REG_SDIO_CTRL_8812: u16 = 0x0070;
const REG_SYS_ISO_CTRL: u16 = 0x0000;
const REG_APS_FSMCO_PLUS_1: u16 = 0x0005;
const REG_APS_FSMCO_PLUS_2: u16 = 0x0006;
const REG_SYS_FUNC_EN: u16 = 0x0002;
const REG_SYS_FUNC_EN_PLUS_1: u16 = 0x0003;
const REG_SYS_CLKR: u16 = 0x0008;
const REG_RSV_CTRL: u16 = 0x001c;
const REG_AFE_XTAL_CTRL: u16 = 0x0024;
const REG_AFE_PLL_CTRL: u16 = 0x0028;
const REG_RF_CTRL: u16 = 0x001f;
const REG_RF_B_CTRL_8812: u16 = 0x0076;
const REG_MAC_PHY_CTRL: u16 = 0x002c;
const REG_MCUFWDL: u16 = 0x0080;
const REG_MCUFWDL_PLUS_2: u16 = REG_MCUFWDL + 2;
const REG_CR: u16 = 0x0100;
const REG_MSR: u16 = REG_CR + 2;
const REG_PBP: u16 = 0x0104;
const REG_TRXDMA_CTRL: u16 = 0x010c;
const REG_TRXFF_BNDY: u16 = 0x0114;
const REG_LLT_INIT: u16 = 0x01e0;
const REG_RQPN: u16 = 0x0200;
const REG_TDECTRL: u16 = 0x0208;
const REG_RQPN_NPQ: u16 = 0x0214;
const REG_RXDMA_STATUS: u16 = 0x0288;
const REG_RXDMA_PRO_8812: u16 = 0x0290;
const REG_EARLY_MODE_CONTROL_8812: u16 = 0x02bc;
const REG_FWHW_TXQ_CTRL: u16 = 0x0420;
const REG_HWSEQ_CTRL: u16 = 0x0423;
const REG_BCNQ_BDNY: u16 = 0x0424;
const REG_MGQ_BDNY: u16 = 0x0425;
const REG_SPEC_SIFS: u16 = 0x0428;
const REG_RETRY_LIMIT: u16 = 0x042a;
const REG_RRSR: u16 = 0x0440;
const REG_ARFR0_8812: u16 = 0x0444;
const REG_ARFR1_8812: u16 = 0x044c;
const REG_CCK_CHECK_8812: u16 = 0x0454;
const REG_AMPDU_MAX_TIME_8812: u16 = 0x0456;
const REG_AMPDU_MAX_LENGTH_8812: u16 = 0x0458;
const REG_WMAC_LBK_BF_HD: u16 = 0x045d;
const REG_TXPKT_EMPTY: u16 = 0x041a;
const REG_DATA_SC_8812: u16 = 0x0483;
const REG_ARFR2_8812: u16 = 0x048c;
const REG_ARFR3_8812: u16 = 0x0494;
const REG_QUEUE_CTRL: u16 = 0x04c6;
const REG_HT_SINGLE_AMPDU_8812: u16 = 0x04c7;
const REG_MAX_AGGR_NUM: u16 = 0x04ca;
const REG_BAR_MODE_CTRL: u16 = 0x04cc;
const REG_TX_RPT_TIME: u16 = 0x04f0;
const REG_BCNTCFG: u16 = 0x0510;
const REG_EDCA_VO_PARAM: u16 = 0x0500;
const REG_EDCA_VI_PARAM: u16 = 0x0504;
const REG_EDCA_BE_PARAM: u16 = 0x0508;
const REG_EDCA_BK_PARAM: u16 = 0x050c;
const REG_PIFS: u16 = 0x0512;
const REG_SIFS_CTX: u16 = 0x0514;
const REG_SIFS_TRX: u16 = 0x0516;
const REG_TXPAUSE: u16 = 0x0522;
const REG_TBTT_PROHIBIT: u16 = 0x0540;
const REG_DRVERLYINT: u16 = 0x0558;
const REG_BCNDMATIM: u16 = 0x0559;
const REG_USTIME_TSF: u16 = 0x055c;
const REG_RCR: u16 = 0x0608;
const REG_RX_PKT_LIMIT: u16 = 0x060c;
const REG_RX_DRVINFO_SZ: u16 = 0x060f;
const REG_MACID: u16 = 0x0610;
const REG_MAR: u16 = 0x0620;
const REG_USTIME_EDCA: u16 = 0x0638;
const REG_MAC_SPEC_SIFS: u16 = 0x063a;
const REG_ACKTO: u16 = 0x0640;
const REG_NAV_UPPER: u16 = 0x0652;
const REG_WMAC_TRXPTCL_CTL: u16 = 0x0668;
const REG_RXFLTMAP1: u16 = 0x06a2;
const REG_RXFLTMAP2: u16 = 0x06a4;
const REG_BCN_CTRL: u16 = 0x0550;
const REG_AGC_TABLE_JAGUAR: u16 = 0x082c;
const REG_OFDMCCKEN_JAGUAR: u16 = 0x0808;
const REG_TX_PATH_JAGUAR: u16 = 0x080c;
const REG_PWED_TH_JAGUAR: u16 = 0x0830;
const REG_BW_INDICATION_JAGUAR: u16 = 0x0834;
const REG_CCA_ON_SEC_JAGUAR: u16 = 0x0838;
const REG_L1_PEAK_TH_JAGUAR: u16 = 0x0848;
const REG_FC_AREA_JAGUAR: u16 = 0x0860;
const REG_HSSI_READ_JAGUAR: u16 = 0x08b0;
const REG_RF_MOD_JAGUAR: u16 = 0x08ac;
const REG_ADC_BUF_CLK_JAGUAR: u16 = 0x08c4;
const REG_IQK_MACBB_0X0520: u16 = 0x0520;
const REG_IQK_MACBB_0X090C: u16 = 0x090c;
const REG_ANTSEL_SW_JAGUAR: u16 = 0x0900;
const REG_SINGLE_TONE_CONT_TX_JAGUAR: u16 = 0x0914;
const REG_IQK_TRIGGER_980: u16 = 0x0980;
const REG_CCK_SYSTEM_JAGUAR: u16 = 0x0a00;
const REG_CCK_RX_JAGUAR: u16 = 0x0a04;
const REG_CCK_RX_PATH_JAGUAR: u16 = 0x0a07;
const REG_RF_PI_MODE_A_JAGUAR: u16 = 0x0c00;
const REG_IQK_RX_IQC_A_JAGUAR: u16 = 0x0c10;
const REG_OFDM0_XBAGCCORE1: u16 = 0x0c58;
const REG_IQK_AFE_A_C5C: u16 = 0x0c5c;
const REG_IQK_AFE_A_C60: u16 = 0x0c60;
const REG_IQK_AFE_A_C64: u16 = 0x0c64;
const REG_IQK_AFE_A_C68: u16 = 0x0c68;
const REG_IQK_TX_TONE_A_C80: u16 = 0x0c80;
const REG_IQK_RX_TONE_A_C84: u16 = 0x0c84;
const REG_IQK_RFE_SETTING_A_C88: u16 = 0x0c88;
const REG_IQK_RFE_SETTING_A_C8C: u16 = 0x0c8c;
const REG_IQK_RESULT_A_D00: u16 = 0x0d00;
const REG_RF_PI_READ_A_JAGUAR: u16 = 0x0d04;
const REG_RF_SI_READ_A_JAGUAR: u16 = 0x0d08;
const REG_IQK_RESULT_B_D40: u16 = 0x0d40;
const REG_RF_PATH_A_3WIRE: u16 = 0x0c90;
const REG_TX_BB_CTRL_A_JAGUAR: u16 = REG_RF_PATH_A_3WIRE;
const REG_IQK_TX_POWER_CTRL_A_C94: u16 = 0x0c94;
const REG_TX_SCALE_A_JAGUAR: u16 = 0x0c1c;
const REG_TX_AGC_A_CCK_JAGUAR: u16 = 0x0c20;
const REG_TX_AGC_A_OFDM18_OFDM6_JAGUAR: u16 = 0x0c24;
const REG_TX_AGC_A_OFDM54_OFDM24_JAGUAR: u16 = 0x0c28;
const REG_TX_AGC_A_MCS3_MCS0_JAGUAR: u16 = 0x0c2c;
const REG_TX_AGC_A_MCS7_MCS4_JAGUAR: u16 = 0x0c30;
const REG_TX_AGC_A_NSS1_7_NSS1_4_JAGUAR: u16 = 0x0c34;
const REG_TX_AGC_A_NSS1_11_NSS1_8_JAGUAR: u16 = 0x0c38;
const REG_TX_AGC_A_NSS1_3_NSS1_0_JAGUAR: u16 = 0x0c3c;
const REG_TX_AGC_A_NSS2_3_NSS2_0_JAGUAR: u16 = 0x0c40;
const REG_TX_AGC_A_NSS2_7_NSS2_4_JAGUAR: u16 = 0x0c44;
const REG_TX_AGC_A_NSS2_11_NSS2_8_JAGUAR: u16 = 0x0c48;
const REG_TX_AGC_A_NSS3_3_NSS3_0_JAGUAR: u16 = 0x0c4c;
const REG_TX_PWR_OFFSET_A_JAGUAR: u16 = 0x0c50;
const REG_TX_PWR_TRAIN_A_JAGUAR: u16 = 0x0c54;
const REG_RFE_PINMUX_A_JAGUAR: u16 = 0x0cb0;
const REG_RFE_INV_A_JAGUAR: u16 = 0x0cb4;
const REG_RFE_TIMING_A_JAGUAR: u16 = 0x0cb8;
const REG_IQK_TX_CTRL_A_CC4: u16 = 0x0cc4;
const REG_IQK_TX_CTRL_A_CC8: u16 = 0x0cc8;
const REG_IQK_TX_Y_A_CCC: u16 = 0x0ccc;
const REG_IQK_TX_X_A_CD4: u16 = 0x0cd4;
const REG_IQK_VDF_A_CE8: u16 = 0x0ce8;
const REG_RF_PI_MODE_B_JAGUAR: u16 = 0x0e00;
const REG_IQK_RX_IQC_B_JAGUAR: u16 = 0x0e10;
const REG_FPGA0_IQK_JAGUAR: u16 = 0x0e28;
const REG_TX_IQK_TONE_A_JAGUAR: u16 = 0x0e30;
const REG_RX_IQK_TONE_A_JAGUAR: u16 = 0x0e34;
const REG_TX_IQK_PI_A_JAGUAR: u16 = 0x0e38;
const REG_RX_IQK_PI_A_JAGUAR: u16 = 0x0e3c;
const REG_TX_IQK_JAGUAR: u16 = 0x0e40;
const REG_RX_IQK_JAGUAR: u16 = 0x0e44;
const REG_IQK_AGC_PTS_JAGUAR: u16 = 0x0e48;
const REG_IQK_AGC_RSP_JAGUAR: u16 = 0x0e4c;
const REG_TX_IQK_TONE_B_JAGUAR: u16 = 0x0e50;
const REG_RX_IQK_TONE_B_JAGUAR: u16 = 0x0e54;
const REG_TX_IQK_PI_B_JAGUAR: u16 = 0x0e58;
const REG_RX_IQK_PI_B_JAGUAR: u16 = REG_IQK_AFE_B_E5C;
const REG_IQK_AGC_CONT_JAGUAR: u16 = REG_IQK_AFE_B_E60;
const REG_TX_AGC_B_CCK_JAGUAR: u16 = 0x0e20;
const REG_TX_AGC_B_OFDM18_OFDM6_JAGUAR: u16 = 0x0e24;
const REG_TX_AGC_B_OFDM54_OFDM24_JAGUAR: u16 = 0x0e28;
const REG_TX_AGC_B_MCS3_MCS0_JAGUAR: u16 = 0x0e2c;
const REG_TX_AGC_B_MCS7_MCS4_JAGUAR: u16 = 0x0e30;
const REG_TX_AGC_B_NSS1_7_NSS1_4_JAGUAR: u16 = 0x0e34;
const REG_TX_AGC_B_NSS1_11_NSS1_8_JAGUAR: u16 = 0x0e38;
const REG_TX_AGC_B_NSS1_3_NSS1_0_JAGUAR: u16 = 0x0e3c;
const REG_TX_AGC_B_NSS2_3_NSS2_0_JAGUAR: u16 = 0x0e40;
const REG_TX_AGC_B_NSS2_7_NSS2_4_JAGUAR: u16 = 0x0e44;
const REG_TX_AGC_B_NSS2_11_NSS2_8_JAGUAR: u16 = 0x0e48;
const REG_TX_AGC_B_NSS3_3_NSS3_0_JAGUAR: u16 = 0x0e4c;
const REG_TX_PWR_OFFSET_B_JAGUAR: u16 = 0x0e50;
const REG_TX_PWR_TRAIN_B_JAGUAR: u16 = 0x0e54;
const REG_IQK_AFE_B_E5C: u16 = 0x0e5c;
const REG_IQK_AFE_B_E60: u16 = 0x0e60;
const REG_IQK_AFE_B_E64: u16 = 0x0e64;
const REG_IQK_AFE_B_E68: u16 = 0x0e68;
const REG_IQK_TX_TONE_B_E80: u16 = 0x0e80;
const REG_IQK_RX_TONE_B_E84: u16 = 0x0e84;
const REG_IQK_RFE_SETTING_B_E88: u16 = 0x0e88;
const REG_IQK_RFE_SETTING_B_E8C: u16 = 0x0e8c;
const REG_RF_PATH_B_3WIRE: u16 = 0x0e90;
const REG_TX_BB_CTRL_B_JAGUAR: u16 = REG_RF_PATH_B_3WIRE;
const REG_TX_POWER_BEFORE_IQK_A_JAGUAR: u16 = 0x0e94;
const REG_TX_POWER_AFTER_IQK_A_JAGUAR: u16 = 0x0e9c;
const REG_RX_POWER_BEFORE_IQK_A_JAGUAR: u16 = 0x0ea0;
const REG_RX_POWER_BEFORE_IQK_A_2_JAGUAR: u16 = 0x0ea4;
const REG_RX_POWER_AFTER_IQK_A_JAGUAR: u16 = 0x0ea8;
const REG_RX_POWER_AFTER_IQK_A_2_JAGUAR: u16 = 0x0eac;
const REG_TX_SCALE_B_JAGUAR: u16 = 0x0e1c;
const REG_RFE_PINMUX_B_JAGUAR: u16 = 0x0eb0;
const REG_RFE_INV_B_JAGUAR: u16 = 0x0eb4;
const REG_TX_POWER_BEFORE_IQK_B_JAGUAR: u16 = REG_RFE_INV_B_JAGUAR;
const REG_RFE_TIMING_B_JAGUAR: u16 = 0x0eb8;
const REG_TX_POWER_AFTER_IQK_B_JAGUAR: u16 = 0x0ebc;
const REG_RX_POWER_BEFORE_IQK_B_JAGUAR: u16 = 0x0ec0;
const REG_RF_PI_READ_B_JAGUAR: u16 = 0x0d44;
const REG_RF_SI_READ_B_JAGUAR: u16 = 0x0d48;
const REG_IQK_TX_CTRL_B_EC4: u16 = 0x0ec4;
const REG_RX_POWER_BEFORE_IQK_B_2_JAGUAR: u16 = REG_IQK_TX_CTRL_B_EC4;
const REG_IQK_TX_CTRL_B_EC8: u16 = 0x0ec8;
const REG_RX_POWER_AFTER_IQK_B_JAGUAR: u16 = REG_IQK_TX_CTRL_B_EC8;
const REG_IQK_TX_Y_B_ECC: u16 = 0x0ecc;
const REG_RX_POWER_AFTER_IQK_B_2_JAGUAR: u16 = REG_IQK_TX_Y_B_ECC;
const REG_IQK_TX_X_B_ED4: u16 = 0x0ed4;
const REG_IQK_VDF_B_EE8: u16 = 0x0ee8;
const REG_USB_HRPWM: u16 = 0xfe58;

const RTL8812AU_EFUSE_REAL_CONTENT_LEN: usize = 512;
const RTL8812AU_EFUSE_LOGICAL_MAP_LEN: usize = 512;
pub const RTL8812AU_EFUSE_TX_POWER_START: usize = 0x10;
pub const RTL8812AU_EFUSE_TX_POWER_LEN: usize = 84;
pub const RTL8812AU_TX_POWER_INDEX_MAX: u8 = 0x3f;
const RTL8812AU_EFUSE_MAX_SECTION: u8 = 64;
const RTL8812AU_EFUSE_MAC_OFFSET: usize = 0x0d7;
const EFUSE_ACCESS_ON_JAGUAR: u8 = 0x69;
const EFUSE_ACCESS_OFF_JAGUAR: u8 = 0x00;

const FW_START_ADDRESS: u16 = 0x1000;
const BIT0: u8 = 1 << 0;
const BIT1: u8 = 1 << 1;
const BIT2: u8 = 1 << 2;
const BIT3: u8 = 1 << 3;
const BIT4: u8 = 1 << 4;
const BIT5: u8 = 1 << 5;
const BIT6: u8 = 1 << 6;
const BIT7: u8 = 1 << 7;
const FEN_ELDR: u16 = 1 << 12;
const ANA8M: u16 = 1 << 1;
const LOADER_CLK_EN: u16 = 1 << 5;
const FEN_BBRSTB: u8 = BIT0;
const FEN_BB_GLB_RSTN: u8 = BIT1;
const FEN_USBA: u8 = BIT2;
const MSR_PORT0_NETTYPE_MASK: u8 = 0x03;
const MAX_DLFW_PAGE_SIZE: usize = 4096;
const MAX_REG_BLOCK_SIZE: usize = 196;
const FIRMWARE_REMAINDER_BLOCK_SIZE: usize = 8;
const TX_PAGE_BOUNDARY_8812: u8 = 0xf7;
const LAST_ENTRY_OF_TX_PKT_BUFFER_8812: u8 = 0xff;
const TX_TOTAL_PAGE_NUMBER_8812: u8 = TX_PAGE_BOUNDARY_8812 - 1;
const RX_DMA_BOUNDARY_8812: u16 = 0x3e7f;
const NORMAL_PAGE_NUM_HPQ_8812: u8 = 0x10;
const NORMAL_PAGE_NUM_LPQ_8812: u8 = 0x10;
const NORMAL_PAGE_NUM_NPQ_8812: u8 = 0x00;
const PBP_512: u8 = 0x03;
const PSTX_PBP_512: u8 = PBP_512 << 4;
const QUEUE_EXTRA: u16 = 0;
const QUEUE_LOW: u16 = 1;
const QUEUE_NORMAL: u16 = 2;
const QUEUE_HIGH: u16 = 3;
const LD_RQPN: u32 = 1 << 31;
const RQPN_PAGE_MASK: u32 = 0x00ff_ffff;
const TXDMA_QUEUE_MAP_MASK: u16 = 0xfff8;
const DRVINFO_SZ: u8 = 4;
const MASK_NETTYPE: u32 = 0x0003_0000;
const NT_LINK_AP: u32 = 0x3;
const NETTYPE_LINK_AP: u32 = NT_LINK_AP << 16;
const RCR_APM: u32 = 1 << 1;
const RCR_AM: u32 = 1 << 2;
const RCR_AB: u32 = 1 << 3;
const RCR_AAP: u32 = 1 << 0;
const RCR_APWRMGT: u32 = 1 << 5;
const RCR_CBSSID_DATA: u32 = 1 << 6;
const RCR_CBSSID_BCN: u32 = 1 << 7;
const RCR_ADF: u32 = 1 << 11;
const RCR_ACF: u32 = 1 << 12;
const RCR_AMF: u32 = 1 << 13;
const RCR_HTC_LOC_CTRL: u32 = 1 << 14;
const RCR_FORCEACK: u32 = 1 << 26;
const RCR_APP_PHYST_RXFF: u32 = 1 << 28;
const RCR_APP_ICV: u32 = 1 << 29;
const RCR_APP_MIC: u32 = 1 << 30;
const RCR_APPFCS: u32 = 1 << 31;
const MAC_RECEIVE_CONFIG: u32 = RCR_APM
    | RCR_AM
    | RCR_AB
    | RCR_CBSSID_DATA
    | RCR_CBSSID_BCN
    | RCR_AMF
    | RCR_HTC_LOC_CTRL
    | RCR_APP_PHYST_RXFF
    | RCR_APP_ICV
    | RCR_APP_MIC
    | RCR_FORCEACK;
const RATE_BITMAP_ALL: u32 = 0x000f_ffff;
const RATE_RRSR_CCK_ONLY_1M: u32 = 0x000f_fff1;
const RL_VAL_STA: u16 = 0x30;
const RETRY_LIMIT_STA: u16 = RL_VAL_STA | (RL_VAL_STA << 8);
const BASIC_RATE_2G: u16 = 0x015f;
const BASIC_RATE_5G: u16 = 0x0150;
const BAR_MODE_CTRL_VALUE: u32 = 0x0201_ffff;
const BAR_MODE_CTRL_READBACK_MASK: u32 = 0xffff_ff7f;
const EN_AMPDU_RTY_NEW: u8 = 1 << 7;
const DIS_TSF_UDT: u8 = BIT4;
const TBTT_PROHIBIT_SETUP_TIME: u8 = 0x04;
const TBTT_PROHIBIT_HOLD_TIME_STOP_BCN: u16 = 0x0064;
const DRIVER_EARLY_INT_TIME_8812: u8 = 0x05;
const BCN_DMA_ATIME_INT_TIME_8812: u8 = 0x02;
const MACTXEN: u8 = 1 << 6;
const MACRXEN: u8 = 1 << 7;
const MAC_TX_RX_ENABLE_MASK: u8 = MACTXEN | MACRXEN;
const RTL8812_CRYSTAL_CAP_MASK: u32 = 0x7ff8_0000;
const RF_CHNLBW_JAGUAR: u32 = 0x18;
const RF_CHNLBW_MOD_AG_MASK: u32 = 0x0007_0300;
const RF_CHNLBW_BW_MASK: u32 = 0x0000_0c00;
const RF_CHNLBW_CHANNEL_MASK: u32 = 0x0000_00ff;
const RF_LCK_JAGUAR: u32 = 0xb4;
const RF_IQK_LOK_READBACK_JAGUAR: u32 = 0x08;
const RF_IQK_TX_0X30_JAGUAR: u32 = 0x30;
const RF_IQK_TX_0X31_JAGUAR: u32 = 0x31;
const RF_IQK_TX_0X32_JAGUAR: u32 = 0x32;
const RF_IQK_LOK_LOAD_JAGUAR: u32 = 0x58;
const RF_IQK_MODE_JAGUAR: u32 = 0xef;
const RF_REGISTER_OFFSET_MASK: u32 = 0x000f_ffff;
const RF_LCK_MODE_BIT: u32 = 1 << 14;
const RF_CHNLBW_LCK_TRIGGER_BIT: u32 = 1 << 15;
const VHT_DATA_SC_20_UPPER_OF_80MHZ: u8 = 1;
const VHT_DATA_SC_20_LOWER_OF_80MHZ: u8 = 2;
const VHT_DATA_SC_20_UPPERST_OF_80MHZ: u8 = 3;
const VHT_DATA_SC_20_LOWEST_OF_80MHZ: u8 = 4;
const VHT_DATA_SC_40_UPPER_OF_80MHZ: u8 = 9;
const VHT_DATA_SC_40_LOWER_OF_80MHZ: u8 = 10;
const LLT_NO_ACTIVE: u32 = 0x0;
const LLT_WRITE_ACCESS: u32 = 0x1;
const LLT_OP_SHIFT: u32 = 30;
const LLT_OP_MASK: u32 = 0x3;
const MCUFWDL_EN: u8 = BIT0;
const MCUFWDL_RDY: u32 = BIT1 as u32;
const FWDL_CHKSUM_RPT_U8: u8 = BIT2;
const FWDL_CHKSUM_RPT_U32: u32 = BIT2 as u32;
const WINTINI_RDY: u32 = BIT6 as u32;
const RAM_DL_SEL: u8 = BIT7;
const CR_ENABLE_BITS: u16 =
    (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 9) | (1 << 10);
const RTL8812A_IQK_PAGE_C1_SELECT_BIT: u32 = 0x8000_0000;
const RTL8812A_IQK_MAX_ATTEMPTS: u8 = 10;
const RTL8812A_IQK_MAX_RX_CANDIDATES: usize = 5;
const RTL8812A_IQK_READY_POLL_LIMIT: u8 = 20;
const RTL8812A_IQK_MAX_SWEEPS: u8 = 3;
const RTL8812A_IQK_READY_MASK: u32 = 1 << 10;
const RTL8812A_IQK_RX_FAIL_MASK: u32 = 1 << 11;
const RTL8812A_IQK_TX_FAIL_MASK: u32 = 1 << 12;
const RTL8812A_IQK_RESULT_FIELD_MASK: u32 = 0x07ff_0000;
const MONITOR_RECEIVE_CONFIG: u32 = RCR_AAP
    | RCR_APM
    | RCR_AM
    | RCR_AB
    | RCR_APWRMGT
    | RCR_ADF
    | RCR_ACF
    | RCR_AMF
    | RCR_APP_PHYST_RXFF
    | RCR_APPFCS;

const RTL8812AU_TX_SCHEDULER_TAIL_U8_WRITES: &[(u16, u8, &str)] = &[
    (REG_FWHW_TXQ_CTRL + 1, 0x0f, "REG_FWHW_TXQ_CTRL+1"),
    (
        REG_EARLY_MODE_CONTROL_8812 + 3,
        0x01,
        "REG_EARLY_MODE_CONTROL_8812+3",
    ),
    (REG_SDIO_CTRL_8812, 0x00, "REG_SDIO_CTRL_8812"),
    (REG_ACLK_MON, 0x00, "REG_ACLK_MON"),
    (REG_USB_HRPWM, 0x00, "REG_USB_HRPWM"),
    (REG_NAV_UPPER, 0x00, "REG_NAV_UPPER"),
];

type Rtl8812auRegisterReadSpec = (&'static str, u16);

const RTL8812A_IQK_MACBB_BACKUP_REGISTERS: &[Rtl8812auRegisterReadSpec] = &[
    ("R_0x520", REG_IQK_MACBB_0X0520),
    ("REG_BCN_CTRL", REG_BCN_CTRL),
    ("REG_OFDMCCKEN_JAGUAR", REG_OFDMCCKEN_JAGUAR),
    ("REG_CCK_RX_JAGUAR", REG_CCK_RX_JAGUAR),
    ("R_0x90c", REG_IQK_MACBB_0X090C),
    ("R_0x978", 0x0978),
    ("R_0x97c", 0x097c),
    ("R_0x984", 0x0984),
    ("rA_PI_Mode_Jaguar", REG_RF_PI_MODE_A_JAGUAR),
    ("rB_PI_Mode_Jaguar", REG_RF_PI_MODE_B_JAGUAR),
    ("REG_CCA_ON_SEC_JAGUAR", REG_CCA_ON_SEC_JAGUAR),
    ("REG_AGC_TABLE_JAGUAR", REG_AGC_TABLE_JAGUAR),
    ("R_0xc94", REG_IQK_TX_POWER_CTRL_A_C94),
    ("R_0xe94", REG_TX_POWER_BEFORE_IQK_A_JAGUAR),
];

const RTL8812A_IQK_AFE_BACKUP_REGISTERS: &[Rtl8812auRegisterReadSpec] = &[
    ("R_0xc5c", REG_IQK_AFE_A_C5C),
    ("R_0xc60", REG_IQK_AFE_A_C60),
    ("R_0xc64", REG_IQK_AFE_A_C64),
    ("R_0xc68", REG_IQK_AFE_A_C68),
    ("rA_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_A_JAGUAR),
    ("rA_RFE_Inv_Jaguar", REG_RFE_INV_A_JAGUAR),
    ("R_0xe5c", REG_IQK_AFE_B_E5C),
    ("R_0xe60", REG_IQK_AFE_B_E60),
    ("R_0xe64", REG_IQK_AFE_B_E64),
    ("R_0xe68", REG_IQK_AFE_B_E68),
    ("rB_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_B_JAGUAR),
    ("rB_RFE_Inv_Jaguar", REG_RFE_INV_B_JAGUAR),
];

const RTL8812A_IQK_PAGE_C1_LATCH_REGISTERS: &[Rtl8812auRegisterReadSpec] = &[
    ("R_0xc80_page_c1", REG_IQK_TX_TONE_A_C80),
    ("R_0xc84_page_c1", REG_IQK_RX_TONE_A_C84),
    ("R_0xc88_page_c1", REG_IQK_RFE_SETTING_A_C88),
    ("R_0xc8c_page_c1", REG_IQK_RFE_SETTING_A_C8C),
    ("R_0xcb8_page_c1", REG_RFE_TIMING_A_JAGUAR),
    ("R_0xcc4_page_c1", REG_IQK_TX_CTRL_A_CC4),
    ("R_0xcc8_page_c1", REG_IQK_TX_CTRL_A_CC8),
    ("R_0xccc_page_c1", REG_IQK_TX_Y_A_CCC),
    ("R_0xcd4_page_c1", REG_IQK_TX_X_A_CD4),
    ("R_0xce8_page_c1", REG_IQK_VDF_A_CE8),
    ("R_0xe80_page_c1", REG_IQK_TX_TONE_B_E80),
    ("R_0xe84_page_c1", REG_IQK_RX_TONE_B_E84),
    ("R_0xe88_page_c1", REG_IQK_RFE_SETTING_B_E88),
    ("R_0xe8c_page_c1", REG_IQK_RFE_SETTING_B_E8C),
    ("R_0xeb8_page_c1", REG_RFE_TIMING_B_JAGUAR),
    ("R_0xec4_page_c1", REG_IQK_TX_CTRL_B_EC4),
    ("R_0xec8_page_c1", REG_IQK_TX_CTRL_B_EC8),
    ("R_0xecc_page_c1", REG_IQK_TX_Y_B_ECC),
    ("R_0xed4_page_c1", REG_IQK_TX_X_B_ED4),
    ("R_0xee8_page_c1", REG_IQK_VDF_B_EE8),
];

const RTL8812A_IQK_RESULT_REGISTERS: &[Rtl8812auRegisterReadSpec] = &[
    ("rA_IQK_Result_Jaguar", REG_OFDM0_XBAGCCORE1),
    ("rA_IQK_Shadow_Jaguar", REG_OFDM0_XBAGCCORE1 + 4),
    ("rA_RX_IQC_Latch_Jaguar", REG_IQK_RX_IQC_A_JAGUAR),
    ("rB_IQK_Result_Jaguar", REG_OFDM0_XBAGCCORE1 + 0x200),
    ("rB_IQK_Shadow_Jaguar", REG_OFDM0_XBAGCCORE1 + 0x204),
    ("rB_RX_IQC_Latch_Jaguar", REG_IQK_RX_IQC_B_JAGUAR),
    ("rFPGA0_IQK", REG_FPGA0_IQK_JAGUAR),
    ("rTx_IQK_Tone_A", REG_TX_IQK_TONE_A_JAGUAR),
    ("rRx_IQK_Tone_A", REG_RX_IQK_TONE_A_JAGUAR),
    ("rTx_IQK_PI_A", REG_TX_IQK_PI_A_JAGUAR),
    ("rRx_IQK_PI_A", REG_RX_IQK_PI_A_JAGUAR),
    ("rTx_IQK", REG_TX_IQK_JAGUAR),
    ("rRx_IQK", REG_RX_IQK_JAGUAR),
    ("rIQK_AGC_Pts", REG_IQK_AGC_PTS_JAGUAR),
    ("rIQK_AGC_Rsp", REG_IQK_AGC_RSP_JAGUAR),
    ("rTx_IQK_Tone_B", REG_TX_IQK_TONE_B_JAGUAR),
    ("rRx_IQK_Tone_B", REG_RX_IQK_TONE_B_JAGUAR),
    ("rTx_IQK_PI_B", REG_TX_IQK_PI_B_JAGUAR),
    ("rRx_IQK_PI_B", REG_RX_IQK_PI_B_JAGUAR),
    ("rIQK_AGC_Cont", REG_IQK_AGC_CONT_JAGUAR),
    ("rTx_Power_Before_IQK_A", REG_TX_POWER_BEFORE_IQK_A_JAGUAR),
    ("rTx_Power_After_IQK_A", REG_TX_POWER_AFTER_IQK_A_JAGUAR),
    ("rRx_Power_Before_IQK_A", REG_RX_POWER_BEFORE_IQK_A_JAGUAR),
    (
        "rRx_Power_Before_IQK_A_2",
        REG_RX_POWER_BEFORE_IQK_A_2_JAGUAR,
    ),
    ("rRx_Power_After_IQK_A", REG_RX_POWER_AFTER_IQK_A_JAGUAR),
    ("rRx_Power_After_IQK_A_2", REG_RX_POWER_AFTER_IQK_A_2_JAGUAR),
    ("rTx_Power_Before_IQK_B", REG_TX_POWER_BEFORE_IQK_B_JAGUAR),
    ("rTx_Power_After_IQK_B", REG_TX_POWER_AFTER_IQK_B_JAGUAR),
    ("rRx_Power_Before_IQK_B", REG_RX_POWER_BEFORE_IQK_B_JAGUAR),
    (
        "rRx_Power_Before_IQK_B_2",
        REG_RX_POWER_BEFORE_IQK_B_2_JAGUAR,
    ),
    ("rRx_Power_After_IQK_B", REG_RX_POWER_AFTER_IQK_B_JAGUAR),
    ("rRx_Power_After_IQK_B_2", REG_RX_POWER_AFTER_IQK_B_2_JAGUAR),
];

const RTL8812A_IQK_RF_BACKUP_OFFSETS: &[u32] = &[
    0x65,
    0x8f,
    0x00,
    RF_IQK_MODE_JAGUAR,
    RF_IQK_TX_0X30_JAGUAR,
    RF_IQK_TX_0X31_JAGUAR,
    RF_IQK_TX_0X32_JAGUAR,
    RF_IQK_LOK_LOAD_JAGUAR,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimePhaseExecution {
    pub phase: Rtl8812auInitPhase,
    pub register_writes: usize,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rtl8812auRfPath {
    A,
    B,
    Both,
}

impl Rtl8812auRfPath {
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::A => Some("A"),
            Self::B => Some("B"),
            Self::Both => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rtl8812auRegisterWriteSpec {
    pub register_name: &'static str,
    pub address: u16,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auRuntimeIqkIqcValue {
    pub x: u32,
    pub x_hex: String,
    pub y: u32,
    pub y_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auRuntimeIqkRawCandidateReport {
    pub x_raw: u32,
    pub x_raw_hex: String,
    pub y_raw: u32,
    pub y_raw_hex: String,
    pub x_field: u32,
    pub x_field_hex: String,
    pub y_field: u32,
    pub y_field_hex: String,
    pub x_signed: i32,
    pub y_signed: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auRuntimeIqkMaskedBbWritePlan {
    pub register_name: &'static str,
    pub address: u16,
    pub address_hex: String,
    pub mask: u32,
    pub mask_hex: String,
    pub data: u32,
    pub data_hex: String,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auRuntimeIqkAttemptReport {
    pub attempt_index: u8,
    pub ready: Option<bool>,
    pub failed: Option<bool>,
    pub delay_count: Option<u8>,
    pub status_raw: Option<u32>,
    pub status_raw_hex: Option<String>,
    pub raw_candidate: Option<Rtl8812auRuntimeIqkRawCandidateReport>,
    pub candidate: Option<Rtl8812auRuntimeIqkIqcValue>,
    pub label: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auRuntimeIqkStageReport {
    pub stage: &'static str,
    pub status: &'static str,
    pub ready: Option<bool>,
    pub failed: Option<bool>,
    pub retry_count: u8,
    pub average_count: u8,
    pub delay_count_max: Option<u8>,
    pub attempts: Vec<Rtl8812auRuntimeIqkAttemptReport>,
    pub candidates: Vec<Rtl8812auRuntimeIqkIqcValue>,
    pub selected_iqc: Option<Rtl8812auRuntimeIqkIqcValue>,
    pub fallback_used: bool,
    pub fallback_iqc: Option<Rtl8812auRuntimeIqkIqcValue>,
    pub failure_label: Option<&'static str>,
    pub fill_plan: Vec<Rtl8812auRuntimeIqkMaskedBbWritePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auRuntimeIqkPathReport {
    pub path: Rtl8812auRfPath,
    pub path_name: &'static str,
    pub tx: Rtl8812auRuntimeIqkStageReport,
    pub rx: Rtl8812auRuntimeIqkStageReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auRuntimeIqkSweepPathSummaryReport {
    pub path_name: &'static str,
    pub tx_status: &'static str,
    pub tx_retry_count: u8,
    pub tx_average_count: u8,
    pub tx_fallback_used: bool,
    pub tx_failure_label: Option<&'static str>,
    pub rx_status: &'static str,
    pub rx_retry_count: u8,
    pub rx_average_count: u8,
    pub rx_fallback_used: bool,
    pub rx_failure_label: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auRuntimeIqkSweepSummaryReport {
    pub sweep_index: u8,
    pub status: &'static str,
    pub cleanup_status: &'static str,
    pub fallback_stage_count: usize,
    pub path_statuses: Vec<Rtl8812auRuntimeIqkSweepPathSummaryReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auRuntimeIqkBackupReport {
    pub hssi_read_register: Rtl8812auRegisterReadReport,
    pub page_select_register: Rtl8812auRegisterReadReport,
    pub tx_pause_register: Rtl8812auRegisterReadReport,
    pub macbb_backup: Vec<Rtl8812auRegisterReadReport>,
    pub afe_backup: Vec<Rtl8812auRegisterReadReport>,
    pub rf_backup_path_a: Vec<Rtl8812auRfSerialReadReport>,
    pub rf_backup_path_b: Vec<Rtl8812auRfSerialReadReport>,
    pub page_c1_latches: Vec<Rtl8812auRegisterReadReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auRuntimeIqkCleanupReport {
    pub status: &'static str,
    pub failures: Vec<String>,
    pub macbb_restore_count: usize,
    pub afe_restore_count: usize,
    pub rf_path_a_restore_count: usize,
    pub rf_path_b_restore_count: usize,
    pub page_c1_latch_restore_count: usize,
    pub hssi_read_restored: Option<bool>,
    pub page_select_restored: Option<bool>,
    pub tx_pause_restored: Option<bool>,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auRuntimeIqkPreSweepStateReport {
    pub bb_agc_state_pre_iqk: Rtl8812auRegisterReadReport,
    pub ofdmccken_pre_iqk: Option<Rtl8812auRegisterReadReport>,
    pub cca_on_sec_pre_iqk: Option<Rtl8812auRegisterReadReport>,
    pub rf_mode_path_a_pre_iqk: Option<Rtl8812auRfSerialReadReport>,
    pub rf_mode_path_b_pre_iqk: Option<Rtl8812auRfSerialReadReport>,
    pub txagc_registers_pre_iqk: Vec<Rtl8812auRegisterReadReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auRuntimeIqkCalibrationReport {
    pub semantics: &'static str,
    pub upstream_basis: &'static str,
    pub mode: &'static str,
    pub sweep_index: u8,
    pub sweep_count: u8,
    pub max_sweeps: u8,
    pub sweep_summaries: Vec<Rtl8812auRuntimeIqkSweepSummaryReport>,
    pub status: &'static str,
    pub cleanup_status: &'static str,
    pub cleanup_failures: Vec<String>,
    pub backup: Option<Rtl8812auRuntimeIqkBackupReport>,
    pub pre_sweep_state: Option<Rtl8812auRuntimeIqkPreSweepStateReport>,
    pub cleanup: Option<Rtl8812auRuntimeIqkCleanupReport>,
    pub selected_iqc_fill_applied: bool,
    pub selected_iqc_fill_register_count: usize,
    pub paths: Vec<Rtl8812auRuntimeIqkPathReport>,
    pub affected_registers: Vec<Rtl8812auRegisterReadReport>,
    pub before_iqk_registers: Vec<Rtl8812auRegisterReadReport>,
    pub after_iqk_registers: Vec<Rtl8812auRegisterReadReport>,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auTxCalibrationProfileReport {
    pub semantics: &'static str,
    pub upstream_basis: &'static str,
    pub profile: TxCalibrationProfile,
    pub channel: u8,
    pub bandwidth_mhz: u16,
    pub register_count: usize,
    pub writes: Vec<Rtl8812auRegisterWriteReport>,
    pub lck: Option<Rtl8812auLckCalibrationReport>,
    pub runtime_iqk: Option<Rtl8812auRuntimeIqkCalibrationReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Rtl8812auRuntimeIqkSetupWritePlan {
    Register {
        phase: &'static str,
        register_name: &'static str,
        address: u16,
        address_hex: String,
        width: &'static str,
        value: u32,
        value_hex: String,
        reason: &'static str,
    },
    MaskedBb {
        phase: &'static str,
        write: Rtl8812auRuntimeIqkMaskedBbWritePlan,
    },
    Rf {
        phase: &'static str,
        path: Rtl8812auRfPath,
        path_name: &'static str,
        rf_offset: u32,
        rf_offset_hex: String,
        value: u32,
        value_hex: String,
        reason: &'static str,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auRegisterReadReport {
    pub register_name: &'static str,
    pub address: u16,
    pub address_hex: String,
    pub width: &'static str,
    pub value: u32,
    pub value_hex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auRegisterWriteReport {
    pub register_name: &'static str,
    pub address: u16,
    pub address_hex: String,
    pub width: &'static str,
    pub before: u32,
    pub before_hex: String,
    pub written: u32,
    pub written_hex: String,
    pub after: u32,
    pub after_hex: String,
    pub changed: bool,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auRfSerialWriteReport {
    pub register_name: &'static str,
    pub path: Rtl8812auRfPath,
    pub path_name: &'static str,
    pub bb_register_name: &'static str,
    pub bb_register: u16,
    pub bb_register_hex: String,
    pub rf_offset: u32,
    pub rf_offset_hex: String,
    pub value: u32,
    pub value_hex: String,
    pub encoded: u32,
    pub encoded_hex: String,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auRfSerialReadReport {
    pub register_name: &'static str,
    pub path: Rtl8812auRfPath,
    pub path_name: &'static str,
    pub rf_offset: u32,
    pub rf_offset_hex: String,
    pub hssi_register_name: &'static str,
    pub hssi_register: u16,
    pub hssi_register_hex: String,
    pub hssi_mask_hex: String,
    pub pi_mode_register_name: &'static str,
    pub pi_mode_register: u16,
    pub pi_mode_register_hex: String,
    pub pi_mode_value: u32,
    pub pi_mode_value_hex: String,
    pub pi_mode: bool,
    pub readback_register_name: &'static str,
    pub readback_register: u16,
    pub readback_register_hex: String,
    pub readback_mask_hex: String,
    pub value: u32,
    pub value_hex: String,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auLckCalibrationReport {
    pub semantics: &'static str,
    pub upstream_basis: &'static str,
    pub rf_path: Rtl8812auRfPath,
    pub rf_path_name: &'static str,
    pub continuous_tx_register: Rtl8812auRegisterReadReport,
    pub continuous_tx_active: bool,
    pub tx_pause_before: Rtl8812auRegisterReadReport,
    pub tx_pause_write: Option<Rtl8812auRegisterWriteReport>,
    pub tx_pause_restore: Option<Rtl8812auRegisterWriteReport>,
    pub rf_chnlbw_backup: Rtl8812auRfSerialReadReport,
    pub rf_lck_before_enter: Rtl8812auRfSerialReadReport,
    pub rf_lck_enter_write: Rtl8812auRfSerialWriteReport,
    pub rf_chnlbw_before_trigger: Rtl8812auRfSerialReadReport,
    pub rf_chnlbw_trigger_write: Rtl8812auRfSerialWriteReport,
    pub delay_ms: u64,
    pub rf_lck_before_exit: Rtl8812auRfSerialReadReport,
    pub rf_lck_exit_write: Rtl8812auRfSerialWriteReport,
    pub rf_chnlbw_restore_write: Rtl8812auRfSerialWriteReport,
    pub rf_chnlbw_after_restore: Rtl8812auRfSerialReadReport,
    pub rf_lck_after_exit: Rtl8812auRfSerialReadReport,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeMonitorOpmodeExecution {
    pub msr_before: u8,
    pub msr_written: u8,
    pub msr_after: u8,
    pub rcr_written: u32,
    pub rcr_after: u32,
    pub rxfltmap2_written: u16,
    pub rxfltmap2_after: u16,
    pub register_writes: usize,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeEfuseReadConfig {
    pub length: usize,
    pub poll_attempts: u32,
    pub poll_delay: Duration,
}

impl Default for RuntimeEfuseReadConfig {
    fn default() -> Self {
        Self {
            length: RTL8812AU_EFUSE_REAL_CONTENT_LEN,
            poll_attempts: 1000,
            poll_delay: Duration::from_micros(1000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeMacAddressExecution {
    pub before: [u8; 6],
    pub written: [u8; 6],
    pub after: [u8; 6],
    pub register_writes: usize,
    pub counters: RuntimeRadioCounters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MonitorReceiveFilterExecution {
    rcr_written: u32,
    rcr_after: u32,
    rxfltmap2_written: u16,
    rxfltmap2_after: u16,
    register_writes: usize,
}

fn read8_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    register_name: &'static str,
    phase: &'static str,
) -> Result<u8, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let value = registers
        .read8(address)
        .map_err(|error| RuntimeRadioError::register_read(register_name, phase, error))?;
    counters.usb_control_reads += 1;
    Ok(value)
}

fn read16_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    register_name: &'static str,
    phase: &'static str,
) -> Result<u16, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let value = registers
        .read16(address)
        .map_err(|error| RuntimeRadioError::register_read(register_name, phase, error))?;
    counters.usb_control_reads += 1;
    Ok(value)
}

fn read32_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    register_name: &'static str,
    phase: &'static str,
) -> Result<u32, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let value = registers
        .read32(address)
        .map_err(|error| RuntimeRadioError::register_read(register_name, phase, error))?;
    counters.usb_control_reads += 1;
    Ok(value)
}

fn write8_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    value: u8,
    register_name: &'static str,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    registers
        .write8(address, value)
        .map_err(|error| RuntimeRadioError::register_write(register_name, phase, error))?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn write16_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    value: u16,
    register_name: &'static str,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    registers
        .write16(address, value)
        .map_err(|error| RuntimeRadioError::register_write(register_name, phase, error))?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn write32_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    value: u32,
    register_name: &'static str,
    phase: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    registers
        .write32(address, value)
        .map_err(|error| RuntimeRadioError::register_write(register_name, phase, error))?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn read8_with_custom_error<T, F>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    error: F,
) -> Result<u8, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
    F: FnOnce(Rtl8812auRegisterError) -> RuntimeRadioError,
{
    let value = registers.read8(address).map_err(error)?;
    counters.usb_control_reads += 1;
    Ok(value)
}

fn read16_with_custom_error<T, F>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    error: F,
) -> Result<u16, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
    F: FnOnce(Rtl8812auRegisterError) -> RuntimeRadioError,
{
    let value = registers.read16(address).map_err(error)?;
    counters.usb_control_reads += 1;
    Ok(value)
}

fn write8_with_custom_error<T, F>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    value: u8,
    error: F,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
    F: FnOnce(Rtl8812auRegisterError) -> RuntimeRadioError,
{
    registers.write8(address, value).map_err(error)?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn write16_with_custom_error<T, F>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    value: u16,
    error: F,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
    F: FnOnce(Rtl8812auRegisterError) -> RuntimeRadioError,
{
    registers.write16(address, value).map_err(error)?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn format_register_address(address: u16) -> String {
    format!("0x{address:04x}")
}

fn format_register_value<T>(value: T, digits: usize) -> String
where
    T: Into<u64>,
{
    format!("0x{:0width$x}", value.into(), width = digits)
}

fn register_read_report(
    register_name: &'static str,
    address: u16,
    width: &'static str,
    value: u32,
    digits: usize,
) -> Rtl8812auRegisterReadReport {
    Rtl8812auRegisterReadReport {
        register_name,
        address,
        address_hex: format_register_address(address),
        width,
        value,
        value_hex: format_register_value(value, digits),
    }
}

pub fn rtl8812au_runtime_iqk_iqc_value(x: u32, y: u32) -> Rtl8812auRuntimeIqkIqcValue {
    let x = x & 0x0000_07ff;
    let y = y & 0x0000_07ff;
    Rtl8812auRuntimeIqkIqcValue {
        x,
        x_hex: format_register_value(x, 3),
        y,
        y_hex: format_register_value(y, 3),
    }
}

fn rtl8812au_iqk_component_to_signed(value: u32) -> i32 {
    let value = (value & 0x0000_07ff) as i32;
    if value & 0x0000_0400 != 0 {
        value - 0x0000_0800
    } else {
        value
    }
}

fn rtl8812au_iqk_signed_to_component(value: i32) -> u32 {
    (value & 0x0000_07ff) as u32
}

fn rtl8812au_runtime_iqk_raw_candidate_report(
    x_raw: u32,
    y_raw: u32,
) -> Rtl8812auRuntimeIqkRawCandidateReport {
    let x_field = bb_masked_field(x_raw, RTL8812A_IQK_RESULT_FIELD_MASK);
    let y_field = bb_masked_field(y_raw, RTL8812A_IQK_RESULT_FIELD_MASK);
    Rtl8812auRuntimeIqkRawCandidateReport {
        x_raw,
        x_raw_hex: format_register_value(x_raw, 8),
        y_raw,
        y_raw_hex: format_register_value(y_raw, 8),
        x_field,
        x_field_hex: format_register_value(x_field, 3),
        y_field,
        y_field_hex: format_register_value(y_field, 3),
        x_signed: rtl8812au_iqk_component_to_signed(x_field),
        y_signed: rtl8812au_iqk_component_to_signed(y_field),
    }
}

pub fn rtl8812au_iqk_select_candidate(
    candidates: &[Rtl8812auRuntimeIqkIqcValue],
) -> Option<Rtl8812auRuntimeIqkIqcValue> {
    for (index, left) in candidates.iter().enumerate() {
        for right in candidates.iter().skip(index + 1) {
            let left_x = rtl8812au_iqk_component_to_signed(left.x);
            let right_x = rtl8812au_iqk_component_to_signed(right.x);
            let left_y = rtl8812au_iqk_component_to_signed(left.y);
            let right_y = rtl8812au_iqk_component_to_signed(right.y);
            let dx = left_x - right_x;
            let dy = left_y - right_y;
            if dx.abs() < 4 && dy.abs() < 4 {
                return Some(rtl8812au_runtime_iqk_iqc_value(
                    rtl8812au_iqk_signed_to_component((left_x + right_x) / 2),
                    rtl8812au_iqk_signed_to_component((left_y + right_y) / 2),
                ));
            }
        }
    }
    None
}

#[derive(Debug, Clone, Default)]
pub struct Rtl8812auRuntimeIqkOneShotPathState {
    attempts: Vec<Rtl8812auRuntimeIqkAttemptReport>,
    candidates: Vec<Rtl8812auRuntimeIqkIqcValue>,
    selected_iqc: Option<Rtl8812auRuntimeIqkIqcValue>,
    retry_count: u8,
    delay_count_max: Option<u8>,
    ready: Option<bool>,
    failed: Option<bool>,
    failure_label: Option<&'static str>,
    finished: bool,
}

impl Rtl8812auRuntimeIqkOneShotPathState {
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn ready(&self) -> Option<bool> {
        self.ready
    }

    pub fn set_ready(&mut self, ready: bool) {
        self.ready = Some(ready);
    }

    pub fn failed(&self) -> Option<bool> {
        self.failed
    }

    pub fn set_failed(&mut self, failed: bool) {
        self.failed = Some(failed);
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    pub fn attempts(&self) -> u8 {
        self.retry_count
            .saturating_add(u8::try_from(self.candidates.len()).unwrap_or(u8::MAX))
    }

    pub fn note_delay_count(&mut self, delay_count: u8) {
        self.delay_count_max = Some(self.delay_count_max.unwrap_or(0).max(delay_count));
    }

    pub fn push_candidate(&mut self, candidate: Rtl8812auRuntimeIqkIqcValue) {
        self.candidates.push(candidate);
        if let Some(selected) = rtl8812au_iqk_select_candidate(&self.candidates) {
            self.selected_iqc = Some(selected);
            self.finished = true;
            self.failure_label = None;
        }
    }

    pub fn push_attempt(
        &mut self,
        ready: Option<bool>,
        failed: Option<bool>,
        delay_count: Option<u8>,
        status_raw: Option<u32>,
        raw_candidate: Option<Rtl8812auRuntimeIqkRawCandidateReport>,
        candidate: Option<Rtl8812auRuntimeIqkIqcValue>,
        label: Option<&'static str>,
    ) {
        let attempt_index = u8::try_from(self.attempts.len() + 1).unwrap_or(u8::MAX);
        self.attempts.push(Rtl8812auRuntimeIqkAttemptReport {
            attempt_index,
            ready,
            failed,
            delay_count,
            status_raw,
            status_raw_hex: status_raw.map(|value| format_register_value(value, 8)),
            raw_candidate,
            candidate,
            label,
        });
    }

    pub fn note_retry(&mut self, label: &'static str) {
        self.retry_count = self.retry_count.saturating_add(1);
        if !self.finished {
            self.failure_label = Some(label);
        }
    }

    pub fn into_stage_report(
        self,
        stage: &'static str,
        fallback_iqc: Rtl8812auRuntimeIqkIqcValue,
        fill_plan: Vec<Rtl8812auRuntimeIqkMaskedBbWritePlan>,
    ) -> Rtl8812auRuntimeIqkStageReport {
        let (status, selected_iqc, fallback_used, fallback_iqc_report, failure_label) =
            if self.finished {
                ("success", self.selected_iqc, false, None, None)
            } else {
                (
                    "failed",
                    Some(fallback_iqc.clone()),
                    true,
                    Some(fallback_iqc),
                    Some(
                        self.failure_label
                            .unwrap_or("iqk_candidate_selection_failed"),
                    ),
                )
            };
        Rtl8812auRuntimeIqkStageReport {
            stage,
            status,
            ready: self.ready,
            failed: self.failed,
            retry_count: self.retry_count,
            average_count: u8::try_from(self.candidates.len()).unwrap_or(u8::MAX),
            delay_count_max: self.delay_count_max,
            attempts: self.attempts,
            candidates: self.candidates,
            selected_iqc,
            fallback_used,
            fallback_iqc: fallback_iqc_report,
            failure_label,
            fill_plan,
        }
    }
}

pub fn rtl8812au_runtime_iqk_skipped_stage_report(
    stage: &'static str,
    label: &'static str,
    fill_plan: Vec<Rtl8812auRuntimeIqkMaskedBbWritePlan>,
) -> Rtl8812auRuntimeIqkStageReport {
    Rtl8812auRuntimeIqkStageReport {
        stage,
        status: "skipped",
        ready: None,
        failed: None,
        retry_count: 0,
        average_count: 0,
        delay_count_max: None,
        attempts: Vec::new(),
        candidates: Vec::new(),
        selected_iqc: Some(rtl8812au_runtime_iqk_iqc_value(0x200, 0)),
        fallback_used: true,
        fallback_iqc: Some(rtl8812au_runtime_iqk_iqc_value(0x200, 0)),
        failure_label: Some(label),
        fill_plan,
    }
}

pub fn rtl8812au_runtime_iqk_stage_success_iqc(
    stage: &Rtl8812auRuntimeIqkStageReport,
) -> Option<Rtl8812auRuntimeIqkIqcValue> {
    if stage.status == "success" && !stage.fallback_used {
        stage.selected_iqc.clone()
    } else {
        None
    }
}

pub fn rtl8812au_runtime_iqk_stage_iqc_or_fallback(
    stage: &Rtl8812auRuntimeIqkStageReport,
) -> Rtl8812auRuntimeIqkIqcValue {
    stage
        .selected_iqc
        .clone()
        .unwrap_or_else(|| rtl8812au_runtime_iqk_iqc_value(0x200, 0))
}

fn rtl8812au_runtime_iqk_path_can_fill(
    tx_stage: &Rtl8812auRuntimeIqkStageReport,
    rx_stage: &Rtl8812auRuntimeIqkStageReport,
) -> bool {
    rtl8812au_runtime_iqk_stage_success_iqc(tx_stage).is_some()
        && rtl8812au_runtime_iqk_stage_success_iqc(rx_stage).is_some()
}

pub fn rtl8812au_runtime_iqk_report_status(
    paths: &[Rtl8812auRuntimeIqkPathReport],
    cleanup_status: &str,
) -> &'static str {
    if cleanup_status != "restored" {
        return "restore_failed";
    }
    if paths
        .iter()
        .all(|path| path.tx.status == "success" && path.rx.status == "success")
    {
        "completed"
    } else {
        "fallback_applied"
    }
}

pub fn rtl8812au_runtime_iqk_sweep_summary(
    paths: &[Rtl8812auRuntimeIqkPathReport],
    status: &'static str,
    cleanup_status: &'static str,
    sweep_index: u8,
) -> Rtl8812auRuntimeIqkSweepSummaryReport {
    let mut fallback_stage_count = 0;
    let path_statuses = paths
        .iter()
        .map(|path| {
            if path.tx.fallback_used || path.tx.status != "success" {
                fallback_stage_count += 1;
            }
            if path.rx.fallback_used || path.rx.status != "success" {
                fallback_stage_count += 1;
            }
            Rtl8812auRuntimeIqkSweepPathSummaryReport {
                path_name: path.path_name,
                tx_status: path.tx.status,
                tx_retry_count: path.tx.retry_count,
                tx_average_count: path.tx.average_count,
                tx_fallback_used: path.tx.fallback_used,
                tx_failure_label: path.tx.failure_label,
                rx_status: path.rx.status,
                rx_retry_count: path.rx.retry_count,
                rx_average_count: path.rx.average_count,
                rx_fallback_used: path.rx.fallback_used,
                rx_failure_label: path.rx.failure_label,
            }
        })
        .collect();

    Rtl8812auRuntimeIqkSweepSummaryReport {
        sweep_index,
        status,
        cleanup_status,
        fallback_stage_count,
        path_statuses,
    }
}

fn bb_masked_field(value: u32, mask: u32) -> u32 {
    if mask == 0 {
        return 0;
    }
    (value & mask) >> mask.trailing_zeros()
}

fn runtime_iqk_write32<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    value: u32,
    error_code: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    write32_with_counter(
        registers,
        counters,
        address,
        value,
        register_name,
        "runtime-iqk",
    )
    .map_err(|error| {
        RuntimeRadioError::new(
            error_code,
            format!(
                "{register_name} {} write {} failed: {}",
                format_register_address(address),
                format_register_value(value, 8),
                error.message
            ),
        )
    })
}

fn runtime_iqk_read32<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    error_code: &'static str,
) -> Result<u32, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    read32_with_counter(registers, counters, address, register_name, "runtime-iqk").map_err(
        |error| {
            RuntimeRadioError::new(
                error_code,
                format!(
                    "{register_name} {} read failed: {}",
                    format_register_address(address),
                    error.message
                ),
            )
        },
    )
}

fn rtl8812au_runtime_iqk_rf_backup_value(
    reports: &[Rtl8812auRfSerialReadReport],
    rf_offset: u32,
) -> Option<Rtl8812auRfSerialReadReport> {
    reports
        .iter()
        .find(|report| report.rf_offset == rf_offset)
        .cloned()
}

fn rtl8812au_runtime_iqk_register_backup_value(
    reports: &[Rtl8812auRegisterReadReport],
    address: u16,
) -> Option<Rtl8812auRegisterReadReport> {
    reports
        .iter()
        .find(|report| report.address == address)
        .cloned()
}

fn rtl8812au_runtime_iqk_pre_sweep_state<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    backup: &Rtl8812auRuntimeIqkBackupReport,
) -> Result<Rtl8812auRuntimeIqkPreSweepStateReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut txagc_registers_pre_iqk = Vec::new();
    for (register_name, address) in rtl8812au_tx_power_agc_registers(Rtl8812auRfPath::Both) {
        let value = runtime_iqk_read32(
            registers,
            counters,
            register_name,
            address,
            "rtl8812a_runtime_iqk_pre_sweep_state_failed",
        )?;
        txagc_registers_pre_iqk.push(register_read_report(
            register_name,
            address,
            "u32",
            value,
            8,
        ));
    }

    Ok(Rtl8812auRuntimeIqkPreSweepStateReport {
        bb_agc_state_pre_iqk: backup.page_select_register.clone(),
        ofdmccken_pre_iqk: rtl8812au_runtime_iqk_register_backup_value(
            &backup.macbb_backup,
            REG_OFDMCCKEN_JAGUAR,
        ),
        cca_on_sec_pre_iqk: rtl8812au_runtime_iqk_register_backup_value(
            &backup.macbb_backup,
            REG_CCA_ON_SEC_JAGUAR,
        ),
        rf_mode_path_a_pre_iqk: rtl8812au_runtime_iqk_rf_backup_value(
            &backup.rf_backup_path_a,
            0x00,
        ),
        rf_mode_path_b_pre_iqk: rtl8812au_runtime_iqk_rf_backup_value(
            &backup.rf_backup_path_b,
            0x00,
        ),
        txagc_registers_pre_iqk,
    })
}

fn runtime_iqk_capture_tx_candidate<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    path_name: &'static str,
    latch_register_name: &'static str,
    latch_register: u16,
    result_register_name: &'static str,
    result_register: u16,
) -> Result<
    (
        Rtl8812auRuntimeIqkIqcValue,
        Rtl8812auRuntimeIqkRawCandidateReport,
    ),
    RuntimeRadioError,
>
where
    T: Rtl8812auUsbTransport,
{
    runtime_iqk_write32(
        registers,
        counters,
        latch_register_name,
        latch_register,
        0x0200_0000,
        "rtl8812a_runtime_iqk_tx_failed",
    )?;
    let tx_x_raw = runtime_iqk_read32(
        registers,
        counters,
        result_register_name,
        result_register,
        "rtl8812a_runtime_iqk_tx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        latch_register_name,
        latch_register,
        0x0400_0000,
        "rtl8812a_runtime_iqk_tx_failed",
    )?;
    let tx_y_raw = runtime_iqk_read32(
        registers,
        counters,
        result_register_name,
        result_register,
        "rtl8812a_runtime_iqk_tx_failed",
    )?;
    let raw_candidate = rtl8812au_runtime_iqk_raw_candidate_report(tx_x_raw, tx_y_raw);
    let candidate = rtl8812au_runtime_iqk_iqc_value(raw_candidate.x_field, raw_candidate.y_field);
    if candidate.x == 0 && candidate.y == 0 {
        return Err(RuntimeRadioError::new(
            "rtl8812a_runtime_iqk_tx_failed",
            format!("path {path_name} TX IQK produced a zero TX_X/TX_Y candidate"),
        ));
    }
    Ok((candidate, raw_candidate))
}

fn apply_rf_mask(original: u32, bitmask: u32, data: u32) -> u32 {
    if bitmask == 0 {
        return original;
    }
    let bitshift = bitmask.trailing_zeros();
    (original & !bitmask) | ((data << bitshift) & bitmask)
}

fn runtime_iqk_set_bb_reg<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    register_name: &'static str,
    address: u16,
    mask: u32,
    data: u32,
    error_code: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    bb_set_bb_reg(registers, counters, address, mask, data, register_name).map_err(|error| {
        RuntimeRadioError::new(
            error_code,
            format!(
                "{register_name} {} masked write mask={} data={} failed: {}",
                format_register_address(address),
                format_register_value(mask, 8),
                format_register_value(data, 8),
                error.message
            ),
        )
    })
}

fn runtime_iqk_rf_masked_write<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    path: Rtl8812auRfPath,
    rf_offset: u32,
    mask: u32,
    data: u32,
    error_code: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before = rf_serial_read_register(registers, path, rf_offset, counters)?;
    let written = apply_rf_mask(before.value, mask, data);
    rf_serial_write_single_path(registers, path, rf_offset, written, counters).map_err(
        |error| {
            RuntimeRadioError::new(
                error_code,
                format!(
                    "RF path {} offset {} masked write mask={} data={} failed: {}",
                    before.path_name,
                    before.rf_offset_hex,
                    format_register_value(mask, 5),
                    format_register_value(data, 5),
                    error.message
                ),
            )
        },
    )?;
    Ok(())
}

fn runtime_iqk_capture_rx_candidate<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    latch_register_name: &'static str,
    latch_register: u16,
    result_register_name: &'static str,
    result_register: u16,
) -> Result<
    (
        Rtl8812auRuntimeIqkIqcValue,
        Rtl8812auRuntimeIqkRawCandidateReport,
    ),
    RuntimeRadioError,
>
where
    T: Rtl8812auUsbTransport,
{
    runtime_iqk_write32(
        registers,
        counters,
        latch_register_name,
        latch_register,
        0x0600_0000,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    let rx_x_raw = runtime_iqk_read32(
        registers,
        counters,
        result_register_name,
        result_register,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        latch_register_name,
        latch_register,
        0x0800_0000,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    let rx_y_raw = runtime_iqk_read32(
        registers,
        counters,
        result_register_name,
        result_register,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    let raw_candidate = rtl8812au_runtime_iqk_raw_candidate_report(rx_x_raw, rx_y_raw);
    Ok((
        rtl8812au_runtime_iqk_iqc_value(raw_candidate.x_field, raw_candidate.y_field),
        raw_candidate,
    ))
}

pub fn run_rtl8812au_runtime_iqk_tx_oneshot<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<
    (
        Rtl8812auRuntimeIqkStageReport,
        Rtl8812auRuntimeIqkStageReport,
    ),
    RuntimeRadioError,
>
where
    T: Rtl8812auUsbTransport,
{
    let mut path_a = Rtl8812auRuntimeIqkOneShotPathState::default();
    let mut path_b = Rtl8812auRuntimeIqkOneShotPathState::default();

    while !(path_a.is_finished() && path_b.is_finished()) {
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xcb8",
            REG_RFE_TIMING_A_JAGUAR,
            0x0010_0000,
            "rtl8812a_runtime_iqk_tx_failed",
        )?;
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xeb8",
            REG_RFE_TIMING_B_JAGUAR,
            0x0010_0000,
            "rtl8812a_runtime_iqk_tx_failed",
        )?;
        runtime_iqk_write32(
            registers,
            counters,
            "R_0x980",
            REG_IQK_TRIGGER_980,
            0xfa00_0000,
            "rtl8812a_runtime_iqk_tx_failed",
        )?;
        runtime_iqk_write32(
            registers,
            counters,
            "R_0x980",
            REG_IQK_TRIGGER_980,
            0xf800_0000,
            "rtl8812a_runtime_iqk_tx_failed",
        )?;

        thread::sleep(Duration::from_millis(10));
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xcb8",
            REG_RFE_TIMING_A_JAGUAR,
            0,
            "rtl8812a_runtime_iqk_tx_failed",
        )?;
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xeb8",
            REG_RFE_TIMING_B_JAGUAR,
            0,
            "rtl8812a_runtime_iqk_tx_failed",
        )?;

        let mut delay_count = 0;
        let mut path_a_status_raw = None;
        let mut path_b_status_raw = None;
        loop {
            if !path_a.is_finished() {
                let value = runtime_iqk_read32(
                    registers,
                    counters,
                    "R_0xd00",
                    REG_IQK_RESULT_A_D00,
                    "rtl8812a_runtime_iqk_tx_failed",
                )?;
                path_a_status_raw = Some(value);
                path_a.set_ready(value & RTL8812A_IQK_READY_MASK != 0);
            }
            if !path_b.is_finished() {
                let value = runtime_iqk_read32(
                    registers,
                    counters,
                    "R_0xd40",
                    REG_IQK_RESULT_B_D40,
                    "rtl8812a_runtime_iqk_tx_failed",
                )?;
                path_b_status_raw = Some(value);
                path_b.set_ready(value & RTL8812A_IQK_READY_MASK != 0);
            }
            let path_a_ready = path_a.is_finished() || path_a.ready().unwrap_or(false);
            let path_b_ready = path_b.is_finished() || path_b.ready().unwrap_or(false);
            if (path_a_ready && path_b_ready) || delay_count > RTL8812A_IQK_READY_POLL_LIMIT {
                break;
            }
            thread::sleep(Duration::from_millis(1));
            delay_count += 1;
        }
        path_a.note_delay_count(delay_count);
        path_b.note_delay_count(delay_count);

        if delay_count < RTL8812A_IQK_READY_POLL_LIMIT {
            if !path_a.is_finished() {
                let value = runtime_iqk_read32(
                    registers,
                    counters,
                    "R_0xd00",
                    REG_IQK_RESULT_A_D00,
                    "rtl8812a_runtime_iqk_tx_failed",
                )?;
                let failed = value & RTL8812A_IQK_TX_FAIL_MASK != 0;
                path_a.set_failed(failed);
                if failed {
                    path_a.push_attempt(
                        path_a.ready(),
                        Some(failed),
                        Some(delay_count),
                        Some(value),
                        None,
                        None,
                        Some("tx_iqk_failed_flag"),
                    );
                    path_a.note_retry("tx_iqk_failed_flag");
                } else {
                    let (candidate, raw_candidate) = runtime_iqk_capture_tx_candidate(
                        registers,
                        counters,
                        "A",
                        "R_0xcb8",
                        REG_RFE_TIMING_A_JAGUAR,
                        "R_0xd00",
                        REG_IQK_RESULT_A_D00,
                    )?;
                    path_a.push_attempt(
                        path_a.ready(),
                        Some(failed),
                        Some(delay_count),
                        Some(value),
                        Some(raw_candidate),
                        Some(candidate.clone()),
                        None,
                    );
                    path_a.push_candidate(candidate);
                }
            }
            if !path_b.is_finished() {
                let value = runtime_iqk_read32(
                    registers,
                    counters,
                    "R_0xd40",
                    REG_IQK_RESULT_B_D40,
                    "rtl8812a_runtime_iqk_tx_failed",
                )?;
                let failed = value & RTL8812A_IQK_TX_FAIL_MASK != 0;
                path_b.set_failed(failed);
                if failed {
                    path_b.push_attempt(
                        path_b.ready(),
                        Some(failed),
                        Some(delay_count),
                        Some(value),
                        None,
                        None,
                        Some("tx_iqk_failed_flag"),
                    );
                    path_b.note_retry("tx_iqk_failed_flag");
                } else {
                    let (candidate, raw_candidate) = runtime_iqk_capture_tx_candidate(
                        registers,
                        counters,
                        "B",
                        "R_0xeb8",
                        REG_RFE_TIMING_B_JAGUAR,
                        "R_0xd40",
                        REG_IQK_RESULT_B_D40,
                    )?;
                    path_b.push_attempt(
                        path_b.ready(),
                        Some(failed),
                        Some(delay_count),
                        Some(value),
                        Some(raw_candidate),
                        Some(candidate.clone()),
                        None,
                    );
                    path_b.push_candidate(candidate);
                }
            }
        } else {
            if !path_a.is_finished() {
                path_a.push_attempt(
                    path_a.ready(),
                    None,
                    Some(delay_count),
                    path_a_status_raw,
                    None,
                    None,
                    Some("tx_iqk_not_ready"),
                );
                path_a.note_retry("tx_iqk_not_ready");
            }
            if !path_b.is_finished() {
                path_b.push_attempt(
                    path_b.ready(),
                    None,
                    Some(delay_count),
                    path_b_status_raw,
                    None,
                    None,
                    Some("tx_iqk_not_ready"),
                );
                path_b.note_retry("tx_iqk_not_ready");
            }
        }

        if path_a.is_finished() && path_b.is_finished() {
            break;
        }
        if path_a.attempts() >= RTL8812A_IQK_MAX_ATTEMPTS
            || path_b.attempts() >= RTL8812A_IQK_MAX_ATTEMPTS
        {
            break;
        }
    }

    Ok((
        path_a.into_stage_report("tx", rtl8812au_runtime_iqk_iqc_value(0x200, 0), Vec::new()),
        path_b.into_stage_report("tx", rtl8812au_runtime_iqk_iqc_value(0x200, 0), Vec::new()),
    ))
}

fn runtime_iqk_prepare_rx_path<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    path: Rtl8812auRfPath,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    for (rf_offset, value) in [
        (RF_IQK_MODE_JAGUAR, 0x80000),
        (RF_IQK_TX_0X30_JAGUAR, 0x30000),
        (RF_IQK_TX_0X31_JAGUAR, 0x3f7ff),
        (RF_IQK_TX_0X32_JAGUAR, 0xfe7bf),
        (0x8f, 0x88001),
        (0x65, 0x931d1),
        (RF_IQK_MODE_JAGUAR, 0),
    ] {
        rf_serial_write_single_path(registers, path, rf_offset, value, counters).map_err(
            |error| {
                RuntimeRadioError::new(
                    "rtl8812a_runtime_iqk_rx_failed",
                    format!(
                        "RF path {:?} RX IQK setup offset {} value {} failed: {}",
                        path,
                        format_register_value(rf_offset, 2),
                        format_register_value(value, 5),
                        error.message
                    ),
                )
            },
        )?;
    }
    Ok(())
}

fn runtime_iqk_load_lok<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    path: Rtl8812auRfPath,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let lok_source =
        rf_serial_read_register(registers, path, RF_IQK_LOK_READBACK_JAGUAR, counters)?;
    let lok_data = bb_masked_field(lok_source.value, 0x000f_fc00);
    runtime_iqk_rf_masked_write(
        registers,
        counters,
        path,
        RF_IQK_LOK_LOAD_JAGUAR,
        0x0007_fe00,
        lok_data,
        "rtl8812a_runtime_iqk_rx_failed",
    )
}

fn runtime_iqk_prepare_rx_oneshot<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    tx_path_a_ready: bool,
    tx_path_b_ready: bool,
    rfe_type: u8,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    rtl8812au_iqk_select_page(registers, counters, false)?;
    if tx_path_a_ready {
        runtime_iqk_prepare_rx_path(registers, counters, Rtl8812auRfPath::A)?;
    }
    if tx_path_b_ready {
        runtime_iqk_prepare_rx_path(registers, counters, Rtl8812auRfPath::B)?;
    }
    runtime_iqk_set_bb_reg(
        registers,
        counters,
        "R_0x978",
        0x0978,
        0x8000_0000,
        1,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_set_bb_reg(
        registers,
        counters,
        "R_0x97c",
        0x097c,
        0x8000_0000,
        0,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        "R_0x90c",
        REG_IQK_MACBB_0X090C,
        0x0000_8000,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        "R_0x984",
        0x0984,
        0x0046_a890,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        "rA_RFE_Pinmux_Jaguar",
        REG_RFE_PINMUX_A_JAGUAR,
        0x7777_7717,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        "rB_RFE_Pinmux_Jaguar",
        REG_RFE_PINMUX_B_JAGUAR,
        0x7777_7717,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    let inv_value = if rfe_type == 1 {
        0x0000_0077
    } else {
        0x0200_0077
    };
    runtime_iqk_write32(
        registers,
        counters,
        "rA_RFE_Inv_Jaguar",
        REG_RFE_INV_A_JAGUAR,
        inv_value,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        "rB_RFE_Inv_Jaguar",
        REG_RFE_INV_B_JAGUAR,
        inv_value,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;

    rtl8812au_iqk_select_page(registers, counters, true)?;
    if tx_path_a_ready {
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xc80",
            REG_IQK_TX_TONE_A_C80,
            0x3800_8c10,
            "rtl8812a_runtime_iqk_rx_failed",
        )?;
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xc84",
            REG_IQK_RX_TONE_A_C84,
            0x1800_8c10,
            "rtl8812a_runtime_iqk_rx_failed",
        )?;
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xc88",
            REG_IQK_RFE_SETTING_A_C88,
            0x8214_0119,
            "rtl8812a_runtime_iqk_rx_failed",
        )?;
    }
    if tx_path_b_ready {
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xe80",
            REG_IQK_TX_TONE_B_E80,
            0x3800_8c10,
            "rtl8812a_runtime_iqk_rx_failed",
        )?;
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xe84",
            REG_IQK_RX_TONE_B_E84,
            0x1800_8c10,
            "rtl8812a_runtime_iqk_rx_failed",
        )?;
        runtime_iqk_write32(
            registers,
            counters,
            "R_0xe88",
            REG_IQK_RFE_SETTING_B_E88,
            0x8214_0119,
            "rtl8812a_runtime_iqk_rx_failed",
        )?;
    }
    Ok(())
}

fn runtime_iqk_trigger_rx_path<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    tx_iqc: &Rtl8812auRuntimeIqkIqcValue,
    mixer_register_name: &'static str,
    mixer_register: u16,
    latch_register_name: &'static str,
    latch_register: u16,
    mixer_value: u32,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    rtl8812au_iqk_select_page(registers, counters, false)?;
    runtime_iqk_set_bb_reg(
        registers,
        counters,
        "R_0x978",
        0x0978,
        0x03ff_8000,
        tx_iqc.x,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_set_bb_reg(
        registers,
        counters,
        "R_0x978",
        0x0978,
        0x0000_07ff,
        tx_iqc.y,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    rtl8812au_iqk_select_page(registers, counters, true)?;
    runtime_iqk_write32(
        registers,
        counters,
        mixer_register_name,
        mixer_register,
        mixer_value,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        latch_register_name,
        latch_register,
        0x0030_0000,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        latch_register_name,
        latch_register,
        0x0010_0000,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    thread::sleep(Duration::from_millis(5));
    runtime_iqk_write32(
        registers,
        counters,
        mixer_register_name,
        mixer_register,
        0x3c00_0000,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    runtime_iqk_write32(
        registers,
        counters,
        latch_register_name,
        latch_register,
        0,
        "rtl8812a_runtime_iqk_rx_failed",
    )?;
    Ok(())
}

pub fn run_rtl8812au_runtime_iqk_rx_oneshot<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    tx_path_a: &Rtl8812auRuntimeIqkStageReport,
    tx_path_b: &Rtl8812auRuntimeIqkStageReport,
    rfe_type: u8,
) -> Result<
    (
        Rtl8812auRuntimeIqkStageReport,
        Rtl8812auRuntimeIqkStageReport,
    ),
    RuntimeRadioError,
>
where
    T: Rtl8812auUsbTransport,
{
    let tx_a_iqc = rtl8812au_runtime_iqk_stage_success_iqc(tx_path_a);
    let tx_b_iqc = rtl8812au_runtime_iqk_stage_success_iqc(tx_path_b);
    let mut path_a = Rtl8812auRuntimeIqkOneShotPathState::default();
    let mut path_b = Rtl8812auRuntimeIqkOneShotPathState::default();

    rtl8812au_iqk_select_page(registers, counters, false)?;
    if tx_a_iqc.is_some() {
        runtime_iqk_load_lok(registers, counters, Rtl8812auRfPath::A)?;
    }
    if tx_b_iqc.is_some() {
        runtime_iqk_load_lok(registers, counters, Rtl8812auRfPath::B)?;
    }
    runtime_iqk_prepare_rx_oneshot(
        registers,
        counters,
        tx_a_iqc.is_some(),
        tx_b_iqc.is_some(),
        rfe_type,
    )?;

    if tx_a_iqc.is_none() && tx_b_iqc.is_none() {
        return Ok((
            rtl8812au_runtime_iqk_skipped_stage_report(
                "rx",
                "rx_iqk_skipped_without_tx_iqk",
                Vec::new(),
            ),
            rtl8812au_runtime_iqk_skipped_stage_report(
                "rx",
                "rx_iqk_skipped_without_tx_iqk",
                Vec::new(),
            ),
        ));
    }

    let path_a_mixer = if rfe_type == 1 {
        0x2816_1500
    } else {
        0x2816_0cc0
    };
    let path_b_mixer = if rfe_type == 1 {
        0x2816_1500
    } else {
        0x2816_0ca0
    };

    while !((path_a.is_finished() || tx_a_iqc.is_none())
        && (path_b.is_finished() || tx_b_iqc.is_none()))
    {
        // The upstream loop re-triggers every TX-ready path on each RX retry,
        // even when that path's RX IQK has already found a stable pair.
        if let Some(tx_iqc) = tx_a_iqc.as_ref() {
            runtime_iqk_trigger_rx_path(
                registers,
                counters,
                tx_iqc,
                "R_0xc8c",
                REG_IQK_RFE_SETTING_A_C8C,
                "R_0xcb8",
                REG_RFE_TIMING_A_JAGUAR,
                path_a_mixer,
            )?;
        }
        if let Some(tx_iqc) = tx_b_iqc.as_ref() {
            runtime_iqk_trigger_rx_path(
                registers,
                counters,
                tx_iqc,
                "R_0xe8c",
                REG_IQK_RFE_SETTING_B_E8C,
                "R_0xeb8",
                REG_RFE_TIMING_B_JAGUAR,
                path_b_mixer,
            )?;
        }

        let mut delay_count = 0;
        let mut path_a_status_raw = None;
        let mut path_b_status_raw = None;
        loop {
            if !path_a.is_finished() && tx_a_iqc.is_some() {
                let value = runtime_iqk_read32(
                    registers,
                    counters,
                    "R_0xd00",
                    REG_IQK_RESULT_A_D00,
                    "rtl8812a_runtime_iqk_rx_failed",
                )?;
                path_a_status_raw = Some(value);
                path_a.set_ready(value & RTL8812A_IQK_READY_MASK != 0);
            }
            if !path_b.is_finished() && tx_b_iqc.is_some() {
                let value = runtime_iqk_read32(
                    registers,
                    counters,
                    "R_0xd40",
                    REG_IQK_RESULT_B_D40,
                    "rtl8812a_runtime_iqk_rx_failed",
                )?;
                path_b_status_raw = Some(value);
                path_b.set_ready(value & RTL8812A_IQK_READY_MASK != 0);
            }
            let path_a_ready =
                path_a.is_finished() || tx_a_iqc.is_none() || path_a.ready().unwrap_or(false);
            let path_b_ready =
                path_b.is_finished() || tx_b_iqc.is_none() || path_b.ready().unwrap_or(false);
            if (path_a_ready && path_b_ready) || delay_count > RTL8812A_IQK_READY_POLL_LIMIT {
                break;
            }
            thread::sleep(Duration::from_millis(1));
            delay_count += 1;
        }
        path_a.note_delay_count(delay_count);
        path_b.note_delay_count(delay_count);

        if delay_count < RTL8812A_IQK_READY_POLL_LIMIT {
            if !path_a.is_finished() && tx_a_iqc.is_some() {
                let value = runtime_iqk_read32(
                    registers,
                    counters,
                    "R_0xd00",
                    REG_IQK_RESULT_A_D00,
                    "rtl8812a_runtime_iqk_rx_failed",
                )?;
                let failed = value & RTL8812A_IQK_RX_FAIL_MASK != 0;
                path_a.set_failed(failed);
                if failed {
                    path_a.push_attempt(
                        path_a.ready(),
                        Some(failed),
                        Some(delay_count),
                        Some(value),
                        None,
                        None,
                        Some("rx_iqk_failed_flag"),
                    );
                    path_a.note_retry("rx_iqk_failed_flag");
                } else {
                    let (candidate, raw_candidate) = runtime_iqk_capture_rx_candidate(
                        registers,
                        counters,
                        "R_0xcb8",
                        REG_RFE_TIMING_A_JAGUAR,
                        "R_0xd00",
                        REG_IQK_RESULT_A_D00,
                    )?;
                    path_a.push_attempt(
                        path_a.ready(),
                        Some(failed),
                        Some(delay_count),
                        Some(value),
                        Some(raw_candidate),
                        Some(candidate.clone()),
                        None,
                    );
                    path_a.push_candidate(candidate);
                }
            }
            if !path_b.is_finished() && tx_b_iqc.is_some() {
                let value = runtime_iqk_read32(
                    registers,
                    counters,
                    "R_0xd40",
                    REG_IQK_RESULT_B_D40,
                    "rtl8812a_runtime_iqk_rx_failed",
                )?;
                let failed = value & RTL8812A_IQK_RX_FAIL_MASK != 0;
                path_b.set_failed(failed);
                if failed {
                    path_b.push_attempt(
                        path_b.ready(),
                        Some(failed),
                        Some(delay_count),
                        Some(value),
                        None,
                        None,
                        Some("rx_iqk_failed_flag"),
                    );
                    path_b.note_retry("rx_iqk_failed_flag");
                } else {
                    let (candidate, raw_candidate) = runtime_iqk_capture_rx_candidate(
                        registers,
                        counters,
                        "R_0xeb8",
                        REG_RFE_TIMING_B_JAGUAR,
                        "R_0xd40",
                        REG_IQK_RESULT_B_D40,
                    )?;
                    path_b.push_attempt(
                        path_b.ready(),
                        Some(failed),
                        Some(delay_count),
                        Some(value),
                        Some(raw_candidate),
                        Some(candidate.clone()),
                        None,
                    );
                    path_b.push_candidate(candidate);
                }
            }
        } else {
            if !path_a.is_finished() && tx_a_iqc.is_some() {
                path_a.push_attempt(
                    path_a.ready(),
                    None,
                    Some(delay_count),
                    path_a_status_raw,
                    None,
                    None,
                    Some("rx_iqk_not_ready"),
                );
                path_a.note_retry("rx_iqk_not_ready");
            }
            if !path_b.is_finished() && tx_b_iqc.is_some() {
                path_b.push_attempt(
                    path_b.ready(),
                    None,
                    Some(delay_count),
                    path_b_status_raw,
                    None,
                    None,
                    Some("rx_iqk_not_ready"),
                );
                path_b.note_retry("rx_iqk_not_ready");
            }
        }

        if (path_a.is_finished() || tx_a_iqc.is_none())
            && (path_b.is_finished() || tx_b_iqc.is_none())
        {
            break;
        }
        if path_a.attempts() >= RTL8812A_IQK_MAX_ATTEMPTS
            || path_b.attempts() >= RTL8812A_IQK_MAX_ATTEMPTS
            || path_a.candidate_count() >= RTL8812A_IQK_MAX_RX_CANDIDATES
            || path_b.candidate_count() >= RTL8812A_IQK_MAX_RX_CANDIDATES
        {
            break;
        }
    }

    let path_a_report = if tx_a_iqc.is_some() {
        path_a.into_stage_report("rx", rtl8812au_runtime_iqk_iqc_value(0x200, 0), Vec::new())
    } else {
        rtl8812au_runtime_iqk_skipped_stage_report(
            "rx",
            "rx_iqk_skipped_without_tx_iqk",
            Vec::new(),
        )
    };
    let path_b_report = if tx_b_iqc.is_some() {
        path_b.into_stage_report("rx", rtl8812au_runtime_iqk_iqc_value(0x200, 0), Vec::new())
    } else {
        rtl8812au_runtime_iqk_skipped_stage_report(
            "rx",
            "rx_iqk_skipped_without_tx_iqk",
            Vec::new(),
        )
    };
    Ok((path_a_report, path_b_report))
}

pub fn rtl8812au_runtime_iqk_masked_bb_write_plan(
    register_name: &'static str,
    address: u16,
    mask: u32,
    data: u32,
    reason: &'static str,
) -> Rtl8812auRuntimeIqkMaskedBbWritePlan {
    Rtl8812auRuntimeIqkMaskedBbWritePlan {
        register_name,
        address,
        address_hex: format_register_address(address),
        mask,
        mask_hex: format_register_value(mask, 8),
        data,
        data_hex: format_register_value(data, 8),
        reason,
    }
}

fn write8_register_report<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    register_name: &'static str,
    address: u16,
    value: u8,
    counters: &mut RuntimeRadioCounters,
) -> Result<Rtl8812auRegisterWriteReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_counters = *counters;
    let before = read8_with_counter(registers, counters, address, register_name, "pre-write")?;
    write8_with_counter(registers, counters, address, value, register_name, "write")?;
    let after = read8_with_counter(registers, counters, address, register_name, "post-write")?;
    Ok(Rtl8812auRegisterWriteReport {
        register_name,
        address,
        address_hex: format_register_address(address),
        width: "u8",
        before: u32::from(before),
        before_hex: format_register_value(before, 2),
        written: u32::from(value),
        written_hex: format_register_value(value, 2),
        after: u32::from(after),
        after_hex: format_register_value(after, 2),
        changed: before != after,
        counters: counters.saturating_sub(before_counters),
    })
}

fn write32_register_report<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    register_name: &'static str,
    address: u16,
    value: u32,
    counters: &mut RuntimeRadioCounters,
) -> Result<Rtl8812auRegisterWriteReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_counters = *counters;
    let before = read32_with_counter(registers, counters, address, register_name, "pre-write")?;
    write32_with_counter(registers, counters, address, value, register_name, "write")?;
    let after = read32_with_counter(registers, counters, address, register_name, "post-write")?;
    Ok(Rtl8812auRegisterWriteReport {
        register_name,
        address,
        address_hex: format_register_address(address),
        width: "u32",
        before,
        before_hex: format_register_value(before, 8),
        written: value,
        written_hex: format_register_value(value, 8),
        after,
        after_hex: format_register_value(after, 8),
        changed: before != after,
        counters: counters.saturating_sub(before_counters),
    })
}

fn bb_set_bb_reg<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    bitmask: u32,
    data: u32,
    register_name: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    if bitmask == u32::MAX {
        return write32_with_counter(registers, counters, address, data, register_name, "bb-set");
    }
    if bitmask == 0 {
        return Ok(());
    }

    let original = read32_with_counter(registers, counters, address, register_name, "bb-set")?;
    let bitshift = bitmask.trailing_zeros();
    let written = (original & !bitmask) | ((data << bitshift) & bitmask);
    write32_with_counter(
        registers,
        counters,
        address,
        written,
        register_name,
        "bb-set",
    )
}

fn encode_rf_serial_write(rf_offset: u32, data: u32) -> u32 {
    (((rf_offset & 0xff) << 20) | (data & RF_REGISTER_OFFSET_MASK)) & 0x0fff_ffff
}

type RfSerialWriteTarget = (Rtl8812auRfPath, &'static str, &'static str, u16);
type RfSerialReadTarget = (
    Rtl8812auRfPath,
    &'static str,
    &'static str,
    u16,
    &'static str,
    u16,
    &'static str,
    u16,
);

const RF_SERIAL_TARGET_A: [RfSerialWriteTarget; 1] = [(
    Rtl8812auRfPath::A,
    "A",
    "rA_LSSIWrite_Jaguar",
    REG_RF_PATH_A_3WIRE,
)];
const RF_SERIAL_TARGET_B: [RfSerialWriteTarget; 1] = [(
    Rtl8812auRfPath::B,
    "B",
    "rB_LSSIWrite_Jaguar",
    REG_RF_PATH_B_3WIRE,
)];
const RF_SERIAL_TARGET_BOTH: [RfSerialWriteTarget; 2] = [
    (
        Rtl8812auRfPath::A,
        "A",
        "rA_LSSIWrite_Jaguar",
        REG_RF_PATH_A_3WIRE,
    ),
    (
        Rtl8812auRfPath::B,
        "B",
        "rB_LSSIWrite_Jaguar",
        REG_RF_PATH_B_3WIRE,
    ),
];
const RF_SERIAL_READ_TARGET_A: RfSerialReadTarget = (
    Rtl8812auRfPath::A,
    "A",
    "rA_PI_Mode_Jaguar",
    REG_RF_PI_MODE_A_JAGUAR,
    "rA_PIRead_Jaguar",
    REG_RF_PI_READ_A_JAGUAR,
    "rA_SIRead_Jaguar",
    REG_RF_SI_READ_A_JAGUAR,
);
const RF_SERIAL_READ_TARGET_B: RfSerialReadTarget = (
    Rtl8812auRfPath::B,
    "B",
    "rB_PI_Mode_Jaguar",
    REG_RF_PI_MODE_B_JAGUAR,
    "rB_PIRead_Jaguar",
    REG_RF_PI_READ_B_JAGUAR,
    "rB_SIRead_Jaguar",
    REG_RF_SI_READ_B_JAGUAR,
);

fn rf_serial_write_targets(path: Rtl8812auRfPath) -> &'static [RfSerialWriteTarget] {
    match path {
        Rtl8812auRfPath::A => &RF_SERIAL_TARGET_A,
        Rtl8812auRfPath::B => &RF_SERIAL_TARGET_B,
        Rtl8812auRfPath::Both => &RF_SERIAL_TARGET_BOTH,
    }
}

fn rf_serial_read_target(path: Rtl8812auRfPath) -> Option<RfSerialReadTarget> {
    match path {
        Rtl8812auRfPath::A => Some(RF_SERIAL_READ_TARGET_A),
        Rtl8812auRfPath::B => Some(RF_SERIAL_READ_TARGET_B),
        Rtl8812auRfPath::Both => None,
    }
}

fn rf_register_display_name(rf_offset: u32) -> &'static str {
    match rf_offset {
        0x00 => "RF_0x00",
        RF_IQK_LOK_READBACK_JAGUAR => "RF_0x08_LOK_readback",
        RF_IQK_TX_0X30_JAGUAR => "RF_0x30_IQK",
        RF_IQK_TX_0X31_JAGUAR => "RF_0x31_IQK",
        RF_IQK_TX_0X32_JAGUAR => "RF_0x32_IQK",
        RF_IQK_LOK_LOAD_JAGUAR => "RF_0x58_LOK_load",
        0x65 => "RF_0x65_IQK_backup",
        0x8f => "RF_0x8f_IQK_backup",
        RF_IQK_MODE_JAGUAR => "RF_0xef_IQK_mode",
        RF_CHNLBW_JAGUAR => "RF_CHNLBW_Jaguar",
        RF_LCK_JAGUAR => "RF_LCK",
        _ => "RF register",
    }
}

fn rf_serial_write_single_path<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    path: Rtl8812auRfPath,
    rf_offset: u32,
    value: u32,
    counters: &mut RuntimeRadioCounters,
) -> Result<Rtl8812auRfSerialWriteReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut reports = Vec::new();
    for &(path, path_name, bb_register_name, bb_register) in rf_serial_write_targets(path) {
        let before = *counters;
        let value = value & RF_REGISTER_OFFSET_MASK;
        let encoded = encode_rf_serial_write(rf_offset, value);
        write32_with_counter(
            registers,
            counters,
            bb_register,
            encoded,
            bb_register_name,
            "rf-serial-write",
        )?;
        reports.push(Rtl8812auRfSerialWriteReport {
            register_name: rf_register_display_name(rf_offset),
            path,
            path_name,
            bb_register_name,
            bb_register,
            bb_register_hex: format_register_value(bb_register, 4),
            rf_offset,
            rf_offset_hex: format_register_value(rf_offset, 2),
            value,
            value_hex: format_register_value(value, 5),
            encoded,
            encoded_hex: format_register_value(encoded, 8),
            counters: counters.saturating_sub(before),
        });
        thread::sleep(Duration::from_micros(1));
    }

    reports.pop().ok_or_else(|| {
        RuntimeRadioError::new(
            "rf_serial_write_failed",
            format!("RF serial write produced no report for path {path:?}"),
        )
    })
}

fn rf_serial_read_register<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    path: Rtl8812auRfPath,
    rf_offset: u32,
    counters: &mut RuntimeRadioCounters,
) -> Result<Rtl8812auRfSerialReadReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let (
        path,
        path_name,
        pi_mode_register_name,
        pi_mode_register,
        pi_readback_register_name,
        pi_readback_register,
        si_readback_register_name,
        si_readback_register,
    ) = rf_serial_read_target(path).ok_or_else(|| {
        RuntimeRadioError::new(
            "rf_serial_read_path_unsupported",
            "RF serial read requires path A or path B, not both",
        )
    })?;
    let before = *counters;
    let rf_offset = rf_offset & 0xff;

    let pi_mode_value = read32_with_counter(
        registers,
        counters,
        pi_mode_register,
        pi_mode_register_name,
        "rf-serial-read",
    )?;
    let pi_mode = pi_mode_value & 0x4 != 0;

    bb_set_bb_reg(
        registers,
        counters,
        REG_HSSI_READ_JAGUAR,
        0x0000_00ff,
        rf_offset,
        "rHSSIRead_Jaguar",
    )?;
    thread::sleep(Duration::from_micros(20));

    let (readback_register_name, readback_register) = if pi_mode {
        (pi_readback_register_name, pi_readback_register)
    } else {
        (si_readback_register_name, si_readback_register)
    };
    let value = read32_with_counter(
        registers,
        counters,
        readback_register,
        readback_register_name,
        "rf-serial-read",
    )? & RF_REGISTER_OFFSET_MASK;

    Ok(Rtl8812auRfSerialReadReport {
        register_name: rf_register_display_name(rf_offset),
        path,
        path_name,
        rf_offset,
        rf_offset_hex: format_register_value(rf_offset, 2),
        hssi_register_name: "rHSSIRead_Jaguar",
        hssi_register: REG_HSSI_READ_JAGUAR,
        hssi_register_hex: format_register_address(REG_HSSI_READ_JAGUAR),
        hssi_mask_hex: format_register_value(0x0000_00ff_u32, 8),
        pi_mode_register_name,
        pi_mode_register,
        pi_mode_register_hex: format_register_address(pi_mode_register),
        pi_mode_value,
        pi_mode_value_hex: format_register_value(pi_mode_value, 8),
        pi_mode,
        readback_register_name,
        readback_register,
        readback_register_hex: format_register_address(readback_register),
        readback_mask_hex: format_register_value(RF_REGISTER_OFFSET_MASK, 5),
        value,
        value_hex: format_register_value(value, 5),
        counters: counters.saturating_sub(before),
    })
}

pub fn rtl8812au_iqk_tx_fill_iqc_plan(
    path: Rtl8812auRfPath,
    tx_x: u32,
    tx_y: u32,
    dpk_done: bool,
) -> Result<Vec<Rtl8812auRuntimeIqkMaskedBbWritePlan>, RuntimeRadioError> {
    let _path_name = path.name().ok_or_else(|| {
        RuntimeRadioError::new(
            "rtl8812a_runtime_iqk_invalid_path",
            "RTL8812A IQK TX IQC fill requires path A or path B, not both",
        )
    })?;
    let mut plan = vec![rtl8812au_runtime_iqk_masked_bb_write_plan(
        "REG_AGC_TABLE_JAGUAR",
        REG_AGC_TABLE_JAGUAR,
        RTL8812A_IQK_PAGE_C1_SELECT_BIT,
        1,
        "_iqk_tx_fill_iqc_8812a selects BB page C1 before writing TX IQC latches",
    )];

    let (
        tx_bb_ctrl_name,
        tx_bb_ctrl,
        tx_ctrl_name,
        tx_ctrl,
        tx_latch_name,
        tx_latch,
        tx_y_name,
        tx_y_register,
        tx_x_name,
        tx_x_register,
    ) = match path {
        Rtl8812auRfPath::A => (
            "rA_TxBbCtrl",
            REG_TX_BB_CTRL_A_JAGUAR,
            "R_0xcc4",
            REG_IQK_TX_CTRL_A_CC4,
            "R_0xcc8",
            REG_IQK_TX_CTRL_A_CC8,
            "R_0xccc_TX_Y",
            REG_IQK_TX_Y_A_CCC,
            "R_0xcd4_TX_X",
            REG_IQK_TX_X_A_CD4,
        ),
        Rtl8812auRfPath::B => (
            "rB_TxBbCtrl",
            REG_TX_BB_CTRL_B_JAGUAR,
            "R_0xec4",
            REG_IQK_TX_CTRL_B_EC4,
            "R_0xec8",
            REG_IQK_TX_CTRL_B_EC8,
            "R_0xecc_TX_Y",
            REG_IQK_TX_Y_B_ECC,
            "R_0xed4_TX_X",
            REG_IQK_TX_X_B_ED4,
        ),
        Rtl8812auRfPath::Both => unreachable!("path validated above"),
    };

    plan.push(rtl8812au_runtime_iqk_masked_bb_write_plan(
        tx_bb_ctrl_name,
        tx_bb_ctrl,
        0x0000_0080,
        1,
        "_iqk_tx_fill_iqc_8812a enables TX IQC fill path",
    ));
    plan.push(rtl8812au_runtime_iqk_masked_bb_write_plan(
        tx_ctrl_name,
        tx_ctrl,
        0x0004_0000,
        1,
        "_iqk_tx_fill_iqc_8812a enables TX IQK correction latch",
    ));
    if !dpk_done {
        plan.push(rtl8812au_runtime_iqk_masked_bb_write_plan(
            tx_ctrl_name,
            tx_ctrl,
            0x2000_0000,
            1,
            "_iqk_tx_fill_iqc_8812a enables IQK fill when DPK has not completed",
        ));
    }
    plan.push(rtl8812au_runtime_iqk_masked_bb_write_plan(
        tx_latch_name,
        tx_latch,
        0x2000_0000,
        1,
        "_iqk_tx_fill_iqc_8812a arms the TX IQK result latch",
    ));
    plan.push(rtl8812au_runtime_iqk_masked_bb_write_plan(
        tx_y_name,
        tx_y_register,
        0x0000_07ff,
        tx_y & 0x0000_07ff,
        "_iqk_tx_fill_iqc_8812a writes selected TX_Y IQC",
    ));
    plan.push(rtl8812au_runtime_iqk_masked_bb_write_plan(
        tx_x_name,
        tx_x_register,
        0x0000_07ff,
        tx_x & 0x0000_07ff,
        "_iqk_tx_fill_iqc_8812a writes selected TX_X IQC",
    ));

    Ok(plan)
}

pub fn rtl8812au_iqk_rx_fill_iqc_plan(
    path: Rtl8812auRfPath,
    rx_x: u32,
    rx_y: u32,
) -> Result<Vec<Rtl8812auRuntimeIqkMaskedBbWritePlan>, RuntimeRadioError> {
    let _path_name = path.name().ok_or_else(|| {
        RuntimeRadioError::new(
            "rtl8812a_runtime_iqk_invalid_path",
            "RTL8812A IQK RX IQC fill requires path A or path B, not both",
        )
    })?;
    let (register_name, register) = match path {
        Rtl8812auRfPath::A => ("R_0xc10_RX_IQC_A", REG_IQK_RX_IQC_A_JAGUAR),
        Rtl8812auRfPath::B => ("R_0xe10_RX_IQC_B", REG_IQK_RX_IQC_B_JAGUAR),
        Rtl8812auRfPath::Both => unreachable!("path validated above"),
    };
    let shifted_x = rx_x >> 1;
    let shifted_y = rx_y >> 1;
    let uses_upstream_fallback = shifted_x >= 0x112 || (shifted_y >= 0x12 && shifted_y <= 0x3ee);
    let (iqc_x, iqc_y, reason) = if uses_upstream_fallback {
        (
            0x100,
            0,
            "_iqk_rx_fill_iqc_8812a uses upstream fallback when shifted RX_X/RX_Y is out of range",
        )
    } else {
        (
            shifted_x & 0x03ff,
            shifted_y & 0x03ff,
            "_iqk_rx_fill_iqc_8812a writes selected shifted RX IQC",
        )
    };

    Ok(vec![
        rtl8812au_runtime_iqk_masked_bb_write_plan(
            "REG_AGC_TABLE_JAGUAR",
            REG_AGC_TABLE_JAGUAR,
            RTL8812A_IQK_PAGE_C1_SELECT_BIT,
            0,
            "_iqk_rx_fill_iqc_8812a selects BB page C before writing RX IQC latches",
        ),
        rtl8812au_runtime_iqk_masked_bb_write_plan(
            register_name,
            register,
            0x0000_03ff,
            iqc_x,
            reason,
        ),
        rtl8812au_runtime_iqk_masked_bb_write_plan(
            register_name,
            register,
            0x03ff_0000,
            iqc_y,
            reason,
        ),
    ])
}

fn apply_runtime_iqk_masked_bb_write<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    write: &Rtl8812auRuntimeIqkMaskedBbWritePlan,
    error_code: &'static str,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    bb_set_bb_reg(
        registers,
        counters,
        write.address,
        write.mask,
        write.data,
        write.register_name,
    )
    .map_err(|error| {
        RuntimeRadioError::new(
            error_code,
            format!(
                "{} masked write failed: {}",
                write.register_name, error.message
            ),
        )
    })
}

pub fn apply_rtl8812au_runtime_iqk_fill<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    path: Rtl8812auRfPath,
    tx_stage: &mut Rtl8812auRuntimeIqkStageReport,
    rx_stage: &mut Rtl8812auRuntimeIqkStageReport,
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let tx_iqc = rtl8812au_runtime_iqk_stage_iqc_or_fallback(tx_stage);
    let rx_iqc = rtl8812au_runtime_iqk_stage_iqc_or_fallback(rx_stage);
    let tx_plan = rtl8812au_iqk_tx_fill_iqc_plan(path, tx_iqc.x, tx_iqc.y, false)?;
    let rx_plan = rtl8812au_iqk_rx_fill_iqc_plan(path, rx_iqc.x, rx_iqc.y)?;
    for write in tx_plan.iter().chain(rx_plan.iter()) {
        apply_runtime_iqk_masked_bb_write(
            registers,
            counters,
            write,
            "rtl8812a_runtime_iqk_fill_failed",
        )?;
    }
    let applied = tx_plan.len() + rx_plan.len();
    tx_stage.fill_plan = tx_plan;
    rx_stage.fill_plan = rx_plan;
    Ok(applied)
}

fn rtl8812au_runtime_iqk_setup_write8_plan(
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    value: u8,
    reason: &'static str,
) -> Rtl8812auRuntimeIqkSetupWritePlan {
    Rtl8812auRuntimeIqkSetupWritePlan::Register {
        phase,
        register_name,
        address,
        address_hex: format_register_address(address),
        width: "u8",
        value: u32::from(value),
        value_hex: format_register_value(value, 2),
        reason,
    }
}

fn rtl8812au_runtime_iqk_setup_write32_plan(
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    value: u32,
    reason: &'static str,
) -> Rtl8812auRuntimeIqkSetupWritePlan {
    Rtl8812auRuntimeIqkSetupWritePlan::Register {
        phase,
        register_name,
        address,
        address_hex: format_register_address(address),
        width: "u32",
        value,
        value_hex: format_register_value(value, 8),
        reason,
    }
}

fn rtl8812au_runtime_iqk_setup_masked_bb_plan(
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    mask: u32,
    data: u32,
    reason: &'static str,
) -> Rtl8812auRuntimeIqkSetupWritePlan {
    Rtl8812auRuntimeIqkSetupWritePlan::MaskedBb {
        phase,
        write: rtl8812au_runtime_iqk_masked_bb_write_plan(
            register_name,
            address,
            mask,
            data,
            reason,
        ),
    }
}

fn rtl8812au_runtime_iqk_setup_rf_plan(
    phase: &'static str,
    path: Rtl8812auRfPath,
    rf_offset: u32,
    value: u32,
    reason: &'static str,
) -> Rtl8812auRuntimeIqkSetupWritePlan {
    Rtl8812auRuntimeIqkSetupWritePlan::Rf {
        phase,
        path,
        path_name: path.name().unwrap_or("?"),
        rf_offset,
        rf_offset_hex: format_register_value(rf_offset, 2),
        value,
        value_hex: format_register_value(value, 5),
        reason,
    }
}

pub fn rtl8812au_runtime_iqk_setup_plan(
    band: Band,
    rfe_type: u8,
    ext_pa_5g: bool,
    ext_pa_2g: bool,
) -> Vec<Rtl8812auRuntimeIqkSetupWritePlan> {
    let mut plan = vec![
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "mac_config",
            "REG_AGC_TABLE_JAGUAR",
            REG_AGC_TABLE_JAGUAR,
            RTL8812A_IQK_PAGE_C1_SELECT_BIT,
            0,
            "_iqk_configure_mac_8812a selects page C before MAC gating",
        ),
        rtl8812au_runtime_iqk_setup_write8_plan(
            "mac_config",
            "REG_TXPAUSE",
            REG_TXPAUSE,
            0x3f,
            "_iqk_configure_mac_8812a pauses packet TX queues during IQK",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "mac_config",
            "REG_BCN_CTRL",
            REG_BCN_CTRL,
            0x0000_0808,
            0,
            "_iqk_configure_mac_8812a disables beacon/TBTT interactions during IQK",
        ),
        rtl8812au_runtime_iqk_setup_write8_plan(
            "mac_config",
            "REG_OFDMCCKEN_JAGUAR",
            REG_OFDMCCKEN_JAGUAR,
            0x00,
            "_iqk_configure_mac_8812a disables RX antenna path",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "mac_config",
            "REG_CCA_ON_SEC_JAGUAR",
            REG_CCA_ON_SEC_JAGUAR,
            0x0000_000f,
            0x0c,
            "_iqk_configure_mac_8812a gates CCA during IQK",
        ),
        rtl8812au_runtime_iqk_setup_write8_plan(
            "mac_config",
            "REG_CCK_RX_PATH_JAGUAR",
            REG_CCK_RX_PATH_JAGUAR,
            0x0f,
            "_iqk_configure_mac_8812a disables CCK RX path during IQK",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "tx_setup",
            "REG_AGC_TABLE_JAGUAR",
            REG_AGC_TABLE_JAGUAR,
            RTL8812A_IQK_PAGE_C1_SELECT_BIT,
            0,
            "_iqk_tx_8812a selects page C before AFE/RF setup",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "afe_setup",
            "R_0xc60",
            REG_IQK_AFE_A_C60,
            0x7777_7777,
            "_iqk_tx_8812a enables path A DAC/ADC",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "afe_setup",
            "R_0xc64",
            REG_IQK_AFE_A_C64,
            0x7777_7777,
            "_iqk_tx_8812a enables path A DAC/ADC",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "afe_setup",
            "R_0xe60",
            REG_IQK_AFE_B_E60,
            0x7777_7777,
            "_iqk_tx_8812a enables path B DAC/ADC",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "afe_setup",
            "R_0xe64",
            REG_IQK_AFE_B_E64,
            0x7777_7777,
            "_iqk_tx_8812a enables path B DAC/ADC",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "afe_setup",
            "R_0xc68",
            REG_IQK_AFE_A_C68,
            0x1979_1979,
            "_iqk_tx_8812a configures path A AFE IQK bias",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "afe_setup",
            "R_0xe68",
            REG_IQK_AFE_B_E68,
            0x1979_1979,
            "_iqk_tx_8812a configures path B AFE IQK bias",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "afe_setup",
            "rA_PI_Mode_Jaguar",
            REG_RF_PI_MODE_A_JAGUAR,
            0x0000_000f,
            0x04,
            "_iqk_tx_8812a disables hardware 3-wire path A",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "afe_setup",
            "rB_PI_Mode_Jaguar",
            REG_RF_PI_MODE_B_JAGUAR,
            0x0000_000f,
            0x04,
            "_iqk_tx_8812a disables hardware 3-wire path B",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "afe_setup",
            "R_0xc5c",
            REG_IQK_AFE_A_C5C,
            0x0700_0000,
            0x07,
            "_iqk_tx_8812a sets path A DAC/ADC sampling rate",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "afe_setup",
            "R_0xe5c",
            REG_IQK_AFE_B_E5C,
            0x0700_0000,
            0x07,
            "_iqk_tx_8812a sets path B DAC/ADC sampling rate",
        ),
    ];

    for path in [Rtl8812auRfPath::A, Rtl8812auRfPath::B] {
        plan.extend([
            rtl8812au_runtime_iqk_setup_rf_plan(
                "rf_tx_setup",
                path,
                RF_IQK_MODE_JAGUAR,
                0x80002,
                "_iqk_tx_8812a selects RF IQK mode",
            ),
            rtl8812au_runtime_iqk_setup_rf_plan(
                "rf_tx_setup",
                path,
                RF_IQK_TX_0X30_JAGUAR,
                0x20000,
                "_iqk_tx_8812a programs TX IQK RF register 0x30",
            ),
            rtl8812au_runtime_iqk_setup_rf_plan(
                "rf_tx_setup",
                path,
                RF_IQK_TX_0X31_JAGUAR,
                0x3fffd,
                "_iqk_tx_8812a programs TX IQK RF register 0x31",
            ),
            rtl8812au_runtime_iqk_setup_rf_plan(
                "rf_tx_setup",
                path,
                RF_IQK_TX_0X32_JAGUAR,
                0xfe83f,
                "_iqk_tx_8812a programs TX IQK RF register 0x32",
            ),
            rtl8812au_runtime_iqk_setup_rf_plan(
                "rf_tx_setup",
                path,
                0x65,
                0x931d5,
                "_iqk_tx_8812a programs TX IQK RF register 0x65",
            ),
            rtl8812au_runtime_iqk_setup_rf_plan(
                "rf_tx_setup",
                path,
                0x8f,
                0x8a001,
                "_iqk_tx_8812a programs TX IQK RF register 0x8f",
            ),
        ]);
    }

    let rfe_setting = if band == Band::Ghz5 {
        if ext_pa_5g {
            if rfe_type == 1 {
                0x8214_03e3
            } else {
                0x8214_03f7
            }
        } else {
            0x8214_03f1
        }
    } else if rfe_type == 3 {
        if ext_pa_2g {
            0x8214_03e3
        } else {
            0x8214_03f7
        }
    } else {
        0x8214_03f1
    };
    let mixer_setting = if band == Band::Ghz5 {
        0x6816_3e96
    } else {
        0x2816_3e96
    };

    plan.extend([
        rtl8812au_runtime_iqk_setup_write32_plan(
            "bb_iqk_setup",
            "R_0x90c",
            REG_IQK_MACBB_0X090C,
            0x0000_8000,
            "_iqk_tx_8812a enables IQK MAC/BB mode",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "bb_iqk_setup",
            "R_0xc94",
            REG_IQK_TX_POWER_CTRL_A_C94,
            0x0000_0001,
            1,
            "_iqk_tx_8812a enables path A IQK power latch",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "bb_iqk_setup",
            "R_0xe94",
            REG_TX_POWER_BEFORE_IQK_A_JAGUAR,
            0x0000_0001,
            1,
            "_iqk_tx_8812a enables path B IQK power latch",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "bb_iqk_setup",
            "R_0x978",
            0x0978,
            0x2900_2000,
            "_iqk_tx_8812a programs TX tone X/Y source",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "bb_iqk_setup",
            "R_0x97c",
            0x097c,
            0xa900_2000,
            "_iqk_tx_8812a programs RX tone X/Y source",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "bb_iqk_setup",
            "R_0x984",
            0x0984,
            0x0046_2910,
            "_iqk_tx_8812a enables AGC/idac IQK mask",
        ),
        rtl8812au_runtime_iqk_setup_masked_bb_plan(
            "page_c1_setup",
            "REG_AGC_TABLE_JAGUAR",
            REG_AGC_TABLE_JAGUAR,
            RTL8812A_IQK_PAGE_C1_SELECT_BIT,
            1,
            "_iqk_tx_8812a selects page C1 before tone/RFE latches",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xc88",
            REG_IQK_RFE_SETTING_A_C88,
            rfe_setting,
            "_iqk_tx_8812a programs path A RFE IQK setting",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xe88",
            REG_IQK_RFE_SETTING_B_E88,
            rfe_setting,
            "_iqk_tx_8812a programs path B RFE IQK setting",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xc8c",
            REG_IQK_RFE_SETTING_A_C8C,
            mixer_setting,
            "_iqk_tx_8812a programs path A band-specific mixer setting",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xe8c",
            REG_IQK_RFE_SETTING_B_E8C,
            mixer_setting,
            "_iqk_tx_8812a programs path B band-specific mixer setting",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xc80",
            REG_IQK_TX_TONE_A_C80,
            0x1800_8c10,
            "_iqk_tx_8812a programs path A TX tone for one-shot",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xc84",
            REG_IQK_RX_TONE_A_C84,
            0x3800_8c10,
            "_iqk_tx_8812a programs path A RX tone for one-shot",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xce8",
            REG_IQK_VDF_A_CE8,
            0,
            "_iqk_tx_8812a disables path A VDF branch for HT20/HT40 flow",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xe80",
            REG_IQK_TX_TONE_B_E80,
            0x1800_8c10,
            "_iqk_tx_8812a programs path B TX tone for one-shot",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xe84",
            REG_IQK_RX_TONE_B_E84,
            0x3800_8c10,
            "_iqk_tx_8812a programs path B RX tone for one-shot",
        ),
        rtl8812au_runtime_iqk_setup_write32_plan(
            "page_c1_setup",
            "R_0xee8",
            REG_IQK_VDF_B_EE8,
            0,
            "_iqk_tx_8812a disables path B VDF branch for HT20/HT40 flow",
        ),
    ]);

    plan
}

pub fn apply_rtl8812au_runtime_iqk_setup_plan<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    plan: &[Rtl8812auRuntimeIqkSetupWritePlan],
) -> Result<usize, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut applied = 0;
    for action in plan {
        match action {
            Rtl8812auRuntimeIqkSetupWritePlan::Register {
                register_name,
                address,
                width,
                value,
                ..
            } if *width == "u8" => {
                write8_with_counter(
                    registers,
                    counters,
                    *address,
                    *value as u8,
                    register_name,
                    "runtime-iqk-setup",
                )?;
                applied += 1;
            }
            Rtl8812auRuntimeIqkSetupWritePlan::Register {
                register_name,
                address,
                value,
                ..
            } => {
                write32_with_counter(
                    registers,
                    counters,
                    *address,
                    *value,
                    register_name,
                    "runtime-iqk-setup",
                )?;
                applied += 1;
            }
            Rtl8812auRuntimeIqkSetupWritePlan::MaskedBb { write, .. } => {
                let before = read32_with_counter(
                    registers,
                    counters,
                    write.address,
                    write.register_name,
                    "runtime-iqk-setup",
                )?;
                let shifted = if write.mask == 0 {
                    0
                } else {
                    (write.data << write.mask.trailing_zeros()) & write.mask
                };
                let written = (before & !write.mask) | shifted;
                write32_with_counter(
                    registers,
                    counters,
                    write.address,
                    written,
                    write.register_name,
                    "runtime-iqk-setup",
                )?;
                let _after = read32_with_counter(
                    registers,
                    counters,
                    write.address,
                    write.register_name,
                    "runtime-iqk-setup",
                )?;
                applied += 1;
            }
            Rtl8812auRuntimeIqkSetupWritePlan::Rf {
                path,
                rf_offset,
                value,
                ..
            } => {
                rf_serial_write_single_path(registers, *path, *rf_offset, *value, counters)?;
                applied += 1;
            }
        }
    }
    Ok(applied)
}

fn rtl8812au_iqk_select_page<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    page_c1: bool,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    bb_set_bb_reg(
        registers,
        counters,
        REG_AGC_TABLE_JAGUAR,
        RTL8812A_IQK_PAGE_C1_SELECT_BIT,
        u32::from(page_c1),
        "REG_AGC_TABLE_JAGUAR",
    )
    .map_err(|error| {
        RuntimeRadioError::new(
            "rtl8812a_runtime_iqk_page_select_failed",
            format!(
                "REG_AGC_TABLE_JAGUAR page {} select failed: {}",
                if page_c1 { "C1" } else { "C" },
                error.message
            ),
        )
    })
}

fn rtl8812au_iqk_read32_group<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    group: &[Rtl8812auRegisterReadSpec],
) -> Result<Vec<Rtl8812auRegisterReadReport>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut reports = Vec::with_capacity(group.len());
    for &(register_name, address) in group {
        let value = read32_with_counter(
            registers,
            counters,
            address,
            register_name,
            "runtime-iqk-backup",
        )?;
        reports.push(register_read_report(
            register_name,
            address,
            "u32",
            value,
            8,
        ));
    }
    Ok(reports)
}

fn rtl8812au_iqk_rf_backup_reads<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    path: Rtl8812auRfPath,
) -> Result<Vec<Rtl8812auRfSerialReadReport>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut reports = Vec::with_capacity(RTL8812A_IQK_RF_BACKUP_OFFSETS.len());
    for &rf_offset in RTL8812A_IQK_RF_BACKUP_OFFSETS {
        reports.push(rf_serial_read_register(
            registers, path, rf_offset, counters,
        )?);
    }
    Ok(reports)
}

pub fn run_rtl8812au_runtime_iqk_backup<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<Rtl8812auRuntimeIqkBackupReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let hssi_read_value = read32_with_counter(
        registers,
        counters,
        REG_HSSI_READ_JAGUAR,
        "rHSSIRead_Jaguar",
        "runtime-iqk-backup",
    )
    .map_err(|error| {
        RuntimeRadioError::new(
            "rtl8812a_runtime_iqk_backup_failed",
            format!("rHSSIRead_Jaguar backup read failed: {}", error.message),
        )
    })?;
    let hssi_read_register = register_read_report(
        "rHSSIRead_Jaguar",
        REG_HSSI_READ_JAGUAR,
        "u32",
        hssi_read_value,
        8,
    );

    let page_select_value = read32_with_counter(
        registers,
        counters,
        REG_AGC_TABLE_JAGUAR,
        "REG_AGC_TABLE_JAGUAR",
        "runtime-iqk-backup",
    )
    .map_err(|error| {
        RuntimeRadioError::new(
            "rtl8812a_runtime_iqk_backup_failed",
            format!("REG_AGC_TABLE_JAGUAR backup read failed: {}", error.message),
        )
    })?;
    let page_select_register = register_read_report(
        "REG_AGC_TABLE_JAGUAR",
        REG_AGC_TABLE_JAGUAR,
        "u32",
        page_select_value,
        8,
    );

    let tx_pause_value = read8_with_counter(
        registers,
        counters,
        REG_TXPAUSE,
        "REG_TXPAUSE",
        "runtime-iqk-backup",
    )
    .map_err(|error| {
        RuntimeRadioError::new(
            "rtl8812a_runtime_iqk_backup_failed",
            format!("REG_TXPAUSE backup read failed: {}", error.message),
        )
    })?;
    let tx_pause_register = register_read_report(
        "REG_TXPAUSE",
        REG_TXPAUSE,
        "u8",
        u32::from(tx_pause_value),
        2,
    );

    rtl8812au_iqk_select_page(registers, counters, false)?;
    let macbb_backup =
        rtl8812au_iqk_read32_group(registers, counters, RTL8812A_IQK_MACBB_BACKUP_REGISTERS)?;

    rtl8812au_iqk_select_page(registers, counters, true)?;
    let page_c1_latches =
        rtl8812au_iqk_read32_group(registers, counters, RTL8812A_IQK_PAGE_C1_LATCH_REGISTERS)?;

    rtl8812au_iqk_select_page(registers, counters, false)?;
    let afe_backup =
        rtl8812au_iqk_read32_group(registers, counters, RTL8812A_IQK_AFE_BACKUP_REGISTERS)?;
    let rf_backup_path_a = rtl8812au_iqk_rf_backup_reads(registers, counters, Rtl8812auRfPath::A)?;
    let rf_backup_path_b = rtl8812au_iqk_rf_backup_reads(registers, counters, Rtl8812auRfPath::B)?;

    Ok(Rtl8812auRuntimeIqkBackupReport {
        hssi_read_register,
        page_select_register,
        tx_pause_register,
        macbb_backup,
        afe_backup,
        rf_backup_path_a,
        rf_backup_path_b,
        page_c1_latches,
    })
}

fn restore_runtime_iqk_register_group<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    group_name: &'static str,
    backups: &[Rtl8812auRegisterReadReport],
    failures: &mut Vec<String>,
) -> usize
where
    T: Rtl8812auUsbTransport,
{
    let mut restored = 0;
    for backup in backups {
        match write32_with_counter(
            registers,
            counters,
            backup.address,
            backup.value,
            backup.register_name,
            "runtime-iqk-restore",
        ) {
            Ok(()) => restored += 1,
            Err(error) => failures.push(format!(
                "{group_name} restore {} {} to {} failed: {}",
                backup.register_name, backup.address_hex, backup.value_hex, error.message
            )),
        }
    }
    restored
}

fn restore_runtime_iqk_rf_group<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    path: Rtl8812auRfPath,
    backups: &[Rtl8812auRfSerialReadReport],
    failures: &mut Vec<String>,
) -> usize
where
    T: Rtl8812auUsbTransport,
{
    let mut restored = 0;
    for backup in backups {
        match rf_serial_write_single_path(registers, path, backup.rf_offset, backup.value, counters)
        {
            Ok(_) => restored += 1,
            Err(error) => failures.push(format!(
                "RF path {} restore {} to {} failed: {}",
                backup.path_name, backup.rf_offset_hex, backup.value_hex, error.message
            )),
        }
    }
    restored
}

pub fn restore_rtl8812au_runtime_iqk_backup<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    backup: &Rtl8812auRuntimeIqkBackupReport,
) -> Rtl8812auRuntimeIqkCleanupReport
where
    T: Rtl8812auUsbTransport,
{
    let before = *counters;
    let mut failures = Vec::new();

    if let Err(error) = rtl8812au_iqk_select_page(registers, counters, false) {
        failures.push(error.message);
    }
    let rf_path_a_restore_count = restore_runtime_iqk_rf_group(
        registers,
        counters,
        Rtl8812auRfPath::A,
        &backup.rf_backup_path_a,
        &mut failures,
    );
    let rf_path_b_restore_count = restore_runtime_iqk_rf_group(
        registers,
        counters,
        Rtl8812auRfPath::B,
        &backup.rf_backup_path_b,
        &mut failures,
    );

    if let Err(error) = rtl8812au_iqk_select_page(registers, counters, false) {
        failures.push(error.message);
    }
    let afe_restore_count = restore_runtime_iqk_register_group(
        registers,
        counters,
        "AFE",
        &backup.afe_backup,
        &mut failures,
    );

    if let Err(error) = rtl8812au_iqk_select_page(registers, counters, true) {
        failures.push(error.message);
    }
    let page_c1_latch_restore_count = restore_runtime_iqk_register_group(
        registers,
        counters,
        "page-C1 latch",
        &backup.page_c1_latches,
        &mut failures,
    );

    if let Err(error) = rtl8812au_iqk_select_page(registers, counters, false) {
        failures.push(error.message);
    }
    let macbb_restore_count = restore_runtime_iqk_register_group(
        registers,
        counters,
        "MAC/BB",
        &backup.macbb_backup,
        &mut failures,
    );

    let hssi_read_restored = match write32_with_counter(
        registers,
        counters,
        REG_HSSI_READ_JAGUAR,
        backup.hssi_read_register.value,
        backup.hssi_read_register.register_name,
        "runtime-iqk-restore",
    ) {
        Ok(()) => match read32_with_counter(
            registers,
            counters,
            REG_HSSI_READ_JAGUAR,
            backup.hssi_read_register.register_name,
            "runtime-iqk-restore",
        ) {
            Ok(after) => {
                let restored = after == backup.hssi_read_register.value;
                if !restored {
                    failures.push(format!(
                        "rHSSIRead_Jaguar restored to {}, expected {}",
                        format_register_value(after, 8),
                        backup.hssi_read_register.value_hex
                    ));
                }
                Some(restored)
            }
            Err(error) => {
                failures.push(format!(
                    "rHSSIRead_Jaguar post-restore read failed: {}",
                    error.message
                ));
                None
            }
        },
        Err(error) => {
            failures.push(format!(
                "rHSSIRead_Jaguar restore to {} failed: {}",
                backup.hssi_read_register.value_hex, error.message
            ));
            None
        }
    };

    let page_select_restored = match write32_with_counter(
        registers,
        counters,
        REG_AGC_TABLE_JAGUAR,
        backup.page_select_register.value,
        backup.page_select_register.register_name,
        "runtime-iqk-restore",
    ) {
        Ok(()) => match read32_with_counter(
            registers,
            counters,
            REG_AGC_TABLE_JAGUAR,
            backup.page_select_register.register_name,
            "runtime-iqk-restore",
        ) {
            Ok(after) => {
                let restored = after == backup.page_select_register.value;
                if !restored {
                    failures.push(format!(
                        "REG_AGC_TABLE_JAGUAR restored to {}, expected {}",
                        format_register_value(after, 8),
                        backup.page_select_register.value_hex
                    ));
                }
                Some(restored)
            }
            Err(error) => {
                failures.push(format!(
                    "REG_AGC_TABLE_JAGUAR post-restore read failed: {}",
                    error.message
                ));
                None
            }
        },
        Err(error) => {
            failures.push(format!(
                "REG_AGC_TABLE_JAGUAR restore to {} failed: {}",
                backup.page_select_register.value_hex, error.message
            ));
            None
        }
    };

    let tx_pause_restored = match write8_with_counter(
        registers,
        counters,
        REG_TXPAUSE,
        backup.tx_pause_register.value as u8,
        backup.tx_pause_register.register_name,
        "runtime-iqk-restore",
    ) {
        Ok(()) => match read8_with_counter(
            registers,
            counters,
            REG_TXPAUSE,
            backup.tx_pause_register.register_name,
            "runtime-iqk-restore",
        ) {
            Ok(after) => {
                let restored = u32::from(after) == backup.tx_pause_register.value;
                if !restored {
                    failures.push(format!(
                        "REG_TXPAUSE restored to {}, expected {}",
                        format_register_value(after, 2),
                        backup.tx_pause_register.value_hex
                    ));
                }
                Some(restored)
            }
            Err(error) => {
                failures.push(format!(
                    "REG_TXPAUSE post-restore read failed: {}",
                    error.message
                ));
                None
            }
        },
        Err(error) => {
            failures.push(format!(
                "REG_TXPAUSE restore to {} failed: {}",
                backup.tx_pause_register.value_hex, error.message
            ));
            None
        }
    };

    let status = if failures.is_empty() {
        "restored"
    } else {
        "restore_failed"
    };
    Rtl8812auRuntimeIqkCleanupReport {
        status,
        failures,
        macbb_restore_count,
        afe_restore_count,
        rf_path_a_restore_count,
        rf_path_b_restore_count,
        page_c1_latch_restore_count,
        hssi_read_restored,
        page_select_restored,
        tx_pause_restored,
        counters: counters.saturating_sub(before),
    }
}

pub fn run_rtl8812au_runtime_iqk_calibration<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    channel: Channel,
    rfe_type: u8,
    counters: &mut RuntimeRadioCounters,
) -> Result<Rtl8812auRuntimeIqkCalibrationReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_all_sweeps = *counters;
    let mut sweep_summaries = Vec::new();
    let mut last_report = None;

    for sweep_index in 1..=RTL8812A_IQK_MAX_SWEEPS {
        let mut report =
            run_rtl8812au_runtime_iqk_calibration_sweep(registers, channel, rfe_type, counters)?;
        sweep_summaries.push(rtl8812au_runtime_iqk_sweep_summary(
            &report.paths,
            report.status,
            report.cleanup_status,
            sweep_index,
        ));
        report.sweep_index = sweep_index;
        report.sweep_count = sweep_index;
        report.max_sweeps = RTL8812A_IQK_MAX_SWEEPS;
        report.sweep_summaries = sweep_summaries.clone();
        report.counters = counters.saturating_sub(before_all_sweeps);

        if report.status == "completed" {
            return Ok(report);
        }
        last_report = Some(report);
    }

    last_report.ok_or_else(|| {
        RuntimeRadioError::new(
            "rtl8812a_runtime_iqk_failed",
            "runtime IQK did not execute any calibration sweeps",
        )
    })
}

fn run_rtl8812au_runtime_iqk_calibration_sweep<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    channel: Channel,
    rfe_type: u8,
    counters: &mut RuntimeRadioCounters,
) -> Result<Rtl8812auRuntimeIqkCalibrationReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_counters = *counters;
    let before_iqk_registers =
        rtl8812au_iqk_read32_group(registers, counters, RTL8812A_IQK_RESULT_REGISTERS)?;
    let backup = run_rtl8812au_runtime_iqk_backup(registers, counters)?;
    let pre_sweep_state = rtl8812au_runtime_iqk_pre_sweep_state(registers, counters, &backup)?;
    let setup_plan =
        rtl8812au_runtime_iqk_setup_plan(channel.band, rfe_type, channel.band == Band::Ghz5, false);

    let result = (|| {
        let _setup_writes =
            apply_rtl8812au_runtime_iqk_setup_plan(registers, counters, &setup_plan)?;
        let (tx_a, tx_b) = run_rtl8812au_runtime_iqk_tx_oneshot(registers, counters)?;
        let (rx_a, rx_b) =
            run_rtl8812au_runtime_iqk_rx_oneshot(registers, counters, &tx_a, &tx_b, rfe_type)?;
        Ok::<_, RuntimeRadioError>((tx_a, rx_a, tx_b, rx_b))
    })();

    let cleanup = restore_rtl8812au_runtime_iqk_backup(registers, counters, &backup);

    let (mut tx_a, mut rx_a, mut tx_b, mut rx_b) = match result {
        Ok(stages) => stages,
        Err(error) => {
            if cleanup.status != "restored" {
                return Err(RuntimeRadioError::new(
                    error.code,
                    format!(
                        "{}; runtime IQK cleanup status={} failures={}",
                        error.message,
                        cleanup.status,
                        cleanup.failures.join("; ")
                    ),
                ));
            }
            return Err(error);
        }
    };

    let can_fill = cleanup.status == "restored"
        && rtl8812au_runtime_iqk_path_can_fill(&tx_a, &rx_a)
        && rtl8812au_runtime_iqk_path_can_fill(&tx_b, &rx_b);
    let selected_iqc_fill_register_count = if can_fill {
        let fill_a = apply_rtl8812au_runtime_iqk_fill(
            registers,
            counters,
            Rtl8812auRfPath::A,
            &mut tx_a,
            &mut rx_a,
        )?;
        let fill_b = apply_rtl8812au_runtime_iqk_fill(
            registers,
            counters,
            Rtl8812auRfPath::B,
            &mut tx_b,
            &mut rx_b,
        )?;
        fill_a + fill_b
    } else {
        0
    };
    let selected_iqc_fill_applied = selected_iqc_fill_register_count > 0;
    let after_iqk_registers =
        rtl8812au_iqk_read32_group(registers, counters, RTL8812A_IQK_RESULT_REGISTERS)
            .unwrap_or_default();

    let paths = vec![
        Rtl8812auRuntimeIqkPathReport {
            path: Rtl8812auRfPath::A,
            path_name: "A",
            tx: tx_a,
            rx: rx_a,
        },
        Rtl8812auRuntimeIqkPathReport {
            path: Rtl8812auRfPath::B,
            path_name: "B",
            tx: tx_b,
            rx: rx_b,
        },
    ];
    let status = rtl8812au_runtime_iqk_report_status(&paths, cleanup.status);
    let cleanup_status = cleanup.status;
    let cleanup_failures = cleanup.failures.clone();
    let affected_registers =
        rtl8812au_iqk_read32_group(registers, counters, RTL8812A_IQK_RESULT_REGISTERS)
            .unwrap_or_default();

    Ok(Rtl8812auRuntimeIqkCalibrationReport {
        semantics: "guarded RTL8812A runtime IQK calibration; runs the upstream TX/RX one-shot IQK sequence, restores destructive RF/BB setup state, then fills selected IQC values only when every TX/RX path completed",
        upstream_basis: "aircrack-ng _phy_iq_calibrate_8812a, _iqk_tx_8812a, _iqk_tx_fill_iqc_8812a, and _iqk_rx_fill_iqc_8812a for RTL8812A",
        mode: "runtime_iqk",
        sweep_index: 1,
        sweep_count: 1,
        max_sweeps: 1,
        sweep_summaries: Vec::new(),
        status,
        cleanup_status,
        cleanup_failures,
        backup: Some(backup),
        pre_sweep_state: Some(pre_sweep_state),
        cleanup: Some(cleanup),
        selected_iqc_fill_applied,
        selected_iqc_fill_register_count,
        paths,
        affected_registers,
        before_iqk_registers,
        after_iqk_registers,
        counters: counters.saturating_sub(before_counters),
    })
}

const LINUX_PARITY_CH36_HT20_CALIBRATION_WRITES: &[Rtl8812auRegisterWriteSpec] = &[
    Rtl8812auRegisterWriteSpec {
        register_name: "rA_TxScale_Jaguar",
        address: REG_TX_SCALE_A_JAGUAR,
        value: 0x4000_0003,
    },
    Rtl8812auRegisterWriteSpec {
        register_name: "rB_TxScale_Jaguar",
        address: REG_TX_SCALE_B_JAGUAR,
        value: 0x4000_0003,
    },
    Rtl8812auRegisterWriteSpec {
        register_name: "rA_RFE_Pinmux_Jaguar",
        address: REG_RFE_PINMUX_A_JAGUAR,
        value: 0x5433_7770,
    },
    Rtl8812auRegisterWriteSpec {
        register_name: "rB_RFE_Pinmux_Jaguar",
        address: REG_RFE_PINMUX_B_JAGUAR,
        value: 0x5433_7770,
    },
    Rtl8812auRegisterWriteSpec {
        register_name: "rA_TxBbCtrl",
        address: REG_TX_BB_CTRL_A_JAGUAR,
        value: 0x0180_7c09,
    },
    Rtl8812auRegisterWriteSpec {
        register_name: "rB_TxBbCtrl",
        address: REG_TX_BB_CTRL_B_JAGUAR,
        value: 0x0180_7c09,
    },
];

pub fn rtl8812au_targeted_calibration_writes(
    profile: TxCalibrationProfile,
    channel: Channel,
    bandwidth: Bandwidth,
) -> Result<Option<&'static [Rtl8812auRegisterWriteSpec]>, RuntimeRadioError> {
    match profile {
        TxCalibrationProfile::CurrentDefault
        | TxCalibrationProfile::Rtl8812aLck
        | TxCalibrationProfile::Rtl8812aIqkProbe
        | TxCalibrationProfile::Rtl8812aRuntimeIqk => Ok(None),
        TxCalibrationProfile::LinuxParityCh36Ht20
            if channel.number == 36 && bandwidth == Bandwidth::Mhz20 =>
        {
            Ok(Some(LINUX_PARITY_CH36_HT20_CALIBRATION_WRITES))
        }
        TxCalibrationProfile::LinuxParityCh36Ht20 => Err(RuntimeRadioError::new(
            "tx_calibration_profile_unsupported",
            format!(
                "tx calibration profile linux-parity-ch36-ht20 only supports channel 36 HT20; requested channel {} {} MHz",
                channel.number,
                bandwidth.mhz()
            ),
        )),
    }
}

pub fn run_rtl8812au_targeted_calibration_profile<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    profile: TxCalibrationProfile,
    channel: Channel,
    bandwidth: Bandwidth,
) -> Result<Option<Vec<Rtl8812auRegisterWriteReport>>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let Some(writes) = rtl8812au_targeted_calibration_writes(profile, channel, bandwidth)? else {
        return Ok(None);
    };
    let mut reports = Vec::with_capacity(writes.len());
    for write in writes {
        reports.push(write32_register_report(
            registers,
            write.register_name,
            write.address,
            write.value,
            counters,
        )?);
    }
    Ok(Some(reports))
}

#[derive(Default)]
struct Rtl8812auLckCleanupState {
    tx_pause_restore: Option<u8>,
    rf_lck_restore: Option<u32>,
    rf_chnlbw_restore: Option<u32>,
}

fn cleanup_rtl8812au_lck_after_error<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    cleanup: &mut Rtl8812auLckCleanupState,
) -> Vec<String>
where
    T: Rtl8812auUsbTransport,
{
    let mut failures = Vec::new();
    if let Some(value) = cleanup.rf_lck_restore.take() {
        let encoded = encode_rf_serial_write(RF_LCK_JAGUAR, value);
        if let Err(error) = write32_with_counter(
            registers,
            counters,
            REG_RF_PATH_A_3WIRE,
            encoded,
            "rA_LSSIWrite_Jaguar",
            "lck-cleanup",
        ) {
            failures.push(format!(
                "RF_LCK restore to {} failed: {error}",
                format_register_value(value, 5)
            ));
        }
        thread::sleep(Duration::from_micros(1));
    }
    if let Some(value) = cleanup.rf_chnlbw_restore.take() {
        let encoded = encode_rf_serial_write(RF_CHNLBW_JAGUAR, value);
        if let Err(error) = write32_with_counter(
            registers,
            counters,
            REG_RF_PATH_A_3WIRE,
            encoded,
            "rA_LSSIWrite_Jaguar",
            "lck-cleanup",
        ) {
            failures.push(format!(
                "RF_CHNLBW restore to {} failed: {error}",
                format_register_value(value, 5)
            ));
        }
        thread::sleep(Duration::from_micros(1));
    }
    if let Some(value) = cleanup.tx_pause_restore.take() {
        if let Err(error) = write8_with_counter(
            registers,
            counters,
            REG_TXPAUSE,
            value,
            "REG_TXPAUSE",
            "lck-cleanup",
        ) {
            failures.push(format!(
                "REG_TXPAUSE restore to {} failed: {error}",
                format_register_value(value, 2)
            ));
        }
    }
    failures
}

pub fn run_rtl8812au_lck_calibration<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<Rtl8812auLckCalibrationReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before = *counters;
    let mut cleanup = Rtl8812auLckCleanupState::default();
    match run_rtl8812au_lck_calibration_inner(registers, counters, &mut cleanup) {
        Ok(mut report) => {
            report.counters = counters.saturating_sub(before);
            Ok(report)
        }
        Err(mut error) => {
            let cleanup_failures =
                cleanup_rtl8812au_lck_after_error(registers, counters, &mut cleanup);
            if !cleanup_failures.is_empty() {
                error.message.push_str("; cleanup failures: ");
                error.message.push_str(&cleanup_failures.join("; "));
            }
            Err(error)
        }
    }
}

fn run_rtl8812au_lck_calibration_inner<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    cleanup: &mut Rtl8812auLckCleanupState,
) -> Result<Rtl8812auLckCalibrationReport, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let continuous_tx_value = read32_with_counter(
        registers,
        counters,
        REG_SINGLE_TONE_CONT_TX_JAGUAR,
        "REG_SINGLE_TONE_CONT_TX_JAGUAR",
        "lck",
    )?;
    let continuous_tx_register = register_read_report(
        "REG_SINGLE_TONE_CONT_TX_JAGUAR",
        REG_SINGLE_TONE_CONT_TX_JAGUAR,
        "u32",
        continuous_tx_value,
        8,
    );
    let continuous_tx_active = continuous_tx_value & 0x0007_0000 != 0;

    let tx_pause_before_value =
        read8_with_counter(registers, counters, REG_TXPAUSE, "REG_TXPAUSE", "lck")?;
    let tx_pause_before = register_read_report(
        "REG_TXPAUSE",
        REG_TXPAUSE,
        "u8",
        u32::from(tx_pause_before_value),
        2,
    );

    let rf_chnlbw_backup =
        rf_serial_read_register(registers, Rtl8812auRfPath::A, RF_CHNLBW_JAGUAR, counters)?;

    let tx_pause_write = if continuous_tx_active {
        None
    } else {
        cleanup.tx_pause_restore = Some(tx_pause_before_value);
        Some(write8_register_report(
            registers,
            "REG_TXPAUSE",
            REG_TXPAUSE,
            0xff,
            counters,
        )?)
    };

    let rf_lck_before_enter =
        rf_serial_read_register(registers, Rtl8812auRfPath::A, RF_LCK_JAGUAR, counters)?;
    cleanup.rf_lck_restore = Some(rf_lck_before_enter.value);
    let rf_lck_enter_write = rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::A,
        RF_LCK_JAGUAR,
        rf_lck_before_enter.value | RF_LCK_MODE_BIT,
        counters,
    )?;

    let rf_chnlbw_before_trigger =
        rf_serial_read_register(registers, Rtl8812auRfPath::A, RF_CHNLBW_JAGUAR, counters)?;
    cleanup.rf_chnlbw_restore = Some(rf_chnlbw_before_trigger.value);
    let rf_chnlbw_trigger_write = rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::A,
        RF_CHNLBW_JAGUAR,
        rf_chnlbw_before_trigger.value | RF_CHNLBW_LCK_TRIGGER_BIT,
        counters,
    )?;

    thread::sleep(Duration::from_millis(150));

    let rf_lck_before_exit =
        rf_serial_read_register(registers, Rtl8812auRfPath::A, RF_LCK_JAGUAR, counters)?;
    let rf_lck_exit_write = rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::A,
        RF_LCK_JAGUAR,
        rf_lck_before_exit.value & !RF_LCK_MODE_BIT,
        counters,
    )?;
    cleanup.rf_lck_restore = None;

    let tx_pause_restore = if let Some(restore_value) = cleanup.tx_pause_restore.take() {
        Some(write8_register_report(
            registers,
            "REG_TXPAUSE",
            REG_TXPAUSE,
            restore_value,
            counters,
        )?)
    } else {
        None
    };

    let rf_chnlbw_restore_value = cleanup
        .rf_chnlbw_restore
        .take()
        .unwrap_or(rf_chnlbw_before_trigger.value);
    let rf_chnlbw_restore_write = rf_serial_write_single_path(
        registers,
        Rtl8812auRfPath::A,
        RF_CHNLBW_JAGUAR,
        rf_chnlbw_restore_value,
        counters,
    )?;

    let rf_chnlbw_after_restore =
        rf_serial_read_register(registers, Rtl8812auRfPath::A, RF_CHNLBW_JAGUAR, counters)?;
    let rf_lck_after_exit =
        rf_serial_read_register(registers, Rtl8812auRfPath::A, RF_LCK_JAGUAR, counters)?;

    Ok(Rtl8812auLckCalibrationReport {
        semantics: "guarded RTL8812A local-oscillator calibration; pauses packet TX when needed, runs the upstream RF_LCK/RF_CHNLBW sequence, and restores RF channel state",
        upstream_basis: "aircrack-ng _phy_lc_calibrate_8812a / phy_RFSerialRead for RTL8812A",
        rf_path: Rtl8812auRfPath::A,
        rf_path_name: "A",
        continuous_tx_register,
        continuous_tx_active,
        tx_pause_before,
        tx_pause_write,
        tx_pause_restore,
        rf_chnlbw_backup,
        rf_lck_before_enter,
        rf_lck_enter_write,
        rf_chnlbw_before_trigger,
        rf_chnlbw_trigger_write,
        delay_ms: 150,
        rf_lck_before_exit,
        rf_lck_exit_write,
        rf_chnlbw_restore_write,
        rf_chnlbw_after_restore,
        rf_lck_after_exit,
        counters: RuntimeRadioCounters::default(),
    })
}

pub fn run_rtl8812au_tx_calibration_profile<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    profile: TxCalibrationProfile,
    channel: Channel,
    bandwidth: Bandwidth,
    rfe_type: u8,
) -> Result<Option<Rtl8812auTxCalibrationProfileReport>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    match profile {
        TxCalibrationProfile::CurrentDefault => Ok(None),
        TxCalibrationProfile::Rtl8812aIqkProbe => Err(RuntimeRadioError::new(
            "tx_calibration_profile_diagnostic_only",
            "rtl8812a-iqk-probe is a diagnostic-only evidence marker; runtime callers should use rtl8812a-runtime-iqk for live IQK",
        )),
        TxCalibrationProfile::LinuxParityCh36Ht20 => {
            let writes = run_rtl8812au_targeted_calibration_profile(
                registers, counters, profile, channel, bandwidth,
            )?
            .unwrap_or_default();
            Ok(Some(Rtl8812auTxCalibrationProfileReport {
                semantics: "explicit targeted RF/TX calibration override; rewrites known Linux-final RTL8812AU RFE, TX scale, and TX BB control values after init and before TX while full IQK/LCK remains unported",
                upstream_basis: "aircrack-ng RTL8812AU Linux final register capture for AWUS036ACH channel 36 HT20",
                profile,
                channel: channel.number,
                bandwidth_mhz: bandwidth.mhz(),
                register_count: writes.len(),
                writes,
                lck: None,
                runtime_iqk: None,
            }))
        }
        TxCalibrationProfile::Rtl8812aLck => {
            let lck = run_rtl8812au_lck_calibration(registers, counters)?;
            let register_count = 4
                + usize::from(lck.tx_pause_write.is_some())
                + usize::from(lck.tx_pause_restore.is_some());
            Ok(Some(Rtl8812auTxCalibrationProfileReport {
                semantics: "explicit guarded RTL8812A runtime LCK calibration; ports the Linux local-oscillator calibration sequence without claiming full IQK/Linux parity",
                upstream_basis: "aircrack-ng _phy_lc_calibrate_8812a and RTL8812A RF serial read/write helpers",
                profile,
                channel: channel.number,
                bandwidth_mhz: bandwidth.mhz(),
                register_count,
                writes: Vec::new(),
                lck: Some(lck),
                runtime_iqk: None,
            }))
        }
        TxCalibrationProfile::Rtl8812aRuntimeIqk => {
            let runtime_iqk =
                run_rtl8812au_runtime_iqk_calibration(registers, channel, rfe_type, counters)?;
            let register_count =
                usize::try_from(runtime_iqk.counters.usb_control_writes).unwrap_or(usize::MAX);
            Ok(Some(Rtl8812auTxCalibrationProfileReport {
                semantics: "explicit guarded RTL8812A runtime IQK calibration profile; runs bounded Linux-derived TX/RX IQK, restores destructive setup state, then applies the selected IQC fill before live TX",
                upstream_basis: "aircrack-ng RTL8812A _phy_iq_calibrate_8812a runtime IQK sequence",
                profile,
                channel: channel.number,
                bandwidth_mhz: bandwidth.mhz(),
                register_count,
                writes: Vec::new(),
                lck: None,
                runtime_iqk: Some(runtime_iqk),
            }))
        }
    }
}

pub fn rtl8812au_tx_scheduler_tail_expected_writes() -> usize {
    1 + RTL8812AU_TX_SCHEDULER_TAIL_U8_WRITES.len() + 1
}

pub fn rtl8812au_monitor_receive_config() -> u32 {
    MONITOR_RECEIVE_CONFIG
}

pub fn run_rtl8812au_tx_scheduler_tail<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<RuntimePhaseExecution, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_counters = *counters;
    let mut register_writes = 0usize;
    let queue_ctrl = read8_with_counter(
        registers,
        counters,
        REG_QUEUE_CTRL,
        "REG_QUEUE_CTRL",
        "pre-tail",
    )?;
    write8_with_counter(
        registers,
        counters,
        REG_QUEUE_CTRL,
        queue_ctrl & !BIT3,
        "REG_QUEUE_CTRL",
        "late TX scheduler",
    )?;
    register_writes += 1;

    for (address, value, name) in RTL8812AU_TX_SCHEDULER_TAIL_U8_WRITES {
        write8_with_counter(
            registers,
            counters,
            *address,
            *value,
            name,
            "late TX scheduler",
        )?;
        register_writes += 1;
    }

    write16_with_counter(
        registers,
        counters,
        REG_TX_RPT_TIME,
        0x3df0,
        "REG_TX_RPT_TIME",
        "late TX scheduler",
    )?;
    register_writes += 1;

    Ok(RuntimePhaseExecution {
        phase: Rtl8812auInitPhase::TxSchedulerTail,
        register_writes,
        counters: counters.saturating_sub(before_counters),
    })
}

pub fn run_rtl8812au_monitor_opmode<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<RuntimeMonitorOpmodeExecution, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_counters = *counters;
    let msr_before = read8_with_counter(
        registers,
        counters,
        REG_MSR,
        "REG_MSR",
        "pre-monitor-opmode",
    )?;
    let msr_written = msr_before & !MSR_PORT0_NETTYPE_MASK;
    write8_with_counter(
        registers,
        counters,
        REG_MSR,
        msr_written,
        "REG_MSR",
        "monitor/no-link",
    )?;
    let msr_after = read8_with_counter(
        registers,
        counters,
        REG_MSR,
        "REG_MSR",
        "post-monitor-opmode",
    )?;

    let monitor_filter =
        run_monitor_receive_filter_registers(registers, counters, "post-monitor-opmode")?;

    Ok(RuntimeMonitorOpmodeExecution {
        msr_before,
        msr_written,
        msr_after,
        rcr_written: monitor_filter.rcr_written,
        rcr_after: monitor_filter.rcr_after,
        rxfltmap2_written: monitor_filter.rxfltmap2_written,
        rxfltmap2_after: monitor_filter.rxfltmap2_after,
        register_writes: 1 + monitor_filter.register_writes,
        counters: counters.saturating_sub(before_counters),
    })
}

pub fn run_rtl8812au_monitor_receive_filter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<RuntimeMonitorOpmodeExecution, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_counters = *counters;
    let msr_before = read8_with_counter(
        registers,
        counters,
        REG_MSR,
        "REG_MSR",
        "pre-monitor-filter",
    )?;
    let monitor_filter =
        run_monitor_receive_filter_registers(registers, counters, "post-monitor-filter")?;

    Ok(RuntimeMonitorOpmodeExecution {
        msr_before,
        msr_written: msr_before,
        msr_after: msr_before,
        rcr_written: monitor_filter.rcr_written,
        rcr_after: monitor_filter.rcr_after,
        rxfltmap2_written: monitor_filter.rxfltmap2_written,
        rxfltmap2_after: monitor_filter.rxfltmap2_after,
        register_writes: monitor_filter.register_writes,
        counters: counters.saturating_sub(before_counters),
    })
}

fn run_monitor_receive_filter_registers<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    phase: &'static str,
) -> Result<MonitorReceiveFilterExecution, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    write32_with_counter(
        registers,
        counters,
        REG_RCR,
        MONITOR_RECEIVE_CONFIG,
        "REG_RCR",
        "monitor receive config",
    )?;
    let rcr_after = read32_with_counter(registers, counters, REG_RCR, "REG_RCR", phase)?;

    write16_with_counter(
        registers,
        counters,
        REG_RXFLTMAP2,
        u16::MAX,
        "REG_RXFLTMAP2",
        "monitor receive map",
    )?;
    let rxfltmap2_after =
        read16_with_counter(registers, counters, REG_RXFLTMAP2, "REG_RXFLTMAP2", phase)?;

    Ok(MonitorReceiveFilterExecution {
        rcr_written: MONITOR_RECEIVE_CONFIG,
        rcr_after,
        rxfltmap2_written: u16::MAX,
        rxfltmap2_after,
        register_writes: 2,
    })
}

pub fn rtl8812au_efuse_logical_mac_address(logical_map: &[u8]) -> Option<[u8; 6]> {
    let mac = logical_map.get(RTL8812AU_EFUSE_MAC_OFFSET..RTL8812AU_EFUSE_MAC_OFFSET + 6)?;
    if mac.iter().all(|byte| *byte == 0xff) || mac.iter().all(|byte| *byte == 0x00) {
        None
    } else {
        Some([mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]])
    }
}

pub fn rtl8812au_decode_efuse_logical_map(raw: &[u8]) -> Vec<u8> {
    let mut logical_map = vec![0xff; RTL8812AU_EFUSE_LOGICAL_MAP_LEN];
    let mut raw_offset = 0usize;

    while raw_offset < raw.len() {
        let header = raw[raw_offset];
        raw_offset += 1;
        if header == 0xff {
            break;
        }

        let (section, word_enable) = if efuse_is_extended_header(header) {
            let offset_low = (header & 0xe0) >> 5;
            if raw_offset >= raw.len() {
                break;
            }
            let ext = raw[raw_offset];
            raw_offset += 1;
            if efuse_all_words_disabled(ext) {
                continue;
            }
            (offset_low | ((ext & 0xf0) >> 1), ext & 0x0f)
        } else {
            ((header >> 4) & 0x0f, header & 0x0f)
        };

        let data_len = efuse_word_count(word_enable) * 2;
        if section < RTL8812AU_EFUSE_MAX_SECTION {
            let mut target = usize::from(section) * 8;
            for word in 0..4 {
                if word_enable & (1 << word) == 0 {
                    if raw_offset + 1 >= raw.len() || target + 1 >= logical_map.len() {
                        raw_offset = raw.len();
                        break;
                    }
                    logical_map[target] = raw[raw_offset];
                    logical_map[target + 1] = raw[raw_offset + 1];
                    raw_offset += 2;
                }
                target += 2;
            }
        } else {
            raw_offset = raw_offset.saturating_add(data_len).min(raw.len());
        }
    }

    logical_map
}

fn efuse_is_extended_header(header: u8) -> bool {
    header & 0x1f == 0x0f
}

fn efuse_all_words_disabled(word_enable: u8) -> bool {
    word_enable & 0x0f == 0x0f
}

fn efuse_word_count(word_enable: u8) -> usize {
    (0..4).filter(|word| word_enable & (1 << word) == 0).count()
}

pub fn read_rtl8812au_efuse_mac_address<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
) -> Result<Option<[u8; 6]>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    read_rtl8812au_efuse_mac_address_with_config(
        registers,
        counters,
        RuntimeEfuseReadConfig::default(),
    )
}

pub fn read_rtl8812au_efuse_mac_address_with_config<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    config: RuntimeEfuseReadConfig,
) -> Result<Option<[u8; 6]>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let raw = read_rtl8812au_efuse_physical_with_config(registers, counters, config)?;
    let logical_map = rtl8812au_decode_efuse_logical_map(&raw);
    Ok(rtl8812au_efuse_logical_mac_address(&logical_map))
}

pub fn read_rtl8812au_efuse_physical_with_config<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    config: RuntimeEfuseReadConfig,
) -> Result<Vec<u8>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    if config.length > RTL8812AU_EFUSE_REAL_CONTENT_LEN {
        return Err(RuntimeRadioError::new(
            "efuse_read_invalid_length",
            format!(
                "EFUSE read length must be in 0..={RTL8812AU_EFUSE_REAL_CONTENT_LEN}; got {}",
                config.length
            ),
        ));
    }

    efuse_power_switch_read(registers, counters, true)?;
    let mut raw = Vec::with_capacity(config.length);
    let mut read_error = None;
    for address in 0..config.length {
        match efuse_read_byte(
            registers,
            counters,
            address as u16,
            config.poll_attempts,
            config.poll_delay,
        ) {
            Ok(byte) => raw.push(byte),
            Err(error) => {
                read_error = Some(error);
                break;
            }
        }
    }
    let power_off = efuse_power_switch_read(registers, counters, false);
    if let Some(error) = read_error {
        let _ = power_off;
        Err(error)
    } else {
        power_off?;
        Ok(raw)
    }
}

fn efuse_power_switch_read<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    enabled: bool,
) -> Result<(), RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let grant = if enabled {
        EFUSE_ACCESS_ON_JAGUAR
    } else {
        EFUSE_ACCESS_OFF_JAGUAR
    };
    write8_with_custom_error(
        registers,
        counters,
        REG_EFUSE_BURN_GNT_8812,
        grant,
        |error| {
            RuntimeRadioError::new(
                "efuse_power_switch_failed",
                format!("write REG_EFUSE_BURN_GNT_8812=0x{grant:02x} failed: {error}",),
            )
        },
    )?;

    if enabled {
        let _sys_iso = read16_with_custom_error(registers, counters, REG_SYS_ISO_CTRL, |error| {
            RuntimeRadioError::new(
                "efuse_power_switch_failed",
                format!("read REG_SYS_ISO_CTRL failed: {error}"),
            )
        })?;

        let sys_func = read16_with_custom_error(registers, counters, REG_SYS_FUNC_EN, |error| {
            RuntimeRadioError::new(
                "efuse_power_switch_failed",
                format!("read REG_SYS_FUNC_EN failed: {error}"),
            )
        })?;
        if sys_func & FEN_ELDR == 0 {
            write16_with_custom_error(
                registers,
                counters,
                REG_SYS_FUNC_EN,
                sys_func | FEN_ELDR,
                |error| {
                    RuntimeRadioError::new(
                        "efuse_power_switch_failed",
                        format!("enable FEN_ELDR failed: {error}"),
                    )
                },
            )?;
        }

        let sys_clkr = read16_with_custom_error(registers, counters, REG_SYS_CLKR, |error| {
            RuntimeRadioError::new(
                "efuse_power_switch_failed",
                format!("read REG_SYS_CLKR failed: {error}"),
            )
        })?;
        let required = LOADER_CLK_EN | ANA8M;
        if sys_clkr & required != required {
            write16_with_custom_error(
                registers,
                counters,
                REG_SYS_CLKR,
                sys_clkr | required,
                |error| {
                    RuntimeRadioError::new(
                        "efuse_power_switch_failed",
                        format!("enable EFUSE loader clock failed: {error}"),
                    )
                },
            )?;
        }
    }

    Ok(())
}

fn efuse_read_byte<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    address: u16,
    poll_attempts: u32,
    poll_delay: Duration,
) -> Result<u8, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    write8_with_custom_error(
        registers,
        counters,
        REG_EFUSE_CTRL + 1,
        (address & 0x00ff) as u8,
        |error| {
            RuntimeRadioError::new(
                "efuse_read_failed",
                format!("write EFUSE address low byte for {address:#05x} failed: {error}"),
            )
        },
    )?;
    let high = read8_with_custom_error(registers, counters, REG_EFUSE_CTRL + 2, |error| {
        RuntimeRadioError::new(
            "efuse_read_failed",
            format!("read EFUSE address high latch for {address:#05x} failed: {error}"),
        )
    })?;
    write8_with_custom_error(
        registers,
        counters,
        REG_EFUSE_CTRL + 2,
        (((address >> 8) & 0x03) as u8) | (high & 0xfc),
        |error| {
            RuntimeRadioError::new(
                "efuse_read_failed",
                format!("write EFUSE address high byte for {address:#05x} failed: {error}"),
            )
        },
    )?;

    let command = read8_with_custom_error(registers, counters, REG_EFUSE_CTRL + 3, |error| {
        RuntimeRadioError::new(
            "efuse_read_failed",
            format!("read EFUSE command latch for {address:#05x} failed: {error}"),
        )
    })?;
    write8_with_custom_error(
        registers,
        counters,
        REG_EFUSE_CTRL + 3,
        command & 0x7f,
        |error| {
            RuntimeRadioError::new(
                "efuse_read_failed",
                format!("start EFUSE read for {address:#05x} failed: {error}"),
            )
        },
    )?;

    for attempt in 1..=poll_attempts {
        let status = read8_with_custom_error(registers, counters, REG_EFUSE_CTRL + 3, |error| {
            RuntimeRadioError::new(
                "efuse_read_failed",
                format!("poll EFUSE ready for {address:#05x} failed: {error}"),
            )
        })?;
        if status & 0x80 != 0 {
            return read8_with_custom_error(registers, counters, REG_EFUSE_CTRL, |error| {
                RuntimeRadioError::new(
                    "efuse_read_failed",
                    format!("read EFUSE data byte for {address:#05x} failed: {error}"),
                )
            });
        }
        if attempt < poll_attempts {
            std::thread::sleep(poll_delay);
        }
    }

    let status = read8_with_counter(
        registers,
        counters,
        REG_EFUSE_CTRL + 3,
        "REG_EFUSE_CTRL+3",
        "timeout-status",
    )
    .unwrap_or_default();
    Err(RuntimeRadioError::new(
        "efuse_read_timeout",
        format!(
            "EFUSE byte {address:#05x} did not become ready after {poll_attempts} polls; REG_EFUSE_CTRL+3=0x{status:02x}",
        ),
    ))
}

pub fn read_rtl8812au_macid<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    phase: &'static str,
) -> Result<[u8; 6], RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut mac = [0u8; 6];
    for (offset, value) in mac.iter_mut().enumerate() {
        let address = REG_MACID + offset as u16;
        *value = read8_with_custom_error(registers, counters, address, |error| {
            RuntimeRadioError::new(
                "register_read_failed",
                format!(
                    "REG_MACID {phase} byte read failed at {}: {error}",
                    format_register_address(address)
                ),
            )
        })?;
    }
    Ok(mac)
}

pub fn program_rtl8812au_local_mac<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    local_mac: [u8; 6],
    counters: &mut RuntimeRadioCounters,
) -> Result<RuntimeMacAddressExecution, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let before_counters = *counters;
    let before = read_rtl8812au_macid(registers, counters, "pre-local-MAC")?;
    let mut register_writes = 0usize;
    for (offset, value) in local_mac.iter().copied().enumerate() {
        let address = REG_MACID + offset as u16;
        write8_with_custom_error(registers, counters, address, value, |error| {
            RuntimeRadioError::new(
                "register_write_failed",
                format!(
                    "REG_MACID local MAC byte write failed at {}: {error}",
                    format_register_address(address)
                ),
            )
        })?;
        register_writes += 1;
    }
    let after = read_rtl8812au_macid(registers, counters, "post-local-MAC")?;

    Ok(RuntimeMacAddressExecution {
        before,
        written: local_mac,
        after,
        register_writes,
        counters: counters.saturating_sub(before_counters),
    })
}

pub fn macos_usbhost_bulk_out_endpoints(
    bulk_out_endpoint_count: usize,
) -> Result<Vec<u8>, RuntimeTransportError> {
    match bulk_out_endpoint_count {
        2 => Ok(vec![0x02, 0x03]),
        3 => Ok(vec![0x02, 0x03, 0x04]),
        4 => Ok(vec![0x02, 0x03, 0x04, 0x05]),
        other => Err(RuntimeTransportError::new(
            "unsupported_bulk_out_endpoint_count",
            format!(
                "queue/DMA setup supports 2, 3, or 4 macOS bulk OUT endpoints, configured {other}"
            ),
        )),
    }
}

pub fn macos_usbhost_endpoints(
    config: &MacosUsbHostConfig,
) -> Result<UsbEndpoints, RuntimeTransportError> {
    if config.bulk_in_endpoint & 0x80 == 0 {
        return Err(RuntimeTransportError::new(
            "invalid_macos_bulk_in_endpoint",
            format!(
                "macOS bulk IN endpoint must have the USB IN direction bit set, got 0x{:02x}",
                config.bulk_in_endpoint
            ),
        ));
    }
    if config.bulk_out_endpoint & 0x80 != 0 {
        return Err(RuntimeTransportError::new(
            "invalid_macos_bulk_out_endpoint",
            format!(
                "macOS bulk OUT endpoint must not have the USB IN direction bit set, got 0x{:02x}",
                config.bulk_out_endpoint
            ),
        ));
    }

    let bulk_out_all = macos_usbhost_bulk_out_endpoints(config.bulk_out_endpoint_count)?;
    if !bulk_out_all.contains(&config.bulk_out_endpoint) {
        return Err(RuntimeTransportError::new(
            "macos_bulk_out_endpoint_not_in_layout",
            format!(
                "selected macOS bulk OUT endpoint 0x{:02x} is not in the derived RTL8812AU endpoint layout {:?}",
                config.bulk_out_endpoint, bulk_out_all
            ),
        ));
    }

    Ok(UsbEndpoints {
        interface_number: config.interface_number,
        bulk_in: Some(config.bulk_in_endpoint),
        bulk_out: Some(config.bulk_out_endpoint),
        bulk_in_all: vec![config.bulk_in_endpoint],
        bulk_out_all,
    })
}

pub fn macos_usbhost_adapter_info(vid: u16, pid: u16, endpoints: &UsbEndpoints) -> UsbDeviceInfo {
    let mut endpoint_infos = Vec::with_capacity(1 + endpoints.bulk_out_all.len());
    if let Some(bulk_in) = endpoints.bulk_in {
        endpoint_infos.push(EndpointInfo {
            address: bulk_in,
            direction: "in".to_string(),
            transfer_type: "bulk".to_string(),
            max_packet_size: 512,
            interval: 0,
        });
    }
    for bulk_out in &endpoints.bulk_out_all {
        endpoint_infos.push(EndpointInfo {
            address: *bulk_out,
            direction: "out".to_string(),
            transfer_type: "bulk".to_string(),
            max_packet_size: 512,
            interval: 0,
        });
    }

    UsbDeviceInfo {
        vid,
        pid,
        vid_hex: format!("0x{vid:04x}"),
        pid_hex: format!("0x{pid:04x}"),
        bus: 0,
        address: 0,
        speed: "high-speed (IOUSBHost)".to_string(),
        class_code: 0,
        sub_class_code: 0,
        protocol_code: 0,
        manufacturer: None,
        product: Some("RTL8812AU via macOS IOUSBHost".to_string()),
        serial_number: None,
        known_adapter: radio_core::lookup_known_adapter(vid, pid),
        interfaces: vec![InterfaceInfo {
            number: endpoints.interface_number,
            setting_number: 0,
            class_code: 0xff,
            sub_class_code: 0xff,
            protocol_code: 0xff,
            endpoints: endpoint_infos,
        }],
    }
}

pub fn open_macos_usbhost_transport(
    config: &MacosUsbHostConfig,
    selector: DeviceSelector,
) -> Result<RuntimeUsbTransportOpen, RuntimeTransportError> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (config, selector);
        Err(RuntimeTransportError::new(
            "unsupported_platform",
            "macOS IOUSBHost transport requires macOS",
        ))
    }

    #[cfg(target_os = "macos")]
    {
        if selector.bus.is_some() || selector.address.is_some() {
            return Err(RuntimeTransportError::new(
                "unsupported_macos_selector_location",
                "macOS IOUSBHost transport cannot yet select by USB bus/address; use --vid and --pid",
            ));
        }
        let vid = selector.vid.ok_or_else(|| {
            RuntimeTransportError::new(
                "missing_vid",
                "macOS IOUSBHost transport requires --vid because matching is VID/PID based",
            )
        })?;
        let pid = selector.pid.ok_or_else(|| {
            RuntimeTransportError::new(
                "missing_pid",
                "macOS IOUSBHost transport requires --pid because matching is VID/PID based",
            )
        })?;
        if radio_core::lookup_known_adapter(vid, pid).is_none() {
            return Err(RuntimeTransportError::new(
                "unsupported_adapter",
                format!(
                    "USB device 0x{vid:04x}:0x{pid:04x} is not registered as a supported RTL8812AU adapter"
                ),
            ));
        }

        let endpoints = macos_usbhost_endpoints(config)?;
        let session = macos_usbhost::MacosUsbHostSession::open(
            macos_usbhost::MacosUsbHostSessionOpenRequest {
                vid,
                pid,
                configuration_value: config.configuration_value,
                match_interfaces: true,
                interface_number: config.interface_number,
                bulk_in_endpoint: config.bulk_in_endpoint,
                bulk_out_endpoint: config.bulk_out_endpoint,
                poll_attempts: config.poll_attempts,
                poll_delay: config.poll_delay,
            },
        )
        .map_err(|error| RuntimeTransportError::new("macos_session_open_failed", error))?;
        let initial_usb_control_writes = u64::from(session.interface_probe.configure_attempted);
        let adapter = macos_usbhost_adapter_info(vid, pid, &endpoints);
        Ok(RuntimeUsbTransportOpen {
            transport: RuntimeUsbTransport::Macos(session),
            adapter,
            endpoints,
            initial_usb_control_writes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TxCalibrationProfile {
    CurrentDefault,
    LinuxParityCh36Ht20,
    Rtl8812aLck,
    Rtl8812aIqkProbe,
    Rtl8812aRuntimeIqk,
}

impl TxCalibrationProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::CurrentDefault => "current-default",
            Self::LinuxParityCh36Ht20 => "linux-parity-ch36-ht20",
            Self::Rtl8812aLck => "rtl8812a-lck",
            Self::Rtl8812aIqkProbe => "rtl8812a-iqk-probe",
            Self::Rtl8812aRuntimeIqk => "rtl8812a-runtime-iqk",
        }
    }

    pub fn is_default(self) -> bool {
        matches!(self, Self::CurrentDefault)
    }

    pub fn requires_register_write_authorization(self) -> bool {
        matches!(
            self,
            Self::LinuxParityCh36Ht20 | Self::Rtl8812aLck | Self::Rtl8812aRuntimeIqk
        )
    }

    pub fn is_runtime_calibration(self) -> bool {
        matches!(self, Self::Rtl8812aLck | Self::Rtl8812aRuntimeIqk)
    }

    pub fn before_tx_class(self, captured_tail_applied: bool) -> TxCalibrationClass {
        match self {
            Self::LinuxParityCh36Ht20 => TxCalibrationClass::TargetedLinuxParity,
            Self::Rtl8812aLck | Self::Rtl8812aRuntimeIqk => {
                TxCalibrationClass::RuntimeApproximation
            }
            Self::CurrentDefault | Self::Rtl8812aIqkProbe if captured_tail_applied => {
                TxCalibrationClass::StopGapCaptured
            }
            Self::CurrentDefault | Self::Rtl8812aIqkProbe => TxCalibrationClass::Unknown,
        }
    }

    pub fn evidence_source(
        self,
        captured_tail_applied: bool,
    ) -> RuntimeTxCalibrationEvidenceSource {
        match self {
            Self::CurrentDefault if captured_tail_applied => {
                RuntimeTxCalibrationEvidenceSource::CapturedLinuxTail
            }
            Self::CurrentDefault => RuntimeTxCalibrationEvidenceSource::Default,
            Self::LinuxParityCh36Ht20 => {
                RuntimeTxCalibrationEvidenceSource::TargetedLinuxParityCapture
            }
            Self::Rtl8812aLck => RuntimeTxCalibrationEvidenceSource::RuntimeLck,
            Self::Rtl8812aIqkProbe => RuntimeTxCalibrationEvidenceSource::ReadOnlyIqkProbe,
            Self::Rtl8812aRuntimeIqk => RuntimeTxCalibrationEvidenceSource::RuntimeIqk,
        }
    }

    pub fn validation_status(self) -> RuntimeTxCalibrationValidationStatus {
        if self.is_default() {
            RuntimeTxCalibrationValidationStatus::NotRequired
        } else {
            RuntimeTxCalibrationValidationStatus::ReceiverBackedValidationRequired
        }
    }

    pub fn calibration_decision(
        self,
        captured_tail_applied: bool,
        authorized: bool,
    ) -> Result<RuntimeTxCalibrationDecision, RuntimeRadioError> {
        let requires_live_write_authorization = self.requires_register_write_authorization();
        if requires_live_write_authorization && !authorized {
            return Err(RuntimeRadioError::new(
                "missing_write_authorization",
                format!(
                    "tx calibration profile {} writes live RTL8812A BB/RF calibration registers and requires hardware-write authorization",
                    self.name()
                ),
            ));
        }
        Ok(RuntimeTxCalibrationDecision {
            profile: self,
            class: self.before_tx_class(captured_tail_applied),
            evidence_source: self.evidence_source(captured_tail_applied),
            requires_live_write_authorization,
            authorized,
            validation_status: self.validation_status(),
            production_safe_default: self.is_default(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TxCalibrationClass {
    Unknown,
    StopGapCaptured,
    TargetedLinuxParity,
    RuntimeApproximation,
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        collections::BTreeMap,
        net::{SocketAddr, UdpSocket},
        path::PathBuf,
        time::Duration,
    };

    use radio_core::{
        rtl8812au::{Rtl8812auUsbTransport, TxQueue},
        Band, Bandwidth, Channel, DeviceSelector, FirmwareImage, FirmwareSource, ParsedRxPacket,
        RealtekTableKind, RealtekTablePlan, Rtl8812auRegisterAccess, RxFrame, RxParseOutcome,
        TxOptions, TxSubmitCounters, UsbBulkTransfer, UsbEndpoints, UsbError,
    };

    use super::{
        bind_production_tx_ingress_sockets, create_production_rx_forward_runtimes,
        handle_production_bridge_tx_datagram, macos_usbhost_adapter_info, macos_usbhost_endpoints,
        plan_production_wfb_loop, process_production_rx_packet_outcomes,
        production_rx_forward_snapshots, run_production_bridge_loop, run_production_runtime_flow,
        run_production_runtime_flow_with_session, runtime_unix_ms,
        spawn_production_tx_ingress_receivers, write_production_runtime_ready_marker,
        write_production_runtime_service_health, MacosUsbHostConfig, ProductionRuntimeAirtimeMode,
        ProductionRuntimeAirtimeReport, ProductionRuntimeAirtimeSchedule,
        ProductionRuntimeBridgeLoopRunConfig, ProductionRuntimeBridgeLoopStep,
        ProductionRuntimeBridgeLoopStepOutcome, ProductionRuntimeBridgeLoopStopReason,
        ProductionRuntimeBridgeTxConfig, ProductionRuntimeBridgeTxOverrides,
        ProductionRuntimeBridgeTxProfile, ProductionRuntimeFlowConfig,
        ProductionRuntimeFlowExecutionInputs, ProductionRuntimeFlowReport,
        ProductionRuntimeFlowResult, ProductionRuntimeHeartbeatLedReport,
        ProductionRuntimeInitReadiness, ProductionRuntimeInitTelemetry,
        ProductionRuntimePrimaryRxForwardConfig, ProductionRuntimeQueuedDatagram,
        ProductionRuntimeReadyMarker, ProductionRuntimeRtl8812auInitInputs,
        ProductionRuntimeRxForwardConfig, ProductionRuntimeRxForwardPlan,
        ProductionRuntimeRxForwardSnapshot, ProductionRuntimeServiceHealth,
        ProductionRuntimeServiceLifecycle, ProductionRuntimeServiceOperatorAction,
        ProductionRuntimeTddWindow, ProductionRuntimeUsbConfig, ProductionRuntimeWfbLoopConfig,
        Rtl8812auInitOrder, Rtl8812auInitPhase, Rtl8812auTxPowerControlMode,
        RuntimeFlowRxTelemetry, RuntimeFlowTxTelemetry, RuntimeRadioCounters, RuntimeRadioError,
        RuntimeRadioSession, RuntimeSameSessionInitConfig, RuntimeSameSessionInitPhaseFailure,
        RuntimeSameSessionInitPhaseStatus, RuntimeSameSessionInitPhaseSummary,
        RuntimeSameSessionInitReadiness, RuntimeTxCalibrationEvidenceSource,
        RuntimeTxCalibrationValidationStatus, TxCalibrationClass, TxCalibrationProfile,
        DEFAULT_HEARTBEAT_HALF_PERIOD_MS, PRODUCTION_TX_SOCKET_RCVBUF_BYTES,
    };

    use wfb_bridge::{
        build_wfb_data_header, RxCounters, RxForwardConfig, TxCounters, WfbChannelId,
    };

    #[derive(Debug, Default)]
    struct MockTransport {
        registers: RefCell<BTreeMap<u16, Vec<u8>>>,
        efuse: RefCell<Option<Vec<u8>>>,
        writes: RefCell<Vec<(u16, Vec<u8>)>>,
        bulk_reads: Vec<Vec<u8>>,
        bulk_writes: Vec<(u8, Vec<u8>)>,
        bulk_write_len: Option<usize>,
    }

    impl MockTransport {
        fn with_u8(address: u16, value: u8) -> Self {
            let transport = Self::default();
            transport
                .registers
                .borrow_mut()
                .insert(address, vec![value]);
            transport
        }

        fn with_u32(address: u16, value: u32) -> Self {
            let transport = Self::default();
            transport
                .registers
                .borrow_mut()
                .insert(address, value.to_le_bytes().to_vec());
            transport
        }

        fn insert_u32(&self, address: u16, value: u32) {
            self.registers
                .borrow_mut()
                .insert(address, value.to_le_bytes().to_vec());
        }

        fn insert_u8(&self, address: u16, value: u8) {
            self.registers.borrow_mut().insert(address, vec![value]);
        }

        fn with_macid(mac: [u8; 6]) -> Self {
            let transport = Self::default();
            for (offset, value) in mac.into_iter().enumerate() {
                transport
                    .registers
                    .borrow_mut()
                    .insert(super::REG_MACID + offset as u16, vec![value]);
            }
            transport
        }

        fn with_efuse(raw: Vec<u8>) -> Self {
            Self {
                efuse: RefCell::new(Some(raw)),
                ..Self::default()
            }
        }

        fn with_bulk_read(data: Vec<u8>) -> Self {
            Self {
                bulk_reads: vec![data],
                ..Self::default()
            }
        }

        fn with_short_bulk_write(written: usize) -> Self {
            Self {
                bulk_write_len: Some(written),
                ..Self::default()
            }
        }

        fn writes(&self) -> Vec<(u16, Vec<u8>)> {
            self.writes.borrow().clone()
        }

        fn register_bytes(&self, address: u16) -> Option<Vec<u8>> {
            self.registers.borrow().get(&address).cloned()
        }
    }

    impl Rtl8812auUsbTransport for &MockTransport {
        fn read_vendor(
            &self,
            value: u16,
            _index: u16,
            data: &mut [u8],
            _timeout: Duration,
        ) -> std::result::Result<usize, UsbError> {
            data.fill(0);
            if self.efuse.borrow().is_some() && data.len() == 1 {
                if value == super::REG_EFUSE_CTRL + 3 {
                    data[0] = 0x80;
                    return Ok(data.len());
                }
                if value == super::REG_EFUSE_CTRL {
                    let low = self
                        .registers
                        .borrow()
                        .get(&(super::REG_EFUSE_CTRL + 1))
                        .and_then(|bytes| bytes.first().copied())
                        .unwrap_or(0);
                    let high = self
                        .registers
                        .borrow()
                        .get(&(super::REG_EFUSE_CTRL + 2))
                        .and_then(|bytes| bytes.first().copied())
                        .unwrap_or(0)
                        & 0x03;
                    let address = (usize::from(high) << 8) | usize::from(low);
                    data[0] = self
                        .efuse
                        .borrow()
                        .as_ref()
                        .and_then(|raw| raw.get(address).copied())
                        .unwrap_or(0xff);
                    return Ok(data.len());
                }
            }
            if let Some(stored) = self.registers.borrow().get(&value) {
                let len = data.len().min(stored.len());
                data[..len].copy_from_slice(&stored[..len]);
            }
            Ok(data.len())
        }

        fn write_vendor(
            &self,
            value: u16,
            _index: u16,
            data: &[u8],
            _timeout: Duration,
        ) -> std::result::Result<usize, UsbError> {
            self.registers.borrow_mut().insert(value, data.to_vec());
            self.writes.borrow_mut().push((value, data.to_vec()));
            Ok(data.len())
        }
    }

    impl UsbBulkTransfer for MockTransport {
        fn read_bulk_transfer(
            &mut self,
            _endpoint: u8,
            data: &mut [u8],
            _timeout: Duration,
        ) -> std::result::Result<usize, UsbError> {
            if self.bulk_reads.is_empty() {
                return Ok(0);
            }
            let read = self.bulk_reads.remove(0);
            let len = read.len().min(data.len());
            data[..len].copy_from_slice(&read[..len]);
            Ok(len)
        }

        fn write_bulk_transfer(
            &mut self,
            endpoint: u8,
            data: &[u8],
            _timeout: Duration,
        ) -> std::result::Result<usize, UsbError> {
            let written = self.bulk_write_len.unwrap_or(data.len()).min(data.len());
            self.bulk_writes.push((endpoint, data[..written].to_vec()));
            Ok(written)
        }
    }

    #[derive(Debug)]
    struct TransientReadbackTransport {
        value: RefCell<u8>,
        stale_next_read: RefCell<Option<u8>>,
        stale_after_write: u8,
    }

    impl TransientReadbackTransport {
        fn new(initial: u8, stale_after_write: u8) -> Self {
            Self {
                value: RefCell::new(initial),
                stale_next_read: RefCell::new(None),
                stale_after_write,
            }
        }
    }

    impl Rtl8812auUsbTransport for &TransientReadbackTransport {
        fn read_vendor(
            &self,
            _value: u16,
            _index: u16,
            data: &mut [u8],
            _timeout: Duration,
        ) -> std::result::Result<usize, UsbError> {
            data.fill(0);
            if !data.is_empty() {
                data[0] = self
                    .stale_next_read
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| *self.value.borrow());
            }
            Ok(data.len())
        }

        fn write_vendor(
            &self,
            _value: u16,
            _index: u16,
            data: &[u8],
            _timeout: Duration,
        ) -> std::result::Result<usize, UsbError> {
            if let Some(value) = data.first() {
                *self.value.borrow_mut() = *value;
                *self.stale_next_read.borrow_mut() = Some(self.stale_after_write);
            }
            Ok(data.len())
        }
    }

    #[test]
    fn production_masked_readback_retries_transient_stale_value() {
        let transport = TransientReadbackTransport::new(0x20, 0x20);
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        super::production_rmw8_preserve(
            &registers,
            &mut counters,
            "REG_RSV_CTRL",
            super::REG_RSV_CTRL,
            !(super::BIT5 | super::BIT6),
            super::BIT5 | super::BIT6,
            super::BIT5 | super::BIT6,
            "test",
        )
        .expect("transient stale readback should retry");

        assert_eq!(*transport.value.borrow(), super::BIT5 | super::BIT6);
        assert_eq!(counters.usb_control_writes, 1);
        assert_eq!(counters.usb_control_reads, 3);
    }

    #[test]
    fn calibration_profiles_mark_live_register_write_authorization() {
        assert!(TxCalibrationProfile::LinuxParityCh36Ht20.requires_register_write_authorization());
        assert!(TxCalibrationProfile::Rtl8812aLck.requires_register_write_authorization());
        assert!(TxCalibrationProfile::Rtl8812aRuntimeIqk.requires_register_write_authorization());

        for profile in [
            TxCalibrationProfile::CurrentDefault,
            TxCalibrationProfile::Rtl8812aIqkProbe,
        ] {
            assert!(
                !profile.requires_register_write_authorization(),
                "{} should not require live write authorization",
                profile.name()
            );
        }
    }

    #[test]
    fn calibration_decision_labels_evidence_and_validation() {
        let decision = TxCalibrationProfile::LinuxParityCh36Ht20
            .calibration_decision(false, true)
            .expect("authorized profile");
        assert_eq!(decision.class, TxCalibrationClass::TargetedLinuxParity);
        assert_eq!(
            decision.evidence_source,
            RuntimeTxCalibrationEvidenceSource::TargetedLinuxParityCapture
        );
        assert_eq!(
            decision.validation_status,
            RuntimeTxCalibrationValidationStatus::ReceiverBackedValidationRequired
        );
        assert!(decision.requires_live_write_authorization);
        assert!(decision.authorized);
        assert!(!decision.production_safe_default);

        let default_decision = TxCalibrationProfile::CurrentDefault
            .calibration_decision(true, false)
            .expect("default profile");
        assert_eq!(default_decision.class, TxCalibrationClass::StopGapCaptured);
        assert_eq!(
            default_decision.evidence_source,
            RuntimeTxCalibrationEvidenceSource::CapturedLinuxTail
        );
        assert_eq!(
            default_decision.validation_status,
            RuntimeTxCalibrationValidationStatus::NotRequired
        );
        assert!(!default_decision.requires_live_write_authorization);
        assert!(default_decision.production_safe_default);
    }

    #[test]
    fn calibration_decision_rejects_unauthorized_live_writes() {
        let error = TxCalibrationProfile::Rtl8812aLck
            .calibration_decision(false, false)
            .expect_err("unauthorized LCK should fail");
        assert_eq!(error.code, "missing_write_authorization");
        assert!(error.message.contains("rtl8812a-lck"));
    }

    #[test]
    fn rtl8812au_lck_calibration_runs_runtime_rf_sequence() {
        let transport = MockTransport::with_u32(super::REG_SINGLE_TONE_CONT_TX_JAGUAR, 0);
        transport.insert_u8(super::REG_TXPAUSE, 0x00);
        transport.insert_u32(super::REG_RF_PI_MODE_A_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_HSSI_READ_JAGUAR, 0);
        transport.insert_u32(super::REG_RF_PI_READ_A_JAGUAR, 0x0001_2345);
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let report = super::run_rtl8812au_lck_calibration(&registers, &mut counters)
            .expect("runtime LCK report");

        assert_eq!(report.rf_path, super::Rtl8812auRfPath::A);
        assert_eq!(report.rf_path_name, "A");
        assert!(!report.continuous_tx_active);
        assert_eq!(report.tx_pause_before.value, 0);
        assert!(report.tx_pause_write.is_some());
        assert!(report.tx_pause_restore.is_some());
        assert_eq!(
            report.rf_lck_enter_write.value,
            0x0001_2345 | super::RF_LCK_MODE_BIT
        );
        assert_eq!(
            report.rf_chnlbw_trigger_write.value,
            0x0001_2345 | super::RF_CHNLBW_LCK_TRIGGER_BIT
        );
        assert_eq!(
            report.rf_lck_exit_write.value,
            0x0001_2345 & !super::RF_LCK_MODE_BIT
        );
        assert_eq!(report.counters, counters);
        assert!(counters.usb_control_reads > 0);
        assert!(counters.usb_control_writes > 0);

        let writes = transport.writes();
        assert!(writes
            .iter()
            .any(|(address, bytes)| *address == super::REG_TXPAUSE && bytes.as_slice() == [0xff]));
        assert!(writes
            .iter()
            .any(|(address, bytes)| *address == super::REG_TXPAUSE && bytes.as_slice() == [0x00]));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_RF_PATH_A_3WIRE
                && bytes.as_slice()
                    == super::encode_rf_serial_write(
                        super::RF_LCK_JAGUAR,
                        0x0001_2345 | super::RF_LCK_MODE_BIT,
                    )
                    .to_le_bytes()
        }));
    }

    #[test]
    fn targeted_linux_parity_profile_runs_runtime_register_writes() {
        let channel = Channel::from_number(36).expect("channel 36");
        let specs = super::rtl8812au_targeted_calibration_writes(
            TxCalibrationProfile::LinuxParityCh36Ht20,
            channel,
            Bandwidth::Mhz20,
        )
        .expect("targeted specs")
        .expect("writes");
        assert_eq!(specs.len(), 6);
        assert_eq!(specs[0].address, super::REG_TX_SCALE_A_JAGUAR);
        assert_eq!(specs[0].value, 0x4000_0003);
        assert_eq!(specs[2].address, super::REG_RFE_PINMUX_A_JAGUAR);
        assert_eq!(specs[2].value, 0x5433_7770);
        assert!(super::rtl8812au_targeted_calibration_writes(
            TxCalibrationProfile::LinuxParityCh36Ht20,
            channel,
            Bandwidth::Mhz40,
        )
        .is_err());

        let transport = MockTransport::default();
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();
        let reports = super::run_rtl8812au_targeted_calibration_profile(
            &registers,
            &mut counters,
            TxCalibrationProfile::LinuxParityCh36Ht20,
            channel,
            Bandwidth::Mhz20,
        )
        .expect("targeted profile")
        .expect("reports");

        assert_eq!(reports.len(), 6);
        assert_eq!(reports[0].written, 0x4000_0003);
        assert_eq!(reports[2].written, 0x5433_7770);
        assert_eq!(counters.usb_control_reads, 12);
        assert_eq!(counters.usb_control_writes, 6);
    }

    fn decode_hex_fixture(input: &str) -> Vec<u8> {
        let hex: Vec<u8> = input
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        assert_eq!(hex.len() % 2, 0, "fixture hex must have byte pairs");
        hex.chunks(2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).expect("high hex nibble");
                let low = (pair[1] as char).to_digit(16).expect("low hex nibble");
                ((high << 4) | low) as u8
            })
            .collect()
    }

    fn awus036ach_ch36_tx_power_fixture() -> Vec<u8> {
        decode_hex_fixture(include_str!(
            "../../../fixtures/rf-quality/awus036ach-ch36-efuse-tx-power.hex"
        ))
    }

    fn tx_power_plan_value(plan: &super::Rtl8812auTxPowerEfusePlanReport, address: u16) -> u32 {
        plan.writes
            .iter()
            .find(|write| write.address == address)
            .map(|write| write.value)
            .unwrap_or_else(|| panic!("missing TXAGC plan write for 0x{address:04x}"))
    }

    fn tx_power_plan_lane(
        plan: &super::Rtl8812auTxPowerEfusePlanReport,
        address: u16,
        lane: u8,
    ) -> &super::Rtl8812auTxPowerDerivedLaneReport {
        plan.writes
            .iter()
            .find(|write| write.address == address)
            .and_then(|write| write.lanes.iter().find(|entry| entry.lane == lane))
            .unwrap_or_else(|| panic!("missing TXAGC lane {lane} for 0x{address:04x}"))
    }

    #[test]
    fn rtl8812au_tx_power_agc_registers_select_path_sets() {
        let path_a = super::rtl8812au_tx_power_agc_registers(super::Rtl8812auRfPath::A);
        let path_b = super::rtl8812au_tx_power_agc_registers(super::Rtl8812auRfPath::B);
        let both = super::rtl8812au_tx_power_agc_registers(super::Rtl8812auRfPath::Both);

        assert_eq!(super::rtl8812au_tx_power_agc_value(0x1a), 0x1a1a_1a1a);
        assert_eq!(path_a.len(), 12);
        assert_eq!(path_b.len(), 12);
        assert_eq!(both.len(), 24);
        assert!(path_a.contains(&("rA_TxAGC_CCK", super::REG_TX_AGC_A_CCK_JAGUAR)));
        assert!(path_b.contains(&("rB_TxAGC_CCK", super::REG_TX_AGC_B_CCK_JAGUAR)));
        assert!(both.contains(&(
            "rA_TxAGC_OFDM18_OFDM6",
            super::REG_TX_AGC_A_OFDM18_OFDM6_JAGUAR
        )));
        assert!(both.contains(&(
            "rB_TxAGC_OFDM18_OFDM6",
            super::REG_TX_AGC_B_OFDM18_OFDM6_JAGUAR
        )));
    }

    #[test]
    fn rtl8812au_efuse_tx_power_plan_matches_linux_ch36_ht20_txagc() {
        let plan = super::plan_rtl8812au_efuse_tx_power(
            &awus036ach_ch36_tx_power_fixture(),
            Channel::from_number(36).expect("channel 36"),
            Bandwidth::Mhz20,
            super::Rtl8812auRfPath::Both,
            super::Rtl8812auTxPowerSafetyProfile::LinuxCh36Ht20,
            super::RTL8812AU_TX_POWER_INDEX_MAX,
        )
        .expect("TX power plan");

        assert_eq!(plan.channel_group.group, 0);
        assert_eq!(
            plan.programmed_paths,
            vec![super::Rtl8812auRfPath::A, super::Rtl8812auRfPath::B]
        );
        assert_eq!(plan.writes.len(), 22);
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_OFDM18_OFDM6_JAGUAR),
            0x1b1b_1b1b
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_OFDM54_OFDM24_JAGUAR),
            0x1b1b_1b1b
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_MCS3_MCS0_JAGUAR),
            0x1717_1717
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_MCS7_MCS4_JAGUAR),
            0x1717_1717
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_NSS1_7_NSS1_4_JAGUAR),
            0x1515_1515
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_NSS1_11_NSS1_8_JAGUAR),
            0x1515_1515
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_NSS1_3_NSS1_0_JAGUAR),
            0x1717_1717
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_NSS2_3_NSS2_0_JAGUAR),
            0x1717_1717
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_NSS2_7_NSS2_4_JAGUAR),
            0x1515_1717
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_NSS2_11_NSS2_8_JAGUAR),
            0x1515_1515
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_A_NSS3_3_NSS3_0_JAGUAR),
            0x1515_1515
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_OFDM18_OFDM6_JAGUAR),
            0x1d1d_1d1d
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_OFDM54_OFDM24_JAGUAR),
            0x1d1d_1d1d
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_MCS3_MCS0_JAGUAR),
            0x1c1c_1c1c
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_MCS7_MCS4_JAGUAR),
            0x1c1c_1c1c
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_NSS1_7_NSS1_4_JAGUAR),
            0x1a1a_1a1a
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_NSS1_11_NSS1_8_JAGUAR),
            0x1a1a_1a1a
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_NSS1_3_NSS1_0_JAGUAR),
            0x1c1c_1c1c
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_NSS2_3_NSS2_0_JAGUAR),
            0x1c1c_1c1c
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_NSS2_7_NSS2_4_JAGUAR),
            0x1a1a_1c1c
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_NSS2_11_NSS2_8_JAGUAR),
            0x1a1a_1a1a
        );
        assert_eq!(
            tx_power_plan_value(&plan, super::REG_TX_AGC_B_NSS3_3_NSS3_0_JAGUAR),
            0x1a1a_1a1a
        );

        let lane = tx_power_plan_lane(&plan, super::REG_TX_AGC_A_OFDM18_OFDM6_JAGUAR, 0);
        assert_eq!(lane.efuse_base_value, 0x29);
        assert_eq!(lane.efuse_diff_value, -2);
        assert_eq!(lane.by_rate_offset, 14);
        assert_eq!(lane.unclamped_index, 0x35);
        assert_eq!(lane.clamp_max_index, 0x1b);
        assert_eq!(lane.final_index, 0x1b);
        assert!(lane.clamped);
    }

    #[test]
    fn rtl8812au_efuse_tx_power_plan_rejects_2ghz_until_ported() {
        let error = super::plan_rtl8812au_efuse_tx_power(
            &awus036ach_ch36_tx_power_fixture(),
            Channel::from_number(6).expect("channel 6"),
            Bandwidth::Mhz20,
            super::Rtl8812auRfPath::Both,
            super::Rtl8812auTxPowerSafetyProfile::LinuxCh36Ht20,
            super::RTL8812AU_TX_POWER_INDEX_MAX,
        )
        .expect_err("2 GHz unsupported");
        assert_eq!(error.code, "tx_power_efuse_band_unsupported");
    }

    #[test]
    fn rtl8812au_tx_power_execution_writes_runtime_reports() {
        let transport = MockTransport::default();
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let writes = super::run_rtl8812au_manual_tx_power(
            &registers,
            &mut counters,
            super::Rtl8812auRfPath::A,
            0x1a,
        )
        .expect("manual TX power writes");

        assert_eq!(writes.len(), 12);
        assert_eq!(writes[0].register_name, "rA_TxAGC_CCK");
        assert_eq!(writes[0].written, 0x1a1a_1a1a);
        assert_eq!(counters.usb_control_reads, 24);
        assert_eq!(counters.usb_control_writes, 12);
    }

    #[test]
    fn rtl8812au_runtime_iqk_tx_fill_iqc_plan_matches_upstream_masks() {
        let plan =
            super::rtl8812au_iqk_tx_fill_iqc_plan(super::Rtl8812auRfPath::A, 0x2aa, 0x155, false)
                .expect("path A TX IQC plan");

        assert_eq!(plan.len(), 7);
        assert_eq!(plan[0].address, super::REG_AGC_TABLE_JAGUAR);
        assert_eq!(plan[0].mask, super::RTL8812A_IQK_PAGE_C1_SELECT_BIT);
        assert_eq!(plan[0].data, 1);
        assert_eq!(plan[1].address, super::REG_TX_BB_CTRL_A_JAGUAR);
        assert_eq!(plan[1].mask, 0x0000_0080);
        assert_eq!(plan[2].address, super::REG_IQK_TX_CTRL_A_CC4);
        assert_eq!(plan[2].mask, 0x0004_0000);
        assert_eq!(plan[3].address, super::REG_IQK_TX_CTRL_A_CC4);
        assert_eq!(plan[3].mask, 0x2000_0000);
        assert_eq!(plan[4].address, super::REG_IQK_TX_CTRL_A_CC8);
        assert_eq!(plan[4].mask, 0x2000_0000);
        assert_eq!(plan[5].address, super::REG_IQK_TX_Y_A_CCC);
        assert_eq!(plan[5].data, 0x155);
        assert_eq!(plan[6].address, super::REG_IQK_TX_X_A_CD4);
        assert_eq!(plan[6].data, 0x2aa);

        let path_b_dpk_done =
            super::rtl8812au_iqk_tx_fill_iqc_plan(super::Rtl8812auRfPath::B, 0x801, 0x802, true)
                .expect("path B TX IQC plan");
        assert_eq!(path_b_dpk_done.len(), 6);
        assert!(!path_b_dpk_done.iter().any(
            |write| write.address == super::REG_IQK_TX_CTRL_B_EC4 && write.mask == 0x2000_0000
        ));
        assert_eq!(
            path_b_dpk_done
                .iter()
                .find(|write| write.address == super::REG_IQK_TX_Y_B_ECC)
                .expect("path B TX_Y")
                .data,
            0x002
        );
        assert!(super::rtl8812au_iqk_tx_fill_iqc_plan(
            super::Rtl8812auRfPath::Both,
            0x200,
            0,
            false
        )
        .is_err());
    }

    #[test]
    fn rtl8812au_runtime_iqk_rx_fill_iqc_plan_matches_upstream_fallback() {
        let normal = super::rtl8812au_iqk_rx_fill_iqc_plan(super::Rtl8812auRfPath::B, 0x20, 0x10)
            .expect("path B RX IQC plan");
        assert_eq!(normal.len(), 3);
        assert_eq!(normal[0].address, super::REG_AGC_TABLE_JAGUAR);
        assert_eq!(normal[0].mask, super::RTL8812A_IQK_PAGE_C1_SELECT_BIT);
        assert_eq!(normal[0].data, 0);
        assert_eq!(normal[1].address, super::REG_IQK_RX_IQC_B_JAGUAR);
        assert_eq!(normal[1].mask, 0x0000_03ff);
        assert_eq!(normal[1].data, 0x10);
        assert_eq!(normal[2].mask, 0x03ff_0000);
        assert_eq!(normal[2].data, 0x08);

        let fallback = super::rtl8812au_iqk_rx_fill_iqc_plan(super::Rtl8812auRfPath::A, 0x224, 0)
            .expect("path A RX fallback plan");
        assert_eq!(fallback[1].address, super::REG_IQK_RX_IQC_A_JAGUAR);
        assert_eq!(fallback[1].data, 0x100);
        assert_eq!(fallback[2].data, 0);
        assert!(fallback[1].reason.contains("fallback"));
        assert!(
            super::rtl8812au_iqk_rx_fill_iqc_plan(super::Rtl8812auRfPath::Both, 0x200, 0).is_err()
        );
    }

    #[test]
    fn rtl8812au_runtime_iqk_fill_applies_live_masked_writes() {
        let transport = MockTransport::default();
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();
        let mut tx_stage = super::Rtl8812auRuntimeIqkStageReport {
            stage: "tx",
            status: "success",
            ready: Some(true),
            failed: Some(false),
            retry_count: 0,
            average_count: 2,
            delay_count_max: Some(0),
            attempts: Vec::new(),
            candidates: Vec::new(),
            selected_iqc: Some(super::rtl8812au_runtime_iqk_iqc_value(0x2aa, 0x155)),
            fallback_used: false,
            fallback_iqc: None,
            failure_label: None,
            fill_plan: Vec::new(),
        };
        let mut rx_stage = super::Rtl8812auRuntimeIqkStageReport {
            stage: "rx",
            selected_iqc: Some(super::rtl8812au_runtime_iqk_iqc_value(0x20, 0x10)),
            ..tx_stage.clone()
        };
        assert!(super::rtl8812au_runtime_iqk_path_can_fill(
            &tx_stage, &rx_stage
        ));

        let applied = super::apply_rtl8812au_runtime_iqk_fill(
            &registers,
            &mut counters,
            super::Rtl8812auRfPath::A,
            &mut tx_stage,
            &mut rx_stage,
        )
        .expect("runtime IQK fill");

        assert_eq!(applied, 10);
        assert_eq!(tx_stage.fill_plan.len(), 7);
        assert_eq!(rx_stage.fill_plan.len(), 3);
        assert_eq!(counters.usb_control_reads, 10);
        assert_eq!(counters.usb_control_writes, 10);
        assert!(transport.writes().iter().any(|(address, _)| {
            *address == super::REG_IQK_TX_Y_A_CCC
                || *address == super::REG_IQK_TX_X_A_CD4
                || *address == super::REG_IQK_RX_IQC_A_JAGUAR
        }));
    }

    #[test]
    fn rtl8812au_runtime_iqk_candidate_selection_matches_upstream_tolerance() {
        let selected = super::rtl8812au_iqk_select_candidate(&[
            super::rtl8812au_runtime_iqk_iqc_value(0x120, 0x080),
            super::rtl8812au_runtime_iqk_iqc_value(0x122, 0x083),
            super::rtl8812au_runtime_iqk_iqc_value(0x180, 0x100),
        ])
        .expect("selected candidate");
        assert_eq!(selected.x, 0x121);
        assert_eq!(selected.y, 0x081);

        assert!(super::rtl8812au_iqk_select_candidate(&[
            super::rtl8812au_runtime_iqk_iqc_value(0x120, 0x080),
            super::rtl8812au_runtime_iqk_iqc_value(0x124, 0x083),
        ])
        .is_none());

        let signed_wrap_selected = super::rtl8812au_iqk_select_candidate(&[
            super::rtl8812au_runtime_iqk_iqc_value(0x1f7, 0x7ff),
            super::rtl8812au_runtime_iqk_iqc_value(0x1f5, 0x7ee),
            super::rtl8812au_runtime_iqk_iqc_value(0x1fa, 0x001),
        ])
        .expect("selected signed-wrap candidate");
        assert_eq!(signed_wrap_selected.x, 0x1f8);
        assert_eq!(signed_wrap_selected.y, 0x000);

        let delayed_pair_selected = super::rtl8812au_iqk_select_candidate(&[
            super::rtl8812au_runtime_iqk_iqc_value(0x1fc, 0x7fc),
            super::rtl8812au_runtime_iqk_iqc_value(0x201, 0x7f9),
            super::rtl8812au_runtime_iqk_iqc_value(0x1f5, 0x001),
            super::rtl8812au_runtime_iqk_iqc_value(0x1ff, 0x7fb),
        ])
        .expect("selected delayed path-A RX candidate pair");
        assert_eq!(delayed_pair_selected.x, 0x1fd);
        assert_eq!(delayed_pair_selected.y, 0x7fc);
    }

    #[test]
    fn rtl8812au_runtime_iqk_report_state_serializes_failure_and_summary() {
        let mut state = super::Rtl8812auRuntimeIqkOneShotPathState::default();
        state.set_ready(false);
        state.set_failed(true);
        state.note_delay_count(21);
        state.push_attempt(
            state.ready(),
            state.failed(),
            Some(21),
            Some(0x0000_1000),
            None,
            None,
            Some("tx_iqk_not_ready"),
        );
        state.note_retry("tx_iqk_not_ready");

        let stage = state.into_stage_report(
            "tx",
            super::rtl8812au_runtime_iqk_iqc_value(0x200, 0),
            super::rtl8812au_iqk_tx_fill_iqc_plan(super::Rtl8812auRfPath::A, 0x200, 0, false)
                .expect("fallback TX fill plan"),
        );
        assert_eq!(stage.status, "failed");
        assert_eq!(stage.retry_count, 1);
        assert!(stage.fallback_used);
        assert_eq!(stage.failure_label, Some("tx_iqk_not_ready"));

        let value = serde_json::to_value(&stage).expect("serialize stage");
        assert_eq!(value["attempts"][0]["status_raw_hex"], "0x00001000");
        assert_eq!(value["selected_iqc"]["x_hex"], "0x200");
        assert_eq!(value["fill_plan"].as_array().expect("fill plan").len(), 7);

        let skipped_rx = super::rtl8812au_runtime_iqk_skipped_stage_report(
            "rx",
            "rx_iqk_skipped_without_tx_iqk",
            Vec::new(),
        );
        assert!(!super::rtl8812au_runtime_iqk_path_can_fill(
            &stage,
            &skipped_rx
        ));
        let paths = vec![super::Rtl8812auRuntimeIqkPathReport {
            path: super::Rtl8812auRfPath::A,
            path_name: "A",
            tx: stage,
            rx: skipped_rx,
        }];
        assert_eq!(
            super::rtl8812au_runtime_iqk_report_status(&paths, "restored"),
            "fallback_applied"
        );
        let summary =
            super::rtl8812au_runtime_iqk_sweep_summary(&paths, "fallback_applied", "restored", 2);
        assert_eq!(summary.sweep_index, 2);
        assert_eq!(summary.fallback_stage_count, 2);
        assert_eq!(summary.path_statuses[0].tx_retry_count, 1);
        assert_eq!(
            summary.path_statuses[0].rx_failure_label,
            Some("rx_iqk_skipped_without_tx_iqk")
        );
    }

    #[test]
    fn rtl8812au_runtime_iqk_tx_oneshot_runs_attempt_loop() {
        let transport = MockTransport::default();
        transport.insert_u32(
            super::REG_IQK_RESULT_A_D00,
            super::RTL8812A_IQK_READY_MASK | (0x120 << 16),
        );
        transport.insert_u32(
            super::REG_IQK_RESULT_B_D40,
            super::RTL8812A_IQK_READY_MASK | (0x121 << 16),
        );
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let (path_a, path_b) =
            super::run_rtl8812au_runtime_iqk_tx_oneshot(&registers, &mut counters)
                .expect("TX IQK one-shot");

        assert_eq!(path_a.stage, "tx");
        assert_eq!(path_a.status, "success");
        assert_eq!(path_a.average_count, 2);
        assert_eq!(
            path_a.selected_iqc.as_ref().map(|iqc| (iqc.x, iqc.y)),
            Some((0x120, 0x120))
        );
        assert_eq!(
            path_a.attempts[0].raw_candidate.as_ref().map(|raw| (
                raw.x_field,
                raw.y_field,
                raw.x_signed,
                raw.y_signed
            )),
            Some((0x120, 0x120, 0x120, 0x120))
        );
        assert_eq!(path_b.status, "success");
        assert_eq!(path_b.average_count, 2);
        assert_eq!(
            path_b.selected_iqc.as_ref().map(|iqc| (iqc.x, iqc.y)),
            Some((0x121, 0x121))
        );
        assert!(counters.usb_control_reads > 0);
        assert!(counters.usb_control_writes > 0);

        let writes = transport.writes();
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_IQK_TRIGGER_980
                && bytes.as_slice() == 0xfa00_0000_u32.to_le_bytes()
        }));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_IQK_TRIGGER_980
                && bytes.as_slice() == 0xf800_0000_u32.to_le_bytes()
        }));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_RFE_TIMING_A_JAGUAR
                && bytes.as_slice() == 0x0010_0000_u32.to_le_bytes()
        }));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_RFE_TIMING_A_JAGUAR && bytes.as_slice() == 0_u32.to_le_bytes()
        }));
    }

    #[test]
    fn rtl8812au_runtime_iqk_rx_oneshot_runs_lok_prep_and_attempt_loop() {
        let transport = MockTransport::default();
        transport.insert_u32(super::REG_RF_PI_MODE_A_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_RF_PI_MODE_B_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_RF_PI_READ_A_JAGUAR, 0x000a_a000);
        transport.insert_u32(super::REG_RF_PI_READ_B_JAGUAR, 0x000b_b000);
        transport.insert_u32(
            super::REG_IQK_RESULT_A_D00,
            super::RTL8812A_IQK_READY_MASK | (0x130 << 16),
        );
        transport.insert_u32(
            super::REG_IQK_RESULT_B_D40,
            super::RTL8812A_IQK_READY_MASK | (0x131 << 16),
        );
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();
        let tx_a = super::Rtl8812auRuntimeIqkStageReport {
            stage: "tx",
            status: "success",
            ready: Some(true),
            failed: Some(false),
            retry_count: 0,
            average_count: 2,
            delay_count_max: Some(0),
            attempts: Vec::new(),
            candidates: Vec::new(),
            selected_iqc: Some(super::rtl8812au_runtime_iqk_iqc_value(0x120, 0x020)),
            fallback_used: false,
            fallback_iqc: None,
            failure_label: None,
            fill_plan: Vec::new(),
        };
        let tx_b = super::Rtl8812auRuntimeIqkStageReport {
            selected_iqc: Some(super::rtl8812au_runtime_iqk_iqc_value(0x121, 0x021)),
            ..tx_a.clone()
        };

        let (rx_a, rx_b) =
            super::run_rtl8812au_runtime_iqk_rx_oneshot(&registers, &mut counters, &tx_a, &tx_b, 3)
                .expect("RX IQK one-shot");

        assert_eq!(rx_a.stage, "rx");
        assert_eq!(rx_a.status, "success");
        assert_eq!(rx_a.average_count, 2);
        assert_eq!(
            rx_a.selected_iqc.as_ref().map(|iqc| (iqc.x, iqc.y)),
            Some((0x130, 0x130))
        );
        assert_eq!(
            rx_a.attempts[0]
                .raw_candidate
                .as_ref()
                .map(|raw| (raw.x_field, raw.y_field)),
            Some((0x130, 0x130))
        );
        assert_eq!(rx_b.status, "success");
        assert_eq!(rx_b.average_count, 2);
        assert_eq!(
            rx_b.selected_iqc.as_ref().map(|iqc| (iqc.x, iqc.y)),
            Some((0x131, 0x131))
        );
        assert!(counters.usb_control_reads > 0);
        assert!(counters.usb_control_writes > 0);

        let writes = transport.writes();
        assert!(writes.iter().any(|(address, bytes)| {
            let Ok(encoded) = <[u8; 4]>::try_from(bytes.as_slice()).map(u32::from_le_bytes) else {
                return false;
            };
            *address == super::REG_RF_PATH_A_3WIRE
                && ((encoded >> 20) & 0xff) == super::RF_IQK_LOK_LOAD_JAGUAR
        }));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_IQK_RFE_SETTING_A_C8C
                && bytes.as_slice() == 0x2816_0cc0_u32.to_le_bytes()
        }));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_RFE_PINMUX_A_JAGUAR
                && bytes.as_slice() == 0x7777_7717_u32.to_le_bytes()
        }));
    }

    #[test]
    fn rtl8812au_runtime_iqk_calibration_runs_sweep_and_reports_delta() {
        let transport = MockTransport::default();
        transport.insert_u8(super::REG_TXPAUSE, 0);
        transport.insert_u32(super::REG_RF_PI_MODE_A_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_RF_PI_MODE_B_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_RF_PI_READ_A_JAGUAR, 0x000a_a000);
        transport.insert_u32(super::REG_RF_PI_READ_B_JAGUAR, 0x000b_b000);
        transport.insert_u32(
            super::REG_IQK_RESULT_A_D00,
            super::RTL8812A_IQK_READY_MASK | (0x130 << 16),
        );
        transport.insert_u32(
            super::REG_IQK_RESULT_B_D40,
            super::RTL8812A_IQK_READY_MASK | (0x131 << 16),
        );
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let report = super::run_rtl8812au_runtime_iqk_calibration(
            &registers,
            Channel::from_number(36).expect("channel 36"),
            3,
            &mut counters,
        )
        .expect("runtime IQK calibration");

        assert_eq!(report.mode, "runtime_iqk");
        assert_eq!(report.status, "completed");
        assert_eq!(report.cleanup_status, "restored");
        assert_eq!(report.sweep_index, 1);
        assert_eq!(report.sweep_count, 1);
        assert_eq!(report.max_sweeps, 3);
        assert_eq!(report.sweep_summaries.len(), 1);
        assert_eq!(report.sweep_summaries[0].fallback_stage_count, 0);
        assert_eq!(report.paths.len(), 2);
        assert!(report.selected_iqc_fill_applied);
        assert_eq!(report.selected_iqc_fill_register_count, 20);
        assert!(report.backup.is_some());
        assert!(report.pre_sweep_state.is_some());
        assert_eq!(
            report
                .pre_sweep_state
                .as_ref()
                .expect("pre-sweep state")
                .txagc_registers_pre_iqk
                .len(),
            super::rtl8812au_tx_power_agc_registers(super::Rtl8812auRfPath::Both).len()
        );
        assert!(report.cleanup.is_some());
        assert_eq!(
            report.before_iqk_registers.len(),
            super::RTL8812A_IQK_RESULT_REGISTERS.len()
        );
        assert_eq!(
            report.affected_registers.len(),
            super::RTL8812A_IQK_RESULT_REGISTERS.len()
        );
        assert!(report.counters.usb_control_reads > 0);
        assert!(report.counters.usb_control_writes > 0);
        assert_eq!(report.counters, counters);
        assert_eq!(
            transport
                .register_bytes(super::REG_IQK_TX_Y_A_CCC)
                .as_deref(),
            Some(&0x0000_0130_u32.to_le_bytes()[..])
        );
        assert_eq!(
            transport
                .register_bytes(super::REG_IQK_TX_X_A_CD4)
                .as_deref(),
            Some(&0x0000_0130_u32.to_le_bytes()[..])
        );
        assert_eq!(
            transport
                .register_bytes(super::REG_IQK_TX_Y_B_ECC)
                .as_deref(),
            Some(&0x0000_0131_u32.to_le_bytes()[..])
        );
        assert_eq!(
            transport
                .register_bytes(super::REG_IQK_TX_X_B_ED4)
                .as_deref(),
            Some(&0x0000_0131_u32.to_le_bytes()[..])
        );
        assert_eq!(
            transport
                .register_bytes(super::REG_IQK_RX_IQC_A_JAGUAR)
                .as_deref(),
            Some(&0x0000_0100_u32.to_le_bytes()[..])
        );
        assert_eq!(
            transport
                .register_bytes(super::REG_IQK_RX_IQC_B_JAGUAR)
                .as_deref(),
            Some(&0x0000_0100_u32.to_le_bytes()[..])
        );
    }

    #[test]
    fn rtl8812au_tx_calibration_profile_routes_default_and_targeted() {
        let transport = MockTransport::default();
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();
        let channel = Channel::from_number(36).expect("channel 36");

        let default = super::run_rtl8812au_tx_calibration_profile(
            &registers,
            &mut counters,
            TxCalibrationProfile::CurrentDefault,
            channel,
            Bandwidth::Mhz20,
            3,
        )
        .expect("default profile");
        assert!(default.is_none());
        assert_eq!(counters, RuntimeRadioCounters::default());

        let targeted = super::run_rtl8812au_tx_calibration_profile(
            &registers,
            &mut counters,
            TxCalibrationProfile::LinuxParityCh36Ht20,
            channel,
            Bandwidth::Mhz20,
            3,
        )
        .expect("targeted profile")
        .expect("targeted report");
        assert_eq!(targeted.profile, TxCalibrationProfile::LinuxParityCh36Ht20);
        assert_eq!(targeted.channel, 36);
        assert_eq!(targeted.bandwidth_mhz, 20);
        assert_eq!(targeted.register_count, 6);
        assert_eq!(targeted.writes.len(), 6);
        assert!(targeted.lck.is_none());
        assert!(targeted.runtime_iqk.is_none());
        assert!(counters.usb_control_reads > 0);
        assert!(counters.usb_control_writes > 0);
    }

    #[test]
    fn rtl8812au_tx_calibration_profile_routes_runtime_iqk() {
        let transport = MockTransport::default();
        transport.insert_u8(super::REG_TXPAUSE, 0);
        transport.insert_u32(super::REG_RF_PI_MODE_A_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_RF_PI_MODE_B_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_RF_PI_READ_A_JAGUAR, 0x000a_a000);
        transport.insert_u32(super::REG_RF_PI_READ_B_JAGUAR, 0x000b_b000);
        transport.insert_u32(
            super::REG_IQK_RESULT_A_D00,
            super::RTL8812A_IQK_READY_MASK | (0x130 << 16),
        );
        transport.insert_u32(
            super::REG_IQK_RESULT_B_D40,
            super::RTL8812A_IQK_READY_MASK | (0x131 << 16),
        );
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let report = super::run_rtl8812au_tx_calibration_profile(
            &registers,
            &mut counters,
            TxCalibrationProfile::Rtl8812aRuntimeIqk,
            Channel::from_number(36).expect("channel 36"),
            Bandwidth::Mhz20,
            3,
        )
        .expect("runtime IQK profile")
        .expect("runtime IQK report");

        assert_eq!(report.profile, TxCalibrationProfile::Rtl8812aRuntimeIqk);
        assert!(report.writes.is_empty());
        assert!(report.lck.is_none());
        let iqk = report.runtime_iqk.expect("runtime IQK report");
        assert_eq!(iqk.status, "completed");
        assert_eq!(iqk.cleanup_status, "restored");
        assert_eq!(iqk.sweep_count, 1);
        assert!(report.register_count > 0);
    }

    #[test]
    fn rtl8812au_runtime_iqk_setup_plan_ports_mac_afe_rf_prerequisites() {
        fn register_value(
            plan: &[super::Rtl8812auRuntimeIqkSetupWritePlan],
            address: u16,
        ) -> Option<(u32, &'static str)> {
            plan.iter().find_map(|write| match write {
                super::Rtl8812auRuntimeIqkSetupWritePlan::Register {
                    address: candidate,
                    value,
                    width,
                    ..
                } if *candidate == address => Some((*value, *width)),
                _ => None,
            })
        }

        fn masked_data(
            plan: &[super::Rtl8812auRuntimeIqkSetupWritePlan],
            address: u16,
            mask: u32,
        ) -> Option<u32> {
            plan.iter().find_map(|write| match write {
                super::Rtl8812auRuntimeIqkSetupWritePlan::MaskedBb { write, .. }
                    if write.address == address && write.mask == mask =>
                {
                    Some(write.data)
                }
                _ => None,
            })
        }

        fn rf_value(
            plan: &[super::Rtl8812auRuntimeIqkSetupWritePlan],
            path: super::Rtl8812auRfPath,
            rf_offset: u32,
        ) -> Option<u32> {
            plan.iter().find_map(|write| match write {
                super::Rtl8812auRuntimeIqkSetupWritePlan::Rf {
                    path: candidate_path,
                    rf_offset: candidate_offset,
                    value,
                    ..
                } if *candidate_path == path && *candidate_offset == rf_offset => Some(*value),
                _ => None,
            })
        }

        let plan = super::rtl8812au_runtime_iqk_setup_plan(Band::Ghz5, 0x03, true, false);
        assert_eq!(
            register_value(&plan, super::REG_TXPAUSE),
            Some((0x3f, "u8"))
        );
        assert_eq!(
            masked_data(
                &plan,
                super::REG_AGC_TABLE_JAGUAR,
                super::RTL8812A_IQK_PAGE_C1_SELECT_BIT
            ),
            Some(0)
        );
        assert_eq!(
            masked_data(&plan, super::REG_BCN_CTRL, 0x0000_0808),
            Some(0)
        );
        assert_eq!(
            register_value(&plan, super::REG_IQK_AFE_A_C60),
            Some((0x7777_7777, "u32"))
        );
        assert_eq!(
            masked_data(&plan, super::REG_RF_PI_MODE_A_JAGUAR, 0x0000_000f),
            Some(0x04)
        );
        assert_eq!(
            rf_value(&plan, super::Rtl8812auRfPath::A, super::RF_IQK_MODE_JAGUAR),
            Some(0x80002)
        );
        assert_eq!(
            rf_value(
                &plan,
                super::Rtl8812auRfPath::B,
                super::RF_IQK_TX_0X32_JAGUAR
            ),
            Some(0xfe83f)
        );
        assert_eq!(
            register_value(&plan, super::REG_IQK_RFE_SETTING_A_C88),
            Some((0x8214_03f7, "u32"))
        );
        assert_eq!(
            register_value(&plan, super::REG_IQK_RFE_SETTING_A_C8C),
            Some((0x6816_3e96, "u32"))
        );
        assert_eq!(
            register_value(&plan, super::REG_IQK_TX_TONE_A_C80),
            Some((0x1800_8c10, "u32"))
        );
        assert_eq!(
            register_value(&plan, super::REG_IQK_RX_TONE_B_E84),
            Some((0x3800_8c10, "u32"))
        );

        let path_a_rf_writes = plan
            .iter()
            .filter(|write| {
                matches!(
                    write,
                    super::Rtl8812auRuntimeIqkSetupWritePlan::Rf {
                        path: super::Rtl8812auRfPath::A,
                        ..
                    }
                )
            })
            .count();
        let path_b_rf_writes = plan
            .iter()
            .filter(|write| {
                matches!(
                    write,
                    super::Rtl8812auRuntimeIqkSetupWritePlan::Rf {
                        path: super::Rtl8812auRfPath::B,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(path_a_rf_writes, 6);
        assert_eq!(path_b_rf_writes, 6);
    }

    #[test]
    fn rtl8812au_runtime_iqk_setup_plan_applies_live_writes() {
        let plan = super::rtl8812au_runtime_iqk_setup_plan(Band::Ghz5, 0x03, true, false);
        let expected_reads = plan
            .iter()
            .filter(|write| {
                matches!(
                    write,
                    super::Rtl8812auRuntimeIqkSetupWritePlan::MaskedBb { .. }
                )
            })
            .count()
            * 2;
        let expected_writes = plan.len();
        let transport = MockTransport::default();
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let applied =
            super::apply_rtl8812au_runtime_iqk_setup_plan(&registers, &mut counters, &plan)
                .expect("apply runtime IQK setup");

        assert_eq!(applied, plan.len());
        assert_eq!(counters.usb_control_reads, expected_reads as u64);
        assert_eq!(counters.usb_control_writes, expected_writes as u64);
        let writes = transport.writes();
        assert!(writes
            .iter()
            .any(|(address, bytes)| *address == super::REG_TXPAUSE && bytes.as_slice() == [0x3f]));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_RF_PATH_A_3WIRE
                && bytes.as_slice()
                    == super::encode_rf_serial_write(super::RF_IQK_MODE_JAGUAR, 0x80002)
                        .to_le_bytes()
        }));
    }

    #[test]
    fn rtl8812au_runtime_iqk_backup_and_restore_preserve_state() {
        let transport = MockTransport::default();
        transport.insert_u32(super::REG_HSSI_READ_JAGUAR, 0x0000_0058);
        transport.insert_u32(super::REG_AGC_TABLE_JAGUAR, 0);
        transport.insert_u8(super::REG_TXPAUSE, 0x2a);
        transport.insert_u32(super::REG_RF_PI_MODE_A_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_RF_PI_MODE_B_JAGUAR, 0x0000_0004);
        transport.insert_u32(super::REG_RF_PI_READ_A_JAGUAR, 0x000a_bcde);
        transport.insert_u32(super::REG_RF_PI_READ_B_JAGUAR, 0x000b_cdef);
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let backup = super::run_rtl8812au_runtime_iqk_backup(&registers, &mut counters)
            .expect("runtime IQK backup");

        assert_eq!(backup.hssi_read_register.value, 0x0000_0058);
        assert_eq!(backup.tx_pause_register.value, 0x2a);
        assert_eq!(backup.macbb_backup.len(), 14);
        assert_eq!(backup.afe_backup.len(), 12);
        assert_eq!(backup.page_c1_latches.len(), 20);
        assert_eq!(backup.rf_backup_path_a.len(), 8);
        assert_eq!(backup.rf_backup_path_b.len(), 8);
        assert_eq!(
            backup.rf_backup_path_a[0].value,
            0x000a_bcde & super::RF_REGISTER_OFFSET_MASK
        );
        assert_eq!(
            backup.rf_backup_path_b[0].value,
            0x000b_cdef & super::RF_REGISTER_OFFSET_MASK
        );

        let cleanup =
            super::restore_rtl8812au_runtime_iqk_backup(&registers, &mut counters, &backup);

        assert_eq!(cleanup.status, "restored");
        assert!(cleanup.failures.is_empty());
        assert_eq!(cleanup.macbb_restore_count, 14);
        assert_eq!(cleanup.afe_restore_count, 12);
        assert_eq!(cleanup.page_c1_latch_restore_count, 20);
        assert_eq!(cleanup.rf_path_a_restore_count, 8);
        assert_eq!(cleanup.rf_path_b_restore_count, 8);
        assert_eq!(cleanup.hssi_read_restored, Some(true));
        assert_eq!(cleanup.page_select_restored, Some(true));
        assert_eq!(cleanup.tx_pause_restored, Some(true));
        assert!(cleanup.counters.usb_control_writes > 0);
        let writes = transport.writes();
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_RF_PATH_A_3WIRE
                && bytes.as_slice()
                    == super::encode_rf_serial_write(super::RF_IQK_MODE_JAGUAR, 0xabcde)
                        .to_le_bytes()
        }));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_RF_PATH_A_3WIRE
                && bytes.as_slice()
                    == super::encode_rf_serial_write(super::RF_IQK_TX_0X30_JAGUAR, 0xabcde)
                        .to_le_bytes()
        }));
        assert!(writes.iter().any(|(address, bytes)| {
            *address == super::REG_RF_PATH_A_3WIRE
                && bytes.as_slice()
                    == super::encode_rf_serial_write(super::RF_IQK_LOK_LOAD_JAGUAR, 0xabcde)
                        .to_le_bytes()
        }));
        assert_eq!(
            transport.register_bytes(super::REG_TXPAUSE).as_deref(),
            Some(&[0x2a][..])
        );
    }

    #[test]
    fn before_tx_class_preserves_existing_calibration_semantics() {
        assert_eq!(
            TxCalibrationProfile::LinuxParityCh36Ht20.before_tx_class(false),
            TxCalibrationClass::TargetedLinuxParity
        );
        assert_eq!(
            TxCalibrationProfile::Rtl8812aLck.before_tx_class(false),
            TxCalibrationClass::RuntimeApproximation
        );
        assert_eq!(
            TxCalibrationProfile::Rtl8812aRuntimeIqk.before_tx_class(false),
            TxCalibrationClass::RuntimeApproximation
        );
        assert_eq!(
            TxCalibrationProfile::CurrentDefault.before_tx_class(true),
            TxCalibrationClass::StopGapCaptured
        );
        assert_eq!(
            TxCalibrationProfile::Rtl8812aIqkProbe.before_tx_class(true),
            TxCalibrationClass::StopGapCaptured
        );
        assert_eq!(
            TxCalibrationProfile::Rtl8812aIqkProbe.before_tx_class(false),
            TxCalibrationClass::Unknown
        );
    }

    #[test]
    fn runtime_flow_telemetry_shapes_are_report_neutral() {
        let rx = RuntimeFlowRxTelemetry {
            buffers_read: 2,
            read_timeouts: 1,
            parsed_frames: 7,
            phy_status_frames: 6,
            rssi_valid_frames: 6,
            snr_frames: 5,
            noise_frames: 5,
            signal: super::RuntimeRxSignalSummary::default(),
            forwarded_payloads: 3,
            rx_forwards: Vec::new(),
            dropped_packets: 4,
            need_more_data: 1,
            management_frames: 2,
            control_frames: 3,
            data_frames: 4,
            extension_frames: 5,
            wfb_channel_observations: Vec::new(),
        };
        let tx = RuntimeFlowTxTelemetry {
            datagrams_received: 5,
            submitted_frames: 5,
            failed_submissions: 0,
            dropped_datagrams: 1,
            bytes_written: 4096,
        };

        assert_eq!(rx.forwarded_payloads, 3);
        assert_eq!(rx.snr_frames, 5);
        assert_eq!(rx.noise_frames, 5);
        assert_eq!(rx.need_more_data, 1);
        assert_eq!(rx.management_frames, 2);
        assert_eq!(rx.control_frames, 3);
        assert_eq!(rx.data_frames, 4);
        assert_eq!(rx.extension_frames, 5);
        assert_eq!(tx.bytes_written, 4096);
        assert_eq!(RuntimeFlowRxTelemetry::default().buffers_read, 0);
        assert_eq!(RuntimeFlowRxTelemetry::default().snr_frames, 0);
        assert_eq!(RuntimeFlowTxTelemetry::default().submitted_frames, 0);
    }

    fn production_runtime_flow_config() -> ProductionRuntimeFlowConfig {
        ProductionRuntimeFlowConfig {
            usb: ProductionRuntimeUsbConfig::libusb(DeviceSelector::default()),
            channel: Channel::from_number(36).expect("channel 36"),
            bandwidth: Bandwidth::Mhz20,
            firmware: Some(PathBuf::from("/tmp/rtl8812a_fw.bin")),
            bind_addr: "127.0.0.1:5600".parse::<SocketAddr>().expect("bind addr"),
            tx_binds: vec!["127.0.0.1:5601".parse().expect("tx bind")],
            duration_ms: 10_000,
            rx_timeout_ms: 20,
            tx_burst_limit: 8,
            max_datagrams: 0,
            airtime_schedule: ProductionRuntimeAirtimeSchedule::continuous(),
            ready_file: Some(PathBuf::from("/tmp/radio-run-ready.json")),
            health_file: None,
            tx_authorized: true,
            live_register_write_authorized: false,
            calibration_profile: TxCalibrationProfile::CurrentDefault,
            captured_tail_applied: true,
            primary_rx_forward: ProductionRuntimePrimaryRxForwardConfig {
                link_id: Some(0x00123456),
                radio_port: Some(0x23),
                aggregator: Some("127.0.0.1:5603".parse().expect("primary aggregator")),
            },
            rx_forwards: vec![ProductionRuntimeRxForwardConfig {
                link_id: Some(7669206),
                radio_port: 0,
                aggregator: Some("127.0.0.1:5602".parse().expect("aggregator")),
            }],
            rx_wlan_idx: 0,
            rx_mcs_index: 1,
        }
    }

    fn empty_realtek_table_plan(kind: RealtekTableKind) -> RealtekTablePlan {
        RealtekTablePlan {
            array_name: format!("test_{kind:?}"),
            kind,
            raw_value_count: 0,
            raw_pair_count: 0,
            condition_marker_pairs: 0,
            skipped_write_pairs: 0,
            actions: Vec::new(),
        }
    }

    fn production_runtime_execution_inputs() -> ProductionRuntimeFlowExecutionInputs {
        ProductionRuntimeFlowExecutionInputs {
            rtl8812au_init: Some(ProductionRuntimeRtl8812auInitInputs {
                firmware_image: FirmwareImage::from_bytes(
                    FirmwareSource::External(PathBuf::from("/tmp/test-fw.bin")),
                    vec![0x00, 0x01, 0x02, 0x03],
                )
                .expect("firmware image"),
                mac_plan: empty_realtek_table_plan(RealtekTableKind::Mac),
                phy_plan: empty_realtek_table_plan(RealtekTableKind::BbPhy),
                agc_plan: empty_realtek_table_plan(RealtekTableKind::BbAgc),
                radioa_plan: empty_realtek_table_plan(RealtekTableKind::RfRadioA),
                radiob_plan: empty_realtek_table_plan(RealtekTableKind::RfRadioB),
                init_order: Rtl8812auInitOrder::Default,
                rfe_type: 0,
                init_timeout: Duration::from_millis(500),
            }),
            ..ProductionRuntimeFlowExecutionInputs::default()
        }
    }

    fn production_runtime_short_flow_config() -> ProductionRuntimeFlowConfig {
        let mut config = production_runtime_flow_config();
        config.bind_addr = "127.0.0.1:0".parse().expect("bind addr");
        config.tx_binds = vec!["127.0.0.1:0".parse().expect("tx bind")];
        config.duration_ms = 1;
        config.rx_timeout_ms = 1;
        config
    }

    fn complete_runtime_init_phase(
        session: &mut RuntimeRadioSession<MockTransport>,
        phase: Rtl8812auInitPhase,
    ) -> Result<RuntimeSameSessionInitPhaseSummary, RuntimeSameSessionInitPhaseFailure> {
        let before = session.counters;
        session.counters.usb_control_writes = session.counters.usb_control_writes.saturating_add(1);
        Ok(RuntimeSameSessionInitPhaseSummary::completed(
            phase,
            format!("test completed {phase:?}"),
            before,
            session.counters,
        ))
    }

    #[test]
    fn production_runtime_flow_config_validates_before_usb() {
        let config = production_runtime_flow_config();
        let validation = config.validate().expect("valid production flow");

        assert_eq!(
            validation.calibration.profile,
            TxCalibrationProfile::CurrentDefault
        );
        assert_eq!(
            validation.calibration.evidence_source,
            RuntimeTxCalibrationEvidenceSource::CapturedLinuxTail
        );
        assert!(!validation.calibration.requires_live_write_authorization);
        assert_eq!(validation.wfb_loop.tx_bind_addrs.len(), 2);
        assert_eq!(validation.wfb_loop.rx_forwards.len(), 2);
        assert_eq!(
            validation.wfb_loop.rx_forwards[0].config.channel_id.link_id,
            0x00123456
        );
        assert_eq!(
            validation.wfb_loop.rx_forwards[1].config.channel_id.link_id,
            7669206
        );
    }

    #[test]
    fn production_ready_marker_writer_records_runtime_marker() {
        let path = std::env::temp_dir().join(format!(
            "wfb-radio-runtime-ready-marker-{}-{}.json",
            std::process::id(),
            runtime_unix_ms()
        ));
        let marker = ProductionRuntimeReadyMarker {
            source: "bridge-run".to_string(),
            ready_at_unix_ms: None,
            bind_addr: "127.0.0.1:5600".to_string(),
            bind_addrs: vec!["127.0.0.1:5600".to_string(), "127.0.0.1:5601".to_string()],
            channel: Some(36),
            channel_frequency_mhz: Some(5180),
            bandwidth_mhz: 20,
            max_datagrams: 64,
            duration_ms: Some(2500),
            idle_timeout_ms: None,
            rx_timeout_ms: Some(20),
            tx_burst_limit: Some(8),
            airtime_schedule: ProductionRuntimeAirtimeSchedule::continuous(),
            init_before_tx: true,
            same_session_init_result: Some("pass".to_string()),
            monitor_opmode_applied: Some(true),
            tx_power_control_applied: false,
            tx_calibration_profile_applied: true,
            rx_startup_kick_applied: true,
        };

        write_production_runtime_ready_marker(Some(path.as_path()), marker)
            .expect("write ready marker");
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("ready marker"))
                .expect("ready marker JSON");
        let _ = std::fs::remove_file(path);

        assert_eq!(value["source"], "bridge-run");
        assert!(value["ready_at_unix_ms"].as_u64().unwrap_or(0) > 0);
        assert_eq!(value["bind_addrs"][1], "127.0.0.1:5601");
        assert_eq!(value["channel"], 36);
        assert_eq!(value["bandwidth_mhz"], 20);
        assert_eq!(value["tx_burst_limit"], 8);
        assert_eq!(value["same_session_init_result"], "pass");
        assert_eq!(value["tx_calibration_profile_applied"], true);
        assert_eq!(value["rx_startup_kick_applied"], true);
    }

    #[test]
    fn production_runtime_flow_config_rejects_missing_authorization_before_usb() {
        let mut config = production_runtime_flow_config();
        config.tx_authorized = false;

        let error = config.validate().expect_err("missing tx auth");

        assert_eq!(error.code, "missing_tx_authorization");
    }

    #[test]
    fn production_runtime_flow_config_rejects_live_calibration_without_write_authorization() {
        let mut config = production_runtime_flow_config();
        config.calibration_profile = TxCalibrationProfile::Rtl8812aRuntimeIqk;
        config.live_register_write_authorized = false;

        let error = config.validate().expect_err("missing write auth");

        assert_eq!(error.code, "missing_write_authorization");
    }

    #[test]
    fn production_runtime_flow_config_rejects_invalid_bounds_before_usb() {
        let mut config = production_runtime_flow_config();
        config.tx_burst_limit = 0;

        let error = config.validate().expect_err("invalid tx burst");

        assert_eq!(error.code, "invalid_tx_burst_limit");
    }

    #[test]
    fn production_runtime_flow_execution_rejects_invalid_config_before_inputs() {
        let mut config = production_runtime_flow_config();
        config.tx_authorized = false;

        let report =
            run_production_runtime_flow(config, ProductionRuntimeFlowExecutionInputs::default());

        assert_eq!(report.result, ProductionRuntimeFlowResult::Fail);
        assert_eq!(report.stop_reason, "not_started");
        assert!(report.adapter.is_none());
        assert!(report.endpoints.is_none());
        assert_eq!(
            report.error.as_ref().map(|error| error.code),
            Some("missing_tx_authorization")
        );
    }

    #[test]
    fn production_runtime_flow_execution_rejects_missing_init_assets_before_usb() {
        let config = production_runtime_flow_config();

        let report =
            run_production_runtime_flow(config, ProductionRuntimeFlowExecutionInputs::default());

        assert_eq!(report.result, ProductionRuntimeFlowResult::Fail);
        assert_eq!(report.stop_reason, "not_started");
        assert!(report.adapter.is_none());
        assert_eq!(report.init, ProductionRuntimeInitTelemetry::default());
        assert_eq!(report.rx, RuntimeFlowRxTelemetry::default());
        assert_eq!(report.tx, RuntimeFlowTxTelemetry::default());
        assert_eq!(
            report.error.as_ref().map(|error| error.code),
            Some("missing_runtime_init_assets")
        );
    }

    #[test]
    fn production_runtime_flow_execution_failure_serializes_runtime_report_shape() {
        let mut config = production_runtime_flow_config();
        config.tx_burst_limit = 0;

        let report =
            run_production_runtime_flow(config, ProductionRuntimeFlowExecutionInputs::default());
        let report_json = serde_json::to_string(&report).expect("report JSON");
        let value: serde_json::Value =
            serde_json::from_str(&report_json).expect("report JSON value");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["command"], "radio-run");
        assert_eq!(value["stop_reason"], "not_started");
        assert_eq!(value["result"], "fail");
        assert_eq!(value["error"]["code"], "invalid_tx_burst_limit");
        assert!(value["adapter"].is_null());
        assert!(value["endpoints"].is_null());
        assert_eq!(value["rx"]["buffers_read"], 0);
        assert_eq!(value["tx"]["submitted_frames"], 0);
        for field in [
            "pre_tx_write",
            "pre_tx_register_writes",
            "tx_status",
            "rx_pcap_path",
            "rx_frame_jsonl_path",
        ] {
            assert!(
                !report_json.contains(field),
                "production runtime report should not expose diagnostic field {field}"
            );
        }
    }

    #[test]
    fn production_runtime_flow_invalid_config_writes_failed_health() {
        let health_path = std::env::temp_dir().join(format!(
            "wfb-radio-runtime-invalid-health-{}-{}.json",
            std::process::id(),
            runtime_unix_ms()
        ));
        let mut config = production_runtime_flow_config();
        config.health_file = Some(health_path.clone());
        config.tx_authorized = false;

        let report =
            run_production_runtime_flow(config, ProductionRuntimeFlowExecutionInputs::default());
        let health: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&health_path).expect("health artifact"))
                .expect("health JSON");
        let _ = std::fs::remove_file(&health_path);

        assert_eq!(report.result, ProductionRuntimeFlowResult::Fail);
        assert_eq!(
            report.error.as_ref().map(|error| error.code),
            Some("missing_tx_authorization")
        );
        assert_eq!(health["lifecycle"], "exited_fail");
        assert_eq!(health["operator_action"], "restart");
        assert_eq!(health["result"], "fail");
        assert_eq!(health["error"]["code"], "missing_tx_authorization");
    }

    #[test]
    fn production_runtime_flow_health_write_failure_fails_closed() {
        let mut config = production_runtime_flow_config();
        config.health_file = Some(std::env::temp_dir().join(format!(
            "wfb-radio-runtime-missing-dir-{}-{}/health.json",
            std::process::id(),
            runtime_unix_ms()
        )));

        let report =
            run_production_runtime_flow(config, ProductionRuntimeFlowExecutionInputs::default());

        assert_eq!(report.result, ProductionRuntimeFlowResult::Fail);
        assert_eq!(report.stop_reason, "not_started");
        assert_eq!(
            report.error.as_ref().map(|error| error.code),
            Some("runtime_health_write_failed")
        );
    }

    #[test]
    fn production_runtime_flow_init_failure_writes_failed_health() {
        let health_path = std::env::temp_dir().join(format!(
            "wfb-radio-runtime-init-failure-health-{}-{}.json",
            std::process::id(),
            runtime_unix_ms()
        ));
        let mut config = production_runtime_short_flow_config();
        config.health_file = Some(health_path.clone());
        let inputs = production_runtime_execution_inputs();
        let mut session = runtime_tx_session(MockTransport::default());

        let report = run_production_runtime_flow_with_session(
            config,
            inputs,
            &mut session,
            |session, phase| {
                let before = session.counters;
                Err(RuntimeSameSessionInitPhaseFailure::new(
                    RuntimeSameSessionInitPhaseSummary::blocked(
                        phase,
                        "synthetic init block",
                        before,
                        session.counters,
                    ),
                    RuntimeRadioError::new("test_init_failed", "synthetic init failure"),
                ))
            },
        );
        let health: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&health_path).expect("health artifact"))
                .expect("health JSON");
        let _ = std::fs::remove_file(&health_path);

        assert_eq!(report.result, ProductionRuntimeFlowResult::Fail);
        assert_eq!(report.stop_reason, "not_started");
        assert_eq!(
            report.error.as_ref().map(|error| error.code),
            Some("test_init_failed")
        );
        assert_eq!(health["lifecycle"], "exited_fail");
        assert_eq!(health["operator_action"], "restart");
        assert_eq!(health["init"]["readiness"], "failed");
        assert_eq!(health["init"]["phase_count"], 1);
        assert_eq!(health["init"]["completed_phase_count"], 0);
        assert_eq!(health["error"]["code"], "test_init_failed");
    }

    #[test]
    fn production_service_health_from_config_reports_wait_state() {
        let config = production_runtime_flow_config();

        let health = ProductionRuntimeServiceHealth::from_config(
            &config,
            ProductionRuntimeServiceLifecycle::Validating,
            Some(PathBuf::from("/tmp/radio-run.json")),
            None,
        );

        assert_eq!(health.schema_version, 1);
        assert_eq!(health.command, "radio-run");
        assert_eq!(
            health.lifecycle,
            ProductionRuntimeServiceLifecycle::Validating
        );
        assert_eq!(
            health.operator_action,
            ProductionRuntimeServiceOperatorAction::Wait
        );
        assert_eq!(
            health.report_file,
            Some(PathBuf::from("/tmp/radio-run.json"))
        );
        assert!(health.error.is_none());
    }

    #[test]
    fn production_service_health_from_report_classifies_pass_and_degraded_tx() {
        let config = production_runtime_flow_config();
        let mut report = ProductionRuntimeFlowReport::from_execution(
            &config,
            super::ProductionRuntimeFlowExecutionReport {
                selector: config.usb.selector,
                adapter: None,
                endpoints: None,
                channel: Some(config.channel),
                bandwidth: config.bandwidth,
                duration_ms: config.duration_ms,
                ready_file: config.ready_file.clone(),
                stop_reason: "duration_elapsed",
                bulk_in_endpoint: Some(0x81),
                bulk_out_endpoint: Some(0x02),
                calibration_profile: config.calibration_profile,
                calibration_class: config
                    .calibration_profile
                    .before_tx_class(config.captured_tail_applied),
                tx_power_control: None,
                tx_calibration_profile: None,
                rx_startup_kick: None,
                heartbeat_led: None,
                airtime: ProductionRuntimeAirtimeReport::idle(config.airtime_schedule),
                receiver_backed_validation_required: false,
                init: ProductionRuntimeInitTelemetry {
                    readiness: ProductionRuntimeInitReadiness::Ready,
                    phase_count: 1,
                    completed_phase_count: 1,
                },
                rx: RuntimeFlowRxTelemetry::default(),
                tx: RuntimeFlowTxTelemetry {
                    datagrams_received: 1,
                    submitted_frames: 1,
                    failed_submissions: 0,
                    dropped_datagrams: 0,
                    bytes_written: 64,
                },
                counters: RuntimeRadioCounters::default(),
                result: ProductionRuntimeFlowResult::Pass,
                error: None,
            },
        );
        let healthy = ProductionRuntimeServiceHealth::from_report(
            &report,
            ProductionRuntimeServiceLifecycle::ExitedPass,
            Some(PathBuf::from("/tmp/radio-health.json")),
            None,
        );
        assert_eq!(
            healthy.operator_action,
            ProductionRuntimeServiceOperatorAction::Monitor
        );

        report.tx.dropped_datagrams = 1;
        let degraded = ProductionRuntimeServiceHealth::from_report(
            &report,
            ProductionRuntimeServiceLifecycle::ExitedPass,
            Some(PathBuf::from("/tmp/radio-health.json")),
            None,
        );
        assert_eq!(
            degraded.operator_action,
            ProductionRuntimeServiceOperatorAction::Investigate
        );
    }

    #[test]
    fn production_service_health_from_report_classifies_signal_stop_and_rx_forward_degradation() {
        let config = production_runtime_flow_config();
        let mut report = ProductionRuntimeFlowReport::from_execution(
            &config,
            super::ProductionRuntimeFlowExecutionReport {
                selector: config.usb.selector,
                adapter: None,
                endpoints: None,
                channel: Some(config.channel),
                bandwidth: config.bandwidth,
                duration_ms: config.duration_ms,
                ready_file: config.ready_file.clone(),
                stop_reason: "signal",
                bulk_in_endpoint: Some(0x81),
                bulk_out_endpoint: Some(0x02),
                calibration_profile: config.calibration_profile,
                calibration_class: config
                    .calibration_profile
                    .before_tx_class(config.captured_tail_applied),
                tx_power_control: None,
                tx_calibration_profile: None,
                rx_startup_kick: None,
                heartbeat_led: None,
                airtime: ProductionRuntimeAirtimeReport::idle(config.airtime_schedule),
                receiver_backed_validation_required: false,
                init: ProductionRuntimeInitTelemetry {
                    readiness: ProductionRuntimeInitReadiness::Ready,
                    phase_count: 1,
                    completed_phase_count: 1,
                },
                rx: RuntimeFlowRxTelemetry::default(),
                tx: RuntimeFlowTxTelemetry {
                    datagrams_received: 1,
                    submitted_frames: 1,
                    failed_submissions: 0,
                    dropped_datagrams: 0,
                    bytes_written: 64,
                },
                counters: RuntimeRadioCounters::default(),
                result: ProductionRuntimeFlowResult::Pass,
                error: None,
            },
        );

        let signal = ProductionRuntimeServiceHealth::from_report(
            &report,
            ProductionRuntimeServiceLifecycle::ExitedPass,
            Some(PathBuf::from("/tmp/radio-health.json")),
            None,
        );
        assert_eq!(
            signal.operator_action,
            ProductionRuntimeServiceOperatorAction::Stopped
        );

        report.stop_reason = "duration_elapsed";
        report.rx.rx_forwards = vec![ProductionRuntimeRxForwardSnapshot {
            config: RxForwardConfig {
                channel_id: WfbChannelId::new(0x00123456, 0x23).expect("channel id"),
                wlan_idx: 0,
                mcs_index: 1,
                bandwidth_mhz: 20,
            },
            aggregator: Some("127.0.0.1:5603".parse().expect("aggregator")),
            forwarded_bytes: 0,
            counters: RxCounters {
                send_failed: 1,
                ..RxCounters::default()
            },
        }];
        let degraded = ProductionRuntimeServiceHealth::from_report(
            &report,
            ProductionRuntimeServiceLifecycle::ExitedPass,
            Some(PathBuf::from("/tmp/radio-health.json")),
            None,
        );
        assert_eq!(
            degraded.operator_action,
            ProductionRuntimeServiceOperatorAction::Investigate
        );
    }

    #[test]
    fn production_service_health_writer_handles_absent_path_and_json_shape() {
        let config = production_runtime_flow_config();
        let path = std::env::temp_dir().join(format!(
            "wfb-radio-runtime-health-{}-{}.json",
            std::process::id(),
            runtime_unix_ms()
        ));
        let health = ProductionRuntimeServiceHealth::from_config(
            &config,
            ProductionRuntimeServiceLifecycle::Starting,
            None,
            Some(RuntimeRadioError::new("test_error", "synthetic")),
        );

        write_production_runtime_service_health(None, &health).expect("absent path is no-op");
        write_production_runtime_service_health(Some(&path), &health).expect("write health");
        let json = std::fs::read_to_string(&path).expect("health JSON");
        let value: serde_json::Value = serde_json::from_str(&json).expect("health value");
        let _ = std::fs::remove_file(path);

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["command"], "radio-run");
        assert_eq!(value["lifecycle"], "starting");
        assert_eq!(value["operator_action"], "wait");
        assert_eq!(value["error"]["code"], "test_error");
        for field in [
            "pre_tx_write",
            "pre_tx_register_writes",
            "tx_status",
            "rx_pcap_path",
            "rx_frame_jsonl_path",
            "trace_replay",
        ] {
            assert!(!json.contains(field), "health leaked {field}");
        }
    }

    #[test]
    fn production_runtime_flow_session_runs_init_and_writes_ready_marker() {
        let ready_path = std::env::temp_dir().join(format!(
            "wfb-radio-runtime-flow-ready-{}-{}.json",
            std::process::id(),
            runtime_unix_ms()
        ));
        let health_path = std::env::temp_dir().join(format!(
            "wfb-radio-runtime-flow-health-{}-{}.json",
            std::process::id(),
            runtime_unix_ms()
        ));
        let mut config = production_runtime_short_flow_config();
        config.ready_file = Some(ready_path.clone());
        config.health_file = Some(health_path.clone());
        let inputs = production_runtime_execution_inputs();
        let mut session = runtime_tx_session(MockTransport::default());
        let mut observed_phases = Vec::new();

        let report = run_production_runtime_flow_with_session(
            config,
            inputs,
            &mut session,
            |session, phase| {
                observed_phases.push(phase);
                complete_runtime_init_phase(session, phase)
            },
        );
        let marker: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ready_path).expect("ready marker"))
                .expect("ready marker JSON");
        let health: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&health_path).expect("health artifact"))
                .expect("health JSON");
        let _ = std::fs::remove_file(&ready_path);
        let _ = std::fs::remove_file(&health_path);

        assert!(!observed_phases.is_empty());
        assert_eq!(report.init.readiness, ProductionRuntimeInitReadiness::Ready);
        assert_eq!(report.init.phase_count, observed_phases.len());
        assert_eq!(report.init.completed_phase_count, observed_phases.len());
        assert!(report.adapter.is_some());
        assert!(report.endpoints.is_some());
        assert_eq!(report.bulk_in_endpoint, Some(0x81));
        assert_eq!(report.bulk_out_endpoint, Some(0x02));
        assert_eq!(report.stop_reason, "duration_elapsed");
        assert!(
            report
                .heartbeat_led
                .as_ref()
                .map(|heartbeat| heartbeat.toggles_attempted >= 1)
                .unwrap_or(false),
            "heartbeat should be turned off once on flow exit"
        );
        assert_eq!(report.result, ProductionRuntimeFlowResult::Pass);
        assert!(report.error.is_none());
        assert_eq!(marker["source"], "bridge-run");
        assert_eq!(marker["same_session_init_result"], "pass");
        assert_eq!(marker["bind_addrs"][0], "127.0.0.1:0");
        assert_eq!(marker["bind_addrs"][1], "127.0.0.1:0");
        assert_eq!(marker["channel"], 36);
        assert_eq!(marker["bandwidth_mhz"], 20);
        assert_eq!(marker["tx_burst_limit"], 8);
        assert_eq!(marker["monitor_opmode_applied"], true);
        assert_eq!(health["lifecycle"], "exited_pass");
        assert_eq!(health["operator_action"], "monitor");
        assert_eq!(health["result"], "pass");
        assert_eq!(health["stop_reason"], "duration_elapsed");
        assert_eq!(health["ready_file"], ready_path.to_string_lossy().as_ref());
        assert_eq!(
            health["health_file"],
            health_path.to_string_lossy().as_ref()
        );
        assert!(
            health["heartbeat_led"]["toggles_attempted"]
                .as_u64()
                .unwrap_or(0)
                >= 1
        );
    }

    #[test]
    fn production_runtime_flow_session_preserves_tx_power_report_and_marker() {
        let ready_path = std::env::temp_dir().join(format!(
            "wfb-radio-runtime-flow-tx-power-ready-{}-{}.json",
            std::process::id(),
            runtime_unix_ms()
        ));
        let mut config = production_runtime_short_flow_config();
        config.ready_file = Some(ready_path.clone());
        let mut inputs = production_runtime_execution_inputs();
        inputs.tx_power_control = super::ProductionRuntimeTxPowerControlInput::ManualIndex {
            path: super::Rtl8812auRfPath::Both,
            index: 0x12,
        };
        let mut session = runtime_tx_session(MockTransport::default());

        let report = run_production_runtime_flow_with_session(
            config,
            inputs,
            &mut session,
            complete_runtime_init_phase,
        );
        let marker: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ready_path).expect("ready marker"))
                .expect("ready marker JSON");
        let _ = std::fs::remove_file(ready_path);
        let tx_power = report.tx_power_control.as_ref().expect("tx power report");

        assert_eq!(tx_power.mode, Rtl8812auTxPowerControlMode::ManualIndex);
        assert_eq!(tx_power.manual_index, Some(0x12));
        assert_eq!(tx_power.repeated_value, Some(0x1212_1212));
        assert!(tx_power.register_count > 0);
        assert_eq!(marker["tx_power_control_applied"], true);
        assert_eq!(marker["tx_calibration_profile_applied"], false);
    }

    #[test]
    fn production_runtime_flow_session_preserves_tx_calibration_report_and_marker() {
        let ready_path = std::env::temp_dir().join(format!(
            "wfb-radio-runtime-flow-calibration-ready-{}-{}.json",
            std::process::id(),
            runtime_unix_ms()
        ));
        let mut config = production_runtime_short_flow_config();
        config.ready_file = Some(ready_path.clone());
        config.live_register_write_authorized = true;
        config.calibration_profile = TxCalibrationProfile::LinuxParityCh36Ht20;
        let inputs = production_runtime_execution_inputs();
        let mut session = runtime_tx_session(MockTransport::default());

        let report = run_production_runtime_flow_with_session(
            config,
            inputs,
            &mut session,
            complete_runtime_init_phase,
        );
        let marker: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ready_path).expect("ready marker"))
                .expect("ready marker JSON");
        let _ = std::fs::remove_file(ready_path);
        let calibration = report
            .tx_calibration_profile
            .as_ref()
            .expect("calibration report");

        assert_eq!(
            calibration.profile,
            TxCalibrationProfile::LinuxParityCh36Ht20
        );
        assert!(calibration.register_count > 0);
        assert_eq!(
            report.calibration_class,
            TxCalibrationClass::TargetedLinuxParity
        );
        assert_eq!(marker["tx_power_control_applied"], false);
        assert_eq!(marker["tx_calibration_profile_applied"], true);
    }

    fn production_wfb_loop_config() -> ProductionRuntimeWfbLoopConfig {
        production_runtime_flow_config().wfb_loop_config()
    }

    #[test]
    fn production_wfb_loop_plan_accepts_self_contained_forward_target() {
        let mut config = production_wfb_loop_config();
        config.primary_rx_forward = ProductionRuntimePrimaryRxForwardConfig {
            link_id: None,
            radio_port: None,
            aggregator: None,
        };
        config.rx_forwards = vec![ProductionRuntimeRxForwardConfig {
            link_id: Some(0x00010203),
            radio_port: 0x45,
            aggregator: Some("127.0.0.1:5604".parse().expect("aggregator")),
        }];

        let plan = plan_production_wfb_loop(&config).expect("loop plan");

        assert_eq!(plan.tx_bind_addrs.len(), 2);
        assert_eq!(plan.rx_forwards.len(), 1);
        assert_eq!(plan.rx_forwards[0].config.channel_id.link_id, 0x00010203);
        assert_eq!(plan.rx_forwards[0].config.channel_id.radio_port, 0x45);
        assert_eq!(plan.rx_forwards[0].config.bandwidth_mhz, 20);
    }

    #[test]
    fn production_wfb_loop_plan_rejects_aggregator_without_filter() {
        let mut config = production_wfb_loop_config();
        config.primary_rx_forward = ProductionRuntimePrimaryRxForwardConfig {
            link_id: None,
            radio_port: None,
            aggregator: Some("127.0.0.1:5604".parse().expect("aggregator")),
        };
        config.rx_forwards.clear();

        let error = plan_production_wfb_loop(&config).expect_err("missing filter");

        assert_eq!(error.code, "missing_wfb_rx_filter");
    }

    #[test]
    fn production_wfb_loop_plan_rejects_defaulted_target_without_global_link() {
        let mut config = production_wfb_loop_config();
        config.primary_rx_forward = ProductionRuntimePrimaryRxForwardConfig {
            link_id: None,
            radio_port: None,
            aggregator: None,
        };
        config.rx_forwards = vec![ProductionRuntimeRxForwardConfig {
            link_id: None,
            radio_port: 0x23,
            aggregator: Some("127.0.0.1:5604".parse().expect("aggregator")),
        }];

        let error = plan_production_wfb_loop(&config).expect_err("missing link ID");

        assert_eq!(error.code, "missing_wfb_rx_forward_link_id");
    }

    #[test]
    fn production_tx_ingress_binds_in_order() {
        let bind_addrs = [
            "127.0.0.1:0".parse::<SocketAddr>().expect("addr"),
            "127.0.0.1:0".parse::<SocketAddr>().expect("addr"),
        ];

        let sockets =
            bind_production_tx_ingress_sockets(&bind_addrs, PRODUCTION_TX_SOCKET_RCVBUF_BYTES)
                .expect("bind sockets");

        assert_eq!(sockets.len(), 2);
        assert_eq!(sockets[0].report_index, 0);
        assert_eq!(sockets[1].report_index, 1);
        assert_eq!(sockets[0].bind_addr, bind_addrs[0]);
        assert_eq!(sockets[1].bind_addr, bind_addrs[1]);
    }

    #[test]
    fn production_tx_ingress_reports_bind_failure() {
        let held = UdpSocket::bind("127.0.0.1:0").expect("held socket");
        let addr = held.local_addr().expect("held addr");

        let error = bind_production_tx_ingress_sockets(&[addr], PRODUCTION_TX_SOCKET_RCVBUF_BYTES)
            .expect_err("duplicate bind should fail");

        assert_eq!(error.code, "udp_bind_failed");
    }

    #[test]
    fn production_tx_ingress_receiver_queues_datagrams() {
        let bind_addr = "127.0.0.1:0".parse::<SocketAddr>().expect("addr");
        let sockets =
            bind_production_tx_ingress_sockets(&[bind_addr], PRODUCTION_TX_SOCKET_RCVBUF_BYTES)
                .expect("bind sockets");
        let target = sockets[0].socket.local_addr().expect("target addr");
        let receiver = spawn_production_tx_ingress_receivers(sockets, Duration::from_millis(10))
            .expect("spawn receiver");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender");
        sender.send_to(b"wfb-test", target).expect("send datagram");

        let queued = receiver
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("queued datagram");

        assert_eq!(queued.report_index, 0);
        assert_eq!(queued.data, b"wfb-test");
        assert_eq!(queued.peer, sender.local_addr().expect("sender addr"));
    }

    fn runtime_tx_session(transport: MockTransport) -> RuntimeRadioSession<MockTransport> {
        let endpoints = UsbEndpoints {
            interface_number: 0,
            bulk_in: Some(0x81),
            bulk_out: Some(0x02),
            bulk_in_all: vec![0x81],
            bulk_out_all: vec![0x02],
        };
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        RuntimeRadioSession::new(
            transport,
            adapter,
            endpoints,
            RuntimeRadioCounters::default(),
        )
    }

    fn valid_wfb_tx_datagram() -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&0x0102_0304u32.to_be_bytes());
        packet.extend_from_slice(&[
            0x00, 0x00, 0x0d, 0x00, 0x00, 0x80, 0x08, 0x00, 0x08, 0x00, 0x37, 0x01, 0x03,
        ]);
        packet.extend_from_slice(&[0x08; 24]);
        packet
    }

    fn queued_tx_datagram(data: Vec<u8>) -> ProductionRuntimeQueuedDatagram {
        ProductionRuntimeQueuedDatagram {
            report_index: 0,
            peer: "127.0.0.1:5600".parse().expect("peer"),
            data,
        }
    }

    fn bridge_tx_config() -> ProductionRuntimeBridgeTxConfig {
        ProductionRuntimeBridgeTxConfig {
            channel: Channel::from_number(36).expect("channel 36"),
            channel_bandwidth: Bandwidth::Mhz40,
            overrides: ProductionRuntimeBridgeTxOverrides::default(),
        }
    }

    fn runtime_rx_frame(data: Vec<u8>) -> RxFrame {
        RxFrame {
            data,
            rssi_dbm: -47,
            rssi_dbm_valid: true,
            rssi_dbm_source: radio_core::RxRssiSource::PhyStatusFirstByte,
            noise_dbm: Some(-92),
            snr_db: Some(45),
            snr_db_source: Some(radio_core::RxSnrSource::Rtl8812PhyStatusBestPath),
            channel: Channel::from_number(36).expect("channel 36"),
            phy_status: true,
            driver_info_size: 8,
            rx_shift: 0,
            raw_phy_status: vec![63],
            rx_rate_raw: 0x0d,
            rx_rate: Some(radio_core::TxRate::Mcs(1)),
            rx_bandwidth_raw: 0,
            rx_bandwidth: Some(Bandwidth::Mhz20),
            short_gi: false,
            ldpc: false,
            stbc: false,
            crc_error: false,
        }
    }

    fn runtime_wfb_frame(channel_id: WfbChannelId) -> RxFrame {
        let mut data = Vec::from(build_wfb_data_header(channel_id, 0x0010));
        data.extend_from_slice(b"runtime-rx");
        runtime_rx_frame(data)
    }

    fn rx_forward_plan(
        channel_id: WfbChannelId,
        aggregator: Option<SocketAddr>,
    ) -> ProductionRuntimeRxForwardPlan {
        ProductionRuntimeRxForwardPlan {
            config: RxForwardConfig {
                channel_id,
                wlan_idx: 0,
                mcs_index: 1,
                bandwidth_mhz: 20,
            },
            aggregator,
        }
    }

    #[test]
    fn production_rx_handler_counts_frame_drop_and_tail_outcomes() {
        let packets = vec![
            ParsedRxPacket {
                consumed: 64,
                outcome: RxParseOutcome::Frame,
                frame: Some(runtime_rx_frame(vec![0x08; 24])),
            },
            ParsedRxPacket {
                consumed: 32,
                outcome: RxParseOutcome::Drop,
                frame: None,
            },
            ParsedRxPacket {
                consumed: 0,
                outcome: RxParseOutcome::NeedMoreData,
                frame: None,
            },
        ];
        let mut forwards = Vec::new();

        let outcome =
            process_production_rx_packet_outcomes(&packets, &mut forwards).expect("rx outcomes");

        assert_eq!(outcome.telemetry.parsed_frames, 1);
        assert_eq!(outcome.telemetry.phy_status_frames, 1);
        assert_eq!(outcome.telemetry.rssi_valid_frames, 1);
        assert_eq!(outcome.telemetry.snr_frames, 1);
        assert_eq!(outcome.telemetry.noise_frames, 1);
        assert_eq!(outcome.telemetry.signal.rssi_dbm.sample_count, 1);
        assert_eq!(outcome.telemetry.signal.rssi_dbm.min, Some(-47));
        assert_eq!(outcome.telemetry.signal.rssi_dbm.max, Some(-47));
        assert_eq!(outcome.telemetry.signal.rssi_dbm.average, Some(-47));
        assert_eq!(outcome.telemetry.signal.snr_db.average, Some(45));
        assert_eq!(outcome.telemetry.signal.noise_dbm.average, Some(-92));
        assert_eq!(outcome.telemetry.data_frames, 1);
        assert_eq!(outcome.telemetry.dropped_packets, 1);
        assert_eq!(outcome.telemetry.need_more_data, 1);
        assert!(outcome.rx_forwards.is_empty());
    }

    #[test]
    fn production_rx_handler_observes_wfb_channel_ids() {
        let channel_id = WfbChannelId::new(0x000102, 0x03).expect("channel ID");
        let packets = vec![ParsedRxPacket {
            consumed: 64,
            outcome: RxParseOutcome::Frame,
            frame: Some(runtime_wfb_frame(channel_id)),
        }];
        let mut forwards = Vec::new();

        let outcome =
            process_production_rx_packet_outcomes(&packets, &mut forwards).expect("rx outcomes");

        let observations = outcome.telemetry.wfb_channel_observations;
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].source_channel_id_hex, "0x00010203");
        assert_eq!(observations[0].source_link_id, 0x000102);
        assert_eq!(observations[0].source_radio_port, 0x03);
        assert_eq!(observations[0].destination_channel_id_hex, "0x00010203");
        assert_eq!(observations[0].count, 1);
    }

    #[test]
    fn production_rx_handler_tracks_signal_ranges() {
        let mut second = runtime_rx_frame(vec![0x08; 24]);
        second.rssi_dbm = -55;
        second.snr_db = Some(25);
        second.noise_dbm = Some(-90);
        let packets = vec![
            ParsedRxPacket {
                consumed: 64,
                outcome: RxParseOutcome::Frame,
                frame: Some(runtime_rx_frame(vec![0x08; 24])),
            },
            ParsedRxPacket {
                consumed: 64,
                outcome: RxParseOutcome::Frame,
                frame: Some(second),
            },
        ];
        let mut forwards = Vec::new();

        let outcome =
            process_production_rx_packet_outcomes(&packets, &mut forwards).expect("rx outcomes");
        let signal = outcome.telemetry.signal;

        assert_eq!(signal.rssi_dbm.sample_count, 2);
        assert_eq!(signal.rssi_dbm.min, Some(-55));
        assert_eq!(signal.rssi_dbm.max, Some(-47));
        assert_eq!(signal.rssi_dbm.average, Some(-51));
        assert_eq!(signal.snr_db.min, Some(25));
        assert_eq!(signal.snr_db.max, Some(45));
        assert_eq!(signal.snr_db.average, Some(35));
        assert_eq!(signal.noise_dbm.min, Some(-92));
        assert_eq!(signal.noise_dbm.max, Some(-90));
        assert_eq!(signal.noise_dbm.average, Some(-91));
    }

    #[test]
    fn production_rx_handler_forwards_matching_wfb_frame_to_aggregator() {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("receiver");
        receiver
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("timeout");
        let channel_id = WfbChannelId::new(0x000001, 0x23).expect("channel ID");
        let plans = vec![rx_forward_plan(
            channel_id,
            Some(receiver.local_addr().unwrap()),
        )];
        let mut forwards = create_production_rx_forward_runtimes(&plans).expect("forwards");
        let frame = runtime_wfb_frame(channel_id);
        let packets = vec![ParsedRxPacket {
            consumed: 64,
            outcome: RxParseOutcome::Frame,
            frame: Some(frame),
        }];

        let outcome =
            process_production_rx_packet_outcomes(&packets, &mut forwards).expect("rx outcomes");

        let mut buf = [0u8; 512];
        let (bytes, _) = receiver.recv_from(&mut buf).expect("forwarded datagram");
        assert!(bytes > b"runtime-rx".len());
        assert_eq!(outcome.rx_forwards[0].counters.received, 1);
        assert_eq!(outcome.rx_forwards[0].counters.matched, 1);
        assert_eq!(outcome.rx_forwards[0].counters.forwarded, 1);
        assert_eq!(outcome.rx_forwards[0].forwarded_bytes, bytes as u64);
    }

    #[test]
    fn production_rx_handler_filters_without_aggregator_send() {
        let channel_id = WfbChannelId::new(0x000001, 0x23).expect("channel ID");
        let other_channel_id = WfbChannelId::new(0x000002, 0x23).expect("channel ID");
        let plans = vec![rx_forward_plan(channel_id, None)];
        let mut forwards = create_production_rx_forward_runtimes(&plans).expect("forwards");
        let frame = runtime_wfb_frame(other_channel_id);
        let packets = vec![ParsedRxPacket {
            consumed: 64,
            outcome: RxParseOutcome::Frame,
            frame: Some(frame),
        }];

        let outcome =
            process_production_rx_packet_outcomes(&packets, &mut forwards).expect("rx outcomes");

        assert_eq!(outcome.telemetry.parsed_frames, 1);
        assert_eq!(outcome.rx_forwards[0].counters.received, 1);
        assert_eq!(outcome.rx_forwards[0].counters.filtered, 1);
        assert_eq!(outcome.rx_forwards[0].counters.forwarded, 0);
        assert_eq!(outcome.rx_forwards[0].forwarded_bytes, 0);
    }

    #[test]
    fn production_rx_handler_reports_forward_send_failure() {
        let channel_id = WfbChannelId::new(0x000001, 0x23).expect("channel ID");
        let plans = vec![rx_forward_plan(
            channel_id,
            Some("127.0.0.1:0".parse().expect("port zero")),
        )];
        let mut forwards = create_production_rx_forward_runtimes(&plans).expect("forwards");
        let frame = runtime_wfb_frame(channel_id);
        let packets = vec![ParsedRxPacket {
            consumed: 64,
            outcome: RxParseOutcome::Frame,
            frame: Some(frame),
        }];

        let error = process_production_rx_packet_outcomes(&packets, &mut forwards)
            .expect_err("port zero send should fail");

        assert_eq!(error.code, "rx_forward_send_failed");
        let snapshot = production_rx_forward_snapshots(&forwards);
        assert_eq!(snapshot[0].counters.received, 1);
        assert_eq!(snapshot[0].counters.matched, 1);
        assert_eq!(snapshot[0].counters.send_failed, 1);
    }

    #[test]
    fn production_bridge_tx_handler_submits_valid_datagram() {
        let mut session = runtime_tx_session(MockTransport::default());
        let queued = queued_tx_datagram(valid_wfb_tx_datagram());
        let mut bridge_counters = TxCounters::default();
        let mut submit_counters = TxSubmitCounters::default();

        let outcome = handle_production_bridge_tx_datagram(
            &mut session,
            &queued,
            bridge_tx_config(),
            &mut bridge_counters,
            &mut submit_counters,
        )
        .expect("tx datagram handled");

        let metadata = outcome.metadata.expect("metadata");
        assert_eq!(metadata.peer, queued.peer);
        assert_eq!(metadata.datagram_len, queued.data.len());
        assert_eq!(metadata.fwmark, 0x0102_0304);
        assert_eq!(metadata.radiotap_len, 13);
        assert_eq!(metadata.frame_len, 24);
        assert_eq!(
            metadata.tx_profile,
            ProductionRuntimeBridgeTxProfile::LinuxMonitor
        );
        assert_eq!(metadata.tx_options.queue, TxQueue::Mgnt);
        assert_eq!(
            metadata.tx_descriptor_preview_hex.len(),
            super::TX_DESC_SIZE * 2
        );
        assert_eq!(outcome.datagram_bytes, queued.data.len() as u64);
        assert_eq!(outcome.frame_bytes, 24);
        assert_eq!(outcome.bridge_counters.incoming, 1);
        assert_eq!(outcome.bridge_counters.injected, 1);
        assert_eq!(outcome.submit_counters.submitted, 1);
        assert_eq!(session.transport.bulk_writes.len(), 1);
    }

    #[test]
    fn production_bridge_tx_handler_counts_malformed_datagram() {
        let mut session = runtime_tx_session(MockTransport::default());
        let queued = queued_tx_datagram(vec![0u8; 4]);
        let mut bridge_counters = TxCounters::default();
        let mut submit_counters = TxSubmitCounters::default();

        let outcome = handle_production_bridge_tx_datagram(
            &mut session,
            &queued,
            bridge_tx_config(),
            &mut bridge_counters,
            &mut submit_counters,
        )
        .expect("malformed datagram is non-fatal");

        assert!(outcome.metadata.is_none());
        assert_eq!(outcome.datagram_bytes, 4);
        assert_eq!(outcome.frame_bytes, 0);
        assert_eq!(outcome.bridge_counters.incoming, 1);
        assert_eq!(outcome.bridge_counters.dropped, 1);
        assert_eq!(outcome.bridge_counters.malformed, 1);
        assert_eq!(outcome.submit_counters.attempted, 0);
        assert!(session.transport.bulk_writes.is_empty());
    }

    #[test]
    fn production_bridge_tx_handler_counts_descriptor_build_rejection() {
        let mut session = runtime_tx_session(MockTransport::default());
        let queued = queued_tx_datagram(valid_wfb_tx_datagram());
        let mut bridge_counters = TxCounters::default();
        let mut submit_counters = TxSubmitCounters::default();
        let mut config = bridge_tx_config();
        config.channel = Channel::from_number(165).expect("channel 165");
        config.overrides.tx_bandwidth = Some(Bandwidth::Mhz80);

        let outcome = handle_production_bridge_tx_datagram(
            &mut session,
            &queued,
            config,
            &mut bridge_counters,
            &mut submit_counters,
        )
        .expect("descriptor rejection is non-fatal");

        assert!(outcome.metadata.is_none());
        assert_eq!(outcome.frame_bytes, 24);
        assert_eq!(outcome.bridge_counters.incoming, 1);
        assert_eq!(outcome.bridge_counters.dropped, 1);
        assert_eq!(outcome.bridge_counters.malformed, 1);
        assert_eq!(outcome.submit_counters.attempted, 0);
        assert!(session.transport.bulk_writes.is_empty());
    }

    #[test]
    fn production_bridge_tx_handler_reports_radio_submit_failure() {
        let mut session = runtime_tx_session(MockTransport::with_short_bulk_write(1));
        let queued = queued_tx_datagram(valid_wfb_tx_datagram());
        let mut bridge_counters = TxCounters::default();
        let mut submit_counters = TxSubmitCounters::default();

        let error = handle_production_bridge_tx_datagram(
            &mut session,
            &queued,
            bridge_tx_config(),
            &mut bridge_counters,
            &mut submit_counters,
        )
        .expect_err("short write should fail submission");

        assert_eq!(error.code, "tx_submit_failed");
        assert!(error.message.contains("radio TX failed"));
        assert!(error.metadata.is_some());
        assert_eq!(error.bridge_counters.incoming, 1);
        assert_eq!(error.bridge_counters.dropped, 1);
        assert_eq!(error.submit_counters.attempted, 1);
        assert_eq!(error.submit_counters.failed, 1);
        assert_eq!(error.submit_counters.short_writes, 1);
        assert_eq!(session.transport.bulk_writes.len(), 1);
    }

    #[test]
    fn production_bridge_loop_executor_drains_bounded_tx_bursts() {
        let config = ProductionRuntimeBridgeLoopRunConfig::from_bounds(0, 1, 2, 3);
        let mut tx_remaining = 3u64;
        let mut tx_in_burst = 0u32;
        let mut max_burst_seen = 0u32;
        let mut rx_polls = 0u32;

        let outcome = run_production_bridge_loop(
            config,
            |_| {},
            || false,
            |step| -> Result<ProductionRuntimeBridgeLoopStepOutcome, std::convert::Infallible> {
                match step {
                    ProductionRuntimeBridgeLoopStep::TryTx if tx_remaining > 0 => {
                        tx_remaining -= 1;
                        tx_in_burst += 1;
                        max_burst_seen = max_burst_seen.max(tx_in_burst);
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxProcessed)
                    }
                    ProductionRuntimeBridgeLoopStep::TryTx => {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxEmpty)
                    }
                    ProductionRuntimeBridgeLoopStep::ReadRx { .. } => {
                        rx_polls += 1;
                        tx_in_burst = 0;
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::RxTimeout)
                    }
                }
            },
        )
        .expect("loop outcome");

        assert_eq!(
            outcome.stop_reason,
            ProductionRuntimeBridgeLoopStopReason::TxDatagramLimit
        );
        assert_eq!(outcome.tx_datagrams_processed, 3);
        assert_eq!(max_burst_seen, 2);
        assert!(rx_polls >= 1);
        assert!(outcome.rx_polls >= 1);
    }

    #[test]
    fn production_bridge_loop_executor_stops_on_signal_before_work() {
        let config = ProductionRuntimeBridgeLoopRunConfig::from_bounds(0, 1, 8, 0);

        let outcome = run_production_bridge_loop(
            config,
            |_| panic!("signal stop should avoid iteration tick"),
            || true,
            |_step| -> Result<ProductionRuntimeBridgeLoopStepOutcome, std::convert::Infallible> {
                panic!("signal stop should avoid loop work")
            },
        )
        .expect("loop outcome");

        assert_eq!(
            outcome.stop_reason,
            ProductionRuntimeBridgeLoopStopReason::Signal
        );
        assert_eq!(outcome.tx_polls, 0);
        assert_eq!(outcome.rx_polls, 0);
    }

    #[test]
    fn production_bridge_loop_executor_keeps_duration_bounded_after_max_datagrams() {
        let config = ProductionRuntimeBridgeLoopRunConfig::from_bounds(5, 20, 8, 1);
        let mut tx_processed = false;
        let mut saw_rx_after_tx_limit = false;

        let outcome = run_production_bridge_loop(
            config,
            |_| {},
            || false,
            |step| -> Result<ProductionRuntimeBridgeLoopStepOutcome, std::convert::Infallible> {
                match step {
                    ProductionRuntimeBridgeLoopStep::TryTx if !tx_processed => {
                        tx_processed = true;
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxProcessed)
                    }
                    ProductionRuntimeBridgeLoopStep::TryTx => {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxEmpty)
                    }
                    ProductionRuntimeBridgeLoopStep::ReadRx { .. } => {
                        if tx_processed {
                            saw_rx_after_tx_limit = true;
                        }
                        std::thread::sleep(Duration::from_millis(6));
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::RxTimeout)
                    }
                }
            },
        )
        .expect("loop outcome");

        assert_eq!(
            outcome.stop_reason,
            ProductionRuntimeBridgeLoopStopReason::DurationElapsed
        );
        assert_eq!(outcome.tx_datagrams_processed, 1);
        assert!(saw_rx_after_tx_limit);
        assert!(outcome.rx_polls >= 1);
    }

    #[test]
    fn production_bridge_loop_executor_clamps_rx_timeout_to_deadline() {
        let config = ProductionRuntimeBridgeLoopRunConfig::from_bounds(10, 1_000, 8, 0);
        let mut observed_timeout = None;

        let outcome = run_production_bridge_loop(
            config,
            |_| {},
            || false,
            |step| -> Result<ProductionRuntimeBridgeLoopStepOutcome, std::convert::Infallible> {
                match step {
                    ProductionRuntimeBridgeLoopStep::TryTx => {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxEmpty)
                    }
                    ProductionRuntimeBridgeLoopStep::ReadRx { timeout } => {
                        observed_timeout = Some(timeout);
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::Stop(
                            ProductionRuntimeBridgeLoopStopReason::DurationElapsed,
                        ))
                    }
                }
            },
        )
        .expect("loop outcome");

        assert_eq!(
            outcome.stop_reason,
            ProductionRuntimeBridgeLoopStopReason::DurationElapsed
        );
        assert!(
            observed_timeout.expect("observed timeout") < Duration::from_secs(1),
            "bounded run should clamp RX timeout to remaining duration"
        );
    }

    #[test]
    fn production_airtime_schedule_tdd_gates_tx_windows() {
        let schedule =
            ProductionRuntimeAirtimeSchedule::tdd(ProductionRuntimeTddWindow::Rx, 100, 50, 10, 20);

        assert_eq!(schedule.mode, ProductionRuntimeAirtimeMode::Tdd);
        assert!(!schedule.tx_allowed_at_elapsed(Duration::from_millis(0)));
        assert!(!schedule.tx_allowed_at_elapsed(Duration::from_millis(119)));
        assert!(schedule.tx_allowed_at_elapsed(Duration::from_millis(130)));
        assert!(schedule.tx_allowed_at_elapsed(Duration::from_millis(179)));
        assert!(!schedule.tx_allowed_at_elapsed(Duration::from_millis(180)));
        assert!(!schedule.tx_allowed_at_elapsed(Duration::from_millis(189)));
        assert!(!schedule.tx_allowed_at_elapsed(Duration::from_millis(190)));
    }

    #[test]
    fn production_bridge_loop_executor_gates_tx_until_tdd_tx_window() {
        let config = ProductionRuntimeBridgeLoopRunConfig::from_bounds(6, 1, 4, 0)
            .with_airtime_schedule(ProductionRuntimeAirtimeSchedule::tdd(
                ProductionRuntimeTddWindow::Rx,
                100,
                100,
                0,
                0,
            ));
        let mut saw_tx = false;

        let outcome = run_production_bridge_loop(
            config,
            |_| {},
            || false,
            |step| -> Result<ProductionRuntimeBridgeLoopStepOutcome, std::convert::Infallible> {
                match step {
                    ProductionRuntimeBridgeLoopStep::TryTx => {
                        saw_tx = true;
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxEmpty)
                    }
                    ProductionRuntimeBridgeLoopStep::ReadRx { .. } => {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::RxTimeout)
                    }
                }
            },
        )
        .expect("loop outcome");

        assert_eq!(
            outcome.stop_reason,
            ProductionRuntimeBridgeLoopStopReason::DurationElapsed
        );
        assert!(!saw_tx);
        assert_eq!(outcome.tx_polls, 0);
        assert_eq!(outcome.airtime_tx_allowed_iterations, 0);
        assert!(outcome.airtime_tx_gated_iterations > 0);
    }

    #[test]
    fn production_bridge_loop_executor_fires_iteration_tick_per_outer_iteration() {
        let config = ProductionRuntimeBridgeLoopRunConfig::from_bounds(0, 1, 4, 2);
        let mut tx_remaining = 2u64;
        let mut tick_count = 0u32;

        let outcome = run_production_bridge_loop(
            config,
            |_now| {
                tick_count += 1;
            },
            || false,
            |step| -> Result<ProductionRuntimeBridgeLoopStepOutcome, std::convert::Infallible> {
                match step {
                    ProductionRuntimeBridgeLoopStep::TryTx if tx_remaining > 0 => {
                        tx_remaining -= 1;
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxProcessed)
                    }
                    ProductionRuntimeBridgeLoopStep::TryTx => {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::TxEmpty)
                    }
                    ProductionRuntimeBridgeLoopStep::ReadRx { .. } => {
                        Ok(ProductionRuntimeBridgeLoopStepOutcome::RxTimeout)
                    }
                }
            },
        )
        .expect("loop outcome");

        assert_eq!(
            outcome.stop_reason,
            ProductionRuntimeBridgeLoopStopReason::TxDatagramLimit
        );
        // Tick fires once per outer loop iteration. Two TX bursts of one
        // each plus a final iteration that hits the TX-datagram limit
        // before the tick: at minimum, tick_count should equal the
        // number of fully-entered iterations (i.e. iterations that ran
        // any work).
        assert!(
            u64::from(tick_count) >= outcome.iterations.saturating_sub(1),
            "iteration tick should fire on every iteration that does work; \
             got tick_count={tick_count}, iterations={iterations}",
            iterations = outcome.iterations
        );
    }

    #[test]
    fn production_runtime_types_serialize_without_diagnostic_register_fields() {
        let config = production_runtime_flow_config();
        let report = ProductionRuntimeFlowReport::from_execution(
            &config,
            super::ProductionRuntimeFlowExecutionReport {
                selector: config.usb.selector,
                adapter: None,
                endpoints: None,
                channel: Some(config.channel),
                bandwidth: config.bandwidth,
                duration_ms: config.duration_ms,
                ready_file: config.ready_file.clone(),
                stop_reason: "duration_elapsed",
                bulk_in_endpoint: Some(0x81),
                bulk_out_endpoint: Some(0x02),
                calibration_profile: config.calibration_profile,
                calibration_class: config
                    .calibration_profile
                    .before_tx_class(config.captured_tail_applied),
                tx_power_control: None,
                tx_calibration_profile: None,
                rx_startup_kick: None,
                heartbeat_led: Some(ProductionRuntimeHeartbeatLedReport {
                    enabled: true,
                    half_period_ms: DEFAULT_HEARTBEAT_HALF_PERIOD_MS,
                    toggles_attempted: 4,
                    toggles_succeeded: 4,
                    toggles_failed: 0,
                }),
                airtime: ProductionRuntimeAirtimeReport::idle(config.airtime_schedule),
                receiver_backed_validation_required: false,
                init: Default::default(),
                rx: RuntimeFlowRxTelemetry::default(),
                tx: RuntimeFlowTxTelemetry::default(),
                counters: RuntimeRadioCounters::default(),
                result: ProductionRuntimeFlowResult::Pass,
                error: None,
            },
        );
        assert_eq!(report.schema_version, 1);
        assert_eq!(report.command, "radio-run");
        assert_eq!(
            report.calibration_evidence_source,
            RuntimeTxCalibrationEvidenceSource::CapturedLinuxTail
        );
        assert_eq!(
            report.heartbeat_led.as_ref().map(|heartbeat| (
                heartbeat.enabled,
                heartbeat.half_period_ms,
                heartbeat.toggles_attempted,
                heartbeat.toggles_succeeded,
                heartbeat.toggles_failed,
            )),
            Some((true, DEFAULT_HEARTBEAT_HALF_PERIOD_MS, 4, 4, 0))
        );
        let config_json = serde_json::to_string(&config).expect("config JSON");
        let report_json = serde_json::to_string(&report).expect("report JSON");
        assert!(report_json.contains("heartbeat_led"));
        for field in [
            "pre_tx_write",
            "pre_tx_rmw",
            "pre_tx_rf_write",
            "tx_status",
            "clear_txdma_status",
            "txdma_status_clear",
        ] {
            assert!(!config_json.contains(field), "config leaked {field}");
            assert!(!report_json.contains(field), "report leaked {field}");
        }
    }

    #[test]
    fn macos_usbhost_config_derives_endpoint_layout() {
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");

        assert_eq!(endpoints.interface_number, 0);
        assert_eq!(endpoints.bulk_in, Some(0x81));
        assert_eq!(endpoints.bulk_out, Some(0x02));
        assert_eq!(endpoints.bulk_in_all, vec![0x81]);
        assert_eq!(endpoints.bulk_out_all, vec![0x02, 0x03, 0x04]);

        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        assert_eq!(adapter.vid_hex, "0x0bda");
        assert_eq!(adapter.pid_hex, "0x8812");
        assert_eq!(adapter.interfaces[0].endpoints.len(), 4);
    }

    #[test]
    fn macos_usbhost_config_rejects_invalid_endpoints() {
        let mut config = MacosUsbHostConfig {
            bulk_in_endpoint: 0x01,
            ..MacosUsbHostConfig::default()
        };
        assert_eq!(
            macos_usbhost_endpoints(&config)
                .expect_err("invalid bulk IN")
                .code,
            "invalid_macos_bulk_in_endpoint"
        );

        config = MacosUsbHostConfig {
            bulk_out_endpoint: 0x82,
            ..MacosUsbHostConfig::default()
        };
        assert_eq!(
            macos_usbhost_endpoints(&config)
                .expect_err("invalid bulk OUT")
                .code,
            "invalid_macos_bulk_out_endpoint"
        );

        config = MacosUsbHostConfig {
            bulk_out_endpoint: 0x05,
            bulk_out_endpoint_count: 3,
            ..MacosUsbHostConfig::default()
        };
        assert_eq!(
            macos_usbhost_endpoints(&config)
                .expect_err("OUT not in layout")
                .code,
            "macos_bulk_out_endpoint_not_in_layout"
        );
    }

    #[test]
    fn runtime_radio_session_carries_metadata_endpoints_and_counters() {
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        let mut session = RuntimeRadioSession::new(
            MockTransport::default(),
            adapter,
            endpoints,
            RuntimeRadioCounters {
                usb_control_writes: 3,
                ..RuntimeRadioCounters::default()
            },
        );

        assert_eq!(session.adapter.vid_hex, "0x0bda");
        assert_eq!(session.selected_bulk_in_endpoint(), Some(0x81));
        assert_eq!(session.selected_bulk_out_endpoint(), Some(0x02));
        assert_eq!(session.counters.usb_control_writes, 3);

        session.add_counters(RuntimeRadioCounters {
            usb_control_reads: 2,
            usb_bulk_out_writes: 4,
            tx_frames: 5,
            ..RuntimeRadioCounters::default()
        });
        assert_eq!(session.counters.usb_control_reads, 2);
        assert_eq!(session.counters.usb_control_writes, 3);
        assert_eq!(session.counters.usb_bulk_out_writes, 4);
        assert_eq!(session.counters.tx_frames, 5);

        let registers = session.register_access();
        assert_eq!(registers.read8(0x1234).expect("mock register read"), 0);
    }

    #[test]
    fn runtime_radio_session_submits_tx_and_updates_runtime_counters() {
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        let mut session = RuntimeRadioSession::new(
            MockTransport::default(),
            adapter,
            endpoints,
            RuntimeRadioCounters::default(),
        );
        let mut submit_counters = TxSubmitCounters::default();
        let channel = Channel::from_number(36).expect("channel 36");
        let frame = [0x08, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];

        let written = session
            .submit_80211_frame(&frame, channel, TxOptions::default(), &mut submit_counters)
            .expect("tx submit");

        assert!(written > frame.len());
        assert_eq!(session.transport.bulk_writes.len(), 1);
        assert_eq!(session.transport.bulk_writes[0].0, 0x02);
        assert_eq!(submit_counters.attempted, 1);
        assert_eq!(submit_counters.submitted, 1);
        assert_eq!(session.counters.usb_bulk_out_writes, 1);
        assert_eq!(session.counters.tx_frames, 1);
        assert_eq!(session.counters.dropped_frames, 0);
    }

    #[test]
    fn production_rx_startup_kick_submits_non_wfb_null_frame() {
        let config = production_runtime_flow_config();
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        let mut session = RuntimeRadioSession::new(
            MockTransport::default(),
            adapter,
            endpoints,
            RuntimeRadioCounters::default(),
        );

        let report = super::apply_production_runtime_rx_startup_kick(&mut session, &config)
            .expect("rx startup kick");

        assert_eq!(
            report.frame_len,
            super::PRODUCTION_RX_STARTUP_KICK_FRAME.len()
        );
        assert_eq!(report.attempted, 1);
        assert_eq!(report.submitted, 1);
        assert_eq!(report.failed, 0);
        assert_eq!(report.tx_options.queue, TxQueue::Mgnt);
        assert_eq!(session.transport.bulk_writes.len(), 1);
        let packet = &session.transport.bulk_writes[0].1;
        assert_eq!(packet[super::TX_DESC_SIZE], 0x48);
        assert_ne!(
            &packet[super::TX_DESC_SIZE + 10..super::TX_DESC_SIZE + 12],
            b"WB"
        );
        assert_eq!(session.counters.usb_bulk_out_writes, 1);
        assert_eq!(session.counters.tx_frames, 1);
    }

    #[test]
    fn runtime_radio_session_submits_raw_tx_packet_and_updates_counters() {
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        let mut session = RuntimeRadioSession::new(
            MockTransport::default(),
            adapter,
            endpoints,
            RuntimeRadioCounters::default(),
        );
        let packet = [0xa5; 48];
        let mut submit_counters = TxSubmitCounters::default();

        let written = session
            .submit_raw_tx_packet(&packet, &mut submit_counters, Duration::from_millis(10))
            .expect("raw tx packet submit");

        assert_eq!(written, packet.len());
        assert_eq!(session.transport.bulk_writes.len(), 1);
        assert_eq!(session.transport.bulk_writes[0].0, 0x02);
        assert_eq!(session.transport.bulk_writes[0].1, packet);
        assert_eq!(submit_counters.attempted, 1);
        assert_eq!(submit_counters.submitted, 1);
        assert_eq!(submit_counters.bytes_written, packet.len() as u64);
        assert_eq!(session.counters.usb_bulk_out_writes, 1);
        assert_eq!(session.counters.tx_frames, 1);
        assert_eq!(session.counters.dropped_frames, 0);
    }

    #[test]
    fn runtime_radio_session_reads_and_parses_rx_packets() {
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        let mut rx_buffer = vec![0u8; 24 + 10 + 4];
        rx_buffer[0..4].copy_from_slice(&(14u32).to_le_bytes());
        rx_buffer[12] = 0x04;
        rx_buffer[24..34].copy_from_slice(&[0x08, 0, 0, 0, 1, 2, 3, 4, 5, 6]);
        let mut session = RuntimeRadioSession::new(
            MockTransport::with_bulk_read(rx_buffer),
            adapter,
            endpoints,
            RuntimeRadioCounters::default(),
        );
        let mut read_buffer = vec![0u8; 2048];
        let channel = Channel::from_number(36).expect("channel 36");

        let read = session
            .read_rx_packets(channel, &mut read_buffer, Duration::from_millis(10))
            .expect("rx read");

        assert_eq!(read.endpoint, 0x81);
        assert_eq!(read.bytes_read, 38);
        assert_eq!(read.packets.len(), 1);
        assert_eq!(
            read.packets[0].frame.as_ref().expect("frame").data.len(),
            10
        );
        assert_eq!(read.counters.usb_bulk_in_reads, 1);
        assert_eq!(read.counters.rx_frames, 1);
        assert_eq!(session.counters.usb_bulk_in_reads, 1);
        assert_eq!(session.counters.rx_frames, 1);
        assert_eq!(session.counters.dropped_frames, 0);
    }

    #[test]
    fn runtime_radio_session_preserves_need_more_data_rx_outcome() {
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        let mut session = RuntimeRadioSession::new(
            MockTransport::with_bulk_read(vec![0u8; 8]),
            adapter,
            endpoints,
            RuntimeRadioCounters::default(),
        );
        let mut read_buffer = vec![0u8; 2048];
        let channel = Channel::from_number(36).expect("channel 36");

        let read = session
            .read_rx_packets(channel, &mut read_buffer, Duration::from_millis(10))
            .expect("rx read");

        assert_eq!(read.bytes_read, 8);
        assert_eq!(read.packets.len(), 1);
        assert_eq!(read.packets[0].outcome, RxParseOutcome::NeedMoreData);
        assert_eq!(read.counters.usb_bulk_in_reads, 1);
        assert_eq!(read.counters.rx_frames, 0);
        assert_eq!(read.counters.dropped_frames, 0);
    }

    #[test]
    fn rtl8812au_default_init_sequence_runs_firmware_before_llt() {
        let sequence = super::rtl8812au_same_session_init_sequence(Rtl8812auInitOrder::Default);

        assert_eq!(sequence[0], Rtl8812auInitPhase::PowerOn);
        assert_eq!(
            super::rtl8812au_llt_firmware_sequence(Rtl8812auInitOrder::Default),
            &[Rtl8812auInitPhase::Firmware, Rtl8812auInitPhase::Llt]
        );
        assert_eq!(
            sequence.last(),
            Some(&Rtl8812auInitPhase::RfCalibrationBeforeTx)
        );
        assert_eq!(
            Rtl8812auInitPhase::TxSchedulerTail.id(),
            "tx_scheduler_tail"
        );
    }

    #[test]
    fn rtl8812au_linux_init_sequence_runs_llt_before_firmware() {
        let sequence = super::rtl8812au_same_session_init_sequence(Rtl8812auInitOrder::Linux);

        assert_eq!(sequence[0], Rtl8812auInitPhase::PowerOn);
        assert_eq!(
            super::rtl8812au_llt_firmware_sequence(Rtl8812auInitOrder::Linux),
            &[Rtl8812auInitPhase::Llt, Rtl8812auInitPhase::Firmware]
        );
        assert_eq!(
            sequence.last(),
            Some(&Rtl8812auInitPhase::RfCalibrationBeforeTx)
        );
    }

    #[test]
    fn same_session_init_executor_records_ready_phase_summaries() {
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        let channel = Channel::from_number(36).expect("channel 36");
        let mut session = RuntimeRadioSession::new(
            MockTransport::default(),
            adapter,
            endpoints,
            RuntimeRadioCounters::default(),
        );
        let mut config = RuntimeSameSessionInitConfig::new(channel, Bandwidth::Mhz20);
        config.init_order = Rtl8812auInitOrder::Linux;

        let result =
            super::run_rtl8812au_same_session_init(&mut session, config, |session, phase| {
                let before = session.counters;
                session.counters.usb_control_writes =
                    session.counters.usb_control_writes.saturating_add(1);
                Ok(RuntimeSameSessionInitPhaseSummary::completed_with_writes(
                    phase,
                    format!("completed {}", phase.id()),
                    1,
                    before,
                    session.counters,
                ))
            })
            .expect("same-session init");

        assert_eq!(result.readiness, RuntimeSameSessionInitReadiness::Ready);
        assert_eq!(
            result.phase_summaries[1].phase,
            Rtl8812auInitPhase::Llt,
            "Linux order runs LLT before firmware"
        );
        assert_eq!(result.phase_summaries.len(), 14);
        assert_eq!(result.counters.usb_control_writes, 14);
        assert_eq!(
            result.phase_summaries[0].status,
            RuntimeSameSessionInitPhaseStatus::Completed
        );
        assert_eq!(result.phase_summaries[0].register_writes, Some(1));
    }

    #[test]
    fn same_session_init_executor_returns_partial_failure() {
        let endpoints =
            macos_usbhost_endpoints(&MacosUsbHostConfig::default()).expect("default endpoints");
        let adapter = macos_usbhost_adapter_info(0x0bda, 0x8812, &endpoints);
        let channel = Channel::from_number(36).expect("channel 36");
        let mut session = RuntimeRadioSession::new(
            MockTransport::default(),
            adapter,
            endpoints,
            RuntimeRadioCounters::default(),
        );
        let config = RuntimeSameSessionInitConfig::new(channel, Bandwidth::Mhz20);

        let failure =
            super::run_rtl8812au_same_session_init(&mut session, config, |session, phase| {
                let before = session.counters;
                if phase == Rtl8812auInitPhase::Llt {
                    let summary = RuntimeSameSessionInitPhaseSummary::blocked(
                        phase,
                        "LLT failed in test",
                        before,
                        session.counters,
                    );
                    return Err(RuntimeSameSessionInitPhaseFailure::new(
                        summary,
                        RuntimeRadioError::new("llt_failed", "test failure"),
                    ));
                }
                session.counters.usb_control_writes =
                    session.counters.usb_control_writes.saturating_add(1);
                Ok(RuntimeSameSessionInitPhaseSummary::completed(
                    phase,
                    format!("completed {}", phase.id()),
                    before,
                    session.counters,
                ))
            })
            .expect_err("LLT should fail");

        assert_eq!(failure.error.code, "llt_failed");
        assert_eq!(
            failure.result.readiness,
            RuntimeSameSessionInitReadiness::Failed
        );
        assert_eq!(failure.result.phase_summaries.len(), 3);
        assert_eq!(
            failure.result.phase_summaries[2].status,
            RuntimeSameSessionInitPhaseStatus::Blocked
        );
    }

    #[test]
    fn tx_scheduler_tail_writes_linux_tail_registers() {
        let transport = MockTransport::with_u8(super::REG_QUEUE_CTRL, 0xff);
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let execution =
            super::run_rtl8812au_tx_scheduler_tail(&registers, &mut counters).expect("tail");

        assert_eq!(execution.phase, Rtl8812auInitPhase::TxSchedulerTail);
        assert_eq!(
            execution.register_writes,
            super::rtl8812au_tx_scheduler_tail_expected_writes()
        );
        assert_eq!(execution.counters.usb_control_reads, 1);
        assert_eq!(execution.counters.usb_control_writes, 8);
        assert_eq!(counters.usb_control_reads, 1);
        assert_eq!(counters.usb_control_writes, 8);

        assert_eq!(
            transport.register_bytes(super::REG_QUEUE_CTRL),
            Some(vec![0xf7])
        );
        assert_eq!(
            transport.writes(),
            vec![
                (super::REG_QUEUE_CTRL, vec![0xf7]),
                (super::REG_FWHW_TXQ_CTRL + 1, vec![0x0f]),
                (super::REG_EARLY_MODE_CONTROL_8812 + 3, vec![0x01]),
                (super::REG_SDIO_CTRL_8812, vec![0x00]),
                (super::REG_ACLK_MON, vec![0x00]),
                (super::REG_USB_HRPWM, vec![0x00]),
                (super::REG_NAV_UPPER, vec![0x00]),
                (super::REG_TX_RPT_TIME, vec![0xf0, 0x3d]),
            ]
        );
    }

    #[test]
    fn monitor_receive_filter_programs_rcr_without_changing_msr() {
        let transport = MockTransport::with_u8(super::REG_MSR, 0x02);
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let execution = super::run_rtl8812au_monitor_receive_filter(&registers, &mut counters)
            .expect("monitor filter");

        assert_eq!(execution.msr_before, 0x02);
        assert_eq!(execution.msr_written, 0x02);
        assert_eq!(execution.msr_after, 0x02);
        assert_eq!(
            execution.rcr_written,
            super::rtl8812au_monitor_receive_config()
        );
        assert_eq!(
            execution.rcr_after,
            super::rtl8812au_monitor_receive_config()
        );
        assert_eq!(execution.rxfltmap2_written, u16::MAX);
        assert_eq!(execution.rxfltmap2_after, u16::MAX);
        assert_eq!(execution.register_writes, 2);
        assert_eq!(execution.counters.usb_control_reads, 3);
        assert_eq!(execution.counters.usb_control_writes, 2);
        assert_eq!(counters.usb_control_reads, 3);
        assert_eq!(counters.usb_control_writes, 2);
        assert_eq!(
            transport.writes(),
            vec![
                (
                    super::REG_RCR,
                    super::rtl8812au_monitor_receive_config()
                        .to_le_bytes()
                        .to_vec(),
                ),
                (super::REG_RXFLTMAP2, u16::MAX.to_le_bytes().to_vec()),
            ]
        );
    }

    #[test]
    fn monitor_opmode_clears_msr_link_type_and_programs_receive_filter() {
        let transport = MockTransport::with_u8(super::REG_MSR, 0x03);
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let execution =
            super::run_rtl8812au_monitor_opmode(&registers, &mut counters).expect("opmode");

        assert_eq!(execution.msr_before, 0x03);
        assert_eq!(execution.msr_written, 0x00);
        assert_eq!(execution.msr_after, 0x00);
        assert_eq!(
            execution.rcr_written,
            super::rtl8812au_monitor_receive_config()
        );
        assert_eq!(
            execution.rcr_after,
            super::rtl8812au_monitor_receive_config()
        );
        assert_eq!(execution.rxfltmap2_written, u16::MAX);
        assert_eq!(execution.rxfltmap2_after, u16::MAX);
        assert_eq!(execution.register_writes, 3);
        assert_eq!(execution.counters.usb_control_reads, 4);
        assert_eq!(execution.counters.usb_control_writes, 3);
        assert_eq!(counters.usb_control_reads, 4);
        assert_eq!(counters.usb_control_writes, 3);
        assert_eq!(
            transport.writes(),
            vec![
                (super::REG_MSR, vec![0x00]),
                (
                    super::REG_RCR,
                    super::rtl8812au_monitor_receive_config()
                        .to_le_bytes()
                        .to_vec(),
                ),
                (super::REG_RXFLTMAP2, u16::MAX.to_le_bytes().to_vec()),
            ]
        );
    }

    #[test]
    fn efuse_logical_mac_address_filters_blank_values() {
        let mut logical = vec![0xff; super::RTL8812AU_EFUSE_LOGICAL_MAP_LEN];
        assert_eq!(super::rtl8812au_efuse_logical_mac_address(&logical), None);

        logical[super::RTL8812AU_EFUSE_MAC_OFFSET..super::RTL8812AU_EFUSE_MAC_OFFSET + 6]
            .copy_from_slice(&[0, 0, 0, 0, 0, 0]);
        assert_eq!(super::rtl8812au_efuse_logical_mac_address(&logical), None);

        logical[super::RTL8812AU_EFUSE_MAC_OFFSET..super::RTL8812AU_EFUSE_MAC_OFFSET + 6]
            .copy_from_slice(&[0x04, 0x31, 0x5d, 0xaa, 0xbb, 0xcc]);
        assert_eq!(
            super::rtl8812au_efuse_logical_mac_address(&logical),
            Some([0x04, 0x31, 0x5d, 0xaa, 0xbb, 0xcc])
        );
    }

    #[test]
    fn efuse_logical_decoder_extracts_extended_header_mac() {
        let raw = vec![
            0x4f, 0x37, 0xff, 0x04, 0x6f, 0x38, 0x31, 0x5d, 0xaa, 0xbb, 0xcc, 0xff, 0xff,
        ];

        let logical = super::rtl8812au_decode_efuse_logical_map(&raw);

        assert_eq!(
            super::rtl8812au_efuse_logical_mac_address(&logical),
            Some([0x04, 0x31, 0x5d, 0xaa, 0xbb, 0xcc])
        );
    }

    #[test]
    fn read_efuse_mac_address_uses_control_sequence_and_decodes_mac() {
        let raw = vec![
            0x4f, 0x37, 0xff, 0x04, 0x6f, 0x38, 0x31, 0x5d, 0xaa, 0xbb, 0xcc, 0xff, 0xff,
        ];
        let transport = MockTransport::with_efuse(raw);
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let mac = super::read_rtl8812au_efuse_mac_address_with_config(
            &registers,
            &mut counters,
            super::RuntimeEfuseReadConfig {
                length: 13,
                poll_attempts: 1,
                poll_delay: Duration::from_micros(0),
            },
        )
        .expect("efuse mac");

        assert_eq!(mac, Some([0x04, 0x31, 0x5d, 0xaa, 0xbb, 0xcc]));
        assert!(counters.usb_control_reads > 0);
        assert!(counters.usb_control_writes > 0);
        let writes = transport.writes();
        assert_eq!(
            writes.first(),
            Some(&(super::REG_EFUSE_BURN_GNT_8812, vec![0x69]))
        );
        assert_eq!(
            writes.last(),
            Some(&(super::REG_EFUSE_BURN_GNT_8812, vec![0x00]))
        );
    }

    #[test]
    fn program_local_mac_writes_reg_macid_bytes() {
        let transport = MockTransport::with_macid([0, 1, 2, 3, 4, 5]);
        let registers = Rtl8812auRegisterAccess::new(&transport);
        let mut counters = RuntimeRadioCounters::default();

        let execution = super::program_rtl8812au_local_mac(
            &registers,
            [0x04, 0x31, 0x5d, 0xaa, 0xbb, 0xcc],
            &mut counters,
        )
        .expect("program mac");

        assert_eq!(execution.before, [0, 1, 2, 3, 4, 5]);
        assert_eq!(execution.written, [0x04, 0x31, 0x5d, 0xaa, 0xbb, 0xcc]);
        assert_eq!(execution.after, [0x04, 0x31, 0x5d, 0xaa, 0xbb, 0xcc]);
        assert_eq!(execution.register_writes, 6);
        assert_eq!(execution.counters.usb_control_reads, 12);
        assert_eq!(execution.counters.usb_control_writes, 6);
        assert_eq!(counters.usb_control_reads, 12);
        assert_eq!(counters.usb_control_writes, 6);
        assert_eq!(
            transport.writes(),
            vec![
                (super::REG_MACID, vec![0x04]),
                (super::REG_MACID + 1, vec![0x31]),
                (super::REG_MACID + 2, vec![0x5d]),
                (super::REG_MACID + 3, vec![0xaa]),
                (super::REG_MACID + 4, vec![0xbb]),
                (super::REG_MACID + 5, vec![0xcc]),
            ]
        );
    }
}
