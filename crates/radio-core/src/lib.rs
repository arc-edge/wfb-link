pub mod channel;
pub mod firmware;
pub mod frame;
pub mod init_plan;
pub mod pcap;
pub mod realtek_table;
pub mod registry;
pub mod rtl8812au;
pub mod trace;
pub mod usb;

pub use channel::{supported_channels, Band, Bandwidth, Channel, ChannelError};
pub use firmware::{
    FirmwareChunk, FirmwareChunks, FirmwareError, FirmwareImage, FirmwarePayload, FirmwareSource,
    REALTEK_FIRMWARE_HEADER_LEN, RTL8812A_FIRMWARE_MAX_LEN,
};
pub use frame::{frame_type, validate_ieee80211_frame, FrameType, Ieee80211FrameError};
pub use init_plan::{
    plan_rtl8812au_init, InitDryRunPlan, InitOperation, InitPhase, InitPhaseCount, InitRegister,
    PlannedInitTransfer, INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE, INIT_PLAN_REFERENCE_COMMIT,
    INIT_PLAN_REFERENCE_REPO,
};
pub use pcap::PcapWriter;
pub use realtek_table::{
    parse_realtek_u32_array, parse_realtek_u8_array, plan_realtek_table, RealtekConditionEnv,
    RealtekTableAction, RealtekTableActionKind, RealtekTableError, RealtekTableKind,
    RealtekTablePlan,
};
pub use registry::{known_adapters, lookup_known_adapter, Chipset, KnownAdapter};
pub use rtl8812au::{
    build_tx_packet, parse_rx_packet, submit_tx_frame, ParsedRxPacket, RegisterWidth,
    Rtl8812auRegisterAccess, Rtl8812auRegisterError, Rtl8812auTxError, Rtl8812auTxSubmitError,
    RxFrame, RxParseOutcome, RxRssiSource, RxSnrSource, TxOptions, TxRate, TxSubmitCounters,
};
pub use trace::{
    compare_usb_traces, import_usbmon_text, UsbTraceComparison, UsbTraceComparisonResult,
    UsbTraceEvent, UsbTraceImport, UsbTraceImportError, UsbTraceKind, UsbTraceMismatch,
};
pub use usb::{
    list_usb_devices, probe_usb, ClaimedUsbDevice, DeviceSelector, EndpointInfo,
    FdClaimedUsbDevice, InterfaceInfo, ProbeClaim, ProbeDevice, ProbeResult, UsbBulkTransfer,
    UsbDeviceInfo, UsbEndpoints, UsbError, UsbProbeReport,
};
