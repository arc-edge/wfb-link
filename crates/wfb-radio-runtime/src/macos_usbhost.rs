use std::{
    ffi::CStr,
    marker::PhantomData,
    os::raw::{c_char, c_int},
    ptr::NonNull,
    time::Duration,
};

use radio_core::{rtl8812au::Rtl8812auUsbTransport, UsbBulkTransfer, UsbError};

const RTL_USB_REQ: u8 = 0x05;
const RTL_READ_REQUEST_TYPE: u8 = 0xc0;
const RTL_WRITE_REQUEST_TYPE: u8 = 0x40;
const USB_STANDARD_READ_REQUEST_TYPE: u8 = 0x80;
const USB_REQUEST_GET_DESCRIPTOR: u8 = 0x06;
const ERROR_BUF_LEN: usize = 512;
const MAX_INTERFACE_PROBE_PIPES: usize = 16;
const PIPE_ERROR_BUF_LEN: usize = 160;

#[repr(C)]
struct WfbMacosUsbHost {
    _private: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomData<()>)>,
}

#[repr(C)]
struct WfbMacosUsbHostSession {
    _private: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomData<()>)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WfbMacosUsbHostPipeProbe {
    address: u8,
    requested: c_int,
    copied: c_int,
    descriptor_available: c_int,
    descriptor_address: u8,
    attributes: u8,
    max_packet_size: u16,
    interval: u8,
    error: [c_char; PIPE_ERROR_BUF_LEN],
}

#[repr(C)]
struct WfbMacosUsbHostInterfaceProbe {
    configure_attempted: c_int,
    configure_ok: c_int,
    match_interfaces: c_int,
    interface_found: c_int,
    interface_opened: c_int,
    poll_attempts_observed: u32,
    matched_interface_count: u32,
    pipe_count: usize,
    pipes: [WfbMacosUsbHostPipeProbe; MAX_INTERFACE_PROBE_PIPES],
}

#[repr(C)]
struct WfbMacosUsbHostBulkTransfer {
    configure_attempted: c_int,
    configure_ok: c_int,
    interface_found: c_int,
    interface_opened: c_int,
    pipe_copied: c_int,
    descriptor_available: c_int,
    transfer_ok: c_int,
    timed_out: c_int,
    poll_attempts_observed: u32,
    matched_interface_count: u32,
    endpoint_address: u8,
    descriptor_address: u8,
    attributes: u8,
    max_packet_size: u16,
    interval: u8,
    requested_len: usize,
    transferred_len: usize,
}

