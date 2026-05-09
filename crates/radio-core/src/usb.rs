use std::time::{SystemTime, UNIX_EPOCH};

use rusb::{
    ConfigDescriptor, Device, DeviceDescriptor, DeviceHandle, Direction, GlobalContext,
    TransferType, UsbContext,
};
use serde::Serialize;
use thiserror::Error;

#[cfg(unix)]
use std::os::fd::RawFd;

use crate::registry::{lookup_known_adapter, KnownAdapter};

const DEFAULT_INTERFACE: u8 = 0;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DeviceSelector {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub bus: Option<u8>,
    pub address: Option<u8>,
}

impl DeviceSelector {
    pub fn is_empty(&self) -> bool {
        self.vid.is_none() && self.pid.is_none() && self.bus.is_none() && self.address.is_none()
    }

    pub fn matches(&self, device: &UsbDeviceInfo) -> bool {
        self.vid.map_or(true, |vid| vid == device.vid)
            && self.pid.map_or(true, |pid| pid == device.pid)
            && self.bus.map_or(true, |bus| bus == device.bus)
            && self
                .address
                .map_or(true, |address| address == device.address)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UsbDeviceInfo {
    pub vid: u16,
    pub pid: u16,
    pub vid_hex: String,
    pub pid_hex: String,
    pub bus: u8,
    pub address: u8,
    pub speed: String,
    pub class_code: u8,
    pub sub_class_code: u8,
    pub protocol_code: u8,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    pub known_adapter: Option<KnownAdapter>,
    pub interfaces: Vec<InterfaceInfo>,
}

pub type ProbeDevice = UsbDeviceInfo;

#[derive(Debug, Clone, Serialize)]
pub struct InterfaceInfo {
    pub number: u8,
    pub setting_number: u8,
    pub class_code: u8,
    pub sub_class_code: u8,
    pub protocol_code: u8,
    pub endpoints: Vec<EndpointInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndpointInfo {
    pub address: u8,
    pub direction: String,
    pub transfer_type: String,
    pub max_packet_size: u16,
    pub interval: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsbEndpoints {
    pub interface_number: u8,
    pub bulk_in: Option<u8>,
    pub bulk_out: Option<u8>,
    pub bulk_in_all: Vec<u8>,
    pub bulk_out_all: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct PlatformInfo {
    pub os: &'static str,
    pub family: &'static str,
    pub arch: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeResult {
    Pass,
    Fail,
}

impl ProbeResult {
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeResult::Pass => "pass",
            ProbeResult::Fail => "fail",
        }
    }

    pub fn is_failure(self) -> bool {
        matches!(self, ProbeResult::Fail)
    }
}

#[derive(Debug, Serialize)]
pub struct ProbeClaim {
    pub attempted: bool,
    pub success: bool,
    pub device: Option<UsbDeviceInfo>,
    pub endpoints: Option<UsbEndpoints>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UsbProbeReport {
    pub schema_version: u8,
    pub command: &'static str,
    pub started_at_unix_ms: u64,
    pub platform: PlatformInfo,
    pub selector: DeviceSelector,
    pub include_unsupported: bool,
    pub claim_requested: bool,
    pub result: ProbeResult,
    pub devices: Vec<UsbDeviceInfo>,
    pub claim: Option<ProbeClaim>,
    pub errors: Vec<String>,
}

#[derive(Debug, Error)]
pub enum UsbError {
    #[error("USB operation failed: {0}")]
    Rusb(#[from] rusb::Error),
    #[error("USB backend operation failed: {0}")]
    Backend(String),
    #[error("USB backend operation timed out: {0}")]
    BackendTimeout(String),
    #[error("no supported adapter matched selector")]
    NoSupportedAdapter,
    #[error("selected device disappeared before claim")]
    DeviceDisappeared,
    #[error("interface {interface} has no usable bulk IN/OUT endpoint pair")]
    MissingBulkEndpoints { interface: u8 },
}

impl UsbError {
    pub fn is_timeout(&self) -> bool {
        matches!(
            self,
            UsbError::Rusb(rusb::Error::Timeout) | UsbError::BackendTimeout(_)
        )
    }
}

pub struct ClaimedUsbDevice {
    pub(crate) handle: DeviceHandle<GlobalContext>,
    interface_number: u8,
    pub info: UsbDeviceInfo,
    pub endpoints: UsbEndpoints,
}

impl ClaimedUsbDevice {
    pub fn read_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: std::time::Duration,
    ) -> Result<usize, UsbError> {
        self.handle
            .read_control(request_type, request, value, index, data, timeout)
            .map_err(UsbError::from)
    }

    pub fn write_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: std::time::Duration,
    ) -> Result<usize, UsbError> {
        self.handle
            .write_control(request_type, request, value, index, data, timeout)
            .map_err(UsbError::from)
    }
}

pub trait UsbBulkTransfer {
    fn read_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &mut [u8],
        timeout: std::time::Duration,
    ) -> Result<usize, UsbError>;

    fn write_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: std::time::Duration,
    ) -> Result<usize, UsbError>;
}

impl UsbBulkTransfer for ClaimedUsbDevice {
    fn read_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &mut [u8],
        timeout: std::time::Duration,
    ) -> Result<usize, UsbError> {
        self.handle
            .read_bulk(endpoint, data, timeout)
            .map_err(UsbError::from)
    }

    fn write_bulk_transfer(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: std::time::Duration,
    ) -> Result<usize, UsbError> {
        self.handle
            .write_bulk(endpoint, data, timeout)
            .map_err(UsbError::from)
    }
}

impl Drop for ClaimedUsbDevice {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(self.interface_number);
    }
}

