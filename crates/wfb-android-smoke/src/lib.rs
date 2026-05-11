use std::{error::Error, fmt, time::Duration};

#[cfg(target_os = "android")]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[cfg(target_os = "android")]
use std::{
    ffi::{c_char, c_int, CString},
    fs,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

#[cfg(target_os = "android")]
use jni::{
    objects::{JObject, JString, JValue},
    sys::{jclass, jobject, jstring, JNIEnv as RawJNIEnv},
    JNIEnv,
};
#[cfg(target_os = "android")]
use radio_core::{
    parse_realtek_u32_array, plan_realtek_table,
    rtl8812au::{build_tx_packet, Rtl8812auUsbTransport, TxQueue},
    Bandwidth, FirmwareImage, RealtekConditionEnv, RealtekTableKind, RealtekTablePlan, TxOptions,
    TxRate, TxSubmitCounters, UsbBulkTransfer, UsbError,
};
use radio_core::{
    rtl8812au::Rtl8812auRegisterError, Channel, DeviceSelector, Rtl8812auRegisterAccess,
    RxParseOutcome,
};
#[cfg(target_os = "android")]
use wfb_bridge::TxCounters;
#[cfg(any(test, target_os = "android"))]
use wfb_bridge::{build_wfb_data_header, WfbChannelId};
#[cfg(target_os = "android")]
use wfb_radio_runtime::{
    android_usbhost_adapter_info, handle_production_bridge_tx_datagram,
    run_production_runtime_flow_on_session, run_rtl8812au_monitor_opmode,
    run_rtl8812au_production_init, ProductionRuntimeAirtimeSchedule,
    ProductionRuntimeBridgeTxConfig, ProductionRuntimeBridgeTxOverrides,
    ProductionRuntimeFlowConfig, ProductionRuntimeFlowExecutionInputs,
    ProductionRuntimePrimaryRxForwardConfig, ProductionRuntimeQueuedDatagram,
    ProductionRuntimeRtl8812auInitInputs, ProductionRuntimeRxForwardConfig,
    ProductionRuntimeUsbConfig, Rtl8812auInitOrder, RuntimeRadioCounters, RuntimeRxRead,
    TxCalibrationProfile,
};
use wfb_radio_runtime::{
    android_usbhost_open_plan, AndroidUsbHostConfig, RuntimeRadioError, RuntimeRadioSession,
    RuntimeTransportError, RuntimeUsbOpenConfig,
};

pub const JNI_REGISTER_SMOKE_SYMBOL: &str =
    "Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRegisterSmoke";
pub const JNI_RX_READ_SMOKE_SYMBOL: &str =
    "Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRxReadSmoke";
pub const JNI_INIT_RX_READ_SMOKE_SYMBOL: &str =
    "Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runInitRxReadSmoke";
pub const JNI_INIT_TX_SMOKE_SYMBOL: &str =
    "Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runInitTxSmoke";
pub const JNI_MANAGED_STREAMS_SMOKE_SYMBOL: &str =
    "Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runManagedStreamsSmoke";
pub const JNI_MANAGED_STREAMS_SDK_SYMBOL: &str =
    "Java_com_arcedge_wfblink_sdk_WfbLinkNative_runManagedStreams";

pub const ANDROID_SMOKE_INVALID_ARGUMENT: i32 = -1;
pub const ANDROID_SMOKE_TRANSPORT_ERROR: i32 = -2;
pub const ANDROID_SMOKE_REGISTER_ERROR: i32 = -3;
pub const ANDROID_SMOKE_RX_TIMEOUT: i32 = -4;
pub const ANDROID_SMOKE_RX_ERROR: i32 = -5;
pub const ANDROID_SMOKE_NATIVE_PANIC: i32 = -6;

#[cfg(target_os = "android")]
const ANDROID_LOG_INFO: c_int = 4;

#[cfg(target_os = "android")]
#[link(name = "log")]
extern "C" {
    fn __android_log_print(
        priority: c_int,
        tag: *const c_char,
        format: *const c_char,
        ...
    ) -> c_int;
}

#[cfg(target_os = "android")]
fn android_log_info(message: impl AsRef<str>) {
    let sanitized = message.as_ref().replace('\0', "\\0");
    let Some(message) = CString::new(sanitized).ok() else {
        return;
    };
    unsafe {
        __android_log_print(
            ANDROID_LOG_INFO,
            c"WfbNativeSmoke".as_ptr(),
            c"%s".as_ptr(),
            message.as_ptr(),
        );
    }
}

#[cfg(target_os = "android")]
fn android_hex_preview(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(target_os = "android")]
const ANDROID_SMOKE_ASSET_DIR: &str = "/data/local/tmp/wfb-link";
#[cfg(target_os = "android")]
const ANDROID_SMOKE_FIRMWARE_PATH: &str = "/data/local/tmp/wfb-link/rtl8812aefw.bin";
#[cfg(target_os = "android")]
const ANDROID_SMOKE_MAC_SOURCE_PATH: &str = "/data/local/tmp/wfb-link/halhwimg8812a_mac.c";
#[cfg(target_os = "android")]
const ANDROID_SMOKE_BB_SOURCE_PATH: &str = "/data/local/tmp/wfb-link/halhwimg8812a_bb.c";
#[cfg(target_os = "android")]
const ANDROID_SMOKE_RF_SOURCE_PATH: &str = "/data/local/tmp/wfb-link/halhwimg8812a_rf.c";
#[cfg(target_os = "android")]
const ANDROID_SMOKE_GS_KEY_PATH: &str = "/data/local/tmp/wfb-link/gs.key";
#[cfg(target_os = "android")]
const BB_PHY_ARRAY: &str = "array_mp_8812a_phy_reg";
#[cfg(target_os = "android")]
const BB_AGC_ARRAY: &str = "array_mp_8812a_agc_tab";
#[cfg(target_os = "android")]
const MAC_REG_ARRAY: &str = "array_mp_8812a_mac_reg";
#[cfg(target_os = "android")]
const RF_RADIOA_ARRAY: &str = "array_mp_8812a_radioa";
#[cfg(target_os = "android")]
const RF_RADIOB_ARRAY: &str = "array_mp_8812a_radiob";
#[cfg(target_os = "android")]
const DEFAULT_RFE_TYPE: u8 = 0x03;
#[cfg(target_os = "android")]
const ANDROID_SMOKE_TX_FRAME_COUNT: usize = 3;
#[cfg(target_os = "android")]
const REG_Q0_INFO: u16 = 0x0400;
#[cfg(target_os = "android")]
const REG_MGQ_INFO: u16 = 0x0410;
#[cfg(target_os = "android")]
const REG_HGQ_INFO: u16 = 0x0414;
#[cfg(target_os = "android")]
const REG_TXPKT_EMPTY: u16 = 0x041a;
#[cfg(target_os = "android")]
const REG_FWHW_TXQ_CTRL: u16 = 0x0420;
#[cfg(target_os = "android")]
const REG_TXPAUSE: u16 = 0x0522;
#[cfg(target_os = "android")]
const ANDROID_SMOKE_WFB_DATAGRAM_COUNT: usize = 3;
#[cfg(any(test, target_os = "android"))]
const ANDROID_SMOKE_WFB_PAYLOAD_LEN: usize = 64;
#[cfg(any(test, target_os = "android"))]
const ANDROID_SMOKE_WFB_LINK_ID: u32 = 0x000001;
#[cfg(any(test, target_os = "android"))]
const ANDROID_SMOKE_WFB_RADIO_PORT: u8 = 0;
#[cfg(target_os = "android")]
const ANDROID_MANAGED_UPLINK_RADIO_PORT: u8 = 6;
#[cfg(target_os = "android")]
const ANDROID_MANAGED_DOWNLINK_RADIO_PORT: u8 = 4;
#[cfg(target_os = "android")]
const ANDROID_MANAGED_RUNTIME_BIND_PORT: u16 = 15_700;
#[cfg(target_os = "android")]
const ANDROID_MANAGED_TX_BIND_PORT: u16 = 15_706;
#[cfg(target_os = "android")]
const ANDROID_MANAGED_RAW_TX_PORT: u16 = 15_606;
#[cfg(target_os = "android")]
const ANDROID_MANAGED_RX_AGGREGATOR_PORT: u16 = 15_804;
#[cfg(target_os = "android")]
const ANDROID_MANAGED_RAW_RX_PORT: u16 = 15_904;
#[cfg(target_os = "android")]
const ANDROID_MANAGED_HELPER_STARTUP_DELAY: Duration = Duration::from_millis(750);
#[cfg(target_os = "android")]
const ANDROID_MANAGED_RAW_PAYLOAD_BYTES: usize = 512;
#[cfg(any(test, target_os = "android"))]
const ANDROID_SMOKE_WFB_RADIOTAP: [u8; 13] = [
    0x00, 0x00, 0x0d, 0x00, 0x00, 0x80, 0x08, 0x00, 0x08, 0x00, 0x37, 0x00, 0x00,
];
#[cfg(target_os = "android")]
const ANDROID_SMOKE_TX_FRAME: [u8; 24] = [
    0x48, 0x00, // data null, no flags
    0x00, 0x00, // duration
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // addr1
    0x02, 0x00, 0x5e, 0x00, 0x00, 0x02, // addr2
    0x02, 0x00, 0x5e, 0x00, 0x00, 0x02, // addr3
    0x00, 0x00, // seq control
];

#[cfg(target_os = "android")]
#[derive(Debug, Clone)]
struct AndroidManagedStreamRuntimeConfig {
    native_library_dir: PathBuf,
    working_dir: PathBuf,
    key_path: PathBuf,
    firmware_path: PathBuf,
    mac_source_path: PathBuf,
    bb_source_path: PathBuf,
    rf_source_path: PathBuf,
    link_id: u32,
    uplink_radio_port: u8,
    downlink_radio_port: u8,
    runtime_bind_port: u16,
    tx_bind_port: u16,
    raw_tx_port: u16,
    rx_aggregator_port: u16,
    raw_rx_port: u16,
    raw_payload_bytes: usize,
    tx_bandwidth_mhz: u8,
    tx_mcs: u8,
    tx_fec_k: u8,
    tx_fec_n: u8,
}

#[cfg(target_os = "android")]
impl AndroidManagedStreamRuntimeConfig {
    fn smoke_defaults(native_library_dir: PathBuf, working_dir: PathBuf) -> Self {
        Self {
            native_library_dir,
            working_dir,
            key_path: PathBuf::from(ANDROID_SMOKE_GS_KEY_PATH),
            firmware_path: PathBuf::from(ANDROID_SMOKE_FIRMWARE_PATH),
            mac_source_path: PathBuf::from(ANDROID_SMOKE_MAC_SOURCE_PATH),
            bb_source_path: PathBuf::from(ANDROID_SMOKE_BB_SOURCE_PATH),
            rf_source_path: PathBuf::from(ANDROID_SMOKE_RF_SOURCE_PATH),
            link_id: ANDROID_SMOKE_WFB_LINK_ID,
            uplink_radio_port: ANDROID_MANAGED_UPLINK_RADIO_PORT,
            downlink_radio_port: ANDROID_MANAGED_DOWNLINK_RADIO_PORT,
            runtime_bind_port: ANDROID_MANAGED_RUNTIME_BIND_PORT,
            tx_bind_port: ANDROID_MANAGED_TX_BIND_PORT,
            raw_tx_port: ANDROID_MANAGED_RAW_TX_PORT,
            rx_aggregator_port: ANDROID_MANAGED_RX_AGGREGATOR_PORT,
            raw_rx_port: ANDROID_MANAGED_RAW_RX_PORT,
            raw_payload_bytes: ANDROID_MANAGED_RAW_PAYLOAD_BYTES,
            tx_bandwidth_mhz: 20,
            tx_mcs: 0,
            tx_fec_k: 2,
            tx_fec_n: 4,
        }
    }
}

#[derive(Debug)]
pub enum AndroidSmokeError {
    InvalidArgument(&'static str),
    Transport(RuntimeTransportError),
    Register(Rtl8812auRegisterError),
    Rx(RuntimeRadioError),
}

impl fmt::Display for AndroidSmokeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AndroidSmokeError::InvalidArgument(message) => write!(f, "{message}"),
            AndroidSmokeError::Transport(error) => write!(f, "{error}"),
            AndroidSmokeError::Register(error) => write!(f, "{error}"),
            AndroidSmokeError::Rx(error) => write!(f, "{error}"),
        }
    }
}

