use std::{error::Error, fmt, time::Duration};

#[cfg(target_os = "android")]
use std::ffi::c_void;

use radio_core::{
    rtl8812au::Rtl8812auRegisterError, Channel, DeviceSelector, Rtl8812auRegisterAccess,
    RxParseOutcome,
};
use wfb_radio_runtime::{
    android_usbhost_open_plan, AndroidUsbHostConfig, RuntimeRadioError, RuntimeRadioSession,
    RuntimeTransportError, RuntimeUsbOpenConfig,
};

pub const JNI_REGISTER_SMOKE_SYMBOL: &str =
    "Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRegisterSmoke";
pub const JNI_RX_READ_SMOKE_SYMBOL: &str =
    "Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRxReadSmoke";

pub const ANDROID_SMOKE_INVALID_ARGUMENT: i32 = -1;
pub const ANDROID_SMOKE_TRANSPORT_ERROR: i32 = -2;
pub const ANDROID_SMOKE_REGISTER_ERROR: i32 = -3;
pub const ANDROID_SMOKE_RX_TIMEOUT: i32 = -4;
pub const ANDROID_SMOKE_RX_ERROR: i32 = -5;

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

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "system" fn Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRegisterSmoke(
    _env: *mut c_void,
    _class: *mut c_void,
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

    android_register_smoke_return_code(run_android_usbhost_register_smoke(
        args.fd,
        args.vid,
        args.pid,
        args.interface_number,
        args.bulk_in_endpoint,
        args.bulk_out_endpoint,
        args.bulk_out_endpoint_count,
        args.register_address,
        args.timeout,
    ))
}

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "system" fn Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRxReadSmoke(
    _env: *mut c_void,
    _class: *mut c_void,
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

    android_rx_read_smoke_return_code(run_android_usbhost_rx_read_smoke(
        args.fd,
        args.vid,
        args.pid,
        args.interface_number,
        args.bulk_in_endpoint,
        args.bulk_out_endpoint,
        args.bulk_out_endpoint_count,
        args.channel_number,
        args.read_buffer_len,
        args.timeout,
    ))
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
    }
}
