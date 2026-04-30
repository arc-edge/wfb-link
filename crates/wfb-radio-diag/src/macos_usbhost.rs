use std::{
    ffi::CStr,
    marker::PhantomData,
    os::raw::{c_char, c_int},
    ptr::NonNull,
    time::Duration,
};

use radio_core::{rtl8812au::Rtl8812auUsbTransport, UsbError};

const RTL_USB_REQ: u8 = 0x05;
const RTL_READ_REQUEST_TYPE: u8 = 0xc0;
const RTL_WRITE_REQUEST_TYPE: u8 = 0x40;
const USB_STANDARD_READ_REQUEST_TYPE: u8 = 0x80;
const USB_REQUEST_GET_DESCRIPTOR: u8 = 0x06;
const ERROR_BUF_LEN: usize = 512;

#[repr(C)]
struct WfbMacosUsbHost {
    _private: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomData<()>)>,
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
}

pub struct MacosUsbHostDevice {
    raw: NonNull<WfbMacosUsbHost>,
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
}

impl Drop for MacosUsbHostDevice {
    fn drop(&mut self) {
        unsafe {
            wfb_macos_usbhost_close(self.raw.as_ptr());
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

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn error_message(buffer: &[i8; ERROR_BUF_LEN]) -> String {
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
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
