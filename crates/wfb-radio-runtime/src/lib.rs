//! Runtime-facing policy for the native WFB radio backend.
//!
//! This crate owns stable decisions and live transport abstractions that a
//! production runtime, diagnostic harness, or future daemon must agree on
//! without depending on `wfb-radio-diag`.

use std::{
    error::Error,
    fmt, io,
    net::{SocketAddr, UdpSocket},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use radio_core::{
    build_tx_packet, list_usb_devices, parse_rx_packet,
    rtl8812au::{Rtl8812auUsbTransport, TxQueue, TX_DESC_SIZE},
    submit_tx_frame, Bandwidth, Channel, ClaimedUsbDevice, DeviceSelector, EndpointInfo,
    InterfaceInfo, ParsedRxPacket, Rtl8812auRegisterAccess, Rtl8812auRegisterError,
    Rtl8812auTxSubmitError, RxParseOutcome, TxOptions, TxSubmitCounters, UsbBulkTransfer,
    UsbDeviceInfo, UsbEndpoints, UsbError,
};
use serde::Serialize;
use wfb_bridge::{
    parse_tx_datagram, RadiotapError, RxForwardConfig, TxCounters, TxDatagramError, WfbChannelId,
};

#[cfg(target_os = "macos")]
pub mod macos_usbhost;

pub const PRODUCTION_TX_SOCKET_RCVBUF_BYTES: usize = 4 * 1024 * 1024;
pub const PRODUCTION_TX_RECEIVE_TIMEOUT: Duration = Duration::from_millis(100);

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeFlowRxTelemetry {
    pub buffers_read: u64,
    pub read_timeouts: u64,
    pub parsed_frames: u64,
    pub phy_status_frames: u64,
    pub rssi_valid_frames: u64,
    pub snr_frames: u64,
    pub noise_frames: u64,
    pub forwarded_payloads: u64,
    pub dropped_packets: u64,
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
        rx_wlan_idx: config.rx_wlan_idx,
        rx_mcs_index: config.rx_mcs_index,
        bandwidth_mhz: config.bandwidth.mhz() as u8,
    })
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
        }
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
}

pub fn run_production_bridge_loop<E, StopRequested, HandleStep>(
    config: ProductionRuntimeBridgeLoopRunConfig,
    mut stop_requested: StopRequested,
    mut handle_step: HandleStep,
) -> Result<ProductionRuntimeBridgeLoopOutcome, E>
where
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

    loop {
        iterations = iterations.saturating_add(1);
        if stop_requested() {
            return Ok(ProductionRuntimeBridgeLoopOutcome {
                stop_reason: ProductionRuntimeBridgeLoopStopReason::Signal,
                tx_datagrams_processed,
                iterations,
                tx_polls,
                rx_polls,
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
                });
            }
        } else if !unlimited_datagrams && tx_datagrams_processed >= config.max_datagrams {
            return Ok(ProductionRuntimeBridgeLoopOutcome {
                stop_reason: ProductionRuntimeBridgeLoopStopReason::TxDatagramLimit,
                tx_datagrams_processed,
                iterations,
                tx_polls,
                rx_polls,
            });
        }

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
                    });
                }
                ProductionRuntimeBridgeLoopStepOutcome::RxRead
                | ProductionRuntimeBridgeLoopStepOutcome::RxTimeout => break,
            }
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
    pub ready_file: Option<PathBuf>,
    pub tx_authorized: bool,
    pub live_register_write_authorized: bool,
    pub calibration_profile: TxCalibrationProfile,
    pub captured_tail_applied: bool,
    pub primary_rx_forward: ProductionRuntimePrimaryRxForwardConfig,
    pub rx_forwards: Vec<ProductionRuntimeRxForwardConfig>,
    pub rx_wlan_idx: u8,
    pub rx_mcs_index: u8,
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
    pub receiver_backed_validation_required: bool,
    pub init: ProductionRuntimeInitTelemetry,
    pub rx: RuntimeFlowRxTelemetry,
    pub tx: RuntimeFlowTxTelemetry,
    pub counters: RuntimeRadioCounters,
    pub result: ProductionRuntimeFlowResult,
    pub error: Option<ProductionRuntimeFlowErrorReport>,
}