impl Error for AndroidSmokeError {}

impl AndroidSmokeError {
    fn code(&self) -> &str {
        match self {
            Self::InvalidArgument(name) => name,
            Self::Transport(_) => "android_transport_error",
            Self::Register(_) => "android_register_error",
            Self::Rx(error) => error.code,
        }
    }
}

impl From<RuntimeTransportError> for AndroidSmokeError {
    fn from(error: RuntimeTransportError) -> Self {
        AndroidSmokeError::Transport(error)
    }
}

impl From<Rtl8812auRegisterError> for AndroidSmokeError {
    fn from(error: Rtl8812auRegisterError) -> Self {
        AndroidSmokeError::Register(error)
    }
}

impl From<RuntimeRadioError> for AndroidSmokeError {
    fn from(error: RuntimeRadioError) -> Self {
        AndroidSmokeError::Rx(error)
    }
}

pub fn android_register_smoke_return_code(result: Result<u8, AndroidSmokeError>) -> i32 {
    match result {
        Ok(value) => i32::from(value),
        Err(AndroidSmokeError::InvalidArgument(_)) => ANDROID_SMOKE_INVALID_ARGUMENT,
        Err(AndroidSmokeError::Transport(_)) => ANDROID_SMOKE_TRANSPORT_ERROR,
        Err(AndroidSmokeError::Register(_)) => ANDROID_SMOKE_REGISTER_ERROR,
        Err(AndroidSmokeError::Rx(error)) if error.timeout => ANDROID_SMOKE_RX_TIMEOUT,
        Err(AndroidSmokeError::Rx(_)) => ANDROID_SMOKE_RX_ERROR,
    }
}

pub fn android_rx_read_smoke_return_code(
    result: Result<AndroidRxReadSmokeSummary, AndroidSmokeError>,
) -> i32 {
    match result {
        Ok(summary) => i32::try_from(summary.parsed_frames).unwrap_or(i32::MAX),
        Err(AndroidSmokeError::InvalidArgument(_)) => ANDROID_SMOKE_INVALID_ARGUMENT,
        Err(AndroidSmokeError::Transport(_)) => ANDROID_SMOKE_TRANSPORT_ERROR,
        Err(AndroidSmokeError::Register(_)) => ANDROID_SMOKE_REGISTER_ERROR,
        Err(AndroidSmokeError::Rx(error)) if error.timeout => ANDROID_SMOKE_RX_TIMEOUT,
        Err(AndroidSmokeError::Rx(_)) => ANDROID_SMOKE_RX_ERROR,
    }
}

pub fn android_tx_smoke_return_code(
    result: Result<AndroidTxSmokeSummary, AndroidSmokeError>,
) -> i32 {
    match result {
        Ok(summary) => i32::try_from(summary.submitted).unwrap_or(i32::MAX),
        Err(AndroidSmokeError::InvalidArgument(_)) => ANDROID_SMOKE_INVALID_ARGUMENT,
        Err(AndroidSmokeError::Transport(_)) => ANDROID_SMOKE_TRANSPORT_ERROR,
        Err(AndroidSmokeError::Register(_)) => ANDROID_SMOKE_REGISTER_ERROR,
        Err(AndroidSmokeError::Rx(error)) if error.timeout => ANDROID_SMOKE_RX_TIMEOUT,
        Err(AndroidSmokeError::Rx(_)) => ANDROID_SMOKE_RX_ERROR,
    }
}