pub fn list_usb_devices(include_unsupported: bool) -> Result<Vec<UsbDeviceInfo>, UsbError> {
    let devices = collect_usb_devices()?;
    Ok(devices
        .into_iter()
        .filter(|device| include_unsupported || device.known_adapter.is_some())
        .collect())
}

pub fn probe_usb(
    selector: DeviceSelector,
    include_unsupported: bool,
    claim_requested: bool,
) -> UsbProbeReport {
    let mut errors = Vec::new();
    let all_devices = match collect_usb_devices() {
        Ok(devices) => devices,
        Err(error) => {
            errors.push(error.to_string());
            Vec::new()
        }
    };

    build_probe_report(
        selector,
        include_unsupported,
        claim_requested,
        all_devices,
        errors,
        |device| {
            claim_usb_device(device)
                .map(|claimed| (claimed.info.clone(), claimed.endpoints.clone()))
        },
    )
}

fn build_probe_report<F>(
    selector: DeviceSelector,
    include_unsupported: bool,
    claim_requested: bool,
    all_devices: Vec<UsbDeviceInfo>,
    errors: Vec<String>,
    claim_device: F,
) -> UsbProbeReport
where
    F: Fn(&UsbDeviceInfo) -> Result<(UsbDeviceInfo, UsbEndpoints), UsbError>,
{
    let started_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();

    let matching_supported: Vec<_> = all_devices
        .iter()
        .filter(|device| selector.matches(device) && device.known_adapter.is_some())
        .cloned()
        .collect();

    let visible_devices: Vec<_> = all_devices
        .iter()
        .filter(|device| {
            include_unsupported
                || device.known_adapter.is_some()
                || (!selector.is_empty() && selector.matches(device))
        })
        .cloned()
        .collect();

    let mut result = if matching_supported.is_empty() {
        ProbeResult::Fail
    } else {
        ProbeResult::Pass
    };

    let claim = if claim_requested {
        match matching_supported.first() {
            Some(device) => match claim_device(device) {
                Ok((info, endpoints)) => {
                    result = ProbeResult::Pass;
                    Some(ProbeClaim {
                        attempted: true,
                        success: true,
                        device: Some(info),
                        endpoints: Some(endpoints),
                        error: None,
                    })
                }
                Err(error) => {
                    result = ProbeResult::Fail;
                    Some(ProbeClaim {
                        attempted: true,
                        success: false,
                        device: Some(device.clone()),
                        endpoints: None,
                        error: Some(error.to_string()),
                    })
                }
            },
            None => {
                result = ProbeResult::Fail;
                Some(ProbeClaim {
                    attempted: false,
                    success: false,
                    device: None,
                    endpoints: None,
                    error: Some(no_match_message(&all_devices, selector)),
                })
            }
        }
    } else {
        None
    };

    UsbProbeReport {
        schema_version: 1,
        command: "usb-probe",
        started_at_unix_ms,
        platform: PlatformInfo {
            os: std::env::consts::OS,
            family: std::env::consts::FAMILY,
            arch: std::env::consts::ARCH,
        },
        selector,
        include_unsupported,
        claim_requested,
        result,
        devices: visible_devices,
        claim,
        errors,
    }
}