extern "C" {
    fn wfb_macos_usbhost_open(
        vid: u16,
        pid: u16,
        out_host: *mut *mut WfbMacosUsbHost,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    fn wfb_macos_usbhost_close(host: *mut WfbMacosUsbHost);
    fn wfb_macos_usbhost_control_read(
        host: *mut WfbMacosUsbHost,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: *mut u8,
        len: usize,
        timeout_ms: u64,
        transferred: *mut usize,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    fn wfb_macos_usbhost_control_write(
        host: *mut WfbMacosUsbHost,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: *const u8,
        len: usize,
        timeout_ms: u64,
        transferred: *mut usize,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    fn wfb_macos_usbhost_session_open(
        host: *mut WfbMacosUsbHost,
        vid: u16,
        pid: u16,
        configuration_value: u8,
        match_interfaces: c_int,
        interface_number: u8,
        bulk_in_endpoint: u8,
        bulk_out_endpoint: u8,
        poll_attempts: u32,
        poll_delay_ms: u64,
        out_session: *mut *mut WfbMacosUsbHostSession,
        result: *mut WfbMacosUsbHostInterfaceProbe,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    fn wfb_macos_usbhost_session_close(session: *mut WfbMacosUsbHostSession);
    fn wfb_macos_usbhost_session_bulk_read(
        session: *mut WfbMacosUsbHostSession,
        endpoint_address: u8,
        data: *mut u8,
        len: usize,
        timeout_ms: u64,
        result: *mut WfbMacosUsbHostBulkTransfer,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    fn wfb_macos_usbhost_session_bulk_write(
        session: *mut WfbMacosUsbHostSession,
        endpoint_address: u8,
        data: *const u8,
        len: usize,
        timeout_ms: u64,
        result: *mut WfbMacosUsbHostBulkTransfer,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    fn wfb_macos_usbhost_interface_probe(
        host: *mut WfbMacosUsbHost,
        vid: u16,
        pid: u16,
        configuration_value: u8,
        match_interfaces: c_int,
        interface_number: u8,
        pipe_addresses: *const u8,
        pipe_count: usize,
        poll_attempts: u32,
        poll_delay_ms: u64,
        result: *mut WfbMacosUsbHostInterfaceProbe,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    fn wfb_macos_usbhost_bulk_read_once(
        host: *mut WfbMacosUsbHost,
        vid: u16,
        pid: u16,
        configuration_value: u8,
        match_interfaces: c_int,
        interface_number: u8,
        endpoint_address: u8,
        data: *mut u8,
        len: usize,
        poll_attempts: u32,
        poll_delay_ms: u64,
        timeout_ms: u64,
        result: *mut WfbMacosUsbHostBulkTransfer,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
    fn wfb_macos_usbhost_bulk_write_once(
        host: *mut WfbMacosUsbHost,
        vid: u16,
        pid: u16,
        configuration_value: u8,
        match_interfaces: c_int,
        interface_number: u8,
        endpoint_address: u8,
        data: *const u8,
        len: usize,
        poll_attempts: u32,
        poll_delay_ms: u64,
        timeout_ms: u64,
        result: *mut WfbMacosUsbHostBulkTransfer,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
}

pub struct MacosUsbHostDevice {
    raw: NonNull<WfbMacosUsbHost>,
}

pub struct MacosUsbHostSession {
    raw: NonNull<WfbMacosUsbHostSession>,
    device: MacosUsbHostDevice,
    pub interface_probe: MacosUsbHostInterfaceProbe,
    pub bulk_in_endpoint: u8,
    pub bulk_out_endpoint: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct MacosUsbHostInterfaceProbeRequest<'a> {
    pub vid: u16,
    pub pid: u16,
    pub configuration_value: u8,
    pub match_interfaces: bool,
    pub interface_number: u8,
    pub pipe_addresses: &'a [u8],
    pub poll_attempts: u32,
    pub poll_delay: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct MacosUsbHostBulkReadRequest {
    pub vid: u16,
    pub pid: u16,
    pub configuration_value: u8,
    pub match_interfaces: bool,
    pub interface_number: u8,
    pub endpoint_address: u8,
    pub len: usize,
    pub poll_attempts: u32,
    pub poll_delay: Duration,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct MacosUsbHostBulkWriteRequest<'a> {
    pub vid: u16,
    pub pid: u16,
    pub configuration_value: u8,
    pub match_interfaces: bool,
    pub interface_number: u8,
    pub endpoint_address: u8,
    pub data: &'a [u8],
    pub poll_attempts: u32,
    pub poll_delay: Duration,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct MacosUsbHostSessionOpenRequest {
    pub vid: u16,
    pub pid: u16,
    pub configuration_value: u8,
    pub match_interfaces: bool,
    pub interface_number: u8,
    pub bulk_in_endpoint: u8,
    pub bulk_out_endpoint: u8,
    pub poll_attempts: u32,
    pub poll_delay: Duration,
}

#[derive(Debug, Clone)]
pub struct MacosUsbHostInterfaceProbe {
    pub configure_attempted: bool,
    pub configure_ok: bool,
    pub match_interfaces: bool,
    pub interface_found: bool,
    pub interface_opened: bool,
    pub poll_attempts_observed: u32,
    pub matched_interface_count: u32,
    pub pipes: Vec<MacosUsbHostPipeProbe>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MacosUsbHostPipeProbe {
    pub address: u8,
    pub requested: bool,
    pub copied: bool,
    pub descriptor_available: bool,
    pub descriptor_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MacosUsbHostBulkTransfer {
    pub configure_attempted: bool,
    pub configure_ok: bool,
    pub interface_found: bool,
    pub interface_opened: bool,
    pub pipe_copied: bool,
    pub descriptor_available: bool,
    pub transfer_ok: bool,
    pub timed_out: bool,
    pub poll_attempts_observed: u32,
    pub matched_interface_count: u32,
    pub endpoint_address: u8,
    pub descriptor_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
    pub requested_len: usize,
    pub transferred_len: usize,
    pub data: Vec<u8>,
    pub error: Option<String>,
}

impl MacosUsbHostDevice {
    pub fn open(vid: u16, pid: u16) -> Result<Self, String> {
        let mut raw = std::ptr::null_mut();
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc =
            unsafe { wfb_macos_usbhost_open(vid, pid, &mut raw, error.as_mut_ptr(), error.len()) };
        if rc != 0 {
            return Err(error_message(&error));
        }
        let raw =
            NonNull::new(raw).ok_or_else(|| "wfb_macos_usbhost_open returned null".to_string())?;
        Ok(Self { raw })
    }

    fn control_read(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, String> {
        let mut transferred = 0usize;
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc = unsafe {
            wfb_macos_usbhost_control_read(
                self.raw.as_ptr(),
                request_type,
                request,
                value,
                index,
                data.as_mut_ptr(),
                data.len(),
                duration_ms(timeout),
                &mut transferred,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if rc != 0 {
            Err(error_message(&error))
        } else {
            Ok(transferred)
        }
    }

    fn control_write(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, String> {
        let mut transferred = 0usize;
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc = unsafe {
            wfb_macos_usbhost_control_write(
                self.raw.as_ptr(),
                request_type,
                request,
                value,
                index,
                data.as_ptr(),
                data.len(),
                duration_ms(timeout),
                &mut transferred,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if rc != 0 {
            Err(error_message(&error))
        } else {
            Ok(transferred)
        }
    }

    pub fn get_descriptor(
        &self,
        descriptor_type: u8,
        descriptor_index: u8,
        len: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>, String> {
        let mut data = vec![0u8; len];
        let transferred = self.control_read(
            USB_STANDARD_READ_REQUEST_TYPE,
            USB_REQUEST_GET_DESCRIPTOR,
            u16::from(descriptor_type) << 8 | u16::from(descriptor_index),
            0,
            &mut data,
            timeout,
        )?;
        data.truncate(transferred);
        Ok(data)
    }

    pub fn probe_interface(
        &self,
        request: MacosUsbHostInterfaceProbeRequest<'_>,
    ) -> Result<MacosUsbHostInterfaceProbe, String> {
        if request.pipe_addresses.len() > MAX_INTERFACE_PROBE_PIPES {
            return Err(format!(
                "{} pipe addresses requested, max is {}",
                request.pipe_addresses.len(),
                MAX_INTERFACE_PROBE_PIPES
            ));
        }

        let mut raw_result: WfbMacosUsbHostInterfaceProbe = unsafe { std::mem::zeroed() };
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc = unsafe {
            wfb_macos_usbhost_interface_probe(
                self.raw.as_ptr(),
                request.vid,
                request.pid,
                request.configuration_value,
                c_int::from(request.match_interfaces),
                request.interface_number,
                request.pipe_addresses.as_ptr(),
                request.pipe_addresses.len(),
                request.poll_attempts,
                duration_ms(request.poll_delay),
                &mut raw_result,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if rc != 0 {
            return Err(error_message(&error));
        }

        Ok(interface_probe_from_raw(&raw_result, error_message(&error)))
    }

    pub fn bulk_read_once(
        &self,
        request: MacosUsbHostBulkReadRequest,
    ) -> Result<MacosUsbHostBulkTransfer, String> {
        let mut data = vec![0u8; request.len];
        let mut raw_result: WfbMacosUsbHostBulkTransfer = unsafe { std::mem::zeroed() };
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc = unsafe {
            wfb_macos_usbhost_bulk_read_once(
                self.raw.as_ptr(),
                request.vid,
                request.pid,
                request.configuration_value,
                c_int::from(request.match_interfaces),
                request.interface_number,
                request.endpoint_address,
                data.as_mut_ptr(),
                data.len(),
                request.poll_attempts,
                duration_ms(request.poll_delay),
                duration_ms(request.timeout),
                &mut raw_result,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if rc != 0 {
            return Err(error_message(&error));
        }

        let transferred_len = raw_result.transferred_len.min(data.len());
        data.truncate(transferred_len);
        Ok(bulk_transfer_from_raw(
            &raw_result,
            data,
            error_message(&error),
        ))
    }

    pub fn bulk_write_once(
        &self,
        request: MacosUsbHostBulkWriteRequest<'_>,
    ) -> Result<MacosUsbHostBulkTransfer, String> {
        let mut raw_result: WfbMacosUsbHostBulkTransfer = unsafe { std::mem::zeroed() };
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc = unsafe {
            wfb_macos_usbhost_bulk_write_once(
                self.raw.as_ptr(),
                request.vid,
                request.pid,
                request.configuration_value,
                c_int::from(request.match_interfaces),
                request.interface_number,
                request.endpoint_address,
                request.data.as_ptr(),
                request.data.len(),
                request.poll_attempts,
                duration_ms(request.poll_delay),
                duration_ms(request.timeout),
                &mut raw_result,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if rc != 0 {
            return Err(error_message(&error));
        }

        let transferred_len = raw_result.transferred_len.min(request.data.len());
        Ok(bulk_transfer_from_raw(
            &raw_result,
            request.data[..transferred_len].to_vec(),
            error_message(&error),
        ))
    }
}

impl Drop for MacosUsbHostDevice {
    fn drop(&mut self) {
        unsafe {
            wfb_macos_usbhost_close(self.raw.as_ptr());
        }
    }
}

impl MacosUsbHostSession {
    pub fn open(request: MacosUsbHostSessionOpenRequest) -> Result<Self, String> {
        let device = MacosUsbHostDevice::open(request.vid, request.pid)?;
        let mut raw_session = std::ptr::null_mut();
        let mut raw_probe: WfbMacosUsbHostInterfaceProbe = unsafe { std::mem::zeroed() };
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc = unsafe {
            wfb_macos_usbhost_session_open(
                device.raw.as_ptr(),
                request.vid,
                request.pid,
                request.configuration_value,
                c_int::from(request.match_interfaces),
                request.interface_number,
                request.bulk_in_endpoint,
                request.bulk_out_endpoint,
                request.poll_attempts,
                duration_ms(request.poll_delay),
                &mut raw_session,
                &mut raw_probe,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if rc != 0 {
            return Err(error_message(&error));
        }
        let raw = NonNull::new(raw_session).ok_or_else(|| {
            let message = error_message(&error);
            if message.is_empty() {
                "wfb_macos_usbhost_session_open returned null".to_string()
            } else {
                message
            }
        })?;
        Ok(Self {
            raw,
            device,
            interface_probe: interface_probe_from_raw(&raw_probe, error_message(&error)),
            bulk_in_endpoint: request.bulk_in_endpoint,
            bulk_out_endpoint: request.bulk_out_endpoint,
        })
    }

    pub fn bulk_read_once(
        &mut self,
        endpoint_address: u8,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<MacosUsbHostBulkTransfer, String> {
        let mut raw_result: WfbMacosUsbHostBulkTransfer = unsafe { std::mem::zeroed() };
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc = unsafe {
            wfb_macos_usbhost_session_bulk_read(
                self.raw.as_ptr(),
                endpoint_address,
                data.as_mut_ptr(),
                data.len(),
                duration_ms(timeout),
                &mut raw_result,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if rc != 0 {
            return Err(error_message(&error));
        }
        let transferred_len = raw_result.transferred_len.min(data.len());
        Ok(bulk_transfer_from_raw(
            &raw_result,
            data[..transferred_len].to_vec(),
            error_message(&error),
        ))
    }

    pub fn bulk_write_once(
        &mut self,
        endpoint_address: u8,
        data: &[u8],
        timeout: Duration,
    ) -> Result<MacosUsbHostBulkTransfer, String> {
        let mut raw_result: WfbMacosUsbHostBulkTransfer = unsafe { std::mem::zeroed() };
        let mut error = [0i8; ERROR_BUF_LEN];
        let rc = unsafe {
            wfb_macos_usbhost_session_bulk_write(
                self.raw.as_ptr(),
                endpoint_address,
                data.as_ptr(),
                data.len(),
                duration_ms(timeout),
                &mut raw_result,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if rc != 0 {
            return Err(error_message(&error));
        }
        let transferred_len = raw_result.transferred_len.min(data.len());
        Ok(bulk_transfer_from_raw(
            &raw_result,
            data[..transferred_len].to_vec(),
            error_message(&error),
        ))
    }
}

impl Drop for MacosUsbHostSession {
    fn drop(&mut self) {
        unsafe {
            wfb_macos_usbhost_session_close(self.raw.as_ptr());
        }
    }
}

impl Rtl8812auUsbTransport for &MacosUsbHostDevice {
    fn read_vendor(
        &self,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        self.control_read(
            RTL_READ_REQUEST_TYPE,
            RTL_USB_REQ,
            value,
            index,
            data,
            timeout,
        )
        .map_err(UsbError::Backend)
    }

    fn write_vendor(
        &self,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        self.control_write(
            RTL_WRITE_REQUEST_TYPE,
            RTL_USB_REQ,
            value,
            index,
            data,
            timeout,
        )
        .map_err(UsbError::Backend)
    }
}

impl Rtl8812auUsbTransport for &MacosUsbHostSession {
    fn read_vendor(
        &self,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        self.device
            .control_read(
                RTL_READ_REQUEST_TYPE,
                RTL_USB_REQ,
                value,
                index,
                data,
                timeout,
            )
            .map_err(UsbError::Backend)
    }

    fn write_vendor(
        &self,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        self.device
            .control_write(
                RTL_WRITE_REQUEST_TYPE,
                RTL_USB_REQ,
                value,
                index,
                data,
                timeout,
            )
            .map_err(UsbError::Backend)
    }
}

impl UsbBulkTransfer for MacosUsbHostSession {
    fn read_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        let transfer = self
            .bulk_read_once(endpoint, data, timeout)
            .map_err(UsbError::Backend)?;
        if transfer.transfer_ok {
            Ok(transfer.transferred_len)
        } else if transfer.timed_out {
            Err(UsbError::BackendTimeout(transfer.error.unwrap_or_else(
                || format!("bulk IN endpoint 0x{endpoint:02x} timed out"),
            )))
        } else {
            Err(UsbError::Backend(transfer.error.unwrap_or_else(|| {
                format!("bulk IN endpoint 0x{endpoint:02x} failed")
            })))
        }
    }

    fn write_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        let transfer = self
            .bulk_write_once(endpoint, data, timeout)
            .map_err(UsbError::Backend)?;
        if transfer.transfer_ok {
            Ok(transfer.transferred_len)
        } else if transfer.timed_out {
            Err(UsbError::BackendTimeout(transfer.error.unwrap_or_else(
                || format!("bulk OUT endpoint 0x{endpoint:02x} timed out"),
            )))
        } else {
            Err(UsbError::Backend(transfer.error.unwrap_or_else(|| {
                format!("bulk OUT endpoint 0x{endpoint:02x} failed")
            })))
        }
    }
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn error_message(buffer: &[i8; ERROR_BUF_LEN]) -> String {
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn pipe_error_message(buffer: &[c_char; PIPE_ERROR_BUF_LEN]) -> Option<String> {
    let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    if message.is_empty() {
        None
    } else {
        Some(message)
    }
}

fn interface_probe_from_raw(
    raw: &WfbMacosUsbHostInterfaceProbe,
    error: String,
) -> MacosUsbHostInterfaceProbe {
    let pipe_count = raw.pipe_count.min(MAX_INTERFACE_PROBE_PIPES);
    MacosUsbHostInterfaceProbe {
        configure_attempted: raw.configure_attempted != 0,
        configure_ok: raw.configure_ok != 0,
        match_interfaces: raw.match_interfaces != 0,
        interface_found: raw.interface_found != 0,
        interface_opened: raw.interface_opened != 0,
        poll_attempts_observed: raw.poll_attempts_observed,
        matched_interface_count: raw.matched_interface_count,
        pipes: raw.pipes[..pipe_count]
            .iter()
            .map(|pipe| MacosUsbHostPipeProbe {
                address: pipe.address,
                requested: pipe.requested != 0,
                copied: pipe.copied != 0,
                descriptor_available: pipe.descriptor_available != 0,
                descriptor_address: pipe.descriptor_address,
                attributes: pipe.attributes,
                max_packet_size: pipe.max_packet_size,
                interval: pipe.interval,
                error: pipe_error_message(&pipe.error),
            })
            .collect(),
        error: if error.is_empty() { None } else { Some(error) },
    }
}

fn bulk_transfer_from_raw(
    raw: &WfbMacosUsbHostBulkTransfer,
    data: Vec<u8>,
    error: String,
) -> MacosUsbHostBulkTransfer {
    MacosUsbHostBulkTransfer {
        configure_attempted: raw.configure_attempted != 0,
        configure_ok: raw.configure_ok != 0,
        interface_found: raw.interface_found != 0,
        interface_opened: raw.interface_opened != 0,
        pipe_copied: raw.pipe_copied != 0,
        descriptor_available: raw.descriptor_available != 0,
        transfer_ok: raw.transfer_ok != 0,
        timed_out: raw.timed_out != 0,
        poll_attempts_observed: raw.poll_attempts_observed,
        matched_interface_count: raw.matched_interface_count,
        endpoint_address: raw.endpoint_address,
        descriptor_address: raw.descriptor_address,
        attributes: raw.attributes,
        max_packet_size: raw.max_packet_size,
        interval: raw.interval,
        requested_len: raw.requested_len,
        transferred_len: raw.transferred_len,
        data,
        error: if error.is_empty() { None } else { Some(error) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_ms_handles_zero_and_regular_values() {
        assert_eq!(duration_ms(Duration::ZERO), 0);
        assert_eq!(duration_ms(Duration::from_millis(500)), 500);
    }
}