pub fn android_managed_stream_smoke_return_code(
    result: Result<AndroidManagedStreamSmokeSummary, AndroidSmokeError>,
) -> i32 {
    match result {
        Ok(summary) if summary.runtime_pass => {
            i32::try_from(summary.runtime_submitted_frames).unwrap_or(i32::MAX)
        }
        Ok(_) => ANDROID_SMOKE_RX_ERROR,
        Err(AndroidSmokeError::InvalidArgument(_)) => ANDROID_SMOKE_INVALID_ARGUMENT,
        Err(AndroidSmokeError::Transport(_)) => ANDROID_SMOKE_TRANSPORT_ERROR,
        Err(AndroidSmokeError::Register(_)) => ANDROID_SMOKE_REGISTER_ERROR,
        Err(AndroidSmokeError::Rx(error)) if error.timeout => ANDROID_SMOKE_RX_TIMEOUT,
        Err(AndroidSmokeError::Rx(_)) => ANDROID_SMOKE_RX_ERROR,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_android_usbhost_register_smoke(
    fd: i32,
    vid: u16,
    pid: u16,
    interface_number: u8,
    bulk_in_endpoint: u8,
    bulk_out_endpoint: u8,
    bulk_out_endpoint_count: usize,
    register_address: u16,
    timeout: Duration,
) -> Result<u8, AndroidSmokeError> {
    let session = open_android_usbhost_session(
        fd,
        vid,
        pid,
        interface_number,
        bulk_in_endpoint,
        bulk_out_endpoint,
        bulk_out_endpoint_count,
    )?;
    let registers = Rtl8812auRegisterAccess::new(&session.transport).with_timeout(timeout);
    Ok(registers.read8(register_address)?)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AndroidRxReadSmokeSummary {
    pub bytes_read: usize,
    pub parsed_frames: usize,
    pub dropped_packets: usize,
    pub need_more_data: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AndroidTxSmokeSummary {
    pub attempted: u64,
    pub submitted: u64,
    pub failed: u64,
    pub short_writes: u64,
    pub bytes_written: u64,
    pub wfb_incoming: u64,
    pub wfb_injected: u64,
    pub wfb_malformed: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AndroidManagedStreamSmokeSummary {
    pub raw_tx_sent: u64,
    pub raw_tx_bytes: u64,
    pub raw_rx_received: u64,
    pub raw_rx_bytes: u64,
    pub runtime_tx_datagrams: u64,
    pub runtime_submitted_frames: u64,
    pub runtime_rx_parsed_frames: u64,
    pub runtime_rx_forwarded_payloads: u64,
    pub runtime_pass: bool,
    pub runtime_result: String,
    pub runtime_stop_reason: String,
    pub runtime_error_code: Option<String>,
    pub runtime_error_message: Option<String>,
    pub tx_helper_status: String,
    pub rx_helper_status: String,
    pub runtime_report_json: String,
}

pub fn android_managed_stream_result_json(
    result: Result<AndroidManagedStreamSmokeSummary, AndroidSmokeError>,
) -> String {
    match result {
        Ok(summary) => serde_json::json!({
            "ok": summary.runtime_pass,
            "code": summary.runtime_error_code.as_deref().unwrap_or(if summary.runtime_pass {
                "ok"
            } else {
                "android_managed_runtime_failed"
            }),
            "message": summary.runtime_error_message.as_deref().unwrap_or(if summary.runtime_pass {
                "ok"
            } else {
                "managed runtime reported failure"
            }),
            "submitted_frames": summary.runtime_submitted_frames,
            "tx_datagrams": summary.runtime_tx_datagrams,
            "raw_tx_packets": summary.raw_tx_sent,
            "raw_tx_bytes": summary.raw_tx_bytes,
            "raw_rx_packets": summary.raw_rx_received,
            "raw_rx_bytes": summary.raw_rx_bytes,
            "rx_frames": summary.runtime_rx_parsed_frames,
            "rx_forwarded_payloads": summary.runtime_rx_forwarded_payloads,
            "runtime_result": summary.runtime_result,
            "stop_reason": summary.runtime_stop_reason,
            "tx_helper_status": summary.tx_helper_status,
            "rx_helper_status": summary.rx_helper_status,
            "runtime_report_json": summary.runtime_report_json,
        })
        .to_string(),
        Err(error) => serde_json::json!({
            "ok": false,
            "code": error.code(),
            "message": error.to_string(),
            "submitted_frames": 0,
            "tx_datagrams": 0,
            "raw_tx_packets": 0,
            "raw_tx_bytes": 0,
            "raw_rx_packets": 0,
            "raw_rx_bytes": 0,
            "rx_frames": 0,
            "rx_forwarded_payloads": 0,
            "runtime_result": "not_started",
            "stop_reason": "not_started",
            "tx_helper_status": "not_started",
            "rx_helper_status": "not_started",
            "runtime_report_json": null,
        })
        .to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_android_usbhost_rx_read_smoke(
    fd: i32,
    vid: u16,
    pid: u16,
    interface_number: u8,
    bulk_in_endpoint: u8,
    bulk_out_endpoint: u8,
    bulk_out_endpoint_count: usize,
    channel_number: u8,
    read_buffer_len: usize,
    timeout: Duration,
) -> Result<AndroidRxReadSmokeSummary, AndroidSmokeError> {
    let channel = Channel::from_number(channel_number)
        .map_err(|_| AndroidSmokeError::InvalidArgument("channel_number"))?;
    if read_buffer_len == 0 {
        return Err(AndroidSmokeError::InvalidArgument("read_buffer_len"));
    }
    let mut session = open_android_usbhost_session(
        fd,
        vid,
        pid,
        interface_number,
        bulk_in_endpoint,
        bulk_out_endpoint,
        bulk_out_endpoint_count,
    )?;
    let mut buffer = vec![0u8; read_buffer_len];
    let read = session.read_rx_packets(channel, &mut buffer, timeout)?;
    Ok(AndroidRxReadSmokeSummary {
        bytes_read: read.bytes_read,
        parsed_frames: read
            .packets
            .iter()
            .filter(|packet| matches!(packet.outcome, RxParseOutcome::Frame))
            .count(),
        dropped_packets: read
            .packets
            .iter()
            .filter(|packet| matches!(packet.outcome, RxParseOutcome::Drop))
            .count(),
        need_more_data: read
            .packets
            .iter()
            .filter(|packet| matches!(packet.outcome, RxParseOutcome::NeedMoreData))
            .count(),
    })
}

#[allow(clippy::too_many_arguments)]
fn open_android_usbhost_session(
    fd: i32,
    vid: u16,
    pid: u16,
    interface_number: u8,
    bulk_in_endpoint: u8,
    bulk_out_endpoint: u8,
    bulk_out_endpoint_count: usize,
) -> Result<RuntimeRadioSession, AndroidSmokeError> {
    let config = AndroidUsbHostConfig {
        device_fd: Some(fd),
        interface_number,
        bulk_in_endpoint,
        bulk_out_endpoint,
        bulk_out_endpoint_count,
    };
    let selector = DeviceSelector {
        vid: Some(vid),
        pid: Some(pid),
        bus: None,
        address: None,
    };

    android_usbhost_open_plan(&config, selector)?;
    Ok(RuntimeRadioSession::open(
        RuntimeUsbOpenConfig::android_usbhost(selector, config),
    )?)
}

#[cfg(any(test, target_os = "android"))]
fn android_smoke_runtime_error(
    code: &'static str,
    message: impl Into<String>,
) -> AndroidSmokeError {
    AndroidSmokeError::Rx(RuntimeRadioError::new(code, message.into()))
}

#[cfg(target_os = "android")]
fn android_smoke_condition_env() -> RealtekConditionEnv {
    RealtekConditionEnv {
        cut_version: 0x00,
        package_type: 0x00,
        support_interface: 0x02,
        support_platform: 0x00,
        board_type: 0xd8,
        type_glna: 0x0000,
        type_gpa: 0x0000,
        type_alna: 0x0000,
        type_apa: 0x0000,
    }
}

#[cfg(target_os = "android")]
fn android_smoke_read_source(path: &Path, code: &'static str) -> Result<String, AndroidSmokeError> {
    fs::read_to_string(path).map_err(|error| {
        android_smoke_runtime_error(code, format!("failed to read {}: {error}", path.display()))
    })
}

#[cfg(target_os = "android")]
fn android_smoke_load_table_plan(
    source: &str,
    array_name: &'static str,
    kind: RealtekTableKind,
    condition_env: RealtekConditionEnv,
) -> Result<RealtekTablePlan, AndroidSmokeError> {
    let values = parse_realtek_u32_array(source, array_name).map_err(|error| {
        android_smoke_runtime_error(
            "android_smoke_table_parse_failed",
            format!("failed to parse {array_name}: {error}"),
        )
    })?;
    plan_realtek_table(array_name, kind, &values, condition_env).map_err(|error| {
        android_smoke_runtime_error(
            "android_smoke_table_plan_failed",
            format!("failed to plan {array_name}: {error}"),
        )
    })
}

#[cfg(target_os = "android")]
fn android_smoke_load_init_inputs(
    init_timeout: Duration,
) -> Result<ProductionRuntimeRtl8812auInitInputs, AndroidSmokeError> {
    android_load_init_inputs_from_paths(
        init_timeout,
        Path::new(ANDROID_SMOKE_FIRMWARE_PATH),
        Path::new(ANDROID_SMOKE_MAC_SOURCE_PATH),
        Path::new(ANDROID_SMOKE_BB_SOURCE_PATH),
        Path::new(ANDROID_SMOKE_RF_SOURCE_PATH),
    )
}

#[cfg(target_os = "android")]
fn android_load_init_inputs_from_paths(
    init_timeout: Duration,
    firmware_path: &Path,
    mac_source_path: &Path,
    bb_source_path: &Path,
    rf_source_path: &Path,
) -> Result<ProductionRuntimeRtl8812auInitInputs, AndroidSmokeError> {
    let firmware_image = FirmwareImage::load_external(firmware_path).map_err(|error| {
        android_smoke_runtime_error(
            "android_smoke_firmware_load_failed",
            format!(
                "failed to load firmware from {}: {error}; push smoke assets into {ANDROID_SMOKE_ASSET_DIR}",
                firmware_path.display()
            ),
        )
    })?;
    let condition_env = android_smoke_condition_env();
    let mac_source =
        android_smoke_read_source(mac_source_path, "android_smoke_mac_source_read_failed")?;
    let bb_source =
        android_smoke_read_source(bb_source_path, "android_smoke_bb_source_read_failed")?;
    let rf_source =
        android_smoke_read_source(rf_source_path, "android_smoke_rf_source_read_failed")?;

    let mac_plan = android_smoke_load_table_plan(
        &mac_source,
        MAC_REG_ARRAY,
        RealtekTableKind::Mac,
        condition_env,
    )?;
    let phy_plan = android_smoke_load_table_plan(
        &bb_source,
        BB_PHY_ARRAY,
        RealtekTableKind::BbPhy,
        condition_env,
    )?;
    let agc_plan = android_smoke_load_table_plan(
        &bb_source,
        BB_AGC_ARRAY,
        RealtekTableKind::BbAgc,
        condition_env,
    )?;
    let radioa_plan = android_smoke_load_table_plan(
        &rf_source,
        RF_RADIOA_ARRAY,
        RealtekTableKind::RfRadioA,
        condition_env,
    )?;
    let radiob_plan = android_smoke_load_table_plan(
        &rf_source,
        RF_RADIOB_ARRAY,
        RealtekTableKind::RfRadioB,
        condition_env,
    )?;

    Ok(ProductionRuntimeRtl8812auInitInputs {
        firmware_image,
        mac_plan,
        phy_plan,
        agc_plan,
        radioa_plan,
        radiob_plan,
        init_order: Rtl8812auInitOrder::Linux,
        rfe_type: DEFAULT_RFE_TYPE,
        init_timeout,
    })
}

#[cfg(target_os = "android")]
struct AndroidJniUsbConnection<'local> {
    env: std::cell::RefCell<JNIEnv<'local>>,
    connection: JObject<'local>,
    bulk_in_endpoint_object: JObject<'local>,
    bulk_out_endpoint_object: JObject<'local>,
    bulk_in_endpoint: u8,
    bulk_out_endpoint: u8,
}

#[cfg(target_os = "android")]
impl<'local> AndroidJniUsbConnection<'local> {
    fn new(
        env: JNIEnv<'local>,
        connection: JObject<'local>,
        bulk_in_endpoint_object: JObject<'local>,
        bulk_out_endpoint_object: JObject<'local>,
        bulk_in_endpoint: u8,
        bulk_out_endpoint: u8,
    ) -> Self {
        Self {
            env: std::cell::RefCell::new(env),
            connection,
            bulk_in_endpoint_object,
            bulk_out_endpoint_object,
            bulk_in_endpoint,
            bulk_out_endpoint,
        }
    }

    fn endpoint_object(&self, endpoint: u8) -> Result<&JObject<'local>, UsbError> {
        if endpoint == self.bulk_in_endpoint {
            Ok(&self.bulk_in_endpoint_object)
        } else if endpoint == self.bulk_out_endpoint {
            Ok(&self.bulk_out_endpoint_object)
        } else {
            Err(UsbError::Backend(format!(
                "Android JNI transport has no UsbEndpoint object for endpoint 0x{endpoint:02x}"
            )))
        }
    }
}

#[cfg(target_os = "android")]
impl<'local> Rtl8812auUsbTransport for &AndroidJniUsbConnection<'local> {
    fn read_vendor(
        &self,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        let length = android_transfer_len("control read", data.len())?;
        let timeout_ms = android_timeout_ms(timeout);
        let mut env = self.env.borrow_mut();
        let array = env
            .new_byte_array(length)
            .map_err(|error| android_jni_usb_error("controlTransfer read buffer", error))?;
        let actual_result = env
            .call_method(
                &self.connection,
                "controlTransfer",
                "(IIII[BII)I",
                &[
                    JValue::Int(0xc0),
                    JValue::Int(0x05),
                    JValue::Int(i32::from(value)),
                    JValue::Int(i32::from(index)),
                    JValue::Object(array.as_ref()),
                    JValue::Int(length),
                    JValue::Int(timeout_ms),
                ],
            )
            .and_then(|value| value.i());
        if let Err(error) = actual_result {
            let _ = env.delete_local_ref(array);
            return Err(android_jni_usb_error("controlTransfer read", error));
        }
        let actual = actual_result.expect("checked above");
        if actual < 0 {
            let _ = env.delete_local_ref(array);
            return Err(UsbError::Backend(format!(
                "Android UsbDeviceConnection.controlTransfer read addr=0x{value:04x} returned {actual}"
            )));
        }

        let actual = actual as usize;
        let bytes_result = env.convert_byte_array(&array);
        env.delete_local_ref(array)
            .map_err(|error| android_jni_usb_error("controlTransfer read cleanup", error))?;
        let bytes = bytes_result
            .map_err(|error| android_jni_usb_error("controlTransfer read copy", error))?;
        let copy_len = actual.min(data.len()).min(bytes.len());
        data[..copy_len].copy_from_slice(&bytes[..copy_len]);
        Ok(actual)
    }

    fn write_vendor(
        &self,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        let length = android_transfer_len("control write", data.len())?;
        let timeout_ms = android_timeout_ms(timeout);
        let mut env = self.env.borrow_mut();
        let array = env
            .byte_array_from_slice(data)
            .map_err(|error| android_jni_usb_error("controlTransfer write buffer", error))?;
        let actual_result = env
            .call_method(
                &self.connection,
                "controlTransfer",
                "(IIII[BII)I",
                &[
                    JValue::Int(0x40),
                    JValue::Int(0x05),
                    JValue::Int(i32::from(value)),
                    JValue::Int(i32::from(index)),
                    JValue::Object(array.as_ref()),
                    JValue::Int(length),
                    JValue::Int(timeout_ms),
                ],
            )
            .and_then(|value| value.i());
        env.delete_local_ref(array)
            .map_err(|error| android_jni_usb_error("controlTransfer write cleanup", error))?;
        let actual =
            actual_result.map_err(|error| android_jni_usb_error("controlTransfer write", error))?;
        if actual < 0 {
            return Err(UsbError::Backend(format!(
                "Android UsbDeviceConnection.controlTransfer write addr=0x{value:04x} returned {actual}"
            )));
        }
        Ok(actual as usize)
    }
}

#[cfg(target_os = "android")]
impl<'local> UsbBulkTransfer for AndroidJniUsbConnection<'local> {
    fn read_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        let endpoint_object = self.endpoint_object(endpoint)?;
        let length = android_transfer_len("bulk read", data.len())?;
        let timeout_ms = android_timeout_ms(timeout);
        let mut env = self.env.borrow_mut();
        let array = env
            .new_byte_array(length)
            .map_err(|error| android_jni_usb_error("bulkTransfer read buffer", error))?;
        let actual_result = env
            .call_method(
                &self.connection,
                "bulkTransfer",
                "(Landroid/hardware/usb/UsbEndpoint;[BII)I",
                &[
                    JValue::Object(endpoint_object),
                    JValue::Object(array.as_ref()),
                    JValue::Int(length),
                    JValue::Int(timeout_ms),
                ],
            )
            .and_then(|value| value.i());
        if let Err(error) = actual_result {
            let _ = env.delete_local_ref(array);
            return Err(android_jni_usb_error("bulkTransfer read", error));
        }
        let actual = actual_result.expect("checked above");
        if actual < 0 {
            let _ = env.delete_local_ref(array);
            return Err(UsbError::BackendTimeout(format!(
                "Android UsbDeviceConnection.bulkTransfer read endpoint=0x{endpoint:02x} returned {actual}"
            )));
        }

        let actual = actual as usize;
        let bytes_result = env.convert_byte_array(&array);
        env.delete_local_ref(array)
            .map_err(|error| android_jni_usb_error("bulkTransfer read cleanup", error))?;
        let bytes =
            bytes_result.map_err(|error| android_jni_usb_error("bulkTransfer read copy", error))?;
        let copy_len = actual.min(data.len()).min(bytes.len());
        data[..copy_len].copy_from_slice(&bytes[..copy_len]);
        Ok(actual)
    }

    fn write_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        let endpoint_object = self.endpoint_object(endpoint)?;
        let length = android_transfer_len("bulk write", data.len())?;
        let timeout_ms = android_timeout_ms(timeout);
        let mut env = self.env.borrow_mut();
        let array = env
            .byte_array_from_slice(data)
            .map_err(|error| android_jni_usb_error("bulkTransfer write buffer", error))?;
        let actual_result = env
            .call_method(
                &self.connection,
                "bulkTransfer",
                "(Landroid/hardware/usb/UsbEndpoint;[BII)I",
                &[
                    JValue::Object(endpoint_object),
                    JValue::Object(array.as_ref()),
                    JValue::Int(length),
                    JValue::Int(timeout_ms),
                ],
            )
            .and_then(|value| value.i());
        env.delete_local_ref(array)
            .map_err(|error| android_jni_usb_error("bulkTransfer write cleanup", error))?;
        let actual =
            actual_result.map_err(|error| android_jni_usb_error("bulkTransfer write", error))?;
        if actual < 0 {
            return Err(UsbError::Backend(format!(
                "Android UsbDeviceConnection.bulkTransfer write endpoint=0x{endpoint:02x} returned {actual}"
            )));
        }
        Ok(actual as usize)
    }
}

#[cfg(target_os = "android")]
fn android_timeout_ms(timeout: Duration) -> i32 {
    i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX)
}

#[cfg(target_os = "android")]
fn android_transfer_len(context: &'static str, len: usize) -> Result<i32, UsbError> {
    i32::try_from(len).map_err(|_| {
        UsbError::Backend(format!(
            "Android JNI {context} length {len} exceeds i32::MAX"
        ))
    })
}

#[cfg(target_os = "android")]
fn android_jni_usb_error(context: &str, error: impl fmt::Display) -> UsbError {
    UsbError::Backend(format!(
        "Android UsbDeviceConnection {context} failed: {error}"
    ))
}

#[cfg(target_os = "android")]
#[allow(clippy::too_many_arguments)]
fn open_android_jni_usbhost_session<'local>(
    env: JNIEnv<'local>,
    connection: JObject<'local>,
    bulk_in_endpoint_object: JObject<'local>,
    bulk_out_endpoint_object: JObject<'local>,
    fd: i32,
    vid: u16,
    pid: u16,
    interface_number: u8,
    bulk_in_endpoint: u8,
    bulk_out_endpoint: u8,
    bulk_out_endpoint_count: usize,
) -> Result<RuntimeRadioSession<AndroidJniUsbConnection<'local>>, AndroidSmokeError> {
    if connection.as_raw().is_null() {
        return Err(AndroidSmokeError::InvalidArgument("connection"));
    }
    if bulk_in_endpoint_object.as_raw().is_null() {
        return Err(AndroidSmokeError::InvalidArgument(
            "bulk_in_endpoint_object",
        ));
    }
    if bulk_out_endpoint_object.as_raw().is_null() {
        return Err(AndroidSmokeError::InvalidArgument(
            "bulk_out_endpoint_object",
        ));
    }

    let config = AndroidUsbHostConfig {
        device_fd: Some(fd),
        interface_number,
        bulk_in_endpoint,
        bulk_out_endpoint,
        bulk_out_endpoint_count,
    };
    let selector = DeviceSelector {
        vid: Some(vid),
        pid: Some(pid),
        bus: None,
        address: None,
    };

    let plan = android_usbhost_open_plan(&config, selector)?;
    let adapter = android_usbhost_adapter_info(plan.vid, plan.pid, &plan.endpoints);
    Ok(RuntimeRadioSession::new(
        AndroidJniUsbConnection::new(
            env,
            connection,
            bulk_in_endpoint_object,
            bulk_out_endpoint_object,
            bulk_in_endpoint,
            bulk_out_endpoint,
        ),
        adapter,
        plan.endpoints,
        RuntimeRadioCounters::default(),
    ))
}

