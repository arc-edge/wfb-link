use std::{error::Error, fmt, time::Duration};

#[cfg(target_os = "android")]
use std::ffi::c_void;

use radio_core::{rtl8812au::Rtl8812auRegisterError, DeviceSelector, Rtl8812auRegisterAccess};
use wfb_radio_runtime::{
    android_usbhost_open_plan, AndroidUsbHostConfig, RuntimeRadioSession, RuntimeTransportError,
    RuntimeUsbOpenConfig,
};

pub const JNI_REGISTER_SMOKE_SYMBOL: &str =
    "Java_com_arcedge_wfblink_smoke_WfbNativeSmoke_runRegisterSmoke";

pub const ANDROID_SMOKE_INVALID_ARGUMENT: i32 = -1;
pub const ANDROID_SMOKE_TRANSPORT_ERROR: i32 = -2;
pub const ANDROID_SMOKE_REGISTER_ERROR: i32 = -3;

#[derive(Debug)]
pub enum AndroidSmokeError {
    InvalidArgument(&'static str),
    Transport(RuntimeTransportError),
    Register(Rtl8812auRegisterError),
}

impl fmt::Display for AndroidSmokeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AndroidSmokeError::InvalidArgument(message) => write!(f, "{message}"),
            AndroidSmokeError::Transport(error) => write!(f, "{error}"),
            AndroidSmokeError::Register(error) => write!(f, "{error}"),
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

pub fn android_register_smoke_return_code(result: Result<u8, AndroidSmokeError>) -> i32 {
    match result {
        Ok(value) => i32::from(value),
        Err(AndroidSmokeError::InvalidArgument(_)) => ANDROID_SMOKE_INVALID_ARGUMENT,
        Err(AndroidSmokeError::Transport(_)) => ANDROID_SMOKE_TRANSPORT_ERROR,
        Err(AndroidSmokeError::Register(_)) => ANDROID_SMOKE_REGISTER_ERROR,
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
    let session =
        RuntimeRadioSession::open(RuntimeUsbOpenConfig::android_usbhost(selector, config))?;
    let registers = Rtl8812auRegisterAccess::new(&session.transport).with_timeout(timeout);
    Ok(registers.read8(register_address)?)
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
        assert!(matches!(
            android_smoke_jni_args(42, -1, 0x8812, 0, 0x81, 0x02, 3, 0x0000, 500),
            Err(AndroidSmokeError::InvalidArgument("vid"))
        ));
        assert!(matches!(
            android_smoke_jni_args(42, 0x0bda, 0x8812, 0, 0x81, 0x02, 3, 0x0000, -1),
            Err(AndroidSmokeError::InvalidArgument("timeout_ms"))
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
    }
}
