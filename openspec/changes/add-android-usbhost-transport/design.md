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
- Return stable fail-closed errors until the Android transfer bridge lands.

## Non-Goals

- Implement JNI or NDK USB transfer calls in this slice.
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

### Native Bridge Follow-Up

The next slice should implement an Android transport that satisfies
`Rtl8812auUsbTransport` and `UsbBulkTransfer`. There are two plausible
approaches:

- JNI calls into an app-owned `UsbDeviceConnection` for control and bulk
  transfers.
- Native ownership of a handed-off file descriptor, if Android USB semantics
  permit reliable descriptor lifetime and endpoint I/O for this adapter.

The bridge must define ownership, close behavior, timeout behavior, and thread
safety before hardware validation. Until then, `open_android_usbhost_transport`
returns a stable not-implemented error on Android and an unsupported-platform
error elsewhere.

## Validation

- Unit tests cover endpoint derivation, invalid endpoint rejection, runtime
  backend mapping, service config mapping, and multiple-backend rejection.
- Hardware validation waits for the native Android bridge. The first hardware
  gate should be RX-only descriptor parsing, then single TX, then bounded
  bidirectional WFB distributor datagrams against the existing Linux peer.