impl ProductionRuntimeFlowReport {
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
const REG_SYS_FUNC_EN: u16 = 0x0002;
const REG_SYS_CLKR: u16 = 0x0008;
const REG_CR: u16 = 0x0100;
const REG_MSR: u16 = REG_CR + 2;
const REG_EARLY_MODE_CONTROL_8812: u16 = 0x02bc;
const REG_FWHW_TXQ_CTRL: u16 = 0x0420;
const REG_QUEUE_CTRL: u16 = 0x04c6;
const REG_TX_RPT_TIME: u16 = 0x04f0;
const REG_RCR: u16 = 0x0608;
const REG_MACID: u16 = 0x0610;
const REG_NAV_UPPER: u16 = 0x0652;
const REG_RXFLTMAP2: u16 = 0x06a4;
const REG_USB_HRPWM: u16 = 0xfe58;

const RTL8812AU_EFUSE_REAL_CONTENT_LEN: usize = 512;
const RTL8812AU_EFUSE_LOGICAL_MAP_LEN: usize = 512;
const RTL8812AU_EFUSE_MAX_SECTION: u8 = 64;
const RTL8812AU_EFUSE_MAC_OFFSET: usize = 0x0d7;
const EFUSE_ACCESS_ON_JAGUAR: u8 = 0x69;
const EFUSE_ACCESS_OFF_JAGUAR: u8 = 0x00;

const BIT3: u8 = 1 << 3;
const FEN_ELDR: u16 = 1 << 12;
const ANA8M: u16 = 1 << 1;
const LOADER_CLK_EN: u16 = 1 << 5;
const MSR_PORT0_NETTYPE_MASK: u8 = 0x03;
const RCR_APM: u32 = 1 << 1;
const RCR_AM: u32 = 1 << 2;
const RCR_AB: u32 = 1 << 3;
const RCR_AAP: u32 = 1 << 0;
const RCR_APWRMGT: u32 = 1 << 5;
const RCR_ADF: u32 = 1 << 11;
const RCR_ACF: u32 = 1 << 12;
const RCR_AMF: u32 = 1 << 13;
const RCR_APP_PHYST_RXFF: u32 = 1 << 28;
const RCR_APPFCS: u32 = 1 << 31;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimePhaseExecution {
    pub phase: Rtl8812auInitPhase,
    pub register_writes: usize,
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
        Bandwidth, Channel, DeviceSelector, Rtl8812auRegisterAccess, RxParseOutcome, TxOptions,
        TxSubmitCounters, UsbBulkTransfer, UsbEndpoints, UsbError,
    };

    use super::{
        bind_production_tx_ingress_sockets, handle_production_bridge_tx_datagram,
        macos_usbhost_adapter_info, macos_usbhost_endpoints, plan_production_wfb_loop,
        run_production_bridge_loop, spawn_production_tx_ingress_receivers, MacosUsbHostConfig,
        ProductionRuntimeBridgeLoopRunConfig, ProductionRuntimeBridgeLoopStep,
        ProductionRuntimeBridgeLoopStepOutcome, ProductionRuntimeBridgeLoopStopReason,
        ProductionRuntimeBridgeTxConfig, ProductionRuntimeBridgeTxOverrides,
        ProductionRuntimeBridgeTxProfile, ProductionRuntimeFlowConfig, ProductionRuntimeFlowReport,
        ProductionRuntimeFlowResult, ProductionRuntimePrimaryRxForwardConfig,
        ProductionRuntimeQueuedDatagram, ProductionRuntimeRxForwardConfig,
        ProductionRuntimeUsbConfig, ProductionRuntimeWfbLoopConfig, Rtl8812auInitOrder,
        Rtl8812auInitPhase, RuntimeFlowRxTelemetry, RuntimeFlowTxTelemetry, RuntimeRadioCounters,
        RuntimeRadioError, RuntimeRadioSession, RuntimeSameSessionInitConfig,
        RuntimeSameSessionInitPhaseFailure, RuntimeSameSessionInitPhaseStatus,
        RuntimeSameSessionInitPhaseSummary, RuntimeSameSessionInitReadiness,
        RuntimeTxCalibrationEvidenceSource, RuntimeTxCalibrationValidationStatus,
        TxCalibrationClass, TxCalibrationProfile, PRODUCTION_TX_SOCKET_RCVBUF_BYTES,
    };

    use wfb_bridge::TxCounters;

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
            forwarded_payloads: 3,
            dropped_packets: 4,
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
            ready_file: Some(PathBuf::from("/tmp/radio-run-ready.json")),
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
    fn production_runtime_types_serialize_without_diagnostic_register_fields() {
        let config = production_runtime_flow_config();
        let report = ProductionRuntimeFlowReport {
            schema_version: 1,
            command: "radio-run",
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
            calibration_evidence_source: config
                .calibration_profile
                .evidence_source(config.captured_tail_applied),
            receiver_backed_validation_required: false,
            init: Default::default(),
            rx: RuntimeFlowRxTelemetry::default(),
            tx: RuntimeFlowTxTelemetry::default(),
            counters: RuntimeRadioCounters::default(),
            result: ProductionRuntimeFlowResult::Pass,
            error: None,
        };
        let config_json = serde_json::to_string(&config).expect("config JSON");
        let report_json = serde_json::to_string(&report).expect("report JSON");
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
