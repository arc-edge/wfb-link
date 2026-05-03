//! Runtime-facing policy for the native WFB radio backend.
//!
//! This crate owns stable decisions and live transport abstractions that a
//! production runtime, diagnostic harness, or future daemon must agree on
//! without depending on `wfb-radio-diag`.

use std::{error::Error, fmt, time::Duration};

use radio_core::{
    list_usb_devices, rtl8812au::Rtl8812auUsbTransport, ClaimedUsbDevice, DeviceSelector,
    EndpointInfo, InterfaceInfo, Rtl8812auRegisterAccess, Rtl8812auRegisterError, UsbBulkTransfer,
    UsbDeviceInfo, UsbEndpoints, UsbError,
};
use serde::Serialize;

#[cfg(target_os = "macos")]
pub mod macos_usbhost;

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
}

impl RuntimeRadioError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
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
        matches!(self, Self::Rtl8812aRuntimeIqk)
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
    use std::{cell::RefCell, collections::BTreeMap, time::Duration};

    use radio_core::{rtl8812au::Rtl8812auUsbTransport, Rtl8812auRegisterAccess, UsbError};

    use super::{
        macos_usbhost_adapter_info, macos_usbhost_endpoints, MacosUsbHostConfig,
        Rtl8812auInitOrder, Rtl8812auInitPhase, RuntimeRadioCounters, TxCalibrationClass,
        TxCalibrationProfile,
    };

    #[derive(Debug, Default)]
    struct MockTransport {
        registers: RefCell<BTreeMap<u16, Vec<u8>>>,
        efuse: RefCell<Option<Vec<u8>>>,
        writes: RefCell<Vec<(u16, Vec<u8>)>>,
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

    #[test]
    fn runtime_iqk_requires_live_register_write_authorization() {
        assert!(TxCalibrationProfile::Rtl8812aRuntimeIqk.requires_register_write_authorization());

        for profile in [
            TxCalibrationProfile::CurrentDefault,
            TxCalibrationProfile::LinuxParityCh36Ht20,
            TxCalibrationProfile::Rtl8812aLck,
            TxCalibrationProfile::Rtl8812aIqkProbe,
        ] {
            assert!(
                !profile.requires_register_write_authorization(),
                "{} should not require the runtime-IQK write gate",
                profile.name()
            );
        }
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
