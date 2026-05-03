## MODIFIED Requirements

### Requirement: macOS Direct-Control Fallback
The system SHALL provide a macOS-only direct-control diagnostic and runtime transport path for RTL8812AU adapters that are visible in IOKit but unavailable through libusb enumeration, with retained-session endpoint validation and opening provided by the runtime library.

#### Scenario: IOKit-visible adapter lacks libusb interfaces
- **WHEN** IOKit reports a matching RTL8812AU `IOUSBHostDevice` without libusb-visible interfaces
- **THEN** the system reports the IOKit registration, matching, configuration, location, speed, and interface-child state without requiring a macOS network interface

#### Scenario: Direct-control register access succeeds
- **WHEN** the operator selects a matching VID/PID through the macOS direct-control path
- **THEN** the system can issue bounded standard descriptor reads, RTL8812AU vendor control reads, guarded EFUSE control sequences, guarded power-on/RF-reset control-write diagnostics, guarded firmware download/readiness polling, guarded LLT programming, guarded queue/DMA setup, guarded MAC/WMAC setup, guarded BB PHY/AGC setup, guarded RF radio table setup, IOUSBHost interface/pipe probes, bounded bulk-IN smoke requests, zero-length bulk-OUT smoke requests, and retained-session control plus bulk pipe smokes while clearly reporting that retained full-init RX/TX remains a separate verification target

#### Scenario: Runtime library owns direct-control transport
- **WHEN** macOS direct-control access is used by diagnostic or production runtime code
- **THEN** the IOUSBHost wrapper, retained session, bulk/register trait implementations, endpoint validation, and retained-session opening are provided by the runtime library