pub fn claim_usb_device(info: &UsbDeviceInfo) -> Result<ClaimedUsbDevice, UsbError> {
    let devices = rusb::devices()?;
    for device in devices.iter() {
        let descriptor = device.device_descriptor()?;
        if descriptor.vendor_id() == info.vid
            && descriptor.product_id() == info.pid
            && device.bus_number() == info.bus
            && device.address() == info.address
        {
            let endpoints = discover_bulk_endpoints(&device, DEFAULT_INTERFACE)?;
            if endpoints.bulk_in.is_none() || endpoints.bulk_out.is_none() {
                return Err(UsbError::MissingBulkEndpoints {
                    interface: DEFAULT_INTERFACE,
                });
            }

            let handle = device.open()?;
            #[cfg(target_os = "linux")]
            {
                if handle
                    .kernel_driver_active(DEFAULT_INTERFACE)
                    .unwrap_or(false)
                {
                    let _ = handle.detach_kernel_driver(DEFAULT_INTERFACE);
                }
            }
            handle.claim_interface(DEFAULT_INTERFACE)?;

            return Ok(ClaimedUsbDevice {
                handle,
                interface_number: DEFAULT_INTERFACE,
                info: info.clone(),
                endpoints,
            });
        }
    }

    Err(UsbError::DeviceDisappeared)
}

#[cfg(unix)]
pub fn claim_usb_device_from_fd(
    fd: RawFd,
    info: UsbDeviceInfo,
    endpoints: UsbEndpoints,
    interface_number: u8,
) -> Result<ClaimedUsbDevice, UsbError> {
    if fd < 0 {
        return Err(UsbError::Backend(format!(
            "invalid USB device file descriptor {fd}"
        )));
    }
    if endpoints.bulk_in.is_none() || endpoints.bulk_out.is_none() {
        return Err(UsbError::MissingBulkEndpoints {
            interface: interface_number,
        });
    }

    let handle = unsafe { rusb::GlobalContext::default().open_device_with_fd(fd)? };
    handle.claim_interface(interface_number)?;

    Ok(ClaimedUsbDevice {
        handle,
        interface_number,
        info,
        endpoints,
    })
}

fn collect_usb_devices() -> Result<Vec<UsbDeviceInfo>, UsbError> {
    let devices = rusb::devices()?;
    let mut out = Vec::new();

    for device in devices.iter() {
        let descriptor = match device.device_descriptor() {
            Ok(descriptor) => descriptor,
            Err(error) => {
                tracing::warn!(?error, "skipping USB device with unreadable descriptor");
                continue;
            }
        };

        out.push(build_device_info(&device, &descriptor));
    }

    Ok(out)
}

fn build_device_info(
    device: &Device<GlobalContext>,
    descriptor: &DeviceDescriptor,
) -> UsbDeviceInfo {
    let (manufacturer, product, serial_number) = read_strings(device, descriptor);
    let vid = descriptor.vendor_id();
    let pid = descriptor.product_id();
    UsbDeviceInfo {
        vid,
        pid,
        vid_hex: format!("0x{vid:04x}"),
        pid_hex: format!("0x{pid:04x}"),
        bus: device.bus_number(),
        address: device.address(),
        speed: format!("{:?}", device.speed()).to_ascii_lowercase(),
        class_code: descriptor.class_code(),
        sub_class_code: descriptor.sub_class_code(),
        protocol_code: descriptor.protocol_code(),
        manufacturer,
        product,
        serial_number,
        known_adapter: lookup_known_adapter(vid, pid),
        interfaces: read_interfaces(device).unwrap_or_default(),
    }
}

fn read_strings(
    device: &Device<GlobalContext>,
    descriptor: &DeviceDescriptor,
) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(handle) = device.open() else {
        return (None, None, None);
    };

    let manufacturer = handle
        .read_manufacturer_string_ascii(descriptor)
        .ok()
        .and_then(clean_usb_string);
    let product = handle
        .read_product_string_ascii(descriptor)
        .ok()
        .and_then(clean_usb_string);
    let serial_number = handle
        .read_serial_number_string_ascii(descriptor)
        .ok()
        .and_then(clean_usb_string);

    (manufacturer, product, serial_number)
}