#[cfg(target_os = "android")]
#[allow(clippy::too_many_arguments)]
fn run_android_usbhost_register_smoke_jni<'local>(
    env: JNIEnv<'local>,
    connection: JObject<'local>,
    bulk_in_endpoint_object: JObject<'local>,
    bulk_out_endpoint_object: JObject<'local>,
    args: AndroidSmokeJniArgs,
) -> Result<u8, AndroidSmokeError> {
    let session = open_android_jni_usbhost_session(
        env,
        connection,
        bulk_in_endpoint_object,
        bulk_out_endpoint_object,
        args.fd,
        args.vid,
        args.pid,
        args.interface_number,
        args.bulk_in_endpoint,
        args.bulk_out_endpoint,
        args.bulk_out_endpoint_count,
    )?;
    let registers = Rtl8812auRegisterAccess::new(&session.transport).with_timeout(args.timeout);
    Ok(registers.read8(args.register_address)?)
}

#[cfg(target_os = "android")]
#[allow(clippy::too_many_arguments)]
fn run_android_usbhost_rx_read_smoke_jni<'local>(
    env: JNIEnv<'local>,
    connection: JObject<'local>,
    bulk_in_endpoint_object: JObject<'local>,
    bulk_out_endpoint_object: JObject<'local>,
    args: AndroidRxSmokeJniArgs,
) -> Result<AndroidRxReadSmokeSummary, AndroidSmokeError> {
    let channel = Channel::from_number(args.channel_number)
        .map_err(|_| AndroidSmokeError::InvalidArgument("channel_number"))?;
    if args.read_buffer_len == 0 {
        return Err(AndroidSmokeError::InvalidArgument("read_buffer_len"));
    }
    let mut session = open_android_jni_usbhost_session(
        env,
        connection,
        bulk_in_endpoint_object,
        bulk_out_endpoint_object,
        args.fd,
        args.vid,
        args.pid,
        args.interface_number,
        args.bulk_in_endpoint,
        args.bulk_out_endpoint,
        args.bulk_out_endpoint_count,
    )?;
    let mut buffer = vec![0u8; args.read_buffer_len];
    let read = session.read_rx_packets(channel, &mut buffer, args.timeout)?;
    android_log_rx_read_smoke_summary("pre-init", &read);
    Ok(android_rx_read_smoke_summary(&read))
}

#[cfg(target_os = "android")]
#[allow(clippy::too_many_arguments)]
fn run_android_usbhost_init_rx_read_smoke_jni<'local>(
    env: JNIEnv<'local>,
    connection: JObject<'local>,
    bulk_in_endpoint_object: JObject<'local>,
    bulk_out_endpoint_object: JObject<'local>,
    args: AndroidRxSmokeJniArgs,
) -> Result<AndroidRxReadSmokeSummary, AndroidSmokeError> {
    let channel = Channel::from_number(args.channel_number)
        .map_err(|_| AndroidSmokeError::InvalidArgument("channel_number"))?;
    if args.read_buffer_len == 0 {
        return Err(AndroidSmokeError::InvalidArgument("read_buffer_len"));
    }
    let mut session = open_android_jni_usbhost_session(
        env,
        connection,
        bulk_in_endpoint_object,
        bulk_out_endpoint_object,
        args.fd,
        args.vid,
        args.pid,
        args.interface_number,
        args.bulk_in_endpoint,
        args.bulk_out_endpoint,
        args.bulk_out_endpoint_count,
    )?;
    let init_inputs = android_smoke_load_init_inputs(args.timeout)?;
    let init = run_rtl8812au_production_init(
        &mut session,
        init_inputs,
        channel,
        Bandwidth::Mhz20,
        TxCalibrationProfile::CurrentDefault,
        false,
        false,
    )
    .map_err(|failure| AndroidSmokeError::Rx(failure.error))?;
    android_log_info(format!(
        "init smoke completed: phases={} control_reads={} control_writes={}",
        init.phase_summaries.len(),
        init.counters.usb_control_reads,
        init.counters.usb_control_writes
    ));
    android_apply_monitor_opmode(&mut session, args.timeout, "post-init rx")?;

    let mut buffer = vec![0u8; args.read_buffer_len];
    let deadline = std::time::Instant::now() + args.timeout;
    let mut reads = 0usize;
    let mut aggregate = AndroidRxReadSmokeSummary::default();
    while std::time::Instant::now() < deadline && reads < 32 {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let read_timeout = remaining.min(Duration::from_millis(500));
        match session.read_rx_packets(channel, &mut buffer, read_timeout) {
            Ok(read) => {
                android_log_rx_read_smoke_summary("post-init", &read);
                let summary = android_rx_read_smoke_summary(&read);
                aggregate.bytes_read = aggregate.bytes_read.saturating_add(summary.bytes_read);
                aggregate.parsed_frames = aggregate
                    .parsed_frames
                    .saturating_add(summary.parsed_frames);
                aggregate.dropped_packets = aggregate
                    .dropped_packets
                    .saturating_add(summary.dropped_packets);
                aggregate.need_more_data = aggregate
                    .need_more_data
                    .saturating_add(summary.need_more_data);
                reads = reads.saturating_add(1);
            }
            Err(error) if error.timeout => {
                if aggregate.parsed_frames == 0 && std::time::Instant::now() >= deadline {
                    return Err(AndroidSmokeError::Rx(error));
                }
            }
            Err(error) => return Err(AndroidSmokeError::Rx(error)),
        }
    }
    if aggregate.parsed_frames == 0 {
        return Err(AndroidSmokeError::Rx(RuntimeRadioError {
            code: "android_init_rx_read_timeout",
            message: "post-init RX read loop saw no parsed frames before timeout".to_string(),
            timeout: true,
        }));
    }
    android_log_info(format!(
        "post-init rx aggregate reads={reads} bytes={} frames={} drops={} need_more={}",
        aggregate.bytes_read,
        aggregate.parsed_frames,
        aggregate.dropped_packets,
        aggregate.need_more_data
    ));
    Ok(aggregate)
}

#[cfg(any(test, target_os = "android"))]
fn android_smoke_wfb_datagram(sequence: u16) -> Result<Vec<u8>, AndroidSmokeError> {
    let channel_id = WfbChannelId::new(ANDROID_SMOKE_WFB_LINK_ID, ANDROID_SMOKE_WFB_RADIO_PORT)
        .map_err(|error| {
            android_smoke_runtime_error(
                "android_smoke_wfb_channel_invalid",
                format!("invalid synthetic WFB channel: {error}"),
            )
        })?;
    let mut frame = Vec::with_capacity(24 + ANDROID_SMOKE_WFB_PAYLOAD_LEN);
    frame.extend_from_slice(&build_wfb_data_header(channel_id, sequence));
    for index in 0..ANDROID_SMOKE_WFB_PAYLOAD_LEN {
        frame.push((index % 251) as u8);
    }

    let mut datagram = Vec::with_capacity(4 + ANDROID_SMOKE_WFB_RADIOTAP.len() + frame.len());
    datagram.extend_from_slice(&0u32.to_be_bytes());
    datagram.extend_from_slice(&ANDROID_SMOKE_WFB_RADIOTAP);
    datagram.extend_from_slice(&frame);
    Ok(datagram)
}

#[cfg(target_os = "android")]
fn android_log_tx_scheduler_snapshot<T>(session: &RuntimeRadioSession<T>, timeout: Duration)
where
    for<'a> &'a T: Rtl8812auUsbTransport,
{
    let registers = Rtl8812auRegisterAccess::new(&session.transport).with_timeout(timeout);
    let q0 = registers
        .read32(REG_Q0_INFO)
        .map(|value| format!("0x{value:08x}"))
        .unwrap_or_else(|error| format!("error:{error}"));
    let mgq = registers
        .read32(REG_MGQ_INFO)
        .map(|value| format!("0x{value:08x}"))
        .unwrap_or_else(|error| format!("error:{error}"));
    let hgq = registers
        .read32(REG_HGQ_INFO)
        .map(|value| format!("0x{value:08x}"))
        .unwrap_or_else(|error| format!("error:{error}"));
    let txpkt_empty = registers
        .read16(REG_TXPKT_EMPTY)
        .map(|value| format!("0x{value:04x}"))
        .unwrap_or_else(|error| format!("error:{error}"));
    let fwhw_txq_ctrl = registers
        .read32(REG_FWHW_TXQ_CTRL)
        .map(|value| format!("0x{value:08x}"))
        .unwrap_or_else(|error| format!("error:{error}"));
    let txpause = registers
        .read8(REG_TXPAUSE)
        .map(|value| format!("0x{value:02x}"))
        .unwrap_or_else(|error| format!("error:{error}"));
    android_log_info(format!(
        "post-tx scheduler q0={q0} mgq={mgq} hgq={hgq} txpkt_empty={txpkt_empty} fwhw_txq_ctrl={fwhw_txq_ctrl} txpause={txpause}"
    ));
}

#[cfg(target_os = "android")]
fn android_rx_read_smoke_summary(read: &RuntimeRxRead) -> AndroidRxReadSmokeSummary {
    AndroidRxReadSmokeSummary {
        bytes_read: read.bytes_read,
        parsed_frames: read
            .packets
            .iter()
            .filter(|packet| matches!(packet.outcome, RxParseOutcome::Frame))
            .count(),
        dropped_packets: read
            .packets
            .iter()
            .filter(|packet| matches!(packet.outcome, RxParseOutcome::Drop))
            .count(),
        need_more_data: read
            .packets
            .iter()
            .filter(|packet| matches!(packet.outcome, RxParseOutcome::NeedMoreData))
            .count(),
    }
}

#[cfg(target_os = "android")]
fn android_log_rx_read_smoke_summary(context: &str, read: &RuntimeRxRead) {
    let summary = android_rx_read_smoke_summary(read);
    let mut management_frames = 0usize;
    let mut control_frames = 0usize;
    let mut data_frames = 0usize;
    let mut wfb_like_frames = 0usize;
    let mut phy_status_frames = 0usize;
    let mut rssi_valid_frames = 0usize;
    let mut first_signal = None;
    for packet in &read.packets {
        let Some(frame) = packet.frame.as_ref() else {
            continue;
        };
        if frame.phy_status {
            phy_status_frames += 1;
        }
        if frame.rssi_dbm_valid {
            rssi_valid_frames += 1;
        }
        if first_signal.is_none() {
            first_signal = Some((frame.rssi_dbm, frame.snr_db, frame.rx_rate));
        }
        if frame.data.windows(2).any(|window| window == b"WB") {
            wfb_like_frames += 1;
        }
        if let Some(frame_type) = frame.data.first().map(|byte| (byte >> 2) & 0x03) {
            match frame_type {
                0 => management_frames += 1,
                1 => control_frames += 1,
                2 => data_frames += 1,
                _ => {}
            }
        }
    }
    let first_signal = first_signal
        .map(|(rssi, snr, rate)| format!(" rssi_dbm={rssi} snr_db={snr:?} rate={rate:?}"))
        .unwrap_or_default();
    android_log_info(format!(
        "{context} rx read endpoint=0x{:02x} bytes={} frames={} drops={} need_more={} mgmt={} ctrl={} data={} wfb_like={} phy_status={} rssi_valid={}{}",
        read.endpoint,
        summary.bytes_read,
        summary.parsed_frames,
        summary.dropped_packets,
        summary.need_more_data,
        management_frames,
        control_frames,
        data_frames,
        wfb_like_frames,
        phy_status_frames,
        rssi_valid_frames,
        first_signal
    ));
}

