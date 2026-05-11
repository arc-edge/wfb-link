use std::{error::Error, fmt, time::Duration};

#[cfg(target_os = "android")]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[cfg(target_os = "android")]
use std::{
    ffi::{c_char, c_int, CString},
    fs,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    path::{Path, PathBuf},
};

#[cfg(target_os = "android")]
use jni::{
    objects::{JObject, JValue},
    sys::{jclass, jobject, JNIEnv as RawJNIEnv},
    JNIEnv,
};
#[cfg(target_os = "android")]
use radio_core::{
    parse_realtek_u32_array, plan_realtek_table,
    rtl8812au::{Rtl8812auUsbTransport, TxQueue},
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
    run_rtl8812au_production_init, ProductionRuntimeBridgeTxConfig,
    ProductionRuntimeBridgeTxOverrides, ProductionRuntimeQueuedDatagram,
    ProductionRuntimeRtl8812auInitInputs, Rtl8812auInitOrder, RuntimeRadioCounters,
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
    let firmware_path = PathBuf::from(ANDROID_SMOKE_FIRMWARE_PATH);
    let firmware_image = FirmwareImage::load_external(&firmware_path).map_err(|error| {
        android_smoke_runtime_error(
            "android_smoke_firmware_load_failed",
            format!(
                "failed to load firmware from {}: {error}; push smoke assets into {ANDROID_SMOKE_ASSET_DIR}",
                firmware_path.display()
            ),
        )
    })?;
    let condition_env = android_smoke_condition_env();
    let mac_source = android_smoke_read_source(
        Path::new(ANDROID_SMOKE_MAC_SOURCE_PATH),
        "android_smoke_mac_source_read_failed",
    )?;
    let bb_source = android_smoke_read_source(
        Path::new(ANDROID_SMOKE_BB_SOURCE_PATH),
        "android_smoke_bb_source_read_failed",
    )?;
    let rf_source = android_smoke_read_source(
        Path::new(ANDROID_SMOKE_RF_SOURCE_PATH),
        "android_smoke_rf_source_read_failed",
    )?;

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
        let actual = env
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
            .and_then(|value| value.i())
            .map_err(|error| android_jni_usb_error("controlTransfer read", error))?;
        if actual < 0 {
            return Err(UsbError::Backend(format!(
                "Android UsbDeviceConnection.controlTransfer read addr=0x{value:04x} returned {actual}"
            )));
        }

        let actual = actual as usize;
        let bytes = env
            .convert_byte_array(&array)
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
        let actual = env
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
            .and_then(|value| value.i())
            .map_err(|error| android_jni_usb_error("controlTransfer write", error))?;
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
        let actual = env
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
            .and_then(|value| value.i())
            .map_err(|error| android_jni_usb_error("bulkTransfer read", error))?;
        if actual < 0 {
            return Err(UsbError::BackendTimeout(format!(
                "Android UsbDeviceConnection.bulkTransfer read endpoint=0x{endpoint:02x} returned {actual}"
            )));
        }

        let actual = actual as usize;
        let bytes = env
            .convert_byte_array(&array)
            .map_err(|error| android_jni_usb_error("bulkTransfer read copy", error))?;
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
        let actual = env
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
            .and_then(|value| value.i())
            .map_err(|error| android_jni_usb_error("bulkTransfer write", error))?;
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

    let mut buffer = vec![0u8; args.read_buffer_len];
    let read = session.read_rx_packets(channel, &mut buffer, args.timeout)?;
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
        handle_production_bridge_tx_datagram(
            &mut session,
            &queued,
            bridge_config,
            &mut bridge_counters,
            &mut wfb_submit_counters,
        )
        .map_err(|error| {
            AndroidSmokeError::Rx(RuntimeRadioError::new(error.code, error.message))
        })?;
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

#[cfg(any(test, target_os = "android"))]
fn usize_from_jni(name: &'static str, value: i32) -> Result<usize, AndroidSmokeError> {
    usize::try_from(value).map_err(|_| AndroidSmokeError::InvalidArgument(name))
}

#[cfg(any(test, target_os = "android"))]
fn u64_from_jni(name: &'static str, value: i32) -> Result<u64, AndroidSmokeError> {
    u64::try_from(value).map_err(|_| AndroidSmokeError::InvalidArgument(name))
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