fn clean_usb_string(value: String) -> Option<String> {
    let cleaned = value.trim_matches(|ch: char| ch == '\0' || ch.is_whitespace());
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn read_interfaces(device: &Device<GlobalContext>) -> Result<Vec<InterfaceInfo>, rusb::Error> {
    let config = read_config_descriptor(device)?;
    let mut interfaces = Vec::new();
    for interface in config.interfaces() {
        for descriptor in interface.descriptors() {
            let endpoints = descriptor
                .endpoint_descriptors()
                .map(|endpoint| EndpointInfo {
                    address: endpoint.address(),
                    direction: direction_name(endpoint.direction()).to_string(),
                    transfer_type: transfer_type_name(endpoint.transfer_type()).to_string(),
                    max_packet_size: endpoint.max_packet_size(),
                    interval: endpoint.interval(),
                })
                .collect();

            interfaces.push(InterfaceInfo {
                number: descriptor.interface_number(),
                setting_number: descriptor.setting_number(),
                class_code: descriptor.class_code(),
                sub_class_code: descriptor.sub_class_code(),
                protocol_code: descriptor.protocol_code(),
                endpoints,
            });
        }
    }
    Ok(interfaces)
}

fn discover_bulk_endpoints(
    device: &Device<GlobalContext>,
    interface_number: u8,
) -> Result<UsbEndpoints, UsbError> {
    let config = read_config_descriptor(device)?;
    let mut bulk_in_all = Vec::new();
    let mut bulk_out_all = Vec::new();

    for interface in config.interfaces() {
        for descriptor in interface.descriptors() {
            if descriptor.interface_number() != interface_number {
                continue;
            }
            for endpoint in descriptor.endpoint_descriptors() {
                if endpoint.transfer_type() != TransferType::Bulk {
                    continue;
                }
                match endpoint.direction() {
                    Direction::In => bulk_in_all.push(endpoint.address()),
                    Direction::Out => bulk_out_all.push(endpoint.address()),
                }
            }
        }
    }

    bulk_in_all.sort_unstable();
    bulk_out_all.sort_unstable();
    bulk_in_all.dedup();
    bulk_out_all.dedup();

    Ok(UsbEndpoints {
        interface_number,
        bulk_in: bulk_in_all.first().copied(),
        bulk_out: bulk_out_all.first().copied(),
        bulk_in_all,
        bulk_out_all,
    })
}

fn read_config_descriptor(device: &Device<GlobalContext>) -> Result<ConfigDescriptor, rusb::Error> {
    device
        .active_config_descriptor()
        .or_else(|_| device.config_descriptor(0))
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::In => "in",
        Direction::Out => "out",
    }
}

fn transfer_type_name(transfer_type: TransferType) -> &'static str {
    match transfer_type {
        TransferType::Control => "control",
        TransferType::Isochronous => "isochronous",
        TransferType::Bulk => "bulk",
        TransferType::Interrupt => "interrupt",
    }
}