#[cfg(target_os = "android")]
fn android_apply_monitor_opmode<T>(
    session: &mut RuntimeRadioSession<T>,
    timeout: Duration,
    context: &str,
) -> Result<(), AndroidSmokeError>
where
    for<'a> &'a T: Rtl8812auUsbTransport,
{
    let registers = Rtl8812auRegisterAccess::new(&session.transport).with_timeout(timeout);
    let monitor = run_rtl8812au_monitor_opmode(&registers, &mut session.counters)
        .map_err(AndroidSmokeError::Rx)?;
    android_log_info(format!(
        "{context} monitor opmode msr_before=0x{:02x} msr_after=0x{:02x} rcr=0x{:08x} rxfltmap2=0x{:04x}",
        monitor.msr_before,
        monitor.msr_after,
        monitor.rcr_after,
        monitor.rxfltmap2_after
    ));
    Ok(())
}

#[cfg(target_os = "android")]
#[allow(clippy::too_many_arguments)]
fn run_android_usbhost_init_tx_smoke_jni<'local>(
    env: JNIEnv<'local>,
    connection: JObject<'local>,
    bulk_in_endpoint_object: JObject<'local>,
    bulk_out_endpoint_object: JObject<'local>,
    args: AndroidRxSmokeJniArgs,
) -> Result<AndroidTxSmokeSummary, AndroidSmokeError> {
    let channel = Channel::from_number(args.channel_number)
        .map_err(|_| AndroidSmokeError::InvalidArgument("channel_number"))?;
    let mut session = open_android_jni_usbhost_session(
        env,
        connection,
        bulk_in_endpoint_object,
        bulk_out_endpoint_object,
        args.fd,
        args.vid,
        args.pid,
        args.interface_number,
        args.bulk_in_endpoint,
        args.bulk_out_endpoint,
        args.bulk_out_endpoint_count,
    )?;
    let init_inputs = android_smoke_load_init_inputs(args.timeout)?;
    let init = run_rtl8812au_production_init(
        &mut session,
        init_inputs,
        channel,
        Bandwidth::Mhz20,
        TxCalibrationProfile::CurrentDefault,
        false,
        false,
    )
    .map_err(|failure| AndroidSmokeError::Rx(failure.error))?;
    android_log_info(format!(
        "tx smoke init completed: phases={} control_reads={} control_writes={}",
        init.phase_summaries.len(),
        init.counters.usb_control_reads,
        init.counters.usb_control_writes
    ));
    android_apply_monitor_opmode(&mut session, args.timeout, "pre-tx")?;
    android_log_info(format!(
        "tx smoke endpoints bulk_in=0x{:02x} bulk_out=0x{:02x} bulk_out_count={} channel={} HT20",
        args.bulk_in_endpoint, args.bulk_out_endpoint, args.bulk_out_endpoint_count, channel.number
    ));

    let options = TxOptions {
        rate: TxRate::Ofdm6m,
        bandwidth: Bandwidth::Mhz20,
        channel_bandwidth: Some(Bandwidth::Mhz20),
        queue: TxQueue::Mgnt,
        retries: 0,
        no_retry: true,
        rate_fallback_limit: 0,
        ..TxOptions::default()
    };
    let null_packet =
        build_tx_packet(&ANDROID_SMOKE_TX_FRAME, channel, options).map_err(|error| {
            AndroidSmokeError::Rx(RuntimeRadioError::new(
                "android_smoke_tx_descriptor_build_failed",
                error.to_string(),
            ))
        })?;
    android_log_info(format!(
        "null-tx descriptor len={} preview={} options={:?}",
        null_packet.len(),
        android_hex_preview(&null_packet[..40.min(null_packet.len())]),
        options
    ));
    let mut counters = TxSubmitCounters::default();
    for _ in 0..ANDROID_SMOKE_TX_FRAME_COUNT {
        session.submit_80211_frame(&ANDROID_SMOKE_TX_FRAME, channel, options, &mut counters)?;
    }
    let mut bridge_counters = TxCounters::default();
    let mut wfb_submit_counters = TxSubmitCounters::default();
    let bridge_config = ProductionRuntimeBridgeTxConfig {
        channel,
        channel_bandwidth: Bandwidth::Mhz20,
        overrides: ProductionRuntimeBridgeTxOverrides::default(),
    };
    let peer = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));
    for sequence in 0..ANDROID_SMOKE_WFB_DATAGRAM_COUNT {
        let queued = ProductionRuntimeQueuedDatagram {
            report_index: sequence,
            peer,
            data: android_smoke_wfb_datagram(sequence as u16)?,
        };
        let outcome = handle_production_bridge_tx_datagram(
            &mut session,
            &queued,
            bridge_config,
            &mut bridge_counters,
            &mut wfb_submit_counters,
        )
        .map_err(|error| {
            AndroidSmokeError::Rx(RuntimeRadioError::new(error.code, error.message))
        })?;
        if sequence == 0 {
            if let Some(metadata) = outcome.metadata {
                android_log_info(format!(
                    "wfb-tx descriptor len={} datagram_len={} frame_len={} radiotap_len={} preview={} options={:?} channel_observation={:?}",
                    metadata.packet_len,
                    metadata.datagram_len,
                    metadata.frame_len,
                    metadata.radiotap_len,
                    metadata.tx_descriptor_preview_hex,
                    metadata.tx_options,
                    metadata.wfb_channel_observation
                ));
            }
        }
    }
    android_log_tx_scheduler_snapshot(&session, args.timeout);
    Ok(AndroidTxSmokeSummary {
        attempted: counters
            .attempted
            .saturating_add(wfb_submit_counters.attempted),
        submitted: counters
            .submitted
            .saturating_add(wfb_submit_counters.submitted),
        failed: counters.failed.saturating_add(wfb_submit_counters.failed),
        short_writes: counters
            .short_writes
            .saturating_add(wfb_submit_counters.short_writes),
        bytes_written: counters
            .bytes_written
            .saturating_add(wfb_submit_counters.bytes_written),
        wfb_incoming: bridge_counters.incoming,
        wfb_injected: bridge_counters.injected,
        wfb_malformed: bridge_counters.malformed,
    })
}

#[cfg(target_os = "android")]
struct AndroidManagedChildGuard {
    name: &'static str,
    child: Child,
    log_path: PathBuf,
}

#[cfg(target_os = "android")]
impl AndroidManagedChildGuard {
    fn new(name: &'static str, child: Child, log_path: PathBuf) -> Self {
        Self {
            name,
            child,
            log_path,
        }
    }

    fn ensure_running(&mut self) -> Result<(), AndroidSmokeError> {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                let log_tail = fs::read_to_string(&self.log_path)
                    .unwrap_or_else(|error| format!("<failed to read helper log: {error}>"));
                Err(android_smoke_runtime_error(
                    "android_managed_helper_exited",
                    format!(
                        "managed helper {} exited before runtime start: {status}; log: {}",
                        self.name, log_tail
                    ),
                ))
            }
            Ok(None) => Ok(()),
            Err(error) => Err(android_smoke_runtime_error(
                "android_managed_helper_status_failed",
                format!(
                    "failed to check managed helper {} status: {error}",
                    self.name
                ),
            )),
        }
    }

    fn status_label(&mut self) -> String {
        match self.child.try_wait() {
            Ok(Some(status)) => format!("exited: {status}"),
            Ok(None) => "running_at_runtime_end".to_string(),
            Err(error) => format!("status_error: {error}"),
        }
    }
}

#[cfg(target_os = "android")]
impl Drop for AndroidManagedChildGuard {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

#[cfg(target_os = "android")]
fn android_managed_helper_path(
    native_library_dir: &Path,
    helper_name: &'static str,
) -> Result<PathBuf, AndroidSmokeError> {
    let path = native_library_dir.join(helper_name);
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => Ok(path),
        Ok(_) => Err(android_smoke_runtime_error(
            "android_managed_helper_not_file",
            format!("managed helper {} is not a file", path.display()),
        )),
        Err(error) => Err(android_smoke_runtime_error(
            "android_managed_helper_missing",
            format!("managed helper {} is unavailable: {error}", path.display()),
        )),
    }
}

#[cfg(target_os = "android")]
fn android_spawn_managed_child(
    name: &'static str,
    command: &Path,
    args: &[String],
    working_dir: &Path,
) -> Result<AndroidManagedChildGuard, AndroidSmokeError> {
    android_log_info(format!(
        "starting managed helper {name}: {} {}",
        command.display(),
        args.join(" ")
    ));
    let log_path = working_dir.join(format!("android-managed-{name}.log"));
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| {
            android_smoke_runtime_error(
                "android_managed_helper_log_open_failed",
                format!(
                    "failed to open managed helper log {}: {error}",
                    log_path.display()
                ),
            )
        })?;
    let log_stderr = log.try_clone().map_err(|error| {
        android_smoke_runtime_error(
            "android_managed_helper_log_clone_failed",
            format!(
                "failed to clone managed helper log {}: {error}",
                log_path.display()
            ),
        )
    })?;
    let child = Command::new(command)
        .args(args)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_stderr))
        .spawn()
        .map_err(|error| {
            android_smoke_runtime_error(
                "android_managed_helper_spawn_failed",
                format!("failed to start managed helper {name}: {error}"),
            )
        })?;
    Ok(AndroidManagedChildGuard::new(name, child, log_path))
}

#[cfg(target_os = "android")]
fn android_managed_payload(sequence: u32, payload_bytes: usize) -> Vec<u8> {
    let mut payload = vec![0u8; payload_bytes];
    payload[..4].copy_from_slice(&sequence.to_be_bytes());
    for (index, byte) in payload[4..].iter_mut().enumerate() {
        *byte = (sequence as u8).wrapping_add(index as u8);
    }
    payload
}

