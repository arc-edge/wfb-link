## Context

`wfb-link` already separates the product-facing lifecycle from the platform
radio path. macOS uses direct userspace USB through IOUSBHost or libusb. Linux
should use native WFB-NG over monitor mode. Android should follow the macOS
direct-radio shape because Android apps can own USB host devices but generally
cannot depend on a monitor-mode kernel driver stack.

The first Android slice should establish the contract without pretending the
native transfer bridge exists. That lets product config, endpoint shape, and
runtime selection compile now, while hardware work proceeds behind a stable
boundary.

## Goals

- Represent Android USBHost as a first-class runtime backend.
- Keep Android stream, WFB, tunnel, readiness, health, and report contracts
  identical to the existing userspace radio backend.
- Let an Android app layer provide transport handoff data, starting with a
  native device file descriptor field.
- Reject invalid endpoint layouts and multiple enabled USB backends before any
  hardware open attempt.
- Use stable validation errors for missing fd, invalid fd, unsupported adapter
  metadata, unsupported selector shape, and invalid endpoint layout.

## Non-Goals

- Add an Android app package, Gradle build, or USB permission UI in this slice.
- Change RF defaults, WFB packet formats, calibration behavior, or Linux/macOS
  backend behavior.

## Design

### Runtime Config

`AndroidUsbHostConfig` mirrors the endpoint choices the RTL8812AU runtime needs:
interface number, selected bulk IN endpoint, selected bulk OUT endpoint, and
bulk OUT endpoint layout count. `device_fd` is optional because service files
may describe static profile defaults while an Android app supplies the concrete
opened device handle at runtime.

`ProductionRuntimeUsbConfig` carries `backend = AndroidUsbHost` plus an optional
serialized Android config. `to_runtime_open_config` maps that snapshot into the
live runtime open request in the same way macOS does.

### Endpoint And Adapter Shape

Android endpoint validation reuses the same known RTL8812AU queue layouts as
macOS: two, three, or four bulk OUT endpoints. The default AWUS036ACH layout is
bulk IN `0x81` plus bulk OUT `0x02`, `0x03`, and `0x04`.

The adapter metadata is synthetic until the native Android bridge can query the
device descriptors directly. It still preserves VID/PID, known-adapter lookup,
interface number, endpoint list, and a platform-specific speed string so JSON
reports and link health have the same shape.

### Native Bridge Decision

Use app-owned `UsbDeviceConnection` calls through JNI for Android hardware.

The Android app layer owns permission, opens `UsbDeviceConnection`, claims the
RTL8812AU interface, resolves the selected `UsbEndpoint` objects, and keeps all
of those Java objects alive until Rust returns. Rust implements the existing
`Rtl8812auUsbTransport` and `UsbBulkTransfer` traits by calling
`controlTransfer` and `bulkTransfer` on those Java objects.

The original design preferred `getFileDescriptor()` plus
`libusb_wrap_sys_device` to avoid per-transfer JNI. Live Pixel 7 Pro testing
showed Java control transfers worked while libusb fd wrapping failed with
`Input/Output Error`, so the smoke path moved to direct JNI. The runtime config
still carries fd metadata for validation/report compatibility, but fd wrapping
is not the active Android hardware path.

## Validation

- Unit tests cover endpoint derivation, invalid endpoint rejection, runtime
  backend mapping, open-plan validation, fd preflight validation, service config
  mapping, and multiple-backend rejection.
- Android target checking requires an NDK compiler such as
  `aarch64-linux-android-clang`; without that toolchain, the vendored libusb
  build stops before Android Rust code can be checked.
- Hardware validation now has an Android app handoff harness. Pixel 7 Pro live
  smoke has passed USB permission, Java control transfer, Rust JNI register
  read, full production RTL8812AU init, monitor opmode receive filtering,
  receiver-backed Android TX, and receiver-backed Android RX frame parsing
  against the existing Linux peer. The remaining hardware gate is a packaged
  Android managed-stream profile comparable to the macOS managed-stream bench.