fn no_match_message(devices: &[UsbDeviceInfo], selector: DeviceSelector) -> String {
    if devices.iter().any(|device| selector.matches(device)) {
        "matching USB device is present but is not in the supported adapter registry".to_string()
    } else if selector.is_empty() {
        "no supported RTL8812AU adapter found".to_string()
    } else {
        "no USB device matched selector".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::{
        io::{Read, Write},
        os::fd::AsRawFd,
        os::unix::net::UnixStream,
    };

    fn sample_device(known_adapter: Option<KnownAdapter>) -> UsbDeviceInfo {
        UsbDeviceInfo {
            vid: 0x0bda,
            pid: 0x8812,
            vid_hex: "0x0bda".to_string(),
            pid_hex: "0x8812".to_string(),
            bus: 1,
            address: 2,
            speed: "high".to_string(),
            class_code: 0,
            sub_class_code: 0,
            protocol_code: 0,
            manufacturer: None,
            product: None,
            serial_number: None,
            known_adapter,
            interfaces: Vec::new(),
        }
    }

    fn sample_endpoints() -> UsbEndpoints {
        UsbEndpoints {
            interface_number: 0,
            bulk_in: Some(0x81),
            bulk_out: Some(0x02),
            bulk_in_all: vec![0x81],
            bulk_out_all: vec![0x02],
        }
    }

    #[test]
    fn selector_matches_only_supplied_fields() {
        let device = sample_device(None);
        assert!(DeviceSelector::default().matches(&device));
        assert!(DeviceSelector {
            vid: Some(0x0bda),
            pid: None,
            bus: Some(1),
            address: None
        }
        .matches(&device));
        assert!(!DeviceSelector {
            vid: Some(0x0bda),
            pid: Some(0xffff),
            bus: None,
            address: None
        }
        .matches(&device));
    }

    #[test]
    fn no_match_message_distinguishes_unsupported_match() {
        let unsupported = sample_device(None);
        let msg = no_match_message(
            &[unsupported],
            DeviceSelector {
                vid: Some(0x0bda),
                pid: Some(0x8812),
                bus: None,
                address: None,
            },
        );
        assert!(msg.contains("not in the supported adapter registry"));
    }

    #[test]
    fn nonexistent_selector_probe_fails_without_claim_attempt() {
        let report = probe_usb(
            DeviceSelector {
                vid: Some(0xffff),
                pid: Some(0xffff),
                bus: None,
                address: None,
            },
            false,
            false,
        );
        assert_eq!(report.result, ProbeResult::Fail);
        assert!(report.claim.is_none());
    }

    #[test]
    fn report_builder_handles_absent_supported_adapter() {
        let report = build_probe_report(
            DeviceSelector::default(),
            false,
            true,
            Vec::new(),
            Vec::new(),
            |_| unreachable!("claim must not run without a supported candidate"),
        );

        assert_eq!(report.result, ProbeResult::Fail);
        let claim = report.claim.expect("claim result");
        assert!(!claim.attempted);
        assert!(claim.error.expect("error").contains("no supported"));
    }

    #[test]
    fn report_builder_handles_unsupported_matching_adapter() {
        let report = build_probe_report(
            DeviceSelector {
                vid: Some(0x0bda),
                pid: Some(0x8812),
                bus: None,
                address: None,
            },
            false,
            true,
            vec![sample_device(None)],
            Vec::new(),
            |_| unreachable!("claim must not run for unsupported devices"),
        );

        assert_eq!(report.result, ProbeResult::Fail);
        assert_eq!(report.devices.len(), 1);
        let claim = report.claim.expect("claim result");
        assert!(!claim.attempted);
        assert!(claim.error.expect("error").contains("not in the supported"));
    }

    #[test]
    fn report_builder_handles_claim_failure() {
        let known = lookup_known_adapter(0x0bda, 0x8812);
        let report = build_probe_report(
            DeviceSelector::default(),
            false,
            true,
            vec![sample_device(known)],
            Vec::new(),
            |_| Err(UsbError::MissingBulkEndpoints { interface: 0 }),
        );

        assert_eq!(report.result, ProbeResult::Fail);
        let claim = report.claim.expect("claim result");
        assert!(claim.attempted);
        assert!(!claim.success);
        assert!(claim.error.expect("error").contains("no usable bulk"));
    }

    #[test]
    fn report_builder_handles_claim_success() {
        let known = lookup_known_adapter(0x0bda, 0x8812);
        let device = sample_device(known);
        let report = build_probe_report(
            DeviceSelector::default(),
            false,
            true,
            vec![device.clone()],
            Vec::new(),
            |_| Ok((device.clone(), sample_endpoints())),
        );

        assert_eq!(report.result, ProbeResult::Pass);
        let claim = report.claim.expect("claim result");
        assert!(claim.attempted);
        assert!(claim.success);
        assert_eq!(claim.endpoints.expect("endpoints").bulk_in, Some(0x81));
    }

    #[test]
    fn usb_error_classifies_timeout_variants() {
        assert!(UsbError::Rusb(rusb::Error::Timeout).is_timeout());
        assert!(UsbError::BackendTimeout("timed out".to_string()).is_timeout());
        assert!(!UsbError::Backend("failed".to_string()).is_timeout());
    }

    #[cfg(unix)]
    #[test]
    fn fd_claim_rejects_invalid_fd_before_libusb_open() {
        let error = match claim_usb_device_from_fd(-1, sample_device(None), sample_endpoints(), 0) {
            Ok(_) => panic!("invalid fd opened"),
            Err(error) => error,
        };

        assert!(error
            .to_string()
            .contains("invalid USB device file descriptor"));
    }

    #[cfg(unix)]
    #[test]
    fn fd_claim_rejects_missing_bulk_endpoint_shape_before_libusb_open() {
        let mut endpoints = sample_endpoints();
        endpoints.bulk_out = None;
        let error = match claim_usb_device_from_fd(42, sample_device(None), endpoints, 0) {
            Ok(_) => panic!("missing bulk OUT opened"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            UsbError::MissingBulkEndpoints { interface: 0 }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn fd_claim_does_not_close_non_usb_fd_when_libusb_rejects_it() {
        let (mut candidate, mut peer) = UnixStream::pair().expect("unix stream pair");
        let error = match claim_usb_device_from_fd(
            candidate.as_raw_fd(),
            sample_device(None),
            sample_endpoints(),
            0,
        ) {
            Ok(_) => panic!("non-USB fd opened as USB"),
            Err(error) => error,
        };
        assert!(!error.is_timeout());

        peer.write_all(b"x")
            .expect("peer write after failed USB wrap");
        let mut byte = [0u8; 1];
        candidate
            .read_exact(&mut byte)
            .expect("candidate fd still owned by caller");
        assert_eq!(byte, [b'x']);
    }
}