#[cfg(target_os = "android")]
fn spawn_android_managed_raw_producer(
    ready_file: PathBuf,
    target: SocketAddr,
    payload_count: usize,
    payload_bytes: usize,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<(u64, u64), String>> {
    thread::spawn(move || {
        let ready_deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !stop.load(Ordering::SeqCst) && std::time::Instant::now() < ready_deadline {
            if ready_file.exists() {
                thread::sleep(Duration::from_millis(250));
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        if stop.load(Ordering::SeqCst) {
            return Ok((0, 0));
        }
        let socket = UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))
            .map_err(|error| format!("raw producer bind failed: {error}"))?;
        let mut sent = 0u64;
        let mut bytes = 0u64;
        let mut failed = 0u64;
        for sequence in 0..payload_count {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let payload = android_managed_payload(sequence as u32, payload_bytes);
            match socket.send_to(&payload, target) {
                Ok(written) => {
                    sent = sent.saturating_add(1);
                    bytes = bytes.saturating_add(written as u64);
                }
                Err(error) => {
                    failed = failed.saturating_add(1);
                    if failed <= 3 {
                        android_log_info(format!(
                            "managed raw producer send failed target={target} sequence={sequence}: {error}"
                        ));
                    }
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        if failed > 0 {
            android_log_info(format!(
                "managed raw producer send failures={failed} sent={sent} target={target}"
            ));
        }
        Ok((sent, bytes))
    })
}

#[cfg(target_os = "android")]
fn spawn_android_managed_raw_receiver(
    socket: UdpSocket,
    duration: Duration,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<(u64, u64), String>> {
    thread::spawn(move || {
        let deadline = std::time::Instant::now() + duration + Duration::from_secs(2);
        let mut buffer = [0u8; 2048];
        let mut received = 0u64;
        let mut bytes = 0u64;
        while !stop.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            match socket.recv_from(&mut buffer) {
                Ok((len, _peer)) => {
                    received = received.saturating_add(1);
                    bytes = bytes.saturating_add(len as u64);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => return Err(format!("raw receiver read failed: {error}")),
            }
        }
        Ok((received, bytes))
    })
}

#[cfg(target_os = "android")]
fn android_join_managed_thread(
    name: &'static str,
    handle: thread::JoinHandle<Result<(u64, u64), String>>,
) -> Result<(u64, u64), AndroidSmokeError> {
    handle
        .join()
        .map_err(|_| {
            android_smoke_runtime_error(
                "android_managed_thread_panicked",
                format!("managed smoke thread {name} panicked"),
            )
        })?
        .map_err(|message| android_smoke_runtime_error("android_managed_thread_failed", message))
}

#[cfg(target_os = "android")]
#[allow(clippy::too_many_arguments)]
fn run_android_usbhost_managed_stream_smoke_jni<'local>(
    env: JNIEnv<'local>,
    connection: JObject<'local>,
    bulk_in_endpoint_object: JObject<'local>,
    bulk_out_endpoint_object: JObject<'local>,
    args: AndroidRxSmokeJniArgs,
    native_library_dir: PathBuf,
    working_dir: PathBuf,
    duration: Duration,
    payload_count: usize,
) -> Result<AndroidManagedStreamSmokeSummary, AndroidSmokeError> {
    run_android_usbhost_managed_streams_jni(
        env,
        connection,
        bulk_in_endpoint_object,
        bulk_out_endpoint_object,
        args,
        AndroidManagedStreamRuntimeConfig::smoke_defaults(native_library_dir, working_dir),
        duration,
        payload_count,
    )
}

#[cfg(target_os = "android")]
#[allow(clippy::too_many_arguments)]
fn run_android_usbhost_managed_streams_jni<'local>(
    env: JNIEnv<'local>,
    connection: JObject<'local>,
    bulk_in_endpoint_object: JObject<'local>,
    bulk_out_endpoint_object: JObject<'local>,
    args: AndroidRxSmokeJniArgs,
    managed: AndroidManagedStreamRuntimeConfig,
    duration: Duration,
    payload_count: usize,
) -> Result<AndroidManagedStreamSmokeSummary, AndroidSmokeError> {
    let channel = Channel::from_number(args.channel_number)
        .map_err(|_| AndroidSmokeError::InvalidArgument("channel_number"))?;
    if duration.is_zero() {
        return Err(AndroidSmokeError::InvalidArgument("duration_ms"));
    }
    if managed.raw_payload_bytes < 4 {
        return Err(AndroidSmokeError::InvalidArgument("raw_payload_bytes"));
    }
    if managed.tx_bandwidth_mhz == 0 {
        return Err(AndroidSmokeError::InvalidArgument("tx_bandwidth_mhz"));
    }
    if managed.tx_fec_k == 0 || managed.tx_fec_n == 0 || managed.tx_fec_k > managed.tx_fec_n {
        return Err(AndroidSmokeError::InvalidArgument("tx_fec"));
    }
    if !managed.key_path.is_file() {
        return Err(android_smoke_runtime_error(
            "android_managed_key_missing",
            format!(
                "managed stream smoke requires GS key at {}; generate/copy paired keys first",
                managed.key_path.display()
            ),
        ));
    }

    let tx_helper = android_managed_helper_path(&managed.native_library_dir, "libwfb_tx_exec.so")?;
    let rx_helper = android_managed_helper_path(&managed.native_library_dir, "libwfb_rx_exec.so")?;
    let raw_tx_port = managed.raw_tx_port.to_string();
    let tx_bind = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, managed.tx_bind_port));
    let rx_aggregator = SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::LOCALHOST,
        managed.rx_aggregator_port,
    ));
    let raw_rx_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, managed.raw_rx_port));
    fs::create_dir_all(&managed.working_dir).map_err(|error| {
        android_smoke_runtime_error(
            "android_managed_working_dir_failed",
            format!(
                "failed to create managed smoke working directory {}: {error}",
                managed.working_dir.display()
            ),
        )
    })?;
    let ready_file = managed.working_dir.join("android-managed-ready.json");
    let _ = fs::remove_file(&ready_file);

    let tx_args = vec![
        "-d".to_string(),
        "-K".to_string(),
        managed.key_path.display().to_string(),
        "-i".to_string(),
        managed.link_id.to_string(),
        "-p".to_string(),
        managed.uplink_radio_port.to_string(),
        "-B".to_string(),
        managed.tx_bandwidth_mhz.to_string(),
        "-M".to_string(),
        managed.tx_mcs.to_string(),
        "-k".to_string(),
        managed.tx_fec_k.to_string(),
        "-n".to_string(),
        managed.tx_fec_n.to_string(),
        "-u".to_string(),
        raw_tx_port,
        tx_bind.to_string(),
    ];
    let rx_args = vec![
        "-a".to_string(),
        managed.rx_aggregator_port.to_string(),
        "-K".to_string(),
        managed.key_path.display().to_string(),
        "-i".to_string(),
        managed.link_id.to_string(),
        "-p".to_string(),
        managed.downlink_radio_port.to_string(),
        "-c".to_string(),
        Ipv4Addr::LOCALHOST.to_string(),
        "-u".to_string(),
        managed.raw_rx_port.to_string(),
    ];

    let mut tx_child =
        android_spawn_managed_child("wfb_tx", &tx_helper, &tx_args, &managed.working_dir)?;
    let mut rx_child =
        android_spawn_managed_child("wfb_rx", &rx_helper, &rx_args, &managed.working_dir)?;
    thread::sleep(ANDROID_MANAGED_HELPER_STARTUP_DELAY);
    tx_child.ensure_running()?;
    rx_child.ensure_running()?;

    let raw_rx_socket = UdpSocket::bind(raw_rx_addr).map_err(|error| {
        android_smoke_runtime_error(
            "android_managed_raw_rx_bind_failed",
            format!("failed to bind raw managed RX socket {raw_rx_addr}: {error}"),
        )
    })?;
    raw_rx_socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|error| {
            android_smoke_runtime_error(
                "android_managed_raw_rx_timeout_failed",
                format!("failed to configure raw managed RX socket: {error}"),
            )
        })?;
    let stop = Arc::new(AtomicBool::new(false));
    let receiver = spawn_android_managed_raw_receiver(raw_rx_socket, duration, Arc::clone(&stop));
    let producer = spawn_android_managed_raw_producer(
        ready_file.clone(),
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, managed.raw_tx_port)),
        payload_count,
        managed.raw_payload_bytes,
        Arc::clone(&stop),
    );

    let mut session = open_android_jni_usbhost_session(
        env,
        connection,
        bulk_in_endpoint_object,
        bulk_out_endpoint_object,
        args.fd,
        args.vid,
        args.pid,
        args.interface_number,
        args.bulk_in_endpoint,
        args.bulk_out_endpoint,
        args.bulk_out_endpoint_count,
    )?;
    let selector = DeviceSelector {
        vid: Some(args.vid),
        pid: Some(args.pid),
        bus: None,
        address: None,
    };
    let usb_config = AndroidUsbHostConfig {
        device_fd: Some(args.fd),
        interface_number: args.interface_number,
        bulk_in_endpoint: args.bulk_in_endpoint,
        bulk_out_endpoint: args.bulk_out_endpoint,
        bulk_out_endpoint_count: args.bulk_out_endpoint_count,
    };
    let config = ProductionRuntimeFlowConfig {
        usb: ProductionRuntimeUsbConfig::android_usbhost(selector, usb_config),
        channel,
        bandwidth: Bandwidth::Mhz20,
        firmware: Some(managed.firmware_path.clone()),
        bind_addr: SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            managed.runtime_bind_port,
        )),
        tx_binds: vec![tx_bind],
        duration_ms: duration.as_millis().try_into().unwrap_or(u64::MAX),
        rx_timeout_ms: 20,
        tx_burst_limit: 8,
        tx_min_interval_us: 0,
        max_datagrams: 0,
        airtime_schedule: ProductionRuntimeAirtimeSchedule::continuous(),
        ready_file: Some(ready_file),
        health_file: None,
        tx_authorized: true,
        live_register_write_authorized: false,
        calibration_profile: TxCalibrationProfile::CurrentDefault,
        captured_tail_applied: false,
        primary_rx_forward: ProductionRuntimePrimaryRxForwardConfig {
            link_id: Some(managed.link_id),
            radio_port: Some(managed.downlink_radio_port),
            aggregator: Some(rx_aggregator),
        },
        rx_forwards: Vec::<ProductionRuntimeRxForwardConfig>::new(),
        rx_wlan_idx: 0,
        rx_mcs_index: 0,
    };
    let inputs = ProductionRuntimeFlowExecutionInputs {
        rtl8812au_init: Some(android_load_init_inputs_from_paths(
            args.timeout,
            &managed.firmware_path,
            &managed.mac_source_path,
            &managed.bb_source_path,
            &managed.rf_source_path,
        )?),
        ..ProductionRuntimeFlowExecutionInputs::default()
    };

    let report = run_production_runtime_flow_on_session(config, inputs, &mut session);
    stop.store(true, Ordering::SeqCst);
    let (raw_tx_sent, raw_tx_bytes) = android_join_managed_thread("raw_tx", producer)?;
    let (raw_rx_received, raw_rx_bytes) = android_join_managed_thread("raw_rx", receiver)?;
    let tx_helper_status = tx_child.status_label();
    let rx_helper_status = rx_child.status_label();
    let runtime_report_json = serde_json::to_string(&report).unwrap_or_else(|error| {
        serde_json::json!({
            "serialize_error": error.to_string()
        })
        .to_string()
    });
    let runtime_error_code = report.error.as_ref().map(|error| error.code.to_string());
    let runtime_error_message = report.error.as_ref().map(|error| error.message.clone());
    android_log_info(format!(
        "managed stream runtime result={:?} stop={} tx_datagrams={} tx_submitted={} rx_frames={} rx_forwarded={} raw_tx={}/{} raw_rx={}/{}",
        report.result,
        report.stop_reason,
        report.tx.datagrams_received,
        report.tx.submitted_frames,
        report.rx.parsed_frames,
        report.rx.forwarded_payloads,
        raw_tx_sent,
        raw_tx_bytes,
        raw_rx_received,
        raw_rx_bytes
    ));
    Ok(AndroidManagedStreamSmokeSummary {
        raw_tx_sent,
        raw_tx_bytes,
        raw_rx_received,
        raw_rx_bytes,
        runtime_tx_datagrams: report.tx.datagrams_received,
        runtime_submitted_frames: report.tx.submitted_frames,
        runtime_rx_parsed_frames: report.rx.parsed_frames,
        runtime_rx_forwarded_payloads: report.rx.forwarded_payloads,
        runtime_pass: report.result.as_str() == "pass" && runtime_error_code.is_none(),
        runtime_result: report.result.as_str().to_string(),
        runtime_stop_reason: report.stop_reason.to_string(),
        runtime_error_code,
        runtime_error_message,
        tx_helper_status,
        rx_helper_status,
        runtime_report_json,
    })
}

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "system" fn Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRegisterSmoke(
    env: *mut RawJNIEnv,
    _class: jclass,
    connection: jobject,
    bulk_in_endpoint_object: jobject,
    bulk_out_endpoint_object: jobject,
    fd: i32,
    vid: i32,
    pid: i32,
    interface_number: i32,
    bulk_in_endpoint: i32,
    bulk_out_endpoint: i32,
    bulk_out_endpoint_count: i32,
    register_address: i32,
    timeout_ms: i32,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let args = match android_smoke_jni_args(
            fd,
            vid,
            pid,
            interface_number,
            bulk_in_endpoint,
            bulk_out_endpoint,
            bulk_out_endpoint_count,
            register_address,
            timeout_ms,
        ) {
            Ok(args) => args,
            Err(error) => return android_register_smoke_return_code(Err(error)),
        };
        let env = match unsafe { JNIEnv::from_raw(env) } {
            Ok(env) => env,
            Err(error) => {
                android_log_info(format!("register smoke invalid JNIEnv: {error}"));
                return ANDROID_SMOKE_INVALID_ARGUMENT;
            }
        };
        let connection = unsafe { JObject::from_raw(connection) };
        let bulk_in_endpoint_object = unsafe { JObject::from_raw(bulk_in_endpoint_object) };
        let bulk_out_endpoint_object = unsafe { JObject::from_raw(bulk_out_endpoint_object) };

        let result = run_android_usbhost_register_smoke_jni(
            env,
            connection,
            bulk_in_endpoint_object,
            bulk_out_endpoint_object,
            args,
        );
        if let Err(error) = &result {
            android_log_info(format!("register smoke error: {error:?}"));
        }
        android_register_smoke_return_code(result)
    }))
    .unwrap_or_else(|panic| {
        android_log_info(format!("register smoke panic: {panic:?}"));
        ANDROID_SMOKE_NATIVE_PANIC
    })
}

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "system" fn Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRxReadSmoke(
    env: *mut RawJNIEnv,
    _class: jclass,
    connection: jobject,
    bulk_in_endpoint_object: jobject,
    bulk_out_endpoint_object: jobject,
    fd: i32,
    vid: i32,
    pid: i32,
    interface_number: i32,
    bulk_in_endpoint: i32,
    bulk_out_endpoint: i32,
    bulk_out_endpoint_count: i32,
    channel_number: i32,
    read_buffer_len: i32,
    timeout_ms: i32,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let args = match android_rx_smoke_jni_args(
            fd,
            vid,
            pid,
            interface_number,
            bulk_in_endpoint,
            bulk_out_endpoint,
            bulk_out_endpoint_count,
            channel_number,
            read_buffer_len,
            timeout_ms,
        ) {
            Ok(args) => args,
            Err(error) => return android_rx_read_smoke_return_code(Err(error)),
        };
        let env = match unsafe { JNIEnv::from_raw(env) } {
            Ok(env) => env,
            Err(error) => {
                android_log_info(format!("rx read smoke invalid JNIEnv: {error}"));
                return ANDROID_SMOKE_INVALID_ARGUMENT;
            }
        };
        let connection = unsafe { JObject::from_raw(connection) };
        let bulk_in_endpoint_object = unsafe { JObject::from_raw(bulk_in_endpoint_object) };
        let bulk_out_endpoint_object = unsafe { JObject::from_raw(bulk_out_endpoint_object) };

        let result = run_android_usbhost_rx_read_smoke_jni(
            env,
            connection,
            bulk_in_endpoint_object,
            bulk_out_endpoint_object,
            args,
        );
        if let Err(error) = &result {
            android_log_info(format!("rx read smoke error: {error:?}"));
        }
        android_rx_read_smoke_return_code(result)
    }))
    .unwrap_or_else(|panic| {
        android_log_info(format!("rx read smoke panic: {panic:?}"));
        ANDROID_SMOKE_NATIVE_PANIC
    })
}

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "system" fn Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runInitRxReadSmoke(
    env: *mut RawJNIEnv,
    _class: jclass,
    connection: jobject,
    bulk_in_endpoint_object: jobject,
    bulk_out_endpoint_object: jobject,
    fd: i32,
    vid: i32,
    pid: i32,
    interface_number: i32,
    bulk_in_endpoint: i32,
    bulk_out_endpoint: i32,
    bulk_out_endpoint_count: i32,
    channel_number: i32,
    read_buffer_len: i32,
    timeout_ms: i32,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let args = match android_rx_smoke_jni_args(
            fd,
            vid,
            pid,
            interface_number,
            bulk_in_endpoint,
            bulk_out_endpoint,
            bulk_out_endpoint_count,
            channel_number,
            read_buffer_len,
            timeout_ms,
        ) {
            Ok(args) => args,
            Err(error) => return android_rx_read_smoke_return_code(Err(error)),
        };
        let env = match unsafe { JNIEnv::from_raw(env) } {
            Ok(env) => env,
            Err(error) => {
                android_log_info(format!("init rx read smoke invalid JNIEnv: {error}"));
                return ANDROID_SMOKE_INVALID_ARGUMENT;
            }
        };
        let connection = unsafe { JObject::from_raw(connection) };
        let bulk_in_endpoint_object = unsafe { JObject::from_raw(bulk_in_endpoint_object) };
        let bulk_out_endpoint_object = unsafe { JObject::from_raw(bulk_out_endpoint_object) };

        let result = run_android_usbhost_init_rx_read_smoke_jni(
            env,
            connection,
            bulk_in_endpoint_object,
            bulk_out_endpoint_object,
            args,
        );
        if let Err(error) = &result {
            android_log_info(format!("init rx read smoke error: {error:?}"));
        }
        android_rx_read_smoke_return_code(result)
    }))
    .unwrap_or_else(|panic| {
        android_log_info(format!("init rx read smoke panic: {panic:?}"));
        ANDROID_SMOKE_NATIVE_PANIC
    })
}

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "system" fn Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runInitTxSmoke(
    env: *mut RawJNIEnv,
    _class: jclass,
    connection: jobject,
    bulk_in_endpoint_object: jobject,
    bulk_out_endpoint_object: jobject,
    fd: i32,
    vid: i32,
    pid: i32,
    interface_number: i32,
    bulk_in_endpoint: i32,
    bulk_out_endpoint: i32,
    bulk_out_endpoint_count: i32,
    channel_number: i32,
    timeout_ms: i32,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let args = match android_rx_smoke_jni_args(
            fd,
            vid,
            pid,
            interface_number,
            bulk_in_endpoint,
            bulk_out_endpoint,
            bulk_out_endpoint_count,
            channel_number,
            1,
            timeout_ms,
        ) {
            Ok(args) => args,
            Err(error) => return android_tx_smoke_return_code(Err(error)),
        };
        let env = match unsafe { JNIEnv::from_raw(env) } {
            Ok(env) => env,
            Err(error) => {
                android_log_info(format!("init tx smoke invalid JNIEnv: {error}"));
                return ANDROID_SMOKE_INVALID_ARGUMENT;
            }
        };
        let connection = unsafe { JObject::from_raw(connection) };
        let bulk_in_endpoint_object = unsafe { JObject::from_raw(bulk_in_endpoint_object) };
        let bulk_out_endpoint_object = unsafe { JObject::from_raw(bulk_out_endpoint_object) };

        let result = run_android_usbhost_init_tx_smoke_jni(
            env,
            connection,
            bulk_in_endpoint_object,
            bulk_out_endpoint_object,
            args,
        );
        if let Ok(summary) = &result {
            android_log_info(format!(
                "init tx smoke submitted={}/{} bytes={} failed={} short_writes={} wfb_incoming={} wfb_injected={} wfb_malformed={}",
                summary.submitted,
                summary.attempted,
                summary.bytes_written,
                summary.failed,
                summary.short_writes,
                summary.wfb_incoming,
                summary.wfb_injected,
                summary.wfb_malformed
            ));
        }
        if let Err(error) = &result {
            android_log_info(format!("init tx smoke error: {error:?}"));
        }
        android_tx_smoke_return_code(result)
    }))
    .unwrap_or_else(|panic| {
        android_log_info(format!("init tx smoke panic: {panic:?}"));
        ANDROID_SMOKE_NATIVE_PANIC
    })
}

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "system" fn Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runManagedStreamsSmoke(
    env: *mut RawJNIEnv,
    _class: jclass,
    connection: jobject,
    bulk_in_endpoint_object: jobject,
    bulk_out_endpoint_object: jobject,
    fd: i32,
    vid: i32,
    pid: i32,
    interface_number: i32,
    bulk_in_endpoint: i32,
    bulk_out_endpoint: i32,
    bulk_out_endpoint_count: i32,
    channel_number: i32,
    timeout_ms: i32,
    native_library_dir: jstring,
    working_dir: jstring,
    duration_ms: i32,
    payload_count: i32,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let args = match android_rx_smoke_jni_args(
            fd,
            vid,
            pid,
            interface_number,
            bulk_in_endpoint,
            bulk_out_endpoint,
            bulk_out_endpoint_count,
            channel_number,
            1,
            timeout_ms,
        ) {
            Ok(args) => args,
            Err(error) => return android_managed_stream_smoke_return_code(Err(error)),
        };
        let duration = match u64_from_jni("duration_ms", duration_ms) {
            Ok(ms) => Duration::from_millis(ms),
            Err(error) => return android_managed_stream_smoke_return_code(Err(error)),
        };
        let payload_count = match usize_from_jni("payload_count", payload_count) {
            Ok(count) => count,
            Err(error) => return android_managed_stream_smoke_return_code(Err(error)),
        };
        let mut env = match unsafe { JNIEnv::from_raw(env) } {
            Ok(env) => env,
            Err(error) => {
                android_log_info(format!("managed stream smoke invalid JNIEnv: {error}"));
                return ANDROID_SMOKE_INVALID_ARGUMENT;
            }
        };
        let native_library_dir = unsafe { JString::from_raw(native_library_dir) };
        let native_library_dir = match env.get_string(&native_library_dir) {
            Ok(value) => PathBuf::from(value.to_string_lossy().into_owned()),
            Err(error) => {
                android_log_info(format!(
                    "managed stream smoke invalid native library dir string: {error}"
                ));
                return ANDROID_SMOKE_INVALID_ARGUMENT;
            }
        };
        let working_dir = unsafe { JString::from_raw(working_dir) };
        let working_dir = match env.get_string(&working_dir) {
            Ok(value) => PathBuf::from(value.to_string_lossy().into_owned()),
            Err(error) => {
                android_log_info(format!(
                    "managed stream smoke invalid working dir string: {error}"
                ));
                return ANDROID_SMOKE_INVALID_ARGUMENT;
            }
        };
        let connection = unsafe { JObject::from_raw(connection) };
        let bulk_in_endpoint_object = unsafe { JObject::from_raw(bulk_in_endpoint_object) };
        let bulk_out_endpoint_object = unsafe { JObject::from_raw(bulk_out_endpoint_object) };

        let result = run_android_usbhost_managed_stream_smoke_jni(
            env,
            connection,
            bulk_in_endpoint_object,
            bulk_out_endpoint_object,
            args,
            native_library_dir,
            working_dir,
            duration,
            payload_count,
        );
        if let Ok(summary) = &result {
            android_log_info(format!(
                "managed stream smoke submitted={} tx_datagrams={} raw_tx={} raw_rx={} rx_forwarded={}",
                summary.runtime_submitted_frames,
                summary.runtime_tx_datagrams,
                summary.raw_tx_sent,
                summary.raw_rx_received,
                summary.runtime_rx_forwarded_payloads
            ));
        }
        if let Err(error) = &result {
            android_log_info(format!("managed stream smoke error: {error:?}"));
        }
        android_managed_stream_smoke_return_code(result)
    }))
    .unwrap_or_else(|panic| {
        android_log_info(format!("managed stream smoke panic: {panic:?}"));
        ANDROID_SMOKE_NATIVE_PANIC
    })
}

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "system" fn Java_com_arcedge_wfblink_sdk_WfbLinkNative_runManagedStreams(
    env: *mut RawJNIEnv,
    _class: jclass,
    connection: jobject,
    bulk_in_endpoint_object: jobject,
    bulk_out_endpoint_object: jobject,
    fd: i32,
    vid: i32,
    pid: i32,
    interface_number: i32,
    bulk_in_endpoint: i32,
    bulk_out_endpoint: i32,
    bulk_out_endpoint_count: i32,
    channel_number: i32,
    timeout_ms: i32,
    native_library_dir: jstring,
    working_dir: jstring,
    key_path: jstring,
    firmware_path: jstring,
    mac_source_path: jstring,
    bb_source_path: jstring,
    rf_source_path: jstring,
    duration_ms: i32,
    payload_count: i32,
    link_id: i32,
    uplink_radio_port: i32,
    downlink_radio_port: i32,
    runtime_bind_port: i32,
    tx_bind_port: i32,
    raw_tx_port: i32,
    rx_aggregator_port: i32,
    raw_rx_port: i32,
    raw_payload_bytes: i32,
    tx_bandwidth_mhz: i32,
    tx_mcs: i32,
    tx_fec_k: i32,
    tx_fec_n: i32,
) -> jstring {
    let env_raw = env;
    let json = catch_unwind(AssertUnwindSafe(|| {
        let result = (|| {
            let mut env = unsafe { JNIEnv::from_raw(env_raw) }
                .map_err(|_| AndroidSmokeError::InvalidArgument("jni_env"))?;
            let args = android_rx_smoke_jni_args(
                fd,
                vid,
                pid,
                interface_number,
                bulk_in_endpoint,
                bulk_out_endpoint,
                bulk_out_endpoint_count,
                channel_number,
                1,
                timeout_ms,
            )?;
            let duration = Duration::from_millis(u64_from_jni("duration_ms", duration_ms)?);
            let payload_count = usize_from_jni("payload_count", payload_count)?;
            let managed = AndroidManagedStreamRuntimeConfig {
                native_library_dir: android_jstring_to_path(
                    &mut env,
                    native_library_dir,
                    "native_library_dir",
                )?,
                working_dir: android_jstring_to_path(&mut env, working_dir, "working_dir")?,
                key_path: android_jstring_to_path(&mut env, key_path, "key_path")?,
                firmware_path: android_jstring_to_path(&mut env, firmware_path, "firmware_path")?,
                mac_source_path: android_jstring_to_path(
                    &mut env,
                    mac_source_path,
                    "mac_source_path",
                )?,
                bb_source_path: android_jstring_to_path(
                    &mut env,
                    bb_source_path,
                    "bb_source_path",
                )?,
                rf_source_path: android_jstring_to_path(
                    &mut env,
                    rf_source_path,
                    "rf_source_path",
                )?,
                link_id: u32_from_jni("link_id", link_id)?,
                uplink_radio_port: u8_from_jni("uplink_radio_port", uplink_radio_port)?,
                downlink_radio_port: u8_from_jni("downlink_radio_port", downlink_radio_port)?,
                runtime_bind_port: u16_from_jni("runtime_bind_port", runtime_bind_port)?,
                tx_bind_port: u16_from_jni("tx_bind_port", tx_bind_port)?,
                raw_tx_port: u16_from_jni("raw_tx_port", raw_tx_port)?,
                rx_aggregator_port: u16_from_jni("rx_aggregator_port", rx_aggregator_port)?,
                raw_rx_port: u16_from_jni("raw_rx_port", raw_rx_port)?,
                raw_payload_bytes: usize_from_jni("raw_payload_bytes", raw_payload_bytes)?,
                tx_bandwidth_mhz: u8_from_jni("tx_bandwidth_mhz", tx_bandwidth_mhz)?,
                tx_mcs: u8_from_jni("tx_mcs", tx_mcs)?,
                tx_fec_k: u8_from_jni("tx_fec_k", tx_fec_k)?,
                tx_fec_n: u8_from_jni("tx_fec_n", tx_fec_n)?,
            };
            let connection = unsafe { JObject::from_raw(connection) };
            let bulk_in_endpoint_object = unsafe { JObject::from_raw(bulk_in_endpoint_object) };
            let bulk_out_endpoint_object = unsafe { JObject::from_raw(bulk_out_endpoint_object) };
            run_android_usbhost_managed_streams_jni(
                env,
                connection,
                bulk_in_endpoint_object,
                bulk_out_endpoint_object,
                args,
                managed,
                duration,
                payload_count,
            )
        })();
        android_managed_stream_result_json(result)
    }))
    .unwrap_or_else(|panic| {
        android_log_info(format!("managed stream SDK panic: {panic:?}"));
        serde_json::json!({
            "ok": false,
            "code": "android_native_panic",
            "message": "native panic caught at JNI boundary",
        })
        .to_string()
    });
    let env = match unsafe { JNIEnv::from_raw(env_raw) } {
        Ok(env) => env,
        Err(_error) => return std::ptr::null_mut(),
    };
    match env.new_string(json) {
        Ok(value) => value.into_raw(),
        Err(_error) => std::ptr::null_mut(),
    }
}

#[cfg(any(test, target_os = "android"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AndroidSmokeJniArgs {
    fd: i32,
    vid: u16,
    pid: u16,
    interface_number: u8,
    bulk_in_endpoint: u8,
    bulk_out_endpoint: u8,
    bulk_out_endpoint_count: usize,
    register_address: u16,
    timeout: Duration,
}

#[cfg(any(test, target_os = "android"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AndroidRxSmokeJniArgs {
    fd: i32,
    vid: u16,
    pid: u16,
    interface_number: u8,
    bulk_in_endpoint: u8,
    bulk_out_endpoint: u8,
    bulk_out_endpoint_count: usize,
    channel_number: u8,
    read_buffer_len: usize,
    timeout: Duration,
}

#[cfg(any(test, target_os = "android"))]
#[allow(clippy::too_many_arguments)]
fn android_smoke_jni_args(
    fd: i32,
    vid: i32,
    pid: i32,
    interface_number: i32,
    bulk_in_endpoint: i32,
    bulk_out_endpoint: i32,
    bulk_out_endpoint_count: i32,
    register_address: i32,
    timeout_ms: i32,
) -> Result<AndroidSmokeJniArgs, AndroidSmokeError> {
    Ok(AndroidSmokeJniArgs {
        fd,
        vid: u16_from_jni("vid", vid)?,
        pid: u16_from_jni("pid", pid)?,
        interface_number: u8_from_jni("interface_number", interface_number)?,
        bulk_in_endpoint: u8_from_jni("bulk_in_endpoint", bulk_in_endpoint)?,
        bulk_out_endpoint: u8_from_jni("bulk_out_endpoint", bulk_out_endpoint)?,
        bulk_out_endpoint_count: usize_from_jni(
            "bulk_out_endpoint_count",
            bulk_out_endpoint_count,
        )?,
        register_address: u16_from_jni("register_address", register_address)?,
        timeout: Duration::from_millis(u64_from_jni("timeout_ms", timeout_ms)?),
    })
}

#[cfg(any(test, target_os = "android"))]
#[allow(clippy::too_many_arguments)]
fn android_rx_smoke_jni_args(
    fd: i32,
    vid: i32,
    pid: i32,
    interface_number: i32,
    bulk_in_endpoint: i32,
    bulk_out_endpoint: i32,
    bulk_out_endpoint_count: i32,
    channel_number: i32,
    read_buffer_len: i32,
    timeout_ms: i32,
) -> Result<AndroidRxSmokeJniArgs, AndroidSmokeError> {
    Ok(AndroidRxSmokeJniArgs {
        fd,
        vid: u16_from_jni("vid", vid)?,
        pid: u16_from_jni("pid", pid)?,
        interface_number: u8_from_jni("interface_number", interface_number)?,
        bulk_in_endpoint: u8_from_jni("bulk_in_endpoint", bulk_in_endpoint)?,
        bulk_out_endpoint: u8_from_jni("bulk_out_endpoint", bulk_out_endpoint)?,
        bulk_out_endpoint_count: usize_from_jni(
            "bulk_out_endpoint_count",
            bulk_out_endpoint_count,
        )?,
        channel_number: u8_from_jni("channel_number", channel_number)?,
        read_buffer_len: usize_from_jni("read_buffer_len", read_buffer_len)?,
        timeout: Duration::from_millis(u64_from_jni("timeout_ms", timeout_ms)?),
    })
}

#[cfg(any(test, target_os = "android"))]
fn u8_from_jni(name: &'static str, value: i32) -> Result<u8, AndroidSmokeError> {
    u8::try_from(value).map_err(|_| AndroidSmokeError::InvalidArgument(name))
}

#[cfg(any(test, target_os = "android"))]
fn u16_from_jni(name: &'static str, value: i32) -> Result<u16, AndroidSmokeError> {
    u16::try_from(value).map_err(|_| AndroidSmokeError::InvalidArgument(name))
}

#[cfg(target_os = "android")]
fn u32_from_jni(name: &'static str, value: i32) -> Result<u32, AndroidSmokeError> {
    u32::try_from(value).map_err(|_| AndroidSmokeError::InvalidArgument(name))
}

#[cfg(any(test, target_os = "android"))]
fn usize_from_jni(name: &'static str, value: i32) -> Result<usize, AndroidSmokeError> {
    usize::try_from(value).map_err(|_| AndroidSmokeError::InvalidArgument(name))
}

#[cfg(any(test, target_os = "android"))]
fn u64_from_jni(name: &'static str, value: i32) -> Result<u64, AndroidSmokeError> {
    u64::try_from(value).map_err(|_| AndroidSmokeError::InvalidArgument(name))
}

#[cfg(target_os = "android")]
fn android_jstring_to_path(
    env: &mut JNIEnv<'_>,
    value: jstring,
    name: &'static str,
) -> Result<PathBuf, AndroidSmokeError> {
    if value.is_null() {
        return Err(AndroidSmokeError::InvalidArgument(name));
    }
    let value = unsafe { JString::from_raw(value) };
    env.get_string(&value)
        .map(|value| PathBuf::from(value.to_string_lossy().into_owned()))
        .map_err(|_| AndroidSmokeError::InvalidArgument(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use radio_core::{Bandwidth, TxRate};

    #[test]
    fn jni_args_validate_integer_ranges() {
        assert!(android_smoke_jni_args(42, 0x0bda, 0x8812, 0, 0x81, 0x02, 3, 0x0000, 500).is_ok());
        assert!(
            android_rx_smoke_jni_args(42, 0x0bda, 0x8812, 0, 0x81, 0x02, 3, 36, 16384, 500).is_ok()
        );
        assert!(matches!(
            android_smoke_jni_args(42, -1, 0x8812, 0, 0x81, 0x02, 3, 0x0000, 500),
            Err(AndroidSmokeError::InvalidArgument("vid"))
        ));
        assert!(matches!(
            android_smoke_jni_args(42, 0x0bda, 0x8812, 0, 0x81, 0x02, 3, 0x0000, -1),
            Err(AndroidSmokeError::InvalidArgument("timeout_ms"))
        ));
        assert!(matches!(
            android_rx_smoke_jni_args(42, 0x0bda, 0x8812, 0, 0x81, 0x02, 3, 36, -1, 500),
            Err(AndroidSmokeError::InvalidArgument("read_buffer_len"))
        ));
    }

    #[test]
    fn return_code_preserves_register_byte_values_and_error_classes() {
        assert_eq!(android_register_smoke_return_code(Ok(0xab)), 0xab);
        assert_eq!(
            android_register_smoke_return_code(Err(AndroidSmokeError::InvalidArgument("vid"))),
            ANDROID_SMOKE_INVALID_ARGUMENT
        );
        assert_eq!(
            android_register_smoke_return_code(Err(AndroidSmokeError::Transport(
                RuntimeTransportError {
                    code: "x",
                    message: "transport".to_string(),
                },
            ))),
            ANDROID_SMOKE_TRANSPORT_ERROR
        );
        assert_eq!(
            android_rx_read_smoke_return_code(Ok(AndroidRxReadSmokeSummary {
                bytes_read: 1024,
                parsed_frames: 7,
                dropped_packets: 1,
                need_more_data: 0,
            })),
            7
        );
        assert_eq!(
            android_rx_read_smoke_return_code(Err(AndroidSmokeError::Rx(RuntimeRadioError {
                code: "bulk_in_read_timeout",
                message: "timeout".to_string(),
                timeout: true,
            },))),
            ANDROID_SMOKE_RX_TIMEOUT
        );
        assert_eq!(
            android_rx_read_smoke_return_code(Err(AndroidSmokeError::Rx(RuntimeRadioError {
                code: "bulk_in_read_failed",
                message: "failed".to_string(),
                timeout: false,
            },))),
            ANDROID_SMOKE_RX_ERROR
        );
        assert_eq!(ANDROID_SMOKE_NATIVE_PANIC, -6);
    }

    #[test]
    fn android_smoke_wfb_datagram_is_parseable() {
        let datagram = android_smoke_wfb_datagram(0x1234).expect("synthetic WFB datagram");
        let parsed = wfb_bridge::parse_tx_datagram(&datagram).expect("parse tx datagram");
        let expected_channel =
            WfbChannelId::new(ANDROID_SMOKE_WFB_LINK_ID, ANDROID_SMOKE_WFB_RADIO_PORT)
                .expect("channel id");

        assert_eq!(parsed.fwmark, 0);
        assert_eq!(parsed.radiotap_len, ANDROID_SMOKE_WFB_RADIOTAP.len());
        assert_eq!(parsed.tx_options.rate, TxRate::Mcs(0));
        assert_eq!(parsed.tx_options.bandwidth, Bandwidth::Mhz20);
        assert_eq!(
            &parsed.ieee80211_frame[..wfb_bridge::WFB_IEEE80211_HEADER_LEN],
            &build_wfb_data_header(expected_channel, 0x1234)
        );
        assert_eq!(
            parsed.ieee80211_frame.len(),
            wfb_bridge::WFB_IEEE80211_HEADER_LEN + ANDROID_SMOKE_WFB_PAYLOAD_LEN
        );
    }
}
