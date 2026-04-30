use std::{
    fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use radio_core::{
    build_tx_packet, compare_usb_traces, frame_type, import_usbmon_text, parse_realtek_u32_array,
    parse_rx_packet, plan_realtek_table, plan_rtl8812au_init, probe_usb, submit_tx_frame,
    validate_ieee80211_frame, Band, Bandwidth, Channel, DeviceSelector, FirmwareImage, FrameType,
    InitDryRunPlan, InitPhaseCount, PcapWriter, PlannedInitTransfer, RealtekConditionEnv,
    RealtekTableActionKind, RealtekTableKind, RealtekTablePlan, Rtl8812auRegisterAccess,
    RxParseOutcome, TxOptions, TxRate, TxSubmitCounters, UsbBulkTransfer, UsbDeviceInfo,
    UsbEndpoints, UsbTraceComparison, UsbTraceEvent, UsbTraceImport,
};
use serde::Serialize;
use tracing_subscriber::EnvFilter;

#[cfg(target_os = "macos")]
mod macos_usbhost;

#[derive(Debug, Parser)]
#[command(name = "wfb-radio-diag")]
#[command(about = "Diagnostics for native macOS WFB USB radio bring-up")]
struct Cli {
    /// Emit JSON. Human output is the default.
    #[arg(long, global = true)]
    json: bool,

    /// Include unsupported USB devices in reports.
    #[arg(long, global = true)]
    all: bool,

    /// Write the command's JSON report to a file.
    #[arg(long, global = true, value_name = "PATH")]
    report: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Probe USB adapters and optionally claim/release a selected candidate.
    UsbProbe(UsbProbeArgs),
    /// Inspect macOS IOKit USB state, including devices libusb cannot enumerate.
    MacosUsbState(MacosUsbStateArgs),
    /// Read RTL8812AU registers through macOS IOUSBHost direct control transfers.
    MacosRegSmoke(RegSmokeArgs),
    /// Dump RTL8812AU EFUSE through macOS IOUSBHost direct control transfers.
    MacosEfuseDump(EfuseDumpArgs),
    /// Run guarded RTL8812AU power-on writes through macOS IOUSBHost direct control transfers.
    MacosPowerOnSmoke(PowerOnSmokeArgs),
    /// Claim a supported adapter and perform read-only RTL8812AU register reads.
    RegSmoke(RegSmokeArgs),
    /// Dump RTL8812AU EFUSE physical bytes and decoded logical map.
    EfuseDump(EfuseDumpArgs),
    /// Drive RTL8812AU LEDCFG software LED pins with guarded register writes.
    LedSmoke(LedSmokeArgs),
    /// Run the first guarded RTL8812AU power-on/RF-reset register writes.
    PowerOnSmoke(PowerOnSmokeArgs),
    /// Download RTL8812A firmware and poll checksum/readiness with guarded writes.
    FirmwareSmoke(FirmwareSmokeArgs),
    /// Program the RTL8812A LLT table with guarded writes and per-entry polling.
    LltSmoke(LltSmokeArgs),
    /// Program RTL8812A queue and DMA boundary registers with guarded writes.
    QueueDmaSmoke(QueueDmaSmokeArgs),
    /// Program RTL8812A MAC/WMAC registers with guarded writes.
    MacSmoke(MacSmokeArgs),
    /// Program RTL8812A BB PHY/AGC tables with guarded writes.
    BbSmoke(BbSmokeArgs),
    /// Program RTL8812A RF radioA/radioB tables with guarded writes.
    RfSmoke(RfSmokeArgs),
    /// Run integrated RTL8812AU live init diagnostics.
    Init(InitArgs),
    /// Run bounded live RX capture diagnostics.
    RxScan(RxScanArgs),
    /// Transmit one validated test frame over bulk OUT.
    TxOnce(TxOnceArgs),
    /// Placeholder for repeated TX diagnostics with explicit operator gating.
    TxRepeat(TxRepeatArgs),
    /// Compare normalized USB trace event sequences.
    TraceCompare(TraceCompareArgs),
    /// Import Linux usbmon text into normalized USB trace JSON.
    TraceImport(TraceImportArgs),
    /// List staged verification commands and their prerequisites.
    Stages,
}

#[derive(Debug, Parser)]
struct UsbProbeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Skip interface claim/release and only list matching devices.
    #[arg(long)]
    no_claim: bool,
}

#[derive(Debug, Parser, Clone)]
struct MacosUsbStateArgs {
    #[command(flatten)]
    adapter: AdapterArgs,
}

#[derive(Debug, Parser, Clone)]
struct RegSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Per-register read timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,
}

#[derive(Debug, Parser, Clone)]
struct EfuseDumpArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Physical EFUSE byte count to read.
    #[arg(long, default_value_t = RTL8812AU_EFUSE_REAL_CONTENT_LEN)]
    length: usize,

    /// Maximum ready-bit polls for each EFUSE byte.
    #[arg(long, default_value_t = 1000)]
    poll_attempts: u32,

    /// Delay between EFUSE ready-bit polls in microseconds.
    #[arg(long, default_value_t = 1000)]
    poll_delay_us: u64,

    /// Optional path for the raw physical EFUSE bytes.
    #[arg(long, value_name = "PATH")]
    raw_out: Option<PathBuf>,

    /// Optional path for the decoded logical EFUSE map.
    #[arg(long, value_name = "PATH")]
    logical_map_out: Option<PathBuf>,

    /// Required acknowledgement that EFUSE reads write control registers.
    #[arg(long)]
    i_understand_this_writes_control_registers: bool,
}

#[derive(Debug, Parser, Clone)]
struct LedSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// RTL8812AU software LED pin to drive.
    #[arg(long, value_enum, default_value = "led0")]
    pin: LedPin,

    /// RTL8812AU LED register path to use.
    #[arg(long, value_enum, default_value = "normal")]
    mode: LedMode,

    /// LED action to perform.
    #[arg(long, value_enum, default_value = "blink")]
    action: LedAction,

    /// Number of on/off pulses for --action blink.
    #[arg(long, default_value_t = 6)]
    blink_count: u32,

    /// Delay between blink state changes in milliseconds.
    #[arg(long, default_value_t = 250)]
    interval_ms: u64,

    /// Required acknowledgement that this command writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum LedPin {
    Led0,
    Led1,
    Led2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum LedMode {
    Normal,
    Antdiv,
    Minicard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum LedAction {
    On,
    Off,
    Blink,
}

#[derive(Debug, Parser, Clone)]
struct PowerOnSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Maximum read attempts for each hardware poll step.
    #[arg(long, default_value_t = 200)]
    poll_attempts: u32,

    /// Delay between hardware poll reads in microseconds.
    #[arg(long, default_value_t = 10)]
    poll_delay_us: u64,

    /// Required acknowledgement that this command writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,
}

#[derive(Debug, Parser, Clone)]
struct FirmwareSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// RTL8812A firmware image to download.
    #[arg(long, value_name = "PATH")]
    firmware: PathBuf,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Maximum firmware write/checksum attempts before failing.
    #[arg(long, default_value_t = 3)]
    download_attempts: u32,

    /// Minimum checksum poll reads, matching the upstream driver guard.
    #[arg(long, default_value_t = 5)]
    checksum_min_attempts: u32,

    /// Checksum poll timeout in milliseconds.
    #[arg(long, default_value_t = 50)]
    checksum_timeout_ms: u64,

    /// Minimum firmware-ready poll reads, matching the upstream driver guard.
    #[arg(long, default_value_t = 10)]
    ready_min_attempts: u32,

    /// Firmware-ready poll timeout in milliseconds.
    #[arg(long, default_value_t = 200)]
    ready_timeout_ms: u64,

    /// Delay between firmware status poll reads in microseconds.
    #[arg(long, default_value_t = 1000)]
    poll_delay_us: u64,

    /// Required acknowledgement that this command writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,
}

#[derive(Debug, Parser, Clone)]
struct LltSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Maximum read attempts for each LLT operation poll.
    #[arg(long, default_value_t = 25)]
    poll_attempts: u32,

    /// Delay between LLT poll reads in microseconds.
    #[arg(long, default_value_t = 10)]
    poll_delay_us: u64,

    /// Required acknowledgement that this command writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,
}

#[derive(Debug, Parser, Clone)]
struct QueueDmaSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Required acknowledgement that this command writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,
}

#[derive(Debug, Parser, Clone)]
struct MacSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Required acknowledgement that this command writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,
}

#[derive(Debug, Parser, Clone)]
struct BbSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Realtek halhwimg8812a_bb.c source file to parse for PHY_REG and AGC_TAB tables.
    #[arg(
        long,
        value_name = "PATH",
        default_value = "/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c"
    )]
    bb_source: PathBuf,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Realtek condition cut version. Zero maps to the driver's "don't care" A-cut value.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    cut_version: u8,

    /// Realtek condition package type. Zero maps to the driver's "don't care" package value.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    package_type: u8,

    /// Realtek condition support interface; RTL8812AU USB is 0x02.
    #[arg(long, default_value = "0x02", value_parser = parse_u8)]
    support_interface: u8,

    /// Realtek condition support platform.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    support_platform: u8,

    /// Realtek board type. Default enables GLNA/GPA/ALNA/APA branches typical of AWUS036ACH-class boards.
    #[arg(long, default_value = "0xd8", value_parser = parse_u8)]
    board_type: u8,

    /// Realtek 2.4 GHz LNA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_glna: u16,

    /// Realtek 2.4 GHz PA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_gpa: u16,

    /// Realtek 5 GHz LNA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_alna: u16,

    /// Realtek 5 GHz PA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_apa: u16,

    /// Crystal-cap value used by the RTL8812A BB config tail step.
    #[arg(long, default_value = "0x20", value_parser = parse_u8)]
    crystal_cap: u8,

    /// Required acknowledgement that this command writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,
}

impl BbSmokeArgs {
    fn condition_env(&self) -> RealtekConditionEnv {
        RealtekConditionEnv {
            cut_version: self.cut_version,
            package_type: self.package_type,
            support_interface: self.support_interface,
            support_platform: self.support_platform,
            board_type: self.board_type,
            type_glna: self.type_glna,
            type_gpa: self.type_gpa,
            type_alna: self.type_alna,
            type_apa: self.type_apa,
        }
    }
}

#[derive(Debug, Parser, Clone)]
struct RfSmokeArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Realtek halhwimg8812a_rf.c source file to parse for radioA/radioB tables.
    #[arg(
        long,
        value_name = "PATH",
        default_value = "/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c"
    )]
    rf_source: PathBuf,

    /// Per-register read/write timeout in milliseconds.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Realtek condition cut version. Zero maps to the driver's "don't care" A-cut value.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    cut_version: u8,

    /// Realtek condition package type. Zero maps to the driver's "don't care" package value.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    package_type: u8,

    /// Realtek condition support interface; RTL8812AU USB is 0x02.
    #[arg(long, default_value = "0x02", value_parser = parse_u8)]
    support_interface: u8,

    /// Realtek condition support platform.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    support_platform: u8,

    /// Realtek board type. Default enables GLNA/GPA/ALNA/APA branches typical of AWUS036ACH-class boards.
    #[arg(long, default_value = "0xd8", value_parser = parse_u8)]
    board_type: u8,

    /// Realtek 2.4 GHz LNA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_glna: u16,

    /// Realtek 2.4 GHz PA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_gpa: u16,

    /// Realtek 5 GHz LNA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_alna: u16,

    /// Realtek 5 GHz PA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_apa: u16,

    /// Required acknowledgement that this command writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,
}

impl RfSmokeArgs {
    fn condition_env(&self) -> RealtekConditionEnv {
        RealtekConditionEnv {
            cut_version: self.cut_version,
            package_type: self.package_type,
            support_interface: self.support_interface,
            support_platform: self.support_platform,
            board_type: self.board_type,
            type_glna: self.type_glna,
            type_gpa: self.type_gpa,
            type_alna: self.type_alna,
            type_apa: self.type_apa,
        }
    }
}

#[derive(Debug, Parser, Clone, Serialize)]
struct AdapterArgs {
    /// Vendor ID, decimal or hex such as 0x0bda.
    #[arg(long, value_parser = parse_u16)]
    vid: Option<u16>,

    /// Product ID, decimal or hex such as 0x8812.
    #[arg(long, value_parser = parse_u16)]
    pid: Option<u16>,

    /// USB bus number.
    #[arg(long)]
    bus: Option<u8>,

    /// USB device address on the bus.
    #[arg(long)]
    address: Option<u8>,
}

impl AdapterArgs {
    fn selector(&self) -> DeviceSelector {
        DeviceSelector {
            vid: self.vid,
            pid: self.pid,
            bus: self.bus,
            address: self.address,
        }
    }
}

#[derive(Debug, Parser, Clone)]
struct InitArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Channel to configure after init.
    #[arg(long, default_value_t = 36)]
    channel: u8,

    /// Requested bandwidth: 20, 40, or 80 MHz.
    #[arg(long, default_value = "20", value_parser = parse_bandwidth)]
    bandwidth: Bandwidth,

    /// RTL8812A firmware image path for live init or dry-run planning.
    #[arg(long)]
    firmware: Option<PathBuf>,

    /// Per-register read/write timeout in milliseconds for live init.
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,

    /// Realtek halhwimg8812a_bb.c source file for live BB table programming.
    #[arg(
        long,
        value_name = "PATH",
        default_value = "/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c"
    )]
    bb_source: PathBuf,

    /// Realtek halhwimg8812a_rf.c source file for live RF table programming.
    #[arg(
        long,
        value_name = "PATH",
        default_value = "/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c"
    )]
    rf_source: PathBuf,

    /// Realtek condition cut version. Zero maps to the driver's "don't care" A-cut value.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    cut_version: u8,

    /// Realtek condition package type. Zero maps to the driver's "don't care" package value.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    package_type: u8,

    /// Realtek condition support interface; RTL8812AU USB is 0x02.
    #[arg(long, default_value = "0x02", value_parser = parse_u8)]
    support_interface: u8,

    /// Realtek condition support platform.
    #[arg(long, default_value = "0x00", value_parser = parse_u8)]
    support_platform: u8,

    /// Realtek board type. Default enables GLNA/GPA/ALNA/APA branches typical of AWUS036ACH-class boards.
    #[arg(long, default_value = "0xd8", value_parser = parse_u8)]
    board_type: u8,

    /// Realtek 2.4 GHz LNA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_glna: u16,

    /// Realtek 2.4 GHz PA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_gpa: u16,

    /// Realtek 5 GHz LNA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_alna: u16,

    /// Realtek 5 GHz PA type condition value.
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    type_apa: u16,

    /// Crystal-cap value used by the RTL8812A BB config tail step.
    #[arg(long, default_value = "0x20", value_parser = parse_u8)]
    crystal_cap: u8,

    /// Required acknowledgement that live init writes hardware registers.
    #[arg(long)]
    i_understand_this_writes_registers: bool,

    /// Plan init transfers without claiming USB or touching hardware.
    #[arg(long)]
    dry_run: bool,

    /// Write init dry-run normalized USB trace events to a JSON file.
    #[arg(long, value_name = "PATH")]
    trace_out: Option<PathBuf>,
}

impl InitArgs {
    fn condition_env(&self) -> RealtekConditionEnv {
        RealtekConditionEnv {
            cut_version: self.cut_version,
            package_type: self.package_type,
            support_interface: self.support_interface,
            support_platform: self.support_platform,
            board_type: self.board_type,
            type_glna: self.type_glna,
            type_gpa: self.type_gpa,
            type_alna: self.type_alna,
            type_apa: self.type_apa,
        }
    }
}

#[derive(Debug, Parser, Clone)]
struct RxScanArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Channel to capture.
    #[arg(long)]
    channel: u8,

    /// Requested bandwidth: 20, 40, or 80 MHz.
    #[arg(long, default_value = "20", value_parser = parse_bandwidth)]
    bandwidth: Bandwidth,

    /// Bounded capture duration in milliseconds.
    #[arg(long, default_value_t = 1000)]
    duration_ms: u64,

    /// Per bulk-IN read timeout in milliseconds.
    #[arg(long, default_value_t = 100)]
    timeout_ms: u64,

    /// Optional PCAP output path, once RX capture is wired in.
    #[arg(long)]
    pcap: Option<PathBuf>,

    /// Optional JSONL output path for raw frames plus RX metadata.
    #[arg(long, value_name = "PATH")]
    frame_jsonl: Option<PathBuf>,

    /// Parse raw RTL8812AU bulk-IN buffers from fixture files instead of touching USB.
    #[arg(long, value_name = "PATH")]
    fixture_bulk_in: Vec<PathBuf>,
}

#[derive(Debug, Parser, Clone)]
struct TxOnceArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Channel to transmit on.
    #[arg(long)]
    channel: u8,

    /// Requested bandwidth: 20, 40, or 80 MHz.
    #[arg(long, default_value = "20", value_parser = parse_bandwidth)]
    bandwidth: Bandwidth,

    /// Optional IEEE 802.11 frame bytes as hex for future TX verification.
    #[arg(long)]
    frame_hex: Option<String>,

    /// Required acknowledgement for live single-frame TX.
    #[arg(long)]
    i_understand_this_transmits: bool,

    /// Build the RTL8812AU descriptor packet without touching USB.
    #[arg(long)]
    dry_run: bool,

    /// Write dry-run descriptor-prefixed bytes to a file.
    #[arg(long, value_name = "PATH")]
    packet_out: Option<PathBuf>,

    #[command(flatten)]
    tx_options: TxOptionArgs,

    #[command(flatten)]
    tx_led: TxActivityLedArgs,

    #[command(flatten)]
    tx_status: TxStatusProbeArgs,
}

#[derive(Debug, Parser, Clone)]
struct TxRepeatArgs {
    #[command(flatten)]
    adapter: AdapterArgs,

    /// Channel to transmit on.
    #[arg(long)]
    channel: u8,

    /// Requested bandwidth: 20, 40, or 80 MHz.
    #[arg(long, default_value = "20", value_parser = parse_bandwidth)]
    bandwidth: Bandwidth,

    /// Explicit number of test frames to transmit.
    #[arg(long)]
    count: u32,

    /// Explicit interval between test frames in milliseconds.
    #[arg(long)]
    interval_ms: u64,

    /// Optional IEEE 802.11 frame bytes as hex for future repeated TX verification.
    #[arg(long)]
    frame_hex: Option<String>,

    /// Required acknowledgement for any repeated TX mode.
    #[arg(long)]
    i_understand_this_transmits: bool,

    #[command(flatten)]
    tx_options: TxOptionArgs,

    #[command(flatten)]
    tx_led: TxActivityLedArgs,

    #[command(flatten)]
    tx_status: TxStatusProbeArgs,
}

#[derive(Debug, Parser, Clone, Default)]
struct TxOptionArgs {
    /// TX descriptor rate: ofdm6m, mcs7, or vht2ss-mcs9.
    #[arg(long, default_value = "ofdm6m", value_parser = parse_tx_rate_arg)]
    tx_rate: TxRate,

    /// Set the TX descriptor short guard interval bit.
    #[arg(long)]
    short_gi: bool,

    /// Set the TX descriptor LDPC bit.
    #[arg(long)]
    ldpc: bool,

    /// Set the TX descriptor STBC bit.
    #[arg(long)]
    stbc: bool,
}

#[derive(Debug, Parser, Clone)]
struct TxActivityLedArgs {
    /// Blink the confirmed software LED around live bulk-OUT TX submissions.
    #[arg(long)]
    tx_led: bool,

    /// LED pin to use for --tx-led.
    #[arg(long, value_enum, default_value = "led0")]
    tx_led_pin: LedPin,

    /// LED register mode to use for --tx-led.
    #[arg(long, value_enum, default_value = "normal")]
    tx_led_mode: LedMode,

    /// Minimum time to keep the LED on after a TX submission or burst.
    #[arg(long, default_value_t = DEFAULT_TX_LED_HOLD_MS)]
    tx_led_hold_ms: u64,
}

impl Default for TxActivityLedArgs {
    fn default() -> Self {
        Self {
            tx_led: false,
            tx_led_pin: LedPin::Led0,
            tx_led_mode: LedMode::Normal,
            tx_led_hold_ms: DEFAULT_TX_LED_HOLD_MS,
        }
    }
}

#[derive(Debug, Parser, Clone)]
struct TxStatusProbeArgs {
    /// Read selected RTL8812AU TX status registers before and after live TX.
    #[arg(long)]
    tx_status: bool,

    /// Delay after TX submission before reading post-TX status registers.
    #[arg(long, default_value_t = DEFAULT_TX_STATUS_DELAY_MS)]
    tx_status_delay_ms: u64,
}

impl Default for TxStatusProbeArgs {
    fn default() -> Self {
        Self {
            tx_status: false,
            tx_status_delay_ms: DEFAULT_TX_STATUS_DELAY_MS,
        }
    }
}

#[derive(Debug, Parser, Clone)]
struct TraceCompareArgs {
    /// JSON file containing expected normalized USB trace events.
    #[arg(long, value_name = "PATH")]
    expected: PathBuf,

    /// JSON file containing observed normalized USB trace events.
    #[arg(long, value_name = "PATH")]
    observed: PathBuf,
}

#[derive(Debug, Parser, Clone)]
struct TraceImportArgs {
    /// Linux usbmon text file.
    #[arg(long, value_name = "PATH")]
    input: PathBuf,

    /// JSON output path for normalized USB trace events.
    #[arg(long, value_name = "PATH")]
    output: PathBuf,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let emit_json = cli.json;
    let include_unsupported = cli.all;
    let report_path = cli.report.clone();

    match cli.command {
        Command::UsbProbe(args) => {
            let report = probe_usb(args.adapter.selector(), include_unsupported, !args.no_claim);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_usb_probe_human(&report);
            }
            if report.result.is_failure() {
                std::process::exit(1);
            }
        }
        Command::MacosUsbState(args) => {
            let report = macos_usb_state_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_macos_usb_state_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::MacosRegSmoke(args) => {
            let report = macos_register_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_register_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::MacosEfuseDump(args) => {
            let report = macos_efuse_dump_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_efuse_dump_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::RegSmoke(args) => {
            let report = register_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_register_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::EfuseDump(args) => {
            let report = efuse_dump_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_efuse_dump_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::MacosPowerOnSmoke(args) => {
            let report = macos_power_on_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_power_on_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::LedSmoke(args) => {
            let report = led_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_led_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::PowerOnSmoke(args) => {
            let report = power_on_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_power_on_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::FirmwareSmoke(args) => {
            let report = firmware_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_firmware_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::LltSmoke(args) => {
            let report = llt_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_llt_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::QueueDmaSmoke(args) => {
            let report = queue_dma_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_queue_dma_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::MacSmoke(args) => {
            let report = mac_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_mac_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::BbSmoke(args) => {
            let report = bb_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_bb_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::RfSmoke(args) => {
            let report = rf_smoke_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_rf_smoke_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::Init(args) => {
            let report = init_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_pending_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::RxScan(args) => {
            let report = rx_scan_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_pending_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::TxOnce(args) => {
            let report = tx_once_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_pending_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::TxRepeat(args) => {
            let report = tx_repeat_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_pending_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::TraceCompare(args) => {
            let report = trace_compare_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_trace_compare_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::TraceImport(args) => {
            let report = trace_import_report(args);
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_trace_import_human(&report);
            }
            if let Some(code) = report.result.exit_code() {
                std::process::exit(code);
            }
        }
        Command::Stages => {
            let report = stages_report();
            emit_report(&report, emit_json, report_path.as_deref())?;
            if !emit_json {
                print_stages_human(&report);
            }
        }
    }

    Ok(())
}

fn emit_report<T: Serialize>(
    report: &T,
    emit_json: bool,
    report_path: Option<&Path>,
) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    if emit_json {
        println!("{json}");
    }
    if let Some(path) = report_path {
        fs::write(path, json).with_context(|| format!("write report {}", path.display()))?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct PendingDiagnosticReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    channel: Option<Channel>,
    bandwidth: Option<Bandwidth>,
    firmware_path: Option<PathBuf>,
    firmware: Option<FirmwareReport>,
    init_dry_run: Option<InitDryRunReport>,
    init_live: Option<InitLiveReport>,
    duration_ms: Option<u64>,
    pcap_path: Option<PathBuf>,
    tx_frame_len: Option<usize>,
    tx_frame_source: Option<&'static str>,
    tx_dry_run: Option<TxDryRunReport>,
    tx_live: Option<TxLiveReport>,
    rx_fixture: Option<RxFixtureReport>,
    repeat_tx: Option<RepeatTxReport>,
    result: DiagnosticResult,
    phases: Vec<DiagnosticPhase>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct PlatformInfo {
    os: &'static str,
    family: &'static str,
    arch: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DiagnosticResult {
    Pass,
    Fail,
    NotImplemented,
}

impl DiagnosticResult {
    fn as_str(self) -> &'static str {
        match self {
            DiagnosticResult::Pass => "pass",
            DiagnosticResult::Fail => "fail",
            DiagnosticResult::NotImplemented => "not_implemented",
        }
    }

    fn exit_code(self) -> Option<i32> {
        match self {
            DiagnosticResult::Pass => None,
            DiagnosticResult::Fail => Some(1),
            DiagnosticResult::NotImplemented => Some(2),
        }
    }
}

#[derive(Debug, Serialize)]
struct DiagnosticPhase {
    id: &'static str,
    status: DiagnosticPhaseStatus,
    detail: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum DiagnosticPhaseStatus {
    Completed,
    Pending,
    Blocked,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
struct DiagnosticCounters {
    usb_control_reads: u64,
    usb_control_writes: u64,
    usb_bulk_in_reads: u64,
    usb_bulk_out_writes: u64,
    rx_frames: u64,
    tx_frames: u64,
    dropped_frames: u64,
}

#[derive(Debug, Serialize)]
struct DiagnosticErrorReport {
    code: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct FirmwareReport {
    source: PathBuf,
    len: usize,
    byte_sum: u32,
    chunk_size: usize,
    chunk_count: usize,
}

#[derive(Debug, Serialize)]
struct InitDryRunReport {
    firmware_len: usize,
    firmware_chunk_size: usize,
    source_repo: &'static str,
    source_commit: &'static str,
    planned_transfers: usize,
    trace_out: Option<PathBuf>,
    phase_counts: Vec<InitPhaseCount>,
    transfers: Vec<PlannedInitTransfer>,
}

#[derive(Debug, Serialize)]
struct InitLiveReport {
    bb_source: PathBuf,
    rf_source: PathBuf,
    condition_env: RealtekConditionEnv,
    crystal_cap_hex: String,
    phase_summaries: Vec<InitLivePhaseSummary>,
    firmware_payload_len: Option<usize>,
    llt_entries_written: u32,
    queue_pages: Option<QueuePageReport>,
    bb_phy_writes_applied: u64,
    bb_agc_writes_applied: u64,
    bb_delays_applied: u64,
    rf_radioa_writes_applied: u64,
    rf_radiob_writes_applied: u64,
    rf_delays_applied: u64,
    effective_channel: Option<Channel>,
    effective_bandwidth: Option<Bandwidth>,
}

#[derive(Debug, Serialize)]
struct InitLivePhaseSummary {
    id: &'static str,
    status: DiagnosticPhaseStatus,
    detail: String,
    usb_control_reads: u64,
    usb_control_writes: u64,
}

#[derive(Debug, Serialize)]
struct TxDryRunReport {
    descriptor_len: usize,
    frame_len: usize,
    packet_len: usize,
    packet_byte_sum: u32,
    descriptor_hex: String,
    packet_out: Option<PathBuf>,
    tx_options: TxOptions,
}

#[derive(Debug, Serialize)]
struct TxLiveReport {
    bulk_out_endpoint: u8,
    bulk_out_endpoint_hex: String,
    frame_len: usize,
    packet_len: usize,
    bytes_written: usize,
    tx_options: TxOptions,
    tx_activity_led: Option<TxActivityLedReport>,
    tx_status: Option<TxStatusProbeReport>,
    submit_counters: TxSubmitCounters,
}

#[derive(Debug, Serialize)]
struct TxActivityLedReport {
    enabled: bool,
    pin: LedPin,
    mode: LedMode,
    hold_ms: u64,
    semantics: &'static str,
    steps: Vec<LedStepReport>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
}

#[derive(Debug, Serialize)]
struct TxStatusProbeReport {
    enabled: bool,
    delay_ms: u64,
    semantics: &'static str,
    pre: Vec<TxStatusRegisterReport>,
    post: Vec<TxStatusRegisterReport>,
    changed: Vec<TxStatusDeltaReport>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
}

#[derive(Debug, Clone, Serialize)]
struct TxStatusRegisterReport {
    name: &'static str,
    address: u16,
    address_hex: String,
    width: &'static str,
    value: u32,
    value_hex: String,
}

#[derive(Debug, Serialize)]
struct TxStatusDeltaReport {
    name: &'static str,
    address: u16,
    address_hex: String,
    width: &'static str,
    before_hex: String,
    after_hex: String,
    xor_hex: String,
}

#[derive(Debug, Default, Serialize)]
struct RxFixtureReport {
    fixture_paths: Vec<PathBuf>,
    frame_jsonl_path: Option<PathBuf>,
    buffers_read: u64,
    read_timeouts: u64,
    bulk_bytes: u64,
    parsed_frames: u64,
    dropped_packets: u64,
    need_more_data: u64,
    bytes_consumed: u64,
    management_frames: u64,
    control_frames: u64,
    data_frames: u64,
    extension_frames: u64,
    pcap_frames_written: u64,
    frame_records_written: u64,
}

#[derive(Debug, Serialize)]
struct RxFrameJsonRecord {
    timestamp_unix_ms: u64,
    frame_len: usize,
    rssi_dbm: i8,
    channel: Channel,
    frequency_mhz: u16,
    band: Band,
    frame_type: String,
    frame_hex: String,
}

#[derive(Debug, Serialize)]
struct RepeatTxReport {
    count: u32,
    interval_ms: u64,
    authorized: bool,
    bulk_out_endpoint: Option<u8>,
    bulk_out_endpoint_hex: Option<String>,
    frame_len: Option<usize>,
    packet_len: Option<usize>,
    elapsed_ms: Option<u64>,
    submitted_per_second: Option<f64>,
    usb_bytes_per_second: Option<f64>,
    cpu: Option<CpuUsageReport>,
    tx_options: Option<TxOptions>,
    tx_activity_led: Option<TxActivityLedReport>,
    tx_status: Option<TxStatusProbeReport>,
    submit_counters: TxSubmitCounters,
}

#[derive(Debug, Serialize)]
struct CpuUsageReport {
    user_us: u64,
    system_us: u64,
    total_us: u64,
    percent_one_core: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct CpuUsageSnapshot {
    user_us: u64,
    system_us: u64,
}

#[derive(Debug, Serialize)]
struct TraceCompareReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    expected_path: PathBuf,
    observed_path: PathBuf,
    result: DiagnosticResult,
    comparison: Option<UsbTraceComparison>,
    error: Option<DiagnosticErrorReport>,
}

#[derive(Debug, Serialize)]
struct TraceImportReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    input_path: PathBuf,
    output_path: PathBuf,
    result: DiagnosticResult,
    import: Option<UsbTraceImport>,
    error: Option<DiagnosticErrorReport>,
}

#[derive(Debug, Serialize)]
struct MacosUsbStateReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    result: DiagnosticResult,
    devices: Vec<MacosUsbDeviceState>,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct MacosUsbDeviceState {
    name: String,
    location_path: Option<String>,
    status: String,
    registered: bool,
    matched: bool,
    active: bool,
    vid: Option<u16>,
    pid: Option<u16>,
    vid_hex: Option<String>,
    pid_hex: Option<String>,
    vendor_name: Option<String>,
    product_name: Option<String>,
    serial_number: Option<String>,
    usb_address: Option<u64>,
    location_id: Option<u64>,
    location_id_hex: Option<String>,
    usb_speed_code: Option<u64>,
    usb_link_speed_bps: Option<u64>,
    b_num_configurations: Option<u64>,
    current_configuration: Option<u64>,
    preferred_configuration: Option<u64>,
    enumeration_state: Option<u64>,
    has_current_configuration: bool,
    has_interface_children: bool,
}

#[derive(Debug, Serialize)]
struct RegisterSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    timeout_ms: u64,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    reads: Vec<RegisterReadReport>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct RegisterReadReport {
    name: &'static str,
    address: u16,
    address_hex: String,
    width: &'static str,
    value: u64,
    value_hex: String,
    bytes_le_hex: String,
}

#[derive(Debug, Serialize)]
struct EfuseDumpReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    timeout_ms: u64,
    length: usize,
    poll_attempts: u32,
    poll_delay_us: u64,
    authorized: bool,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    raw_out: Option<PathBuf>,
    logical_map_out: Option<PathBuf>,
    efuse: Option<EfuseDumpDataReport>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct EfuseDumpDataReport {
    raw_len: usize,
    raw_hex: String,
    logical_map_len: usize,
    logical_map_hex: String,
    decoded_packets: Vec<EfusePacketReport>,
    summary: EfuseSummaryReport,
}

#[derive(Debug, Clone, Serialize)]
struct EfusePacketReport {
    raw_offset: usize,
    section: u8,
    word_enable_hex: String,
    logical_offset: usize,
    data_len: usize,
}

#[derive(Debug, Clone, Serialize)]
struct EfuseSummaryReport {
    raw_used_bytes: usize,
    raw_used_percent: f64,
    terminating_offset: Option<usize>,
    decoded_packet_count: usize,
    named_bytes: Vec<EfuseNamedByteReport>,
    usb_vid_hex: Option<String>,
    usb_pid_hex: Option<String>,
    mac_address: Option<String>,
    tx_power: EfuseTxPowerReport,
}

#[derive(Debug, Clone, Serialize)]
struct EfuseNamedByteReport {
    name: &'static str,
    offset: usize,
    offset_hex: String,
    value: u8,
    value_hex: String,
    programmed: bool,
}

#[derive(Debug, Clone, Serialize)]
struct EfuseTxPowerReport {
    start_offset: usize,
    length: usize,
    data_hex: String,
    non_ff_bytes: usize,
    all_ff: bool,
    regions: Vec<EfuseTxPowerRegionReport>,
}

#[derive(Debug, Clone, Serialize)]
struct EfuseTxPowerRegionReport {
    name: &'static str,
    offset: usize,
    length: usize,
    data_hex: String,
    non_ff_bytes: usize,
}

#[derive(Debug, Serialize)]
struct LedSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    timeout_ms: u64,
    pin: LedPin,
    mode: LedMode,
    action: LedAction,
    blink_count: u32,
    interval_ms: u64,
    authorized: bool,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<LedStepReport>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct LedStepReport {
    phase: &'static str,
    operation: &'static str,
    pin: LedPin,
    mode: LedMode,
    register_name: &'static str,
    address: u16,
    address_hex: String,
    width: &'static str,
    mask_hex: String,
    before_hex: String,
    written_hex: String,
    after_hex: String,
    expected_hex: String,
    passed: bool,
}

#[derive(Debug, Serialize)]
struct PowerOnSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    timeout_ms: u64,
    poll_attempts: u32,
    poll_delay_us: u64,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<PowerOnStepReport>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct FirmwareSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    firmware_path: PathBuf,
    firmware: Option<FirmwareReport>,
    timeout_ms: u64,
    download_attempts: u32,
    checksum_min_attempts: u32,
    checksum_timeout_ms: u64,
    ready_min_attempts: u32,
    ready_timeout_ms: u64,
    poll_delay_us: u64,
    firmware_payload_offset: Option<usize>,
    firmware_payload_len: Option<usize>,
    firmware_signature_hex: Option<String>,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<FirmwareStepReport>,
    firmware_bytes_written: u64,
    firmware_control_writes: u64,
    checksum_poll_attempts: Option<u32>,
    ready_poll_attempts: Option<u32>,
    final_mcu_status_hex: Option<String>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct FirmwareStepReport {
    phase: &'static str,
    operation: &'static str,
    description: &'static str,
    source: &'static str,
    register_name: Option<&'static str>,
    address: Option<u16>,
    address_hex: Option<String>,
    width: Option<&'static str>,
    firmware_attempt: Option<u32>,
    page: Option<usize>,
    page_offset: Option<usize>,
    length: Option<usize>,
    mask_hex: Option<String>,
    value_hex: Option<String>,
    before_hex: Option<String>,
    written_hex: Option<String>,
    after_hex: Option<String>,
    expected_hex: Option<String>,
    attempts: Option<u32>,
    passed: bool,
}

#[derive(Debug, Default)]
struct FirmwareRunStats {
    firmware_bytes_written: u64,
    firmware_control_writes: u64,
    checksum_poll_attempts: Option<u32>,
    ready_poll_attempts: Option<u32>,
    final_mcu_status: Option<u32>,
    firmware_payload_offset: Option<usize>,
    firmware_payload_len: Option<usize>,
    firmware_signature: Option<u16>,
}

#[derive(Debug)]
struct FirmwareSmokeFailureInput {
    firmware: Option<FirmwareReport>,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<FirmwareStepReport>,
    counters: DiagnosticCounters,
    stats: FirmwareRunStats,
    error: DiagnosticErrorReport,
}

#[derive(Debug, Serialize)]
struct LltSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    timeout_ms: u64,
    poll_attempts: u32,
    poll_delay_us: u64,
    tx_page_boundary: u8,
    last_tx_page_entry: u8,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<LltStepReport>,
    entries_written: u32,
    max_poll_attempts_observed: u32,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct LltStepReport {
    phase: &'static str,
    operation: &'static str,
    description: &'static str,
    source: &'static str,
    register_name: &'static str,
    address: u16,
    address_hex: String,
    width: &'static str,
    llt_address: Option<u8>,
    llt_data: Option<u8>,
    value_hex: Option<String>,
    after_hex: Option<String>,
    attempts: Option<u32>,
    passed: bool,
}

#[derive(Debug, Default)]
struct LltRunStats {
    entries_written: u32,
    max_poll_attempts_observed: u32,
}

#[derive(Debug, Serialize)]
struct QueueDmaSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    timeout_ms: u64,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    bulk_out_endpoint_count: Option<usize>,
    out_ep_queue_sel_hex: Option<String>,
    tx_total_page_number: u8,
    tx_page_boundary: u8,
    rx_dma_boundary_hex: String,
    queue_pages: Option<QueuePageReport>,
    steps: Vec<QueueDmaStepReport>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct QueuePageReport {
    hpq: u8,
    lpq: u8,
    npq: u8,
    pubq: u8,
    rqpn_npq_hex: String,
    rqpn_hex: String,
}

#[derive(Debug, Serialize)]
struct QueueDmaStepReport {
    phase: &'static str,
    operation: &'static str,
    description: &'static str,
    source: &'static str,
    register_name: &'static str,
    address: u16,
    address_hex: String,
    width: &'static str,
    mask_hex: Option<String>,
    value_hex: Option<String>,
    before_hex: Option<String>,
    written_hex: Option<String>,
    after_hex: Option<String>,
    expected_hex: Option<String>,
    passed: bool,
}

#[derive(Debug, Serialize)]
struct MacSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    timeout_ms: u64,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    receive_config_hex: String,
    retry_limit_hex: String,
    steps: Vec<QueueDmaStepReport>,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct BbSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    bb_source: PathBuf,
    condition_env: RealtekConditionEnv,
    crystal_cap_hex: String,
    timeout_ms: u64,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    setup_steps: Vec<QueueDmaStepReport>,
    phy_plan: Option<RealtekTablePlan>,
    agc_plan: Option<RealtekTablePlan>,
    phy_writes_applied: u64,
    agc_writes_applied: u64,
    delays_applied: u64,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Default)]
struct BbSmokeStats {
    phy_writes_applied: u64,
    agc_writes_applied: u64,
    delays_applied: u64,
}

#[derive(Debug)]
struct BbSmokeFailureInput {
    condition_env: RealtekConditionEnv,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    setup_steps: Vec<QueueDmaStepReport>,
    phy_plan: Option<RealtekTablePlan>,
    agc_plan: Option<RealtekTablePlan>,
    stats: BbSmokeStats,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
}

#[derive(Debug, Serialize)]
struct RfSmokeReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    selector: DeviceSelector,
    rf_source: PathBuf,
    condition_env: RealtekConditionEnv,
    timeout_ms: u64,
    result: DiagnosticResult,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    setup_steps: Vec<QueueDmaStepReport>,
    radioa_plan: Option<RealtekTablePlan>,
    radiob_plan: Option<RealtekTablePlan>,
    radioa_writes_applied: u64,
    radiob_writes_applied: u64,
    delays_applied: u64,
    counters: DiagnosticCounters,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

#[derive(Debug, Default)]
struct RfSmokeStats {
    radioa_writes_applied: u64,
    radiob_writes_applied: u64,
    delays_applied: u64,
}

#[derive(Debug)]
struct RfSmokeFailureInput {
    condition_env: RealtekConditionEnv,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    setup_steps: Vec<QueueDmaStepReport>,
    radioa_plan: Option<RealtekTablePlan>,
    radiob_plan: Option<RealtekTablePlan>,
    stats: RfSmokeStats,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
}

#[derive(Debug, Clone, Copy)]
struct QueueLayout {
    bulk_out_endpoint_count: usize,
    out_ep_queue_sel: u8,
    hpq: u8,
    lpq: u8,
    npq: u8,
    pubq: u8,
    rqpn_npq: u8,
    rqpn: u32,
    queue_map: u16,
}

#[derive(Debug, Serialize)]
struct PowerOnStepReport {
    phase: &'static str,
    operation: &'static str,
    description: &'static str,
    source: &'static str,
    register_name: &'static str,
    address: u16,
    address_hex: String,
    width: &'static str,
    mask_hex: Option<String>,
    value_hex: Option<String>,
    before_hex: Option<String>,
    written_hex: Option<String>,
    after_hex: Option<String>,
    expected_hex: Option<String>,
    attempts: Option<u32>,
    passed: bool,
}

#[derive(Debug, Clone, Copy)]
struct RegisterSmokeSpec {
    name: &'static str,
    address: u16,
    width: RegisterSmokeWidth,
}

#[derive(Debug, Clone, Copy)]
enum RegisterSmokeWidth {
    U8,
    U16,
    U32,
}

impl RegisterSmokeWidth {
    fn label(self) -> &'static str {
        match self {
            RegisterSmokeWidth::U8 => "u8",
            RegisterSmokeWidth::U16 => "u16",
            RegisterSmokeWidth::U32 => "u32",
        }
    }

    fn value_digits(self) -> usize {
        match self {
            RegisterSmokeWidth::U8 => 2,
            RegisterSmokeWidth::U16 => 4,
            RegisterSmokeWidth::U32 => 8,
        }
    }
}

const REGISTER_SMOKE_READS: &[RegisterSmokeSpec] = &[
    RegisterSmokeSpec {
        name: "REG_SYS_FUNC_EN",
        address: 0x0002,
        width: RegisterSmokeWidth::U8,
    },
    RegisterSmokeSpec {
        name: "REG_APS_FSMCO",
        address: 0x0004,
        width: RegisterSmokeWidth::U32,
    },
    RegisterSmokeSpec {
        name: "REG_SYS_CLKR",
        address: 0x0008,
        width: RegisterSmokeWidth::U16,
    },
    RegisterSmokeSpec {
        name: "REG_RF_CTRL",
        address: 0x001f,
        width: RegisterSmokeWidth::U8,
    },
    RegisterSmokeSpec {
        name: "REG_MCUFWDL",
        address: 0x0080,
        width: RegisterSmokeWidth::U8,
    },
    RegisterSmokeSpec {
        name: "REG_CR",
        address: 0x0100,
        width: RegisterSmokeWidth::U16,
    },
];

const POWER_SOURCE_PWRSEQ: &str = "aircrack-ng/rtl8812au@7344855:include/Hal8812PwrSeq.h";
const POWER_SOURCE_USB_HALINIT: &str =
    "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/usb/usb_halinit.c";
const FIRMWARE_SOURCE_HAL_INIT: &str =
    "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/rtl8812a_hal_init.c";
const LLT_SOURCE_HAL_INIT: &str = "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/rtl8812a_hal_init.c";
const BB_SOURCE_PHYCFG: &str = "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/rtl8812a_phycfg.c";
const RF_SOURCE_PHYCFG: &str = "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/rtl8812a_phycfg.c";
const RF_SOURCE_RF6052: &str = "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/rtl8812a_rf6052.c";
const BB_PHY_ARRAY: &str = "array_mp_8812a_phy_reg";
const BB_AGC_ARRAY: &str = "array_mp_8812a_agc_tab";
const RF_RADIOA_ARRAY: &str = "array_mp_8812a_radioa";
const RF_RADIOB_ARRAY: &str = "array_mp_8812a_radiob";

const RTL8812AU_EFUSE_REAL_CONTENT_LEN: usize = 512;
const RTL8812AU_EFUSE_LOGICAL_MAP_LEN: usize = 512;
const RTL8812AU_EFUSE_MAX_SECTION: u8 = 64;
const RTL8812AU_EFUSE_TX_POWER_START: usize = 0x10;
const RTL8812AU_EFUSE_TX_POWER_LEN: usize = 84;

const FEN_ELDR: u16 = 1 << 12;
const ANA8M: u16 = 1 << 1;
const LOADER_CLK_EN: u16 = 1 << 5;
const EFUSE_ACCESS_ON_JAGUAR: u8 = 0x69;
const EFUSE_ACCESS_OFF_JAGUAR: u8 = 0x00;

const REG_SYS_ISO_CTRL: u16 = 0x0000;
const REG_APS_FSMCO_PLUS_1: u16 = 0x0005;
const REG_APS_FSMCO_PLUS_2: u16 = 0x0006;
const REG_SYS_FUNC_EN: u16 = 0x0002;
const REG_SYS_FUNC_EN_PLUS_1: u16 = 0x0003;
const REG_SYS_CLKR: u16 = 0x0008;
const REG_RSV_CTRL: u16 = 0x001c;
const REG_AFE_XTAL_CTRL: u16 = 0x0024;
const REG_AFE_PLL_CTRL: u16 = 0x0028;
const REG_EFUSE_CTRL: u16 = 0x0030;
const REG_RF_CTRL: u16 = 0x001f;
const REG_RF_B_CTRL_8812: u16 = 0x0076;
const REG_MAC_PHY_CTRL: u16 = 0x002c;
const REG_LEDCFG0: u16 = 0x004c;
const REG_LEDCFG1: u16 = 0x004d;
const REG_LEDCFG2: u16 = 0x004e;
const REG_HISR0_8812: u16 = 0x00b4;
const REG_HISR1_8812: u16 = 0x00bc;
const REG_EFUSE_BURN_GNT_8812: u16 = 0x00cf;
const REG_RF_PATH_A_3WIRE: u16 = 0x0c90;
const REG_RF_PATH_B_3WIRE: u16 = 0x0e90;
const REG_MCUFWDL: u16 = 0x0080;
const REG_MCUFWDL_PLUS_2: u16 = REG_MCUFWDL + 2;
const REG_CR: u16 = 0x0100;
const REG_PBP: u16 = 0x0104;
const REG_TRXDMA_CTRL: u16 = 0x010c;
const REG_TRXFF_BNDY: u16 = 0x0114;
const REG_HISR: u16 = 0x0124;
const REG_HISRE: u16 = 0x012c;
const REG_C2HEVT_MSG_NORMAL: u16 = 0x01a0;
const REG_C2HEVT_CMD_SEQ_88XX: u16 = 0x01a1;
const REG_C2HEVT_CMD_LEN_88XX: u16 = 0x01ae;
const REG_C2HEVT_CLEAR: u16 = 0x01af;
const REG_LLT_INIT: u16 = 0x01e0;
const REG_RQPN: u16 = 0x0200;
const REG_TXDMA_OFFSET_CHK: u16 = 0x020c;
const REG_TXDMA_STATUS: u16 = 0x0210;
const REG_TDECTRL: u16 = 0x0208;
const REG_RQPN_NPQ: u16 = 0x0214;
const REG_BCNQ_BDNY: u16 = 0x0424;
const REG_MGQ_BDNY: u16 = 0x0425;
const REG_FWHW_TXQ_CTRL: u16 = 0x0420;
const REG_HWSEQ_CTRL: u16 = 0x0423;
const REG_SPEC_SIFS: u16 = 0x0428;
const REG_RETRY_LIMIT: u16 = 0x042a;
const REG_RRSR: u16 = 0x0440;
const REG_TXPKT_EMPTY: u16 = 0x041a;
const REG_DATA_SC_8812: u16 = 0x0483;
const REG_TX_RPT_CTRL: u16 = 0x04ec;
const REG_CCK_CHECK_8812: u16 = 0x0454;
const REG_WMAC_LBK_BF_HD: u16 = 0x045d;
const REG_EDCA_VO_PARAM: u16 = 0x0500;
const REG_EDCA_VI_PARAM: u16 = 0x0504;
const REG_EDCA_BE_PARAM: u16 = 0x0508;
const REG_EDCA_BK_PARAM: u16 = 0x050c;
const REG_SIFS_CTX: u16 = 0x0514;
const REG_SIFS_TRX: u16 = 0x0516;
const REG_TXPAUSE: u16 = 0x0522;
const REG_USTIME_TSF: u16 = 0x055c;
const REG_SCH_TX_CMD: u16 = 0x05f8;
const REG_RCR: u16 = 0x0608;
const REG_RX_DRVINFO_SZ: u16 = 0x060f;
const REG_MAR: u16 = 0x0620;
const REG_USTIME_EDCA: u16 = 0x0638;
const REG_MAC_SPEC_SIFS: u16 = 0x063a;
const REG_ACKTO: u16 = 0x0640;
const REG_WMAC_TRXPTCL_CTL: u16 = 0x0668;
const REG_RXFLTMAP1: u16 = 0x06a2;
const REG_BAR_MODE_CTRL: u16 = 0x04cc;
const REG_OFDMCCKEN_JAGUAR: u16 = 0x0808;
const REG_TX_PATH_JAGUAR: u16 = 0x080c;
const REG_AGC_TABLE_JAGUAR: u16 = 0x082c;
const REG_PWED_TH_JAGUAR: u16 = 0x0830;
const REG_BW_INDICATION_JAGUAR: u16 = 0x0834;
const REG_CCA_ON_SEC_JAGUAR: u16 = 0x0838;
const REG_L1_PEAK_TH_JAGUAR: u16 = 0x0848;
const REG_FC_AREA_JAGUAR: u16 = 0x0860;
const REG_RF_MOD_JAGUAR: u16 = 0x08ac;
const REG_ADC_BUF_CLK_JAGUAR: u16 = 0x08c4;
const REG_CCK_SYSTEM_JAGUAR: u16 = 0x0a00;
const REG_CCK_RX_JAGUAR: u16 = 0x0a04;
const REG_RFE_PINMUX_A_JAGUAR: u16 = 0x0cb0;
const REG_RFE_INV_A_JAGUAR: u16 = 0x0cb4;
const REG_TX_SCALE_A_JAGUAR: u16 = 0x0c1c;
const REG_RFE_PINMUX_B_JAGUAR: u16 = 0x0eb0;
const REG_RFE_INV_B_JAGUAR: u16 = 0x0eb4;
const REG_TX_SCALE_B_JAGUAR: u16 = 0x0e1c;
const FW_START_ADDRESS: u16 = 0x1000;

const BIT0: u8 = 1 << 0;
const BIT1: u8 = 1 << 1;
const BIT2: u8 = 1 << 2;
const BIT3: u8 = 1 << 3;
const BIT5: u8 = 1 << 5;
const BIT6: u8 = 1 << 6;
const BIT7: u8 = 1 << 7;

const LEDCFG_NORMAL_MASK: u8 = 0x70;
const LEDCFG_READBACK_MASK: u8 = 0x78;
const MAX_LED_SMOKE_TOTAL_MS: u64 = 60_000;
const DEFAULT_TX_LED_HOLD_MS: u64 = 150;
const MAX_TX_LED_HOLD_MS: u64 = 60_000;
const DEFAULT_TX_STATUS_DELAY_MS: u64 = 50;
const MAX_TX_STATUS_DELAY_MS: u64 = 60_000;

const MAX_DLFW_PAGE_SIZE: usize = 4096;
const MAX_REG_BLOCK_SIZE: usize = 196;
const FIRMWARE_REMAINDER_BLOCK_SIZE: usize = 8;
const MAX_FIRMWARE_DOWNLOAD_PAGES: usize = 8;
const TX_PAGE_BOUNDARY_8812: u8 = 0xf9;
const LAST_ENTRY_OF_TX_PKT_BUFFER_8812: u8 = 0xff;
const TX_TOTAL_PAGE_NUMBER_8812: u8 = TX_PAGE_BOUNDARY_8812 - 1;
const RX_DMA_BOUNDARY_8812: u16 = 0x3e7f;

const TX_SELE_HQ: u8 = BIT0;
const TX_SELE_LQ: u8 = BIT1;
const TX_SELE_NQ: u8 = BIT2;
const TX_SELE_EQ: u8 = BIT3;
const NORMAL_PAGE_NUM_HPQ_8812: u8 = 0x10;
const NORMAL_PAGE_NUM_LPQ_8812: u8 = 0x10;
const NORMAL_PAGE_NUM_NPQ_8812: u8 = 0x00;
const PBP_512: u8 = 0x03;
const PSTX_PBP_512: u8 = PBP_512 << 4;
const QUEUE_EXTRA: u16 = 0;
const QUEUE_LOW: u16 = 1;
const QUEUE_NORMAL: u16 = 2;
const QUEUE_HIGH: u16 = 3;
const LD_RQPN: u32 = 1 << 31;
const RQPN_PAGE_MASK: u32 = 0x00ff_ffff;
const TXDMA_QUEUE_MAP_MASK: u16 = 0xfff8;

const DRVINFO_SZ: u8 = 4;
const FEN_BBRSTB: u8 = BIT0;
const FEN_BB_GLB_RSTN: u8 = BIT1;
const FEN_USBA: u8 = BIT2;
const MASK_NETTYPE: u32 = 0x0003_0000;
const NT_LINK_AP: u32 = 0x2;
const NETTYPE_LINK_AP: u32 = NT_LINK_AP << 16;
const RCR_APM: u32 = 1 << 1;
const RCR_AM: u32 = 1 << 2;
const RCR_AB: u32 = 1 << 3;
const RCR_CBSSID_DATA: u32 = 1 << 6;
const RCR_CBSSID_BCN: u32 = 1 << 7;
const RCR_AMF: u32 = 1 << 13;
const RCR_HTC_LOC_CTRL: u32 = 1 << 14;
const RCR_APP_PHYST_RXFF: u32 = 1 << 28;
const RCR_APP_ICV: u32 = 1 << 29;
const RCR_APP_MIC: u32 = 1 << 30;
const RCR_FORCEACK: u32 = 1 << 26;
const MAC_RECEIVE_CONFIG: u32 = RCR_APM
    | RCR_AM
    | RCR_AB
    | RCR_CBSSID_DATA
    | RCR_CBSSID_BCN
    | RCR_AMF
    | RCR_HTC_LOC_CTRL
    | RCR_APP_PHYST_RXFF
    | RCR_APP_ICV
    | RCR_APP_MIC
    | RCR_FORCEACK;
const RATE_BITMAP_ALL: u32 = 0x000f_ffff;
const RATE_RRSR_CCK_ONLY_1M: u32 = 0x000f_fff1;
const RL_VAL_STA: u16 = 0x30;
const RETRY_LIMIT_STA: u16 = RL_VAL_STA | (RL_VAL_STA << 8);
const BASIC_RATE_2G: u16 = 0x015f;
const BASIC_RATE_5G: u16 = 0x0150;
const BAR_MODE_CTRL_VALUE: u32 = 0x0201_ffff;
const BAR_MODE_CTRL_READBACK_MASK: u32 = 0xffff_ff7f;
const EN_AMPDU_RTY_NEW: u8 = 1 << 7;
const MACTXEN: u8 = 1 << 6;
const MACRXEN: u8 = 1 << 7;
const MAC_TX_RX_ENABLE_MASK: u8 = MACTXEN | MACRXEN;
const RTL8812_CRYSTAL_CAP_MASK: u32 = 0x7ff8_0000;
const RF_CHNLBW_JAGUAR: u32 = 0x18;
const RF_CHNLBW_MOD_AG_MASK: u32 = 0x0007_0300;
const RF_CHNLBW_BW_MASK: u32 = 0x0000_0c00;
const RF_CHNLBW_CHANNEL_MASK: u32 = 0x0000_00ff;
const VHT_DATA_SC_20_UPPER_OF_80MHZ: u8 = 1;
const VHT_DATA_SC_20_LOWER_OF_80MHZ: u8 = 2;
const VHT_DATA_SC_20_UPPERST_OF_80MHZ: u8 = 3;
const VHT_DATA_SC_20_LOWEST_OF_80MHZ: u8 = 4;
const VHT_DATA_SC_40_UPPER_OF_80MHZ: u8 = 9;
const VHT_DATA_SC_40_LOWER_OF_80MHZ: u8 = 10;

const LLT_NO_ACTIVE: u32 = 0x0;
const LLT_WRITE_ACCESS: u32 = 0x1;
const LLT_OP_SHIFT: u32 = 30;
const LLT_OP_MASK: u32 = 0x3;

const MCUFWDL_EN: u8 = BIT0;
const MCUFWDL_RDY: u32 = BIT1 as u32;
const FWDL_CHKSUM_RPT_U8: u8 = BIT2;
const FWDL_CHKSUM_RPT_U32: u32 = BIT2 as u32;
const WINTINI_RDY: u32 = BIT6 as u32;
const RAM_DL_SEL: u8 = BIT7;

const CR_ENABLE_BITS: u16 =
    (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 9) | (1 << 10);

fn register_smoke_report(args: RegSmokeArgs) -> RegisterSmokeReport {
    let selector = args.adapter.selector();
    let mut reads = Vec::new();
    let mut counters = DiagnosticCounters::default();

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return register_smoke_failure(
                selector,
                args.timeout_ms,
                None,
                None,
                reads,
                counters,
                error,
            );
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return register_smoke_failure(
                selector,
                args.timeout_ms,
                Some(selected),
                None,
                reads,
                counters,
                DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let timeout = Duration::from_millis(args.timeout_ms);
    let registers = Rtl8812auRegisterAccess::new(&claimed).with_timeout(timeout);

    for spec in REGISTER_SMOKE_READS {
        match read_smoke_register(&registers, *spec) {
            Ok(read) => {
                counters.usb_control_reads += 1;
                reads.push(read);
            }
            Err(error) => {
                return register_smoke_failure(
                    selector,
                    args.timeout_ms,
                    Some(adapter),
                    Some(endpoints),
                    reads,
                    counters,
                    DiagnosticErrorReport {
                        code: "register_read_failed",
                        message: format!("{} at 0x{:04x} failed: {error}", spec.name, spec.address),
                    },
                );
            }
        }
    }

    RegisterSmokeReport {
        schema_version: 1,
        command: "reg-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        reads,
        counters,
        error: None,
        notes: vec!["read-only smoke test: no USB control writes, bulk writes, RF changes, or TX operations were issued"],
    }
}

fn macos_register_smoke_report(args: RegSmokeArgs) -> RegisterSmokeReport {
    let selector = args.adapter.selector();
    let mut reads = Vec::new();
    let mut counters = DiagnosticCounters::default();

    #[cfg(not(target_os = "macos"))]
    {
        return register_smoke_failure_with_command(
            "macos-reg-smoke",
            selector,
            args.timeout_ms,
            RegisterSmokeFailureInput {
                adapter: None,
                endpoints: None,
                reads,
                counters,
                error: DiagnosticErrorReport {
                    code: "unsupported_platform",
                    message: "macos-reg-smoke requires macOS IOUSBHost".to_string(),
                },
            },
        );
    }

    #[cfg(target_os = "macos")]
    {
        let Some(vid) = selector.vid else {
            return register_smoke_failure_with_command(
                "macos-reg-smoke",
                selector,
                args.timeout_ms,
                RegisterSmokeFailureInput {
                    adapter: None,
                    endpoints: None,
                    reads,
                    counters,
                    error: DiagnosticErrorReport {
                        code: "missing_vid",
                        message:
                            "macos-reg-smoke requires --vid because IOUSBHost matching is VID/PID based"
                                .to_string(),
                    },
                },
            );
        };
        let Some(pid) = selector.pid else {
            return register_smoke_failure_with_command(
                "macos-reg-smoke",
                selector,
                args.timeout_ms,
                RegisterSmokeFailureInput {
                    adapter: None,
                    endpoints: None,
                    reads,
                    counters,
                    error: DiagnosticErrorReport {
                        code: "missing_pid",
                        message:
                            "macos-reg-smoke requires --pid because IOUSBHost matching is VID/PID based"
                                .to_string(),
                    },
                },
            );
        };

        let device = match macos_usbhost::MacosUsbHostDevice::open(vid, pid) {
            Ok(device) => device,
            Err(error) => {
                return register_smoke_failure_with_command(
                    "macos-reg-smoke",
                    selector,
                    args.timeout_ms,
                    RegisterSmokeFailureInput {
                        adapter: None,
                        endpoints: None,
                        reads,
                        counters,
                        error: DiagnosticErrorReport {
                            code: "macos_usbhost_open_failed",
                            message: error,
                        },
                    },
                );
            }
        };

        let timeout = Duration::from_millis(args.timeout_ms);
        let registers = Rtl8812auRegisterAccess::new(&device).with_timeout(timeout);
        for spec in REGISTER_SMOKE_READS {
            match read_smoke_register(&registers, *spec) {
                Ok(read) => {
                    counters.usb_control_reads += 1;
                    reads.push(read);
                }
                Err(error) => {
                    return register_smoke_failure_with_command(
                        "macos-reg-smoke",
                        selector,
                        args.timeout_ms,
                        RegisterSmokeFailureInput {
                            adapter: None,
                            endpoints: None,
                            reads,
                            counters,
                            error: DiagnosticErrorReport {
                                code: "register_read_failed",
                                message: format!(
                                    "{} at 0x{:04x} failed: {error}",
                                    spec.name, spec.address
                                ),
                            },
                        },
                    );
                }
            }
        }

        RegisterSmokeReport {
            schema_version: 1,
            command: "macos-reg-smoke",
            started_at_unix_ms: started_at_unix_ms(),
            platform: platform_info(),
            selector,
            timeout_ms: args.timeout_ms,
            result: DiagnosticResult::Pass,
            adapter: None,
            endpoints: None,
            reads,
            counters,
            error: None,
            notes: vec![
                "macOS IOUSBHost direct-control smoke test: no libusb enumeration, USB interface claim, bulk traffic, RF changes, or TX operations were issued",
                "this path can reach the default control endpoint even when macOS has not registered a configured USB interface",
            ],
        }
    }
}

fn register_smoke_failure(
    selector: DeviceSelector,
    timeout_ms: u64,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    reads: Vec<RegisterReadReport>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
) -> RegisterSmokeReport {
    register_smoke_failure_with_command(
        "reg-smoke",
        selector,
        timeout_ms,
        RegisterSmokeFailureInput {
            adapter,
            endpoints,
            reads,
            counters,
            error,
        },
    )
}

struct RegisterSmokeFailureInput {
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    reads: Vec<RegisterReadReport>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
}

fn register_smoke_failure_with_command(
    command: &'static str,
    selector: DeviceSelector,
    timeout_ms: u64,
    input: RegisterSmokeFailureInput,
) -> RegisterSmokeReport {
    RegisterSmokeReport {
        schema_version: 1,
        command,
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms,
        result: DiagnosticResult::Fail,
        adapter: input.adapter,
        endpoints: input.endpoints,
        reads: input.reads,
        counters: input.counters,
        error: Some(input.error),
        notes: vec!["read-only smoke test stopped before any register writes, bulk writes, RF changes, or TX operations"],
    }
}

fn efuse_dump_report(args: EfuseDumpArgs) -> EfuseDumpReport {
    let selector = args.adapter.selector();
    let mut counters = DiagnosticCounters::default();

    if !args.i_understand_this_writes_control_registers {
        return efuse_dump_failure(
            &args,
            selector,
            None,
            None,
            counters,
            DiagnosticErrorReport {
                code: "missing_control_write_authorization",
                message: "EFUSE dump requires --i-understand-this-writes-control-registers because EFUSE reads write EFUSE_CTRL selector bits".to_string(),
            },
        );
    }
    if args.length == 0 || args.length > RTL8812AU_EFUSE_REAL_CONTENT_LEN {
        return efuse_dump_failure(
            &args,
            selector,
            None,
            None,
            counters,
            DiagnosticErrorReport {
                code: "invalid_efuse_length",
                message: format!(
                    "--length must be in 1..={RTL8812AU_EFUSE_REAL_CONTENT_LEN}; got {}",
                    args.length
                ),
            },
        );
    }
    if args.poll_attempts == 0 {
        return efuse_dump_failure(
            &args,
            selector,
            None,
            None,
            counters,
            DiagnosticErrorReport {
                code: "invalid_poll_attempts",
                message: "--poll-attempts must be greater than zero".to_string(),
            },
        );
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return efuse_dump_failure(&args, selector, None, None, counters, error);
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return efuse_dump_failure(
                &args,
                selector,
                Some(selected),
                None,
                counters,
                DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let timeout = Duration::from_millis(args.timeout_ms);
    let registers = Rtl8812auRegisterAccess::new(&claimed).with_timeout(timeout);

    let raw = match read_efuse_physical(
        &registers,
        &mut counters,
        args.length,
        args.poll_attempts,
        Duration::from_micros(args.poll_delay_us),
    ) {
        Ok(raw) => raw,
        Err(error) => {
            return efuse_dump_failure(
                &args,
                selector,
                Some(adapter),
                Some(endpoints),
                counters,
                error,
            );
        }
    };

    let decoded = decode_efuse_logical_map(&raw);
    let summary = summarize_efuse(&raw, &decoded);

    if let Some(path) = &args.raw_out {
        if let Err(error) = fs::write(path, &raw) {
            return efuse_dump_failure(
                &args,
                selector,
                Some(adapter),
                Some(endpoints),
                counters,
                DiagnosticErrorReport {
                    code: "raw_efuse_write_failed",
                    message: format!("write raw EFUSE dump {} failed: {error}", path.display()),
                },
            );
        }
    }
    if let Some(path) = &args.logical_map_out {
        if let Err(error) = fs::write(path, &decoded.logical_map) {
            return efuse_dump_failure(
                &args,
                selector,
                Some(adapter),
                Some(endpoints),
                counters,
                DiagnosticErrorReport {
                    code: "logical_efuse_write_failed",
                    message: format!("write logical EFUSE map {} failed: {error}", path.display()),
                },
            );
        }
    }

    EfuseDumpReport {
        schema_version: 1,
        command: "efuse-dump",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        length: args.length,
        poll_attempts: args.poll_attempts,
        poll_delay_us: args.poll_delay_us,
        authorized: args.i_understand_this_writes_control_registers,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        raw_out: args.raw_out,
        logical_map_out: args.logical_map_out,
        efuse: Some(EfuseDumpDataReport {
            raw_len: raw.len(),
            raw_hex: encode_hex(&raw),
            logical_map_len: decoded.logical_map.len(),
            logical_map_hex: encode_hex(&decoded.logical_map),
            decoded_packets: decoded.packets,
            summary,
        }),
        counters,
        error: None,
        notes: vec![
            "EFUSE dump reads physical EFUSE through EFUSE_CTRL and decodes the logical 512-byte map; no EFUSE programming, bulk traffic, channel retune, or RF TX is issued",
            "TX power bytes are reported for audit only; explicit TX power control remains disabled until EFUSE interpretation is independently verified",
        ],
    }
}

fn macos_efuse_dump_report(args: EfuseDumpArgs) -> EfuseDumpReport {
    let selector = args.adapter.selector();
    let mut counters = DiagnosticCounters::default();

    if !args.i_understand_this_writes_control_registers {
        return efuse_dump_failure_with_command(
            "macos-efuse-dump",
            &args,
            selector,
            None,
            None,
            counters,
            DiagnosticErrorReport {
                code: "missing_control_write_authorization",
                message: "macos-efuse-dump requires --i-understand-this-writes-control-registers because EFUSE reads write EFUSE_CTRL selector bits".to_string(),
            },
        );
    }
    if args.length == 0 || args.length > RTL8812AU_EFUSE_REAL_CONTENT_LEN {
        return efuse_dump_failure_with_command(
            "macos-efuse-dump",
            &args,
            selector,
            None,
            None,
            counters,
            DiagnosticErrorReport {
                code: "invalid_efuse_length",
                message: format!(
                    "--length must be in 1..={RTL8812AU_EFUSE_REAL_CONTENT_LEN}; got {}",
                    args.length
                ),
            },
        );
    }
    if args.poll_attempts == 0 {
        return efuse_dump_failure_with_command(
            "macos-efuse-dump",
            &args,
            selector,
            None,
            None,
            counters,
            DiagnosticErrorReport {
                code: "invalid_poll_attempts",
                message: "--poll-attempts must be greater than zero".to_string(),
            },
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        return efuse_dump_failure_with_command(
            "macos-efuse-dump",
            &args,
            selector,
            None,
            None,
            counters,
            DiagnosticErrorReport {
                code: "unsupported_platform",
                message: "macos-efuse-dump requires macOS IOUSBHost".to_string(),
            },
        );
    }

    #[cfg(target_os = "macos")]
    {
        let Some(vid) = selector.vid else {
            return efuse_dump_failure_with_command(
                "macos-efuse-dump",
                &args,
                selector,
                None,
                None,
                counters,
                DiagnosticErrorReport {
                    code: "missing_vid",
                    message: "macos-efuse-dump requires --vid because IOUSBHost matching is VID/PID based".to_string(),
                },
            );
        };
        let Some(pid) = selector.pid else {
            return efuse_dump_failure_with_command(
                "macos-efuse-dump",
                &args,
                selector,
                None,
                None,
                counters,
                DiagnosticErrorReport {
                    code: "missing_pid",
                    message: "macos-efuse-dump requires --pid because IOUSBHost matching is VID/PID based".to_string(),
                },
            );
        };

        let device = match macos_usbhost::MacosUsbHostDevice::open(vid, pid) {
            Ok(device) => device,
            Err(error) => {
                return efuse_dump_failure_with_command(
                    "macos-efuse-dump",
                    &args,
                    selector,
                    None,
                    None,
                    counters,
                    DiagnosticErrorReport {
                        code: "macos_usbhost_open_failed",
                        message: error,
                    },
                );
            }
        };

        let timeout = Duration::from_millis(args.timeout_ms);
        let registers = Rtl8812auRegisterAccess::new(&device).with_timeout(timeout);
        let raw = match read_efuse_physical(
            &registers,
            &mut counters,
            args.length,
            args.poll_attempts,
            Duration::from_micros(args.poll_delay_us),
        ) {
            Ok(raw) => raw,
            Err(error) => {
                return efuse_dump_failure_with_command(
                    "macos-efuse-dump",
                    &args,
                    selector,
                    None,
                    None,
                    counters,
                    error,
                );
            }
        };

        let decoded = decode_efuse_logical_map(&raw);
        let summary = summarize_efuse(&raw, &decoded);

        if let Some(path) = &args.raw_out {
            if let Err(error) = fs::write(path, &raw) {
                return efuse_dump_failure_with_command(
                    "macos-efuse-dump",
                    &args,
                    selector,
                    None,
                    None,
                    counters,
                    DiagnosticErrorReport {
                        code: "raw_efuse_write_failed",
                        message: format!("write raw EFUSE dump {} failed: {error}", path.display()),
                    },
                );
            }
        }
        if let Some(path) = &args.logical_map_out {
            if let Err(error) = fs::write(path, &decoded.logical_map) {
                return efuse_dump_failure_with_command(
                    "macos-efuse-dump",
                    &args,
                    selector,
                    None,
                    None,
                    counters,
                    DiagnosticErrorReport {
                        code: "logical_efuse_write_failed",
                        message: format!(
                            "write logical EFUSE map {} failed: {error}",
                            path.display()
                        ),
                    },
                );
            }
        }

        EfuseDumpReport {
            schema_version: 1,
            command: "macos-efuse-dump",
            started_at_unix_ms: started_at_unix_ms(),
            platform: platform_info(),
            selector,
            timeout_ms: args.timeout_ms,
            length: args.length,
            poll_attempts: args.poll_attempts,
            poll_delay_us: args.poll_delay_us,
            authorized: args.i_understand_this_writes_control_registers,
            result: DiagnosticResult::Pass,
            adapter: None,
            endpoints: None,
            raw_out: args.raw_out,
            logical_map_out: args.logical_map_out,
            efuse: Some(EfuseDumpDataReport {
                raw_len: raw.len(),
                raw_hex: encode_hex(&raw),
                logical_map_len: decoded.logical_map.len(),
                logical_map_hex: encode_hex(&decoded.logical_map),
                decoded_packets: decoded.packets,
                summary,
            }),
            counters,
            error: None,
            notes: vec![
                "macOS IOUSBHost EFUSE dump reads physical EFUSE through direct default-control transfers; no libusb enumeration, interface claim, bulk traffic, channel retune, or RF TX is issued",
                "TX power bytes are reported for audit only; explicit TX power control remains disabled until EFUSE interpretation is independently verified",
            ],
        }
    }
}

fn efuse_dump_failure(
    args: &EfuseDumpArgs,
    selector: DeviceSelector,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
) -> EfuseDumpReport {
    efuse_dump_failure_with_command(
        "efuse-dump",
        args,
        selector,
        adapter,
        endpoints,
        counters,
        error,
    )
}

fn efuse_dump_failure_with_command(
    command: &'static str,
    args: &EfuseDumpArgs,
    selector: DeviceSelector,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
) -> EfuseDumpReport {
    EfuseDumpReport {
        schema_version: 1,
        command,
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        length: args.length,
        poll_attempts: args.poll_attempts,
        poll_delay_us: args.poll_delay_us,
        authorized: args.i_understand_this_writes_control_registers,
        result: DiagnosticResult::Fail,
        adapter,
        endpoints,
        raw_out: args.raw_out.clone(),
        logical_map_out: args.logical_map_out.clone(),
        efuse: None,
        counters,
        error: Some(error),
        notes: vec![
            "EFUSE dump stopped before any bulk traffic, channel retune, RF TX, or EFUSE programming operation",
            "this diagnostic writes EFUSE control selector registers only after explicit authorization",
        ],
    }
}

fn read_efuse_physical<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    length: usize,
    poll_attempts: u32,
    poll_delay: Duration,
) -> std::result::Result<Vec<u8>, DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    efuse_power_switch_read(registers, counters, true)?;
    let mut raw = Vec::with_capacity(length);
    let mut read_error = None;
    for address in 0..length {
        match efuse_read_byte(
            registers,
            counters,
            address as u16,
            poll_attempts,
            poll_delay,
        ) {
            Ok(byte) => raw.push(byte),
            Err(error) => {
                read_error = Some(error);
                break;
            }
        }
    }
    let power_off = efuse_power_switch_read(registers, counters, false);
    if let Some(error) = read_error {
        let _ = power_off;
        Err(error)
    } else {
        power_off?;
        Ok(raw)
    }
}

fn efuse_power_switch_read<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    enabled: bool,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let grant = if enabled {
        EFUSE_ACCESS_ON_JAGUAR
    } else {
        EFUSE_ACCESS_OFF_JAGUAR
    };
    write8_with_counter(registers, counters, REG_EFUSE_BURN_GNT_8812, grant).map_err(|error| {
        DiagnosticErrorReport {
            code: "efuse_power_switch_failed",
            message: format!(
                "write REG_EFUSE_BURN_GNT_8812={} failed: {error}",
                format_value(u64::from(grant), 2)
            ),
        }
    })?;

    if enabled {
        let _sys_iso =
            read16_with_counter(registers, counters, REG_SYS_ISO_CTRL).map_err(|error| {
                DiagnosticErrorReport {
                    code: "efuse_power_switch_failed",
                    message: format!("read REG_SYS_ISO_CTRL failed: {error}"),
                }
            })?;

        let sys_func =
            read16_with_counter(registers, counters, REG_SYS_FUNC_EN).map_err(|error| {
                DiagnosticErrorReport {
                    code: "efuse_power_switch_failed",
                    message: format!("read REG_SYS_FUNC_EN failed: {error}"),
                }
            })?;
        if sys_func & FEN_ELDR == 0 {
            write16_with_counter(registers, counters, REG_SYS_FUNC_EN, sys_func | FEN_ELDR)
                .map_err(|error| DiagnosticErrorReport {
                    code: "efuse_power_switch_failed",
                    message: format!("enable FEN_ELDR failed: {error}"),
                })?;
        }

        let sys_clkr = read16_with_counter(registers, counters, REG_SYS_CLKR).map_err(|error| {
            DiagnosticErrorReport {
                code: "efuse_power_switch_failed",
                message: format!("read REG_SYS_CLKR failed: {error}"),
            }
        })?;
        let required = LOADER_CLK_EN | ANA8M;
        if sys_clkr & required != required {
            write16_with_counter(registers, counters, REG_SYS_CLKR, sys_clkr | required).map_err(
                |error| DiagnosticErrorReport {
                    code: "efuse_power_switch_failed",
                    message: format!("enable EFUSE loader clock failed: {error}"),
                },
            )?;
        }
    }

    Ok(())
}

fn efuse_read_byte<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
    poll_attempts: u32,
    poll_delay: Duration,
) -> std::result::Result<u8, DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    write8_with_counter(
        registers,
        counters,
        REG_EFUSE_CTRL + 1,
        (address & 0x00ff) as u8,
    )
    .map_err(|error| DiagnosticErrorReport {
        code: "efuse_read_failed",
        message: format!("write EFUSE address low byte for {address:#05x} failed: {error}"),
    })?;
    let high = read8_with_counter(registers, counters, REG_EFUSE_CTRL + 2).map_err(|error| {
        DiagnosticErrorReport {
            code: "efuse_read_failed",
            message: format!("read EFUSE address high latch for {address:#05x} failed: {error}"),
        }
    })?;
    write8_with_counter(
        registers,
        counters,
        REG_EFUSE_CTRL + 2,
        (((address >> 8) & 0x03) as u8) | (high & 0xfc),
    )
    .map_err(|error| DiagnosticErrorReport {
        code: "efuse_read_failed",
        message: format!("write EFUSE address high byte for {address:#05x} failed: {error}"),
    })?;

    let command = read8_with_counter(registers, counters, REG_EFUSE_CTRL + 3).map_err(|error| {
        DiagnosticErrorReport {
            code: "efuse_read_failed",
            message: format!("read EFUSE command latch for {address:#05x} failed: {error}"),
        }
    })?;
    write8_with_counter(registers, counters, REG_EFUSE_CTRL + 3, command & 0x7f).map_err(
        |error| DiagnosticErrorReport {
            code: "efuse_read_failed",
            message: format!("start EFUSE read for {address:#05x} failed: {error}"),
        },
    )?;

    for attempt in 1..=poll_attempts {
        let status =
            read8_with_counter(registers, counters, REG_EFUSE_CTRL + 3).map_err(|error| {
                DiagnosticErrorReport {
                    code: "efuse_read_failed",
                    message: format!("poll EFUSE ready for {address:#05x} failed: {error}"),
                }
            })?;
        if status & 0x80 != 0 {
            return read8_with_counter(registers, counters, REG_EFUSE_CTRL).map_err(|error| {
                DiagnosticErrorReport {
                    code: "efuse_read_failed",
                    message: format!("read EFUSE data byte for {address:#05x} failed: {error}"),
                }
            });
        }
        if attempt < poll_attempts {
            std::thread::sleep(poll_delay);
        }
    }

    let status = read8_with_counter(registers, counters, REG_EFUSE_CTRL + 3).unwrap_or_default();
    Err(DiagnosticErrorReport {
        code: "efuse_read_timeout",
        message: format!(
            "EFUSE byte {address:#05x} did not become ready after {poll_attempts} polls; REG_EFUSE_CTRL+3={}",
            format_value(u64::from(status), 2)
        ),
    })
}

#[derive(Debug)]
struct EfuseLogicalDecode {
    logical_map: Vec<u8>,
    packets: Vec<EfusePacketReport>,
    raw_used_bytes: usize,
    terminating_offset: Option<usize>,
}

fn decode_efuse_logical_map(raw: &[u8]) -> EfuseLogicalDecode {
    let mut logical_map = vec![0xff; RTL8812AU_EFUSE_LOGICAL_MAP_LEN];
    let mut packets = Vec::new();
    let mut raw_offset = 0usize;
    let mut terminating_offset = None;

    while raw_offset < raw.len() {
        let header_offset = raw_offset;
        let header = raw[raw_offset];
        raw_offset += 1;
        if header == 0xff {
            terminating_offset = Some(header_offset);
            break;
        }

        let (section, word_enable) = if efuse_is_extended_header(header) {
            let offset_low = (header & 0xe0) >> 5;
            if raw_offset >= raw.len() {
                break;
            }
            let ext = raw[raw_offset];
            raw_offset += 1;
            if efuse_all_words_disabled(ext) {
                continue;
            }
            (offset_low | ((ext & 0xf0) >> 1), ext & 0x0f)
        } else {
            ((header >> 4) & 0x0f, header & 0x0f)
        };

        let data_len = efuse_word_count(word_enable) * 2;
        if section < RTL8812AU_EFUSE_MAX_SECTION {
            let logical_offset = usize::from(section) * 8;
            let mut target = logical_offset;
            let data_start = raw_offset;
            for word in 0..4 {
                if word_enable & (1 << word) == 0 {
                    if raw_offset + 1 >= raw.len() || target + 1 >= logical_map.len() {
                        raw_offset = raw.len();
                        break;
                    }
                    logical_map[target] = raw[raw_offset];
                    logical_map[target + 1] = raw[raw_offset + 1];
                    raw_offset += 2;
                }
                target += 2;
            }
            packets.push(EfusePacketReport {
                raw_offset: header_offset,
                section,
                word_enable_hex: format_value(u64::from(word_enable), 1),
                logical_offset,
                data_len: raw_offset.saturating_sub(data_start),
            });
        } else {
            raw_offset = raw_offset.saturating_add(data_len).min(raw.len());
        }
    }

    EfuseLogicalDecode {
        logical_map,
        packets,
        raw_used_bytes: terminating_offset.unwrap_or(raw_offset.min(raw.len())),
        terminating_offset,
    }
}

fn efuse_is_extended_header(header: u8) -> bool {
    header & 0x1f == 0x0f
}

fn efuse_all_words_disabled(word_enable: u8) -> bool {
    word_enable & 0x0f == 0x0f
}

fn efuse_word_count(word_enable: u8) -> usize {
    (0..4).filter(|word| word_enable & (1 << word) == 0).count()
}

fn summarize_efuse(raw: &[u8], decoded: &EfuseLogicalDecode) -> EfuseSummaryReport {
    let logical_map = &decoded.logical_map;
    let raw_used_bytes = decoded.raw_used_bytes;
    let raw_used_percent = if raw.is_empty() {
        0.0
    } else {
        (raw_used_bytes as f64 * 100.0) / raw.len() as f64
    };
    let named_bytes = EFUSE_NAMED_OFFSETS
        .iter()
        .filter_map(|(name, offset)| {
            logical_map
                .get(*offset)
                .copied()
                .map(|value| efuse_named_byte_report(name, *offset, value))
        })
        .collect();
    let usb_vid = efuse_le_u16(logical_map, 0xd0).map(|value| format_value(u64::from(value), 4));
    let usb_pid = efuse_le_u16(logical_map, 0xd2).map(|value| format_value(u64::from(value), 4));
    let mac_address = logical_map.get(0xd7..0xdd).and_then(|mac| {
        if mac.iter().all(|byte| *byte == 0xff) {
            None
        } else {
            Some(
                mac.iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(":"),
            )
        }
    });
    let tx_power = summarize_efuse_tx_power(logical_map);

    EfuseSummaryReport {
        raw_used_bytes,
        raw_used_percent,
        terminating_offset: decoded.terminating_offset,
        decoded_packet_count: decoded.packets.len(),
        named_bytes,
        usb_vid_hex: usb_vid,
        usb_pid_hex: usb_pid,
        mac_address,
        tx_power,
    }
}

const EFUSE_NAMED_OFFSETS: &[(&str, usize)] = &[
    ("EEPROM_USB_MODE_8812", 0x08),
    ("EEPROM_ChannelPlan_8812", 0xb8),
    ("EEPROM_XTAL_8812", 0xb9),
    ("EEPROM_THERMAL_METER_8812", 0xba),
    ("EEPROM_IQK_LCK_8812", 0xbb),
    ("EEPROM_PA_TYPE_8812AU", 0xbc),
    ("EEPROM_LNA_TYPE_2G_8812AU", 0xbd),
    ("EEPROM_LNA_TYPE_5G_8812AU", 0xbf),
    ("EEPROM_RF_BOARD_OPTION_8812", 0xc1),
    ("EEPROM_RF_FEATURE_OPTION_8812", 0xc2),
    ("EEPROM_RF_BT_SETTING_8812", 0xc3),
    ("EEPROM_VERSION_8812", 0xc4),
    ("EEPROM_CustomID_8812", 0xc5),
    ("EEPROM_TX_BBSWING_2G_8812", 0xc6),
    ("EEPROM_TX_BBSWING_5G_8812", 0xc7),
    ("EEPROM_TX_PWR_CALIBRATE_RATE_8812", 0xc8),
    ("EEPROM_RF_ANTENNA_OPT_8812", 0xc9),
    ("EEPROM_RFE_OPTION_8812", 0xca),
    ("EEPROM_COUNTRY_CODE_8812", 0xcb),
];

fn efuse_named_byte_report(name: &'static str, offset: usize, value: u8) -> EfuseNamedByteReport {
    EfuseNamedByteReport {
        name,
        offset,
        offset_hex: format_value(offset as u64, 3),
        value,
        value_hex: format_value(u64::from(value), 2),
        programmed: value != 0xff,
    }
}

fn efuse_le_u16(map: &[u8], offset: usize) -> Option<u16> {
    let bytes = map.get(offset..offset + 2)?;
    if bytes.iter().all(|byte| *byte == 0xff) {
        None
    } else {
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
}

fn summarize_efuse_tx_power(logical_map: &[u8]) -> EfuseTxPowerReport {
    let data = logical_map
        .get(
            RTL8812AU_EFUSE_TX_POWER_START
                ..RTL8812AU_EFUSE_TX_POWER_START + RTL8812AU_EFUSE_TX_POWER_LEN,
        )
        .unwrap_or(&[]);
    let regions = [
        ("path_a_2g", 0usize, 18usize),
        ("path_a_5g", 18, 24),
        ("path_b_2g", 42, 18),
        ("path_b_5g", 60, 24),
    ]
    .into_iter()
    .map(|(name, rel_offset, length)| {
        let bytes = data.get(rel_offset..rel_offset + length).unwrap_or(&[]);
        EfuseTxPowerRegionReport {
            name,
            offset: RTL8812AU_EFUSE_TX_POWER_START + rel_offset,
            length: bytes.len(),
            data_hex: encode_hex(bytes),
            non_ff_bytes: bytes.iter().filter(|byte| **byte != 0xff).count(),
        }
    })
    .collect();
    let non_ff_bytes = data.iter().filter(|byte| **byte != 0xff).count();
    EfuseTxPowerReport {
        start_offset: RTL8812AU_EFUSE_TX_POWER_START,
        length: data.len(),
        data_hex: encode_hex(data),
        non_ff_bytes,
        all_ff: non_ff_bytes == 0,
        regions,
    }
}

fn led_smoke_report(args: LedSmokeArgs) -> LedSmokeReport {
    let selector = args.adapter.selector();
    let mut steps = Vec::new();
    let mut counters = DiagnosticCounters::default();

    if !args.i_understand_this_writes_registers {
        return led_smoke_failure(
            &args,
            selector,
            None,
            None,
            steps,
            counters,
            DiagnosticErrorReport {
                code: "missing_write_authorization",
                message: "live LED control requires --i-understand-this-writes-registers"
                    .to_string(),
            },
        );
    }
    if args.action == LedAction::Blink && args.blink_count == 0 {
        return led_smoke_failure(
            &args,
            selector,
            None,
            None,
            steps,
            counters,
            DiagnosticErrorReport {
                code: "invalid_blink_count",
                message: "--blink-count must be greater than zero for --action blink".to_string(),
            },
        );
    }
    if args.action == LedAction::Blink {
        let total_ms = u128::from(args.blink_count) * 2 * u128::from(args.interval_ms);
        if total_ms > u128::from(MAX_LED_SMOKE_TOTAL_MS) {
            return led_smoke_failure(
                &args,
                selector,
                None,
                None,
                steps,
                counters,
                DiagnosticErrorReport {
                    code: "invalid_blink_duration",
                    message: format!(
                        "blink duration is capped at {MAX_LED_SMOKE_TOTAL_MS} ms; requested approximately {total_ms} ms"
                    ),
                },
            );
        }
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return led_smoke_failure(&args, selector, None, None, steps, counters, error);
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return led_smoke_failure(
                &args,
                selector,
                Some(selected),
                None,
                steps,
                counters,
                DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let timeout = Duration::from_millis(args.timeout_ms);
    let registers = Rtl8812auRegisterAccess::new(&claimed).with_timeout(timeout);

    let result = match args.action {
        LedAction::On => led_write_step(
            &registers,
            &mut counters,
            &mut steps,
            args.pin,
            args.mode,
            LedAction::On,
        ),
        LedAction::Off => led_write_step(
            &registers,
            &mut counters,
            &mut steps,
            args.pin,
            args.mode,
            LedAction::Off,
        ),
        LedAction::Blink => {
            let interval = Duration::from_millis(args.interval_ms);
            for index in 0..args.blink_count {
                if let Err(error) = led_write_step(
                    &registers,
                    &mut counters,
                    &mut steps,
                    args.pin,
                    args.mode,
                    LedAction::On,
                ) {
                    return led_smoke_failure(
                        &args,
                        selector,
                        Some(adapter),
                        Some(endpoints),
                        steps,
                        counters,
                        error,
                    );
                }
                std::thread::sleep(interval);
                if let Err(error) = led_write_step(
                    &registers,
                    &mut counters,
                    &mut steps,
                    args.pin,
                    args.mode,
                    LedAction::Off,
                ) {
                    return led_smoke_failure(
                        &args,
                        selector,
                        Some(adapter),
                        Some(endpoints),
                        steps,
                        counters,
                        error,
                    );
                }
                if index + 1 < args.blink_count {
                    std::thread::sleep(interval);
                }
            }
            Ok(())
        }
    };

    if let Err(error) = result {
        return led_smoke_failure(
            &args,
            selector,
            Some(adapter),
            Some(endpoints),
            steps,
            counters,
            error,
        );
    }

    LedSmokeReport {
        schema_version: 1,
        command: "led-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        pin: args.pin,
        mode: args.mode,
        action: args.action,
        blink_count: args.blink_count,
        interval_ms: args.interval_ms,
        authorized: args.i_understand_this_writes_registers,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        steps,
        counters,
        error: None,
        notes: vec![
            "writes only RTL8812AU LEDCFG software-control bits; no bulk traffic, channel retune, or RF TX is issued",
            "mode selects the normal, antenna-diversity, or minicard USB LED path from the upstream RTL8812AU driver",
        ],
    }
}

fn led_smoke_failure(
    args: &LedSmokeArgs,
    selector: DeviceSelector,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<LedStepReport>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
) -> LedSmokeReport {
    LedSmokeReport {
        schema_version: 1,
        command: "led-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        pin: args.pin,
        mode: args.mode,
        action: args.action,
        blink_count: args.blink_count,
        interval_ms: args.interval_ms,
        authorized: args.i_understand_this_writes_registers,
        result: DiagnosticResult::Fail,
        adapter,
        endpoints,
        steps,
        counters,
        error: Some(error),
        notes: vec![
            "LED smoke stopped before any bulk traffic, channel retune, or RF TX operation",
            "try led1, led2, --mode antdiv, or --mode minicard if led0 normal writes pass but the visible enclosure LED does not change",
        ],
    }
}

fn led_write_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<LedStepReport>,
    pin: LedPin,
    mode: LedMode,
    action: LedAction,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let (register_name, address) = led_register_for_mode(pin, mode)?;
    let before = read8_with_counter(registers, counters, address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{register_name} read before LED write failed: {error}"),
        }
    })?;
    let plan = led_write_plan(pin, mode, action, before)?;
    write8_with_counter(registers, counters, address, plan.written).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{register_name} LED write failed: {error}"),
        }
    })?;
    let after = read8_with_counter(registers, counters, address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{register_name} read after LED write failed: {error}"),
        }
    })?;
    let expected = plan.written & plan.verify_mask;
    let observed = after & plan.verify_mask;
    let passed = observed == expected;
    steps.push(LedStepReport {
        phase: plan.phase,
        operation: match action {
            LedAction::On => "on",
            LedAction::Off => "off",
            LedAction::Blink => unreachable!("blink is expanded into on/off LED writes"),
        },
        pin,
        mode,
        register_name,
        address,
        address_hex: format_address(address),
        width: "u8",
        mask_hex: format_value(plan.verify_mask, 2),
        before_hex: format_value(before, 2),
        written_hex: format_value(plan.written, 2),
        after_hex: format_value(after, 2),
        expected_hex: format_value(expected, 2),
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(DiagnosticErrorReport {
            code: "register_readback_mismatch",
            message: format!(
                "{register_name} expected LED mask {} to equal {}, got {}",
                format_value(plan.verify_mask, 2),
                format_value(expected, 2),
                format_value(observed, 2)
            ),
        })
    }
}

fn led_register(pin: LedPin) -> (&'static str, u16) {
    match pin {
        LedPin::Led0 => ("REG_LEDCFG0", REG_LEDCFG0),
        LedPin::Led1 => ("REG_LEDCFG1", REG_LEDCFG1),
        LedPin::Led2 => ("REG_LEDCFG2", REG_LEDCFG2),
    }
}

fn led_register_for_mode(
    pin: LedPin,
    mode: LedMode,
) -> std::result::Result<(&'static str, u16), DiagnosticErrorReport> {
    match mode {
        LedMode::Normal => Ok(led_register(pin)),
        LedMode::Antdiv => match pin {
            LedPin::Led0 => Ok(("REG_LEDCFG2", REG_LEDCFG2)),
            _ => Err(DiagnosticErrorReport {
                code: "unsupported_led_mode",
                message: "--mode antdiv only maps LED0 in the RTL8812AU upstream branch"
                    .to_string(),
            }),
        },
        LedMode::Minicard => match pin {
            LedPin::Led0 | LedPin::Led1 => Ok(("REG_LEDCFG2", REG_LEDCFG2)),
            LedPin::Led2 => Err(DiagnosticErrorReport {
                code: "unsupported_led_mode",
                message: "--mode minicard only maps LED0 and LED1 in the RTL8812AU upstream branch"
                    .to_string(),
            }),
        },
    }
}

#[derive(Debug, Clone, Copy)]
struct LedWritePlan {
    phase: &'static str,
    written: u8,
    verify_mask: u8,
}

fn led_write_plan(
    pin: LedPin,
    mode: LedMode,
    action: LedAction,
    current: u8,
) -> std::result::Result<LedWritePlan, DiagnosticErrorReport> {
    let (phase, written, verify_mask) = match (mode, pin, action) {
        (LedMode::Normal, _, LedAction::On) => (
            "ledcfg_normal_usb",
            led_on_value(current),
            LEDCFG_READBACK_MASK,
        ),
        (LedMode::Normal, _, LedAction::Off) => (
            "ledcfg_normal_usb",
            led_off_value(current),
            LEDCFG_READBACK_MASK,
        ),
        (LedMode::Antdiv, LedPin::Led0, LedAction::On) => (
            "ledcfg_antdiv_usb",
            (current & 0xe0) | BIT7 | BIT6 | BIT5,
            BIT7 | BIT6 | BIT5,
        ),
        (LedMode::Antdiv, LedPin::Led0, LedAction::Off) => (
            "ledcfg_antdiv_usb",
            (current & 0xe0) | BIT3 | BIT7 | BIT6 | BIT5,
            BIT7 | BIT6 | BIT5 | BIT3,
        ),
        (LedMode::Minicard, LedPin::Led0, LedAction::On) => {
            ("ledcfg_minicard_usb", (current & 0xf0) | BIT5 | BIT6, 0xf0)
        }
        (LedMode::Minicard, LedPin::Led0, LedAction::Off) => (
            "ledcfg_minicard_usb",
            current | BIT3 | BIT5 | BIT6,
            BIT3 | BIT5 | BIT6,
        ),
        (LedMode::Minicard, LedPin::Led1, LedAction::On) => {
            ("ledcfg_minicard_usb", (current & 0x0f) | BIT5, 0x2f)
        }
        (LedMode::Minicard, LedPin::Led1, LedAction::Off) => {
            ("ledcfg_minicard_usb", (current & 0x0f) | BIT3, 0x2f)
        }
        (_, _, LedAction::Blink) => unreachable!("blink is expanded into on/off LED writes"),
        _ => {
            return Err(DiagnosticErrorReport {
                code: "unsupported_led_mode",
                message: format!("LED {pin:?} is not mapped for {mode:?} mode"),
            });
        }
    };
    Ok(LedWritePlan {
        phase,
        written,
        verify_mask,
    })
}

fn led_on_value(current: u8) -> u8 {
    (current & LEDCFG_NORMAL_MASK) | BIT5
}

fn led_off_value(current: u8) -> u8 {
    (current & LEDCFG_NORMAL_MASK) | BIT3 | BIT5
}

fn tx_activity_led_report(args: &TxActivityLedArgs) -> Option<TxActivityLedReport> {
    args.tx_led.then(|| TxActivityLedReport {
        enabled: true,
        pin: args.tx_led_pin,
        mode: args.tx_led_mode,
        hold_ms: args.tx_led_hold_ms,
        semantics: "software TX submission activity; this does not prove RF radiation",
        steps: Vec::new(),
        counters: DiagnosticCounters::default(),
        error: None,
    })
}

fn tx_activity_led_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    report: &mut Option<TxActivityLedReport>,
    action: LedAction,
) where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    if let Some(report) = report.as_mut() {
        if report.error.is_some() {
            return;
        }
        if let Err(error) = led_write_step(
            registers,
            &mut report.counters,
            &mut report.steps,
            report.pin,
            report.mode,
            action,
        ) {
            report.error = Some(error);
        }
    }
}

fn tx_activity_led_hold(report: &Option<TxActivityLedReport>) {
    if let Some(report) = report {
        if report.error.is_none() && report.hold_ms > 0 {
            std::thread::sleep(Duration::from_millis(report.hold_ms));
        }
    }
}

fn add_diagnostic_counters(total: &mut DiagnosticCounters, extra: &DiagnosticCounters) {
    total.usb_control_reads += extra.usb_control_reads;
    total.usb_control_writes += extra.usb_control_writes;
    total.usb_bulk_in_reads += extra.usb_bulk_in_reads;
    total.usb_bulk_out_writes += extra.usb_bulk_out_writes;
    total.rx_frames += extra.rx_frames;
    total.tx_frames += extra.tx_frames;
    total.dropped_frames += extra.dropped_frames;
}

fn add_tx_activity_led_counters(
    total: &mut DiagnosticCounters,
    report: &Option<TxActivityLedReport>,
) {
    if let Some(report) = report {
        add_diagnostic_counters(total, &report.counters);
    }
}

#[derive(Debug, Clone, Copy)]
enum TxStatusRegisterWidth {
    U8,
    U16,
    U32,
}

impl TxStatusRegisterWidth {
    fn label(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
        }
    }

    fn hex_digits(self) -> usize {
        match self {
            Self::U8 => 2,
            Self::U16 => 4,
            Self::U32 => 8,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TxStatusRegisterSpec {
    name: &'static str,
    address: u16,
    width: TxStatusRegisterWidth,
}

const TX_STATUS_REGISTERS: &[TxStatusRegisterSpec] = &[
    TxStatusRegisterSpec {
        name: "REG_HISR0_8812",
        address: REG_HISR0_8812,
        width: TxStatusRegisterWidth::U32,
    },
    TxStatusRegisterSpec {
        name: "REG_HISR1_8812",
        address: REG_HISR1_8812,
        width: TxStatusRegisterWidth::U32,
    },
    TxStatusRegisterSpec {
        name: "REG_HISR",
        address: REG_HISR,
        width: TxStatusRegisterWidth::U32,
    },
    TxStatusRegisterSpec {
        name: "REG_HISRE",
        address: REG_HISRE,
        width: TxStatusRegisterWidth::U32,
    },
    TxStatusRegisterSpec {
        name: "REG_TXDMA_OFFSET_CHK",
        address: REG_TXDMA_OFFSET_CHK,
        width: TxStatusRegisterWidth::U32,
    },
    TxStatusRegisterSpec {
        name: "REG_TXDMA_STATUS",
        address: REG_TXDMA_STATUS,
        width: TxStatusRegisterWidth::U32,
    },
    TxStatusRegisterSpec {
        name: "REG_TXPKT_EMPTY",
        address: REG_TXPKT_EMPTY,
        width: TxStatusRegisterWidth::U16,
    },
    TxStatusRegisterSpec {
        name: "REG_FWHW_TXQ_CTRL",
        address: REG_FWHW_TXQ_CTRL,
        width: TxStatusRegisterWidth::U32,
    },
    TxStatusRegisterSpec {
        name: "REG_TX_RPT_CTRL",
        address: REG_TX_RPT_CTRL,
        width: TxStatusRegisterWidth::U8,
    },
    TxStatusRegisterSpec {
        name: "REG_TXPAUSE",
        address: REG_TXPAUSE,
        width: TxStatusRegisterWidth::U8,
    },
    TxStatusRegisterSpec {
        name: "REG_SCH_TX_CMD",
        address: REG_SCH_TX_CMD,
        width: TxStatusRegisterWidth::U8,
    },
    TxStatusRegisterSpec {
        name: "REG_C2HEVT_MSG_NORMAL",
        address: REG_C2HEVT_MSG_NORMAL,
        width: TxStatusRegisterWidth::U8,
    },
    TxStatusRegisterSpec {
        name: "REG_C2HEVT_CMD_SEQ_88XX",
        address: REG_C2HEVT_CMD_SEQ_88XX,
        width: TxStatusRegisterWidth::U8,
    },
    TxStatusRegisterSpec {
        name: "REG_C2HEVT_CMD_LEN_88XX",
        address: REG_C2HEVT_CMD_LEN_88XX,
        width: TxStatusRegisterWidth::U8,
    },
    TxStatusRegisterSpec {
        name: "REG_C2HEVT_CLEAR",
        address: REG_C2HEVT_CLEAR,
        width: TxStatusRegisterWidth::U8,
    },
];

fn tx_status_probe_report(args: &TxStatusProbeArgs) -> Option<TxStatusProbeReport> {
    args.tx_status.then(|| TxStatusProbeReport {
        enabled: true,
        delay_ms: args.tx_status_delay_ms,
        semantics: "read-only RTL8812AU register deltas around USB TX submission; this does not prove RF radiation",
        pre: Vec::new(),
        post: Vec::new(),
        changed: Vec::new(),
        counters: DiagnosticCounters::default(),
        error: None,
    })
}

fn tx_status_probe_pre<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    report: &mut Option<TxStatusProbeReport>,
) where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    if let Some(report) = report.as_mut() {
        if report.error.is_some() {
            return;
        }
        match tx_status_snapshot(registers, &mut report.counters) {
            Ok(snapshot) => report.pre = snapshot,
            Err(error) => report.error = Some(error),
        }
    }
}

fn tx_status_probe_post<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    report: &mut Option<TxStatusProbeReport>,
) where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    if let Some(report) = report.as_mut() {
        if report.error.is_some() {
            return;
        }
        if report.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(report.delay_ms));
        }
        match tx_status_snapshot(registers, &mut report.counters) {
            Ok(snapshot) => {
                report.changed = tx_status_deltas(&report.pre, &snapshot);
                report.post = snapshot;
            }
            Err(error) => report.error = Some(error),
        }
    }
}

fn add_tx_status_probe_counters(
    total: &mut DiagnosticCounters,
    report: &Option<TxStatusProbeReport>,
) {
    if let Some(report) = report {
        add_diagnostic_counters(total, &report.counters);
    }
}

fn tx_status_snapshot<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
) -> std::result::Result<Vec<TxStatusRegisterReport>, DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    TX_STATUS_REGISTERS
        .iter()
        .map(|spec| tx_status_read_register(registers, counters, *spec))
        .collect()
}

fn tx_status_read_register<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    spec: TxStatusRegisterSpec,
) -> std::result::Result<TxStatusRegisterReport, DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let value = match spec.width {
        TxStatusRegisterWidth::U8 => u32::from(
            read8_with_counter(registers, counters, spec.address)
                .map_err(|error| tx_status_read_error(spec, error))?,
        ),
        TxStatusRegisterWidth::U16 => u32::from(
            read16_with_counter(registers, counters, spec.address)
                .map_err(|error| tx_status_read_error(spec, error))?,
        ),
        TxStatusRegisterWidth::U32 => read32_with_counter(registers, counters, spec.address)
            .map_err(|error| tx_status_read_error(spec, error))?,
    };
    Ok(TxStatusRegisterReport {
        name: spec.name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: spec.width.label(),
        value,
        value_hex: format_value(value, spec.width.hex_digits()),
    })
}

fn tx_status_read_error(
    spec: TxStatusRegisterSpec,
    error: radio_core::Rtl8812auRegisterError,
) -> DiagnosticErrorReport {
    DiagnosticErrorReport {
        code: "tx_status_read_failed",
        message: format!(
            "{} read at {} failed: {error}",
            spec.name,
            format_address(spec.address)
        ),
    }
}

fn tx_status_deltas(
    pre: &[TxStatusRegisterReport],
    post: &[TxStatusRegisterReport],
) -> Vec<TxStatusDeltaReport> {
    pre.iter()
        .zip(post.iter())
        .filter(|(before, after)| before.value != after.value)
        .map(|(before, after)| TxStatusDeltaReport {
            name: before.name,
            address: before.address,
            address_hex: before.address_hex.clone(),
            width: before.width,
            before_hex: before.value_hex.clone(),
            after_hex: after.value_hex.clone(),
            xor_hex: format_value(
                before.value ^ after.value,
                tx_status_width_digits(before.width),
            ),
        })
        .collect()
}

fn tx_status_width_digits(width: &str) -> usize {
    match width {
        "u8" => 2,
        "u16" => 4,
        _ => 8,
    }
}

fn select_supported_adapter(
    selector: DeviceSelector,
) -> std::result::Result<UsbDeviceInfo, DiagnosticErrorReport> {
    let devices = radio_core::list_usb_devices(false).map_err(|error| DiagnosticErrorReport {
        code: "usb_list_failed",
        message: error.to_string(),
    })?;
    devices
        .into_iter()
        .find(|device| selector.matches(device))
        .ok_or_else(|| DiagnosticErrorReport {
            code: "no_supported_adapter",
            message: if selector.is_empty() {
                "no supported RTL8812AU adapter found".to_string()
            } else {
                "no supported RTL8812AU adapter matched selector".to_string()
            },
        })
}

fn read_smoke_register<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    spec: RegisterSmokeSpec,
) -> std::result::Result<RegisterReadReport, radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let (value, bytes) = match spec.width {
        RegisterSmokeWidth::U8 => {
            let value = registers.read8(spec.address)?;
            (u64::from(value), vec![value])
        }
        RegisterSmokeWidth::U16 => {
            let value = registers.read16(spec.address)?;
            (u64::from(value), value.to_le_bytes().to_vec())
        }
        RegisterSmokeWidth::U32 => {
            let value = registers.read32(spec.address)?;
            (u64::from(value), value.to_le_bytes().to_vec())
        }
    };

    Ok(RegisterReadReport {
        name: spec.name,
        address: spec.address,
        address_hex: format!("0x{:04x}", spec.address),
        width: spec.width.label(),
        value,
        value_hex: format!("0x{:0width$x}", value, width = spec.width.value_digits()),
        bytes_le_hex: encode_hex(&bytes),
    })
}

fn power_on_smoke_report(args: PowerOnSmokeArgs) -> PowerOnSmokeReport {
    let selector = args.adapter.selector();
    let mut steps = Vec::new();
    let mut counters = DiagnosticCounters::default();

    if !args.i_understand_this_writes_registers {
        return power_on_smoke_failure_with_command(
            "power-on-smoke",
            &args,
            None,
            None,
            steps,
            counters,
            DiagnosticErrorReport {
                code: "missing_write_authorization",
                message: "power-on smoke writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
            },
        );
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return power_on_smoke_failure(&args, None, None, steps, counters, error);
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return power_on_smoke_failure(
                &args,
                Some(selected),
                None,
                steps,
                counters,
                DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let registers =
        Rtl8812auRegisterAccess::new(&claimed).with_timeout(Duration::from_millis(args.timeout_ms));

    if let Err(error) = run_power_on_sequence(&registers, &args, &mut counters, &mut steps) {
        return power_on_smoke_failure(
            &args,
            Some(adapter),
            Some(endpoints),
            steps,
            counters,
            error,
        );
    }

    PowerOnSmokeReport {
        schema_version: 1,
        command: "power-on-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        poll_attempts: args.poll_attempts,
        poll_delay_us: args.poll_delay_us,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        steps,
        counters,
        error: None,
        notes: vec![
            "guarded hardware write test: power-on/RF-reset registers were written",
            "no firmware download, bulk traffic, channel tuning, or TX operation was issued",
        ],
    }
}

fn macos_power_on_smoke_report(args: PowerOnSmokeArgs) -> PowerOnSmokeReport {
    let selector = args.adapter.selector();
    let mut steps = Vec::new();
    let mut counters = DiagnosticCounters::default();

    if !args.i_understand_this_writes_registers {
        return power_on_smoke_failure_with_command(
            "macos-power-on-smoke",
            &args,
            None,
            None,
            steps,
            counters,
            DiagnosticErrorReport {
                code: "missing_write_authorization",
                message: "macOS IOUSBHost power-on smoke writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
            },
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        return power_on_smoke_failure_with_command(
            "macos-power-on-smoke",
            &args,
            None,
            None,
            steps,
            counters,
            DiagnosticErrorReport {
                code: "unsupported_platform",
                message: "macos-power-on-smoke requires macOS IOUSBHost".to_string(),
            },
        );
    }

    #[cfg(target_os = "macos")]
    {
        let Some(vid) = selector.vid else {
            return power_on_smoke_failure_with_command(
                "macos-power-on-smoke",
                &args,
                None,
                None,
                steps,
                counters,
                DiagnosticErrorReport {
                    code: "missing_vid",
                    message: "macos-power-on-smoke requires --vid because IOUSBHost matching is VID/PID based".to_string(),
                },
            );
        };
        let Some(pid) = selector.pid else {
            return power_on_smoke_failure_with_command(
                "macos-power-on-smoke",
                &args,
                None,
                None,
                steps,
                counters,
                DiagnosticErrorReport {
                    code: "missing_pid",
                    message: "macos-power-on-smoke requires --pid because IOUSBHost matching is VID/PID based".to_string(),
                },
            );
        };

        let device = match macos_usbhost::MacosUsbHostDevice::open(vid, pid) {
            Ok(device) => device,
            Err(error) => {
                return power_on_smoke_failure_with_command(
                    "macos-power-on-smoke",
                    &args,
                    None,
                    None,
                    steps,
                    counters,
                    DiagnosticErrorReport {
                        code: "macos_usbhost_open_failed",
                        message: error,
                    },
                );
            }
        };

        let registers = Rtl8812auRegisterAccess::new(&device)
            .with_timeout(Duration::from_millis(args.timeout_ms));
        if let Err(error) = run_power_on_sequence(&registers, &args, &mut counters, &mut steps) {
            return power_on_smoke_failure_with_command(
                "macos-power-on-smoke",
                &args,
                None,
                None,
                steps,
                counters,
                error,
            );
        }

        PowerOnSmokeReport {
            schema_version: 1,
            command: "macos-power-on-smoke",
            started_at_unix_ms: started_at_unix_ms(),
            platform: platform_info(),
            selector,
            timeout_ms: args.timeout_ms,
            poll_attempts: args.poll_attempts,
            poll_delay_us: args.poll_delay_us,
            result: DiagnosticResult::Pass,
            adapter: None,
            endpoints: None,
            steps,
            counters,
            error: None,
            notes: vec![
                "macOS IOUSBHost guarded hardware write test: power-on/RF-reset registers were written through default-control transfers",
                "no libusb enumeration, USB interface claim, firmware download, bulk traffic, channel tuning, or TX operation was issued",
            ],
        }
    }
}

fn power_on_smoke_failure(
    args: &PowerOnSmokeArgs,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<PowerOnStepReport>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
) -> PowerOnSmokeReport {
    power_on_smoke_failure_with_command(
        "power-on-smoke",
        args,
        adapter,
        endpoints,
        steps,
        counters,
        error,
    )
}

fn power_on_smoke_failure_with_command(
    command: &'static str,
    args: &PowerOnSmokeArgs,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<PowerOnStepReport>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
) -> PowerOnSmokeReport {
    PowerOnSmokeReport {
        schema_version: 1,
        command,
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector: args.adapter.selector(),
        timeout_ms: args.timeout_ms,
        poll_attempts: args.poll_attempts,
        poll_delay_us: args.poll_delay_us,
        result: DiagnosticResult::Fail,
        adapter,
        endpoints,
        steps,
        counters,
        error: Some(error),
        notes: vec![
            "guarded hardware write test aborted before firmware download, bulk traffic, channel tuning, or TX operation",
        ],
    }
}

fn firmware_smoke_report(args: FirmwareSmokeArgs) -> FirmwareSmokeReport {
    let selector = args.adapter.selector();
    let mut steps = Vec::new();
    let mut counters = DiagnosticCounters::default();
    let mut stats = FirmwareRunStats::default();

    if !args.i_understand_this_writes_registers {
        return firmware_smoke_failure(
            &args,
            FirmwareSmokeFailureInput {
                firmware: None,
                adapter: None,
                endpoints: None,
                steps,
                counters,
                stats,
                error: DiagnosticErrorReport {
                    code: "missing_write_authorization",
                    message: "firmware smoke writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
                },
            },
        );
    }

    if args.download_attempts == 0 {
        return firmware_smoke_failure(
            &args,
            FirmwareSmokeFailureInput {
                firmware: None,
                adapter: None,
                endpoints: None,
                steps,
                counters,
                stats,
                error: DiagnosticErrorReport {
                    code: "invalid_download_attempts",
                    message: "--download-attempts must be at least 1".to_string(),
                },
            },
        );
    }

    let (firmware_image, firmware) = match load_firmware_with_report(&args.firmware) {
        Ok((image, report)) => (image, report),
        Err(message) => {
            return firmware_smoke_failure(
                &args,
                FirmwareSmokeFailureInput {
                    firmware: None,
                    adapter: None,
                    endpoints: None,
                    steps,
                    counters,
                    stats,
                    error: DiagnosticErrorReport {
                        code: "firmware_load_failed",
                        message,
                    },
                },
            );
        }
    };
    let firmware_payload = firmware_image.realtek_download_payload();
    stats.firmware_payload_offset = Some(firmware_payload.offset);
    stats.firmware_payload_len = Some(firmware_payload.bytes.len());
    stats.firmware_signature = firmware_payload.signature;

    if firmware_page_count(firmware_payload.bytes.len()) > MAX_FIRMWARE_DOWNLOAD_PAGES {
        return firmware_smoke_failure(
            &args,
            FirmwareSmokeFailureInput {
                firmware: Some(firmware),
                adapter: None,
                endpoints: None,
                steps,
                counters,
                stats,
                error: DiagnosticErrorReport {
                    code: "firmware_too_many_pages",
                    message: format!(
                        "firmware requires {} 4 KiB pages, but RTL8812A page selector exposes {} pages",
                        firmware_page_count(firmware_payload.bytes.len()),
                        MAX_FIRMWARE_DOWNLOAD_PAGES
                    ),
                },
            },
        );
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return firmware_smoke_failure(
                &args,
                FirmwareSmokeFailureInput {
                    firmware: Some(firmware),
                    adapter: None,
                    endpoints: None,
                    steps,
                    counters,
                    stats,
                    error,
                },
            );
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return firmware_smoke_failure(
                &args,
                FirmwareSmokeFailureInput {
                    firmware: Some(firmware),
                    adapter: Some(selected),
                    endpoints: None,
                    steps,
                    counters,
                    stats,
                    error: DiagnosticErrorReport {
                        code: "usb_claim_failed",
                        message: error.to_string(),
                    },
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let registers =
        Rtl8812auRegisterAccess::new(&claimed).with_timeout(Duration::from_millis(args.timeout_ms));

    if let Err(error) = run_firmware_sequence(
        &registers,
        &args,
        firmware_payload.bytes,
        &mut counters,
        &mut steps,
        &mut stats,
    ) {
        return firmware_smoke_failure(
            &args,
            FirmwareSmokeFailureInput {
                firmware: Some(firmware),
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                steps,
                counters,
                stats,
                error,
            },
        );
    }

    FirmwareSmokeReport {
        schema_version: 1,
        command: "firmware-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        firmware_path: args.firmware,
        firmware: Some(firmware),
        timeout_ms: args.timeout_ms,
        download_attempts: args.download_attempts,
        checksum_min_attempts: args.checksum_min_attempts,
        checksum_timeout_ms: args.checksum_timeout_ms,
        ready_min_attempts: args.ready_min_attempts,
        ready_timeout_ms: args.ready_timeout_ms,
        poll_delay_us: args.poll_delay_us,
        firmware_payload_offset: stats.firmware_payload_offset,
        firmware_payload_len: stats.firmware_payload_len,
        firmware_signature_hex: stats.firmware_signature.map(|value| format_value(value, 4)),
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        steps,
        firmware_bytes_written: stats.firmware_bytes_written,
        firmware_control_writes: stats.firmware_control_writes,
        checksum_poll_attempts: stats.checksum_poll_attempts,
        ready_poll_attempts: stats.ready_poll_attempts,
        final_mcu_status_hex: stats.final_mcu_status.map(|value| format_value(value, 8)),
        counters,
        error: None,
        notes: vec![
            "guarded firmware smoke test: RTL8812A firmware was written through vendor control transfers",
            "no bulk traffic, channel tuning, RX loop, or TX operation was issued",
        ],
    }
}

fn firmware_smoke_failure(
    args: &FirmwareSmokeArgs,
    input: FirmwareSmokeFailureInput,
) -> FirmwareSmokeReport {
    FirmwareSmokeReport {
        schema_version: 1,
        command: "firmware-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector: args.adapter.selector(),
        firmware_path: args.firmware.clone(),
        firmware: input.firmware,
        timeout_ms: args.timeout_ms,
        download_attempts: args.download_attempts,
        checksum_min_attempts: args.checksum_min_attempts,
        checksum_timeout_ms: args.checksum_timeout_ms,
        ready_min_attempts: args.ready_min_attempts,
        ready_timeout_ms: args.ready_timeout_ms,
        poll_delay_us: args.poll_delay_us,
        firmware_payload_offset: input.stats.firmware_payload_offset,
        firmware_payload_len: input.stats.firmware_payload_len,
        firmware_signature_hex: input.stats.firmware_signature.map(|value| format_value(value, 4)),
        result: DiagnosticResult::Fail,
        adapter: input.adapter,
        endpoints: input.endpoints,
        steps: input.steps,
        firmware_bytes_written: input.stats.firmware_bytes_written,
        firmware_control_writes: input.stats.firmware_control_writes,
        checksum_poll_attempts: input.stats.checksum_poll_attempts,
        ready_poll_attempts: input.stats.ready_poll_attempts,
        final_mcu_status_hex: input.stats.final_mcu_status.map(|value| format_value(value, 8)),
        counters: input.counters,
        error: Some(input.error),
        notes: vec![
            "guarded firmware smoke test stopped before bulk traffic, channel tuning, RX loop, or TX operation",
        ],
    }
}

fn llt_smoke_report(args: LltSmokeArgs) -> LltSmokeReport {
    let selector = args.adapter.selector();
    let mut steps = Vec::new();
    let mut counters = DiagnosticCounters::default();
    let mut stats = LltRunStats::default();

    if !args.i_understand_this_writes_registers {
        return llt_smoke_failure(
            &args,
            None,
            None,
            steps,
            counters,
            stats,
            DiagnosticErrorReport {
                code: "missing_write_authorization",
                message: "LLT smoke writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
            },
        );
    }

    if args.poll_attempts == 0 {
        return llt_smoke_failure(
            &args,
            None,
            None,
            steps,
            counters,
            stats,
            DiagnosticErrorReport {
                code: "invalid_poll_attempts",
                message: "--poll-attempts must be at least 1".to_string(),
            },
        );
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return llt_smoke_failure(&args, None, None, steps, counters, stats, error);
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return llt_smoke_failure(
                &args,
                Some(selected),
                None,
                steps,
                counters,
                stats,
                DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let registers =
        Rtl8812auRegisterAccess::new(&claimed).with_timeout(Duration::from_millis(args.timeout_ms));

    if let Err(error) = run_llt_sequence(&registers, &args, &mut counters, &mut steps, &mut stats) {
        return llt_smoke_failure(
            &args,
            Some(adapter),
            Some(endpoints),
            steps,
            counters,
            stats,
            error,
        );
    }

    LltSmokeReport {
        schema_version: 1,
        command: "llt-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        poll_attempts: args.poll_attempts,
        poll_delay_us: args.poll_delay_us,
        tx_page_boundary: TX_PAGE_BOUNDARY_8812,
        last_tx_page_entry: LAST_ENTRY_OF_TX_PKT_BUFFER_8812,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        steps,
        entries_written: stats.entries_written,
        max_poll_attempts_observed: stats.max_poll_attempts_observed,
        counters,
        error: None,
        notes: vec![
            "guarded LLT smoke test: RTL8812A linked-list table entries were written and polled idle",
            "no firmware download, queue/DMA programming, channel tuning, bulk traffic, RX loop, or TX operation was issued",
        ],
    }
}

fn llt_smoke_failure(
    args: &LltSmokeArgs,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<LltStepReport>,
    counters: DiagnosticCounters,
    stats: LltRunStats,
    error: DiagnosticErrorReport,
) -> LltSmokeReport {
    LltSmokeReport {
        schema_version: 1,
        command: "llt-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector: args.adapter.selector(),
        timeout_ms: args.timeout_ms,
        poll_attempts: args.poll_attempts,
        poll_delay_us: args.poll_delay_us,
        tx_page_boundary: TX_PAGE_BOUNDARY_8812,
        last_tx_page_entry: LAST_ENTRY_OF_TX_PKT_BUFFER_8812,
        result: DiagnosticResult::Fail,
        adapter,
        endpoints,
        steps,
        entries_written: stats.entries_written,
        max_poll_attempts_observed: stats.max_poll_attempts_observed,
        counters,
        error: Some(error),
        notes: vec![
            "guarded LLT smoke test stopped before queue/DMA programming, channel tuning, bulk traffic, RX loop, or TX operation",
        ],
    }
}

fn run_llt_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    args: &LltSmokeArgs,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<LltStepReport>,
    stats: &mut LltRunStats,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let cr = read16_with_counter(registers, counters, REG_CR).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_CR preflight read failed: {error}"),
        }
    })?;
    steps.push(LltStepReport {
        phase: "llt_preflight",
        operation: "read16",
        description: "verify command register block enables before LLT programming",
        source: POWER_SOURCE_USB_HALINIT,
        register_name: "REG_CR",
        address: REG_CR,
        address_hex: format_address(REG_CR),
        width: "u16",
        llt_address: None,
        llt_data: None,
        value_hex: None,
        after_hex: Some(format_value(cr, 4)),
        attempts: None,
        passed: (cr & CR_ENABLE_BITS) == CR_ENABLE_BITS,
    });
    if (cr & CR_ENABLE_BITS) != CR_ENABLE_BITS {
        return Err(DiagnosticErrorReport {
            code: "mac_not_powered_on",
            message: format!(
                "REG_CR expected block-enable mask {} to be set before LLT programming, got {}",
                format_value(CR_ENABLE_BITS, 4),
                format_value(cr, 4)
            ),
        });
    }

    for address in 0..(TX_PAGE_BOUNDARY_8812 - 1) {
        llt_write_step(
            registers,
            args,
            counters,
            steps,
            stats,
            address,
            address + 1,
        )?;
    }
    llt_write_step(
        registers,
        args,
        counters,
        steps,
        stats,
        TX_PAGE_BOUNDARY_8812 - 1,
        0xff,
    )?;
    for address in TX_PAGE_BOUNDARY_8812..LAST_ENTRY_OF_TX_PKT_BUFFER_8812 {
        llt_write_step(
            registers,
            args,
            counters,
            steps,
            stats,
            address,
            address + 1,
        )?;
    }
    llt_write_step(
        registers,
        args,
        counters,
        steps,
        stats,
        LAST_ENTRY_OF_TX_PKT_BUFFER_8812,
        TX_PAGE_BOUNDARY_8812,
    )
}

fn llt_write_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    args: &LltSmokeArgs,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<LltStepReport>,
    stats: &mut LltRunStats,
    llt_address: u8,
    llt_data: u8,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let written = encode_llt_write(llt_address, llt_data);
    write32_with_counter(registers, counters, REG_LLT_INIT, written).map_err(|error| {
        DiagnosticErrorReport {
            code: "llt_write_failed",
            message: format!(
                "LLT write addr=0x{llt_address:02x} data=0x{llt_data:02x} failed: {error}"
            ),
        }
    })?;

    let mut last = 0u32;
    for attempt in 1..=args.poll_attempts {
        last = read32_with_counter(registers, counters, REG_LLT_INIT).map_err(|error| {
            DiagnosticErrorReport {
                code: "llt_poll_failed",
                message: format!(
                    "LLT poll addr=0x{llt_address:02x} data=0x{llt_data:02x} failed: {error}"
                ),
            }
        })?;
        if llt_op_value(last) == LLT_NO_ACTIVE {
            stats.entries_written += 1;
            stats.max_poll_attempts_observed = stats.max_poll_attempts_observed.max(attempt);
            steps.push(llt_step_report(
                llt_address,
                llt_data,
                written,
                last,
                attempt,
                true,
            ));
            return Ok(());
        }
        if args.poll_delay_us > 0 {
            std::thread::sleep(Duration::from_micros(args.poll_delay_us));
        }
    }

    steps.push(llt_step_report(
        llt_address,
        llt_data,
        written,
        last,
        args.poll_attempts,
        false,
    ));
    Err(DiagnosticErrorReport {
        code: "llt_poll_timeout",
        message: format!(
            "LLT write addr=0x{llt_address:02x} data=0x{llt_data:02x} did not become idle after {} attempts, last {}",
            args.poll_attempts,
            format_value(last, 8)
        ),
    })
}

fn llt_step_report(
    llt_address: u8,
    llt_data: u8,
    written: u32,
    after: u32,
    attempts: u32,
    passed: bool,
) -> LltStepReport {
    LltStepReport {
        phase: "llt_program",
        operation: "write32_poll",
        description: "write one LLT entry and poll operation idle",
        source: LLT_SOURCE_HAL_INIT,
        register_name: "REG_LLT_INIT",
        address: REG_LLT_INIT,
        address_hex: format_address(REG_LLT_INIT),
        width: "u32",
        llt_address: Some(llt_address),
        llt_data: Some(llt_data),
        value_hex: Some(format_value(written, 8)),
        after_hex: Some(format_value(after, 8)),
        attempts: Some(attempts),
        passed,
    }
}

fn encode_llt_write(address: u8, data: u8) -> u32 {
    (u32::from(address) << 8) | u32::from(data) | (LLT_WRITE_ACCESS << LLT_OP_SHIFT)
}

fn llt_op_value(value: u32) -> u32 {
    (value >> LLT_OP_SHIFT) & LLT_OP_MASK
}

fn queue_dma_smoke_report(args: QueueDmaSmokeArgs) -> QueueDmaSmokeReport {
    let selector = args.adapter.selector();
    let mut steps = Vec::new();
    let mut counters = DiagnosticCounters::default();

    if !args.i_understand_this_writes_registers {
        return queue_dma_smoke_failure(
            &args,
            None,
            None,
            None,
            steps,
            counters,
            DiagnosticErrorReport {
                code: "missing_write_authorization",
                message: "queue/DMA smoke writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
            },
        );
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return queue_dma_smoke_failure(&args, None, None, None, steps, counters, error);
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return queue_dma_smoke_failure(
                &args,
                Some(selected),
                None,
                None,
                steps,
                counters,
                DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let layout = match queue_layout_from_endpoints(&endpoints) {
        Ok(layout) => layout,
        Err(error) => {
            return queue_dma_smoke_failure(
                &args,
                Some(adapter),
                Some(endpoints),
                None,
                steps,
                counters,
                error,
            );
        }
    };
    let registers =
        Rtl8812auRegisterAccess::new(&claimed).with_timeout(Duration::from_millis(args.timeout_ms));

    if let Err(error) = run_queue_dma_sequence(&registers, &layout, &mut counters, &mut steps) {
        return queue_dma_smoke_failure(
            &args,
            Some(adapter),
            Some(endpoints),
            Some(layout),
            steps,
            counters,
            error,
        );
    }

    QueueDmaSmokeReport {
        schema_version: 1,
        command: "queue-dma-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        bulk_out_endpoint_count: Some(layout.bulk_out_endpoint_count),
        out_ep_queue_sel_hex: Some(format_value(layout.out_ep_queue_sel, 2)),
        tx_total_page_number: TX_TOTAL_PAGE_NUMBER_8812,
        tx_page_boundary: TX_PAGE_BOUNDARY_8812,
        rx_dma_boundary_hex: format_value(RX_DMA_BOUNDARY_8812, 4),
        queue_pages: Some(queue_page_report(layout)),
        steps,
        counters,
        error: None,
        notes: vec![
            "guarded queue/DMA smoke test: RTL8812A reserved-page, boundary, TXDMA map, and page-size registers were written",
            "no MAC receive enable, BB/RF table programming, channel tuning, bulk traffic, RX loop, or TX operation was issued",
        ],
    }
}

fn queue_dma_smoke_failure(
    args: &QueueDmaSmokeArgs,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    layout: Option<QueueLayout>,
    steps: Vec<QueueDmaStepReport>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
) -> QueueDmaSmokeReport {
    QueueDmaSmokeReport {
        schema_version: 1,
        command: "queue-dma-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector: args.adapter.selector(),
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Fail,
        adapter,
        endpoints,
        bulk_out_endpoint_count: layout.map(|layout| layout.bulk_out_endpoint_count),
        out_ep_queue_sel_hex: layout.map(|layout| format_value(layout.out_ep_queue_sel, 2)),
        tx_total_page_number: TX_TOTAL_PAGE_NUMBER_8812,
        tx_page_boundary: TX_PAGE_BOUNDARY_8812,
        rx_dma_boundary_hex: format_value(RX_DMA_BOUNDARY_8812, 4),
        queue_pages: layout.map(queue_page_report),
        steps,
        counters,
        error: Some(error),
        notes: vec![
            "guarded queue/DMA smoke test stopped before MAC receive enable, BB/RF table programming, channel tuning, bulk traffic, RX loop, or TX operation",
        ],
    }
}

fn run_queue_dma_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    layout: &QueueLayout,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    queue_preflight(registers, counters, steps)?;
    queue_write8_step(
        registers,
        counters,
        steps,
        QueueWrite8Spec {
            phase: "queue_reserved_pages",
            register_name: "REG_RQPN_NPQ",
            address: REG_RQPN_NPQ,
            value: layout.rqpn_npq,
            verify_mask: u8::MAX,
            verify_value: layout.rqpn_npq,
            description: "program normal-priority queue reserved pages",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write32_step(
        registers,
        counters,
        steps,
        QueueWrite32Spec {
            phase: "queue_reserved_pages",
            register_name: "REG_RQPN",
            address: REG_RQPN,
            value: layout.rqpn,
            verify_mask: RQPN_PAGE_MASK,
            verify_value: layout.rqpn,
            description: "load TX DMA reserved page numbers",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;

    for (register_name, address) in [
        ("REG_BCNQ_BDNY", REG_BCNQ_BDNY),
        ("REG_MGQ_BDNY", REG_MGQ_BDNY),
        ("REG_WMAC_LBK_BF_HD", REG_WMAC_LBK_BF_HD),
        ("REG_TRXFF_BNDY", REG_TRXFF_BNDY),
        ("REG_TDECTRL + 1", REG_TDECTRL + 1),
    ] {
        queue_write8_step(
            registers,
            counters,
            steps,
            QueueWrite8Spec {
                phase: "tx_buffer_boundary",
                register_name,
                address,
                value: TX_PAGE_BOUNDARY_8812,
                verify_mask: u8::MAX,
                verify_value: TX_PAGE_BOUNDARY_8812,
                description: "set TX buffer page boundary",
                source: POWER_SOURCE_USB_HALINIT,
            },
        )?;
    }

    queue_rmw16_step(
        registers,
        counters,
        steps,
        QueueRmw16Spec {
            phase: "queue_priority",
            register_name: "REG_TRXDMA_CTRL",
            address: REG_TRXDMA_CTRL,
            preserve_mask: 0x0007,
            value_mask: TXDMA_QUEUE_MAP_MASK,
            value: layout.queue_map,
            description: "map traffic classes to USB TXDMA queues",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write16_step(
        registers,
        counters,
        steps,
        QueueWrite16Spec {
            phase: "rx_dma_boundary",
            register_name: "REG_TRXFF_BNDY + 2",
            address: REG_TRXFF_BNDY + 2,
            value: RX_DMA_BOUNDARY_8812,
            verify_mask: u16::MAX,
            verify_value: RX_DMA_BOUNDARY_8812,
            description: "set RX DMA page boundary",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write8_step(
        registers,
        counters,
        steps,
        QueueWrite8Spec {
            phase: "page_size",
            register_name: "REG_PBP",
            address: REG_PBP,
            value: PSTX_PBP_512,
            verify_mask: u8::MAX,
            verify_value: PSTX_PBP_512,
            description: "set TX packet-buffer page size to 512 bytes",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )
}

fn queue_preflight<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let cr = read16_with_counter(registers, counters, REG_CR).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_CR preflight read failed: {error}"),
        }
    })?;
    let cr_passed = (cr & CR_ENABLE_BITS) == CR_ENABLE_BITS;
    steps.push(queue_read_report16(QueueRead16Spec {
        phase: "preflight",
        description: "verify command register block enables before queue/DMA programming",
        register_name: "REG_CR",
        address: REG_CR,
        value: cr,
        mask: Some(CR_ENABLE_BITS),
        expected: Some(CR_ENABLE_BITS),
        passed: cr_passed,
    }));
    if !cr_passed {
        return Err(DiagnosticErrorReport {
            code: "mac_not_powered_on",
            message: format!(
                "REG_CR expected block-enable mask {} to be set before queue/DMA programming, got {}",
                format_value(CR_ENABLE_BITS, 4),
                format_value(cr, 4)
            ),
        });
    }

    let mcu = read8_with_counter(registers, counters, REG_MCUFWDL).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_MCUFWDL preflight read failed: {error}"),
        }
    })?;
    let firmware_ready_mask = RAM_DL_SEL | BIT6 | BIT1;
    let firmware_passed = (mcu & firmware_ready_mask) == firmware_ready_mask;
    steps.push(queue_read_report8(QueueRead8Spec {
        phase: "preflight",
        description: "verify firmware is running before queue/DMA programming",
        register_name: "REG_MCUFWDL",
        address: REG_MCUFWDL,
        value: mcu,
        mask: Some(firmware_ready_mask),
        expected: Some(firmware_ready_mask),
        passed: firmware_passed,
    }));
    if !firmware_passed {
        return Err(DiagnosticErrorReport {
            code: "firmware_not_ready",
            message: format!(
                "REG_MCUFWDL expected firmware-ready mask {} before queue/DMA programming, got {}",
                format_value(firmware_ready_mask, 2),
                format_value(mcu, 2)
            ),
        });
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct QueueWrite8Spec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    value: u8,
    verify_mask: u8,
    verify_value: u8,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct QueueWrite16Spec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    value: u16,
    verify_mask: u16,
    verify_value: u16,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct QueueWrite32Spec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    value: u32,
    verify_mask: u32,
    verify_value: u32,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct QueueRmw16Spec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    preserve_mask: u16,
    value_mask: u16,
    value: u16,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct QueueRead8Spec {
    phase: &'static str,
    description: &'static str,
    register_name: &'static str,
    address: u16,
    value: u8,
    mask: Option<u8>,
    expected: Option<u8>,
    passed: bool,
}

#[derive(Debug, Clone, Copy)]
struct QueueRead16Spec {
    phase: &'static str,
    description: &'static str,
    register_name: &'static str,
    address: u16,
    value: u16,
    mask: Option<u16>,
    expected: Option<u16>,
    passed: bool,
}

fn queue_write8_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    spec: QueueWrite8Spec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    write8_with_counter(registers, counters, spec.address, spec.value).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.verify_value & spec.verify_mask;
    let passed = (after & spec.verify_mask) == expected;
    steps.push(QueueDmaStepReport {
        phase: spec.phase,
        operation: "write8",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u8",
        mask_hex: Some(format_value(spec.verify_mask, 2)),
        value_hex: Some(format_value(spec.value, 2)),
        before_hex: Some(format_value(before, 2)),
        written_hex: Some(format_value(spec.value, 2)),
        after_hex: Some(format_value(after, 2)),
        expected_hex: Some(format_value(expected, 2)),
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(queue_readback_error(
            spec.register_name,
            format_value(spec.verify_mask, 2),
            format_value(expected, 2),
            format_value(after & spec.verify_mask, 2),
        ))
    }
}

fn queue_write16_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    spec: QueueWrite16Spec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read16_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    write16_with_counter(registers, counters, spec.address, spec.value).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read16_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.verify_value & spec.verify_mask;
    let passed = (after & spec.verify_mask) == expected;
    steps.push(QueueDmaStepReport {
        phase: spec.phase,
        operation: "write16",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u16",
        mask_hex: Some(format_value(spec.verify_mask, 4)),
        value_hex: Some(format_value(spec.value, 4)),
        before_hex: Some(format_value(before, 4)),
        written_hex: Some(format_value(spec.value, 4)),
        after_hex: Some(format_value(after, 4)),
        expected_hex: Some(format_value(expected, 4)),
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(queue_readback_error(
            spec.register_name,
            format_value(spec.verify_mask, 4),
            format_value(expected, 4),
            format_value(after & spec.verify_mask, 4),
        ))
    }
}

fn queue_write32_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    spec: QueueWrite32Spec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read32_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    write32_with_counter(registers, counters, spec.address, spec.value).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read32_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.verify_value & spec.verify_mask;
    let passed = (after & spec.verify_mask) == expected;
    steps.push(QueueDmaStepReport {
        phase: spec.phase,
        operation: "write32",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u32",
        mask_hex: Some(format_value(spec.verify_mask, 8)),
        value_hex: Some(format_value(spec.value, 8)),
        before_hex: Some(format_value(before, 8)),
        written_hex: Some(format_value(spec.value, 8)),
        after_hex: Some(format_value(after, 8)),
        expected_hex: Some(format_value(expected, 8)),
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(queue_readback_error(
            spec.register_name,
            format_value(spec.verify_mask, 8),
            format_value(expected, 8),
            format_value(after & spec.verify_mask, 8),
        ))
    }
}

fn queue_rmw16_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    spec: QueueRmw16Spec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read16_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    let written = (before & spec.preserve_mask) | (spec.value & spec.value_mask);
    write16_with_counter(registers, counters, spec.address, written).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read16_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.value & spec.value_mask;
    let passed = (after & spec.value_mask) == expected;
    steps.push(QueueDmaStepReport {
        phase: spec.phase,
        operation: "rmw16",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u16",
        mask_hex: Some(format_value(spec.value_mask, 4)),
        value_hex: Some(format_value(spec.value, 4)),
        before_hex: Some(format_value(before, 4)),
        written_hex: Some(format_value(written, 4)),
        after_hex: Some(format_value(after, 4)),
        expected_hex: Some(format_value(expected, 4)),
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(queue_readback_error(
            spec.register_name,
            format_value(spec.value_mask, 4),
            format_value(expected, 4),
            format_value(after & spec.value_mask, 4),
        ))
    }
}

fn queue_read_report8(spec: QueueRead8Spec) -> QueueDmaStepReport {
    QueueDmaStepReport {
        phase: spec.phase,
        operation: "read8",
        description: spec.description,
        source: POWER_SOURCE_USB_HALINIT,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u8",
        mask_hex: spec.mask.map(|mask| format_value(mask, 2)),
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(spec.value, 2)),
        expected_hex: spec.expected.map(|expected| format_value(expected, 2)),
        passed: spec.passed,
    }
}

fn queue_read_report16(spec: QueueRead16Spec) -> QueueDmaStepReport {
    QueueDmaStepReport {
        phase: spec.phase,
        operation: "read16",
        description: spec.description,
        source: POWER_SOURCE_USB_HALINIT,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u16",
        mask_hex: spec.mask.map(|mask| format_value(mask, 4)),
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(spec.value, 4)),
        expected_hex: spec.expected.map(|expected| format_value(expected, 4)),
        passed: spec.passed,
    }
}

fn queue_layout_from_endpoints(
    endpoints: &UsbEndpoints,
) -> std::result::Result<QueueLayout, DiagnosticErrorReport> {
    let bulk_out_endpoint_count = endpoints.bulk_out_all.len();
    let out_ep_queue_sel = match bulk_out_endpoint_count {
        2 => TX_SELE_HQ | TX_SELE_NQ,
        3 => TX_SELE_HQ | TX_SELE_LQ | TX_SELE_NQ,
        4 => TX_SELE_HQ | TX_SELE_LQ | TX_SELE_NQ | TX_SELE_EQ,
        other => {
            return Err(DiagnosticErrorReport {
                code: "unsupported_bulk_out_endpoint_count",
                message: format!(
                    "queue/DMA smoke supports 2, 3, or 4 bulk OUT endpoints, observed {other}"
                ),
            });
        }
    };

    let hpq = if out_ep_queue_sel & TX_SELE_HQ != 0 {
        NORMAL_PAGE_NUM_HPQ_8812
    } else {
        0
    };
    let lpq = if out_ep_queue_sel & TX_SELE_LQ != 0 {
        NORMAL_PAGE_NUM_LPQ_8812
    } else {
        0
    };
    let npq = if out_ep_queue_sel & TX_SELE_NQ != 0 {
        NORMAL_PAGE_NUM_NPQ_8812
    } else {
        0
    };
    let pubq = TX_TOTAL_PAGE_NUMBER_8812
        .checked_sub(hpq)
        .and_then(|value| value.checked_sub(lpq))
        .and_then(|value| value.checked_sub(npq))
        .ok_or_else(|| DiagnosticErrorReport {
            code: "invalid_queue_page_layout",
            message: "queue reserved-page layout underflowed public queue pages".to_string(),
        })?;
    let rqpn_npq = npq;
    let rqpn = u32::from(hpq) | (u32::from(lpq) << 8) | (u32::from(pubq) << 16) | LD_RQPN;
    let queue_map = queue_map_for_endpoint_count(bulk_out_endpoint_count);

    Ok(QueueLayout {
        bulk_out_endpoint_count,
        out_ep_queue_sel,
        hpq,
        lpq,
        npq,
        pubq,
        rqpn_npq,
        rqpn,
        queue_map,
    })
}

fn queue_map_for_endpoint_count(bulk_out_endpoint_count: usize) -> u16 {
    match bulk_out_endpoint_count {
        2 => queue_map(
            QUEUE_NORMAL,
            QUEUE_NORMAL,
            QUEUE_HIGH,
            QUEUE_HIGH,
            QUEUE_HIGH,
            QUEUE_HIGH,
        ),
        3 => queue_map(
            QUEUE_LOW,
            QUEUE_LOW,
            QUEUE_NORMAL,
            QUEUE_HIGH,
            QUEUE_HIGH,
            QUEUE_HIGH,
        ),
        4 => queue_map(
            QUEUE_LOW,
            QUEUE_LOW,
            QUEUE_NORMAL,
            QUEUE_NORMAL,
            QUEUE_EXTRA,
            QUEUE_HIGH,
        ),
        _ => 0,
    }
}

fn queue_map(beq: u16, bkq: u16, viq: u16, voq: u16, mgq: u16, hiq: u16) -> u16 {
    ((hiq & 0x3) << 14)
        | ((mgq & 0x3) << 12)
        | ((bkq & 0x3) << 10)
        | ((beq & 0x3) << 8)
        | ((viq & 0x3) << 6)
        | ((voq & 0x3) << 4)
}

fn queue_page_report(layout: QueueLayout) -> QueuePageReport {
    QueuePageReport {
        hpq: layout.hpq,
        lpq: layout.lpq,
        npq: layout.npq,
        pubq: layout.pubq,
        rqpn_npq_hex: format_value(layout.rqpn_npq, 2),
        rqpn_hex: format_value(layout.rqpn, 8),
    }
}

fn queue_readback_error(
    register_name: &'static str,
    mask_hex: String,
    expected_hex: String,
    actual_hex: String,
) -> DiagnosticErrorReport {
    DiagnosticErrorReport {
        code: "register_readback_mismatch",
        message: format!(
            "{register_name} expected mask {mask_hex} to equal {expected_hex}, got {actual_hex}"
        ),
    }
}

fn mac_smoke_report(args: MacSmokeArgs) -> MacSmokeReport {
    let selector = args.adapter.selector();
    let mut steps = Vec::new();
    let mut counters = DiagnosticCounters::default();

    if !args.i_understand_this_writes_registers {
        return mac_smoke_failure(
            &args,
            None,
            None,
            steps,
            counters,
            DiagnosticErrorReport {
                code: "missing_write_authorization",
                message: "MAC smoke writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
            },
        );
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return mac_smoke_failure(&args, None, None, steps, counters, error);
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return mac_smoke_failure(
                &args,
                Some(selected),
                None,
                steps,
                counters,
                DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let registers =
        Rtl8812auRegisterAccess::new(&claimed).with_timeout(Duration::from_millis(args.timeout_ms));

    if let Err(error) = run_mac_sequence(&registers, &mut counters, &mut steps) {
        return mac_smoke_failure(
            &args,
            Some(adapter),
            Some(endpoints),
            steps,
            counters,
            error,
        );
    }

    MacSmokeReport {
        schema_version: 1,
        command: "mac-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        receive_config_hex: format_value(MAC_RECEIVE_CONFIG, 8),
        retry_limit_hex: format_value(RETRY_LIMIT_STA, 4),
        steps,
        counters,
        error: None,
        notes: vec![
            "guarded MAC smoke test: RTL8812A driver-info, network-type, WMAC filter, rate/retry, EDCA, HW sequence, BAR, and MAC TX/RX enable registers were written",
            "no BB/RF table programming, channel tuning, bulk traffic, RX loop, or TX operation was issued",
        ],
    }
}

fn mac_smoke_failure(
    args: &MacSmokeArgs,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    steps: Vec<QueueDmaStepReport>,
    counters: DiagnosticCounters,
    error: DiagnosticErrorReport,
) -> MacSmokeReport {
    MacSmokeReport {
        schema_version: 1,
        command: "mac-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector: args.adapter.selector(),
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Fail,
        adapter,
        endpoints,
        receive_config_hex: format_value(MAC_RECEIVE_CONFIG, 8),
        retry_limit_hex: format_value(RETRY_LIMIT_STA, 4),
        steps,
        counters,
        error: Some(error),
        notes: vec![
            "guarded MAC smoke test stopped before BB/RF table programming, channel tuning, bulk traffic, RX loop, or TX operation",
        ],
    }
}

fn run_mac_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    mac_preflight(registers, counters, steps)?;

    queue_write8_step(
        registers,
        counters,
        steps,
        QueueWrite8Spec {
            phase: "driver_info",
            register_name: "REG_RX_DRVINFO_SZ",
            address: REG_RX_DRVINFO_SZ,
            value: DRVINFO_SZ,
            verify_mask: u8::MAX,
            verify_value: DRVINFO_SZ,
            description: "set RX driver-info size to include PHY status",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    mac_rmw32_step(
        registers,
        counters,
        steps,
        MacRmw32Spec {
            phase: "network_type",
            register_name: "REG_CR",
            address: REG_CR,
            preserve_mask: !MASK_NETTYPE,
            value_mask: MASK_NETTYPE,
            value: NETTYPE_LINK_AP,
            description: "set MAC network type to AP-style raw frame mode",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write32_step(
        registers,
        counters,
        steps,
        QueueWrite32Spec {
            phase: "wmac_filter",
            register_name: "REG_RCR",
            address: REG_RCR,
            value: MAC_RECEIVE_CONFIG,
            verify_mask: u32::MAX,
            verify_value: MAC_RECEIVE_CONFIG,
            description: "program WMAC receive configuration",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    for (register_name, address) in [("REG_MAR", REG_MAR), ("REG_MAR + 4", REG_MAR + 4)] {
        queue_write32_step(
            registers,
            counters,
            steps,
            QueueWrite32Spec {
                phase: "wmac_filter",
                register_name,
                address,
                value: u32::MAX,
                verify_mask: u32::MAX,
                verify_value: u32::MAX,
                description: "accept all multicast addresses",
                source: POWER_SOURCE_USB_HALINIT,
            },
        )?;
    }
    queue_write16_step(
        registers,
        counters,
        steps,
        QueueWrite16Spec {
            phase: "wmac_filter",
            register_name: "REG_RXFLTMAP1",
            address: REG_RXFLTMAP1,
            value: 1 << 10,
            verify_mask: u16::MAX,
            verify_value: 1 << 10,
            description: "allow PS-Poll control subtype in RX filter map",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    mac_rmw32_step(
        registers,
        counters,
        steps,
        MacRmw32Spec {
            phase: "adaptive_control",
            register_name: "REG_RRSR",
            address: REG_RRSR,
            preserve_mask: !RATE_BITMAP_ALL,
            value_mask: RATE_BITMAP_ALL,
            value: RATE_RRSR_CCK_ONLY_1M,
            description: "program response-rate set used by MAC acknowledgements",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write16_step(
        registers,
        counters,
        steps,
        QueueWrite16Spec {
            phase: "adaptive_control",
            register_name: "REG_SPEC_SIFS",
            address: REG_SPEC_SIFS,
            value: 0x1010,
            verify_mask: u16::MAX,
            verify_value: 0x1010,
            description: "set initial SIFS timing used in NAV",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write16_step(
        registers,
        counters,
        steps,
        QueueWrite16Spec {
            phase: "adaptive_control",
            register_name: "REG_RETRY_LIMIT",
            address: REG_RETRY_LIMIT,
            value: RETRY_LIMIT_STA,
            verify_mask: u16::MAX,
            verify_value: RETRY_LIMIT_STA,
            description: "set short and long retry limits",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;

    for (register_name, address) in [
        ("REG_SPEC_SIFS", REG_SPEC_SIFS),
        ("REG_MAC_SPEC_SIFS", REG_MAC_SPEC_SIFS),
        ("REG_SIFS_CTX", REG_SIFS_CTX),
        ("REG_SIFS_TRX", REG_SIFS_TRX),
    ] {
        queue_write16_step(
            registers,
            counters,
            steps,
            QueueWrite16Spec {
                phase: "edca",
                register_name,
                address,
                value: 0x100a,
                verify_mask: u16::MAX,
                verify_value: 0x100a,
                description: "set EDCA SIFS timing",
                source: POWER_SOURCE_USB_HALINIT,
            },
        )?;
    }
    for (register_name, address, value) in [
        ("REG_EDCA_BE_PARAM", REG_EDCA_BE_PARAM, 0x005e_a42b),
        ("REG_EDCA_BK_PARAM", REG_EDCA_BK_PARAM, 0x0000_a44f),
        ("REG_EDCA_VI_PARAM", REG_EDCA_VI_PARAM, 0x005e_a324),
        ("REG_EDCA_VO_PARAM", REG_EDCA_VO_PARAM, 0x002f_a226),
    ] {
        queue_write32_step(
            registers,
            counters,
            steps,
            QueueWrite32Spec {
                phase: "edca",
                register_name,
                address,
                value,
                verify_mask: u32::MAX,
                verify_value: value,
                description: "set EDCA TXOP and contention parameters",
                source: POWER_SOURCE_USB_HALINIT,
            },
        )?;
    }
    for (register_name, address) in [
        ("REG_USTIME_TSF", REG_USTIME_TSF),
        ("REG_USTIME_EDCA", REG_USTIME_EDCA),
    ] {
        queue_write8_step(
            registers,
            counters,
            steps,
            QueueWrite8Spec {
                phase: "edca",
                register_name,
                address,
                value: 0x50,
                verify_mask: u8::MAX,
                verify_value: 0x50,
                description: "set 80 MHz-clock microsecond timing",
                source: POWER_SOURCE_USB_HALINIT,
            },
        )?;
    }
    mac_rmw8_step(
        registers,
        counters,
        steps,
        MacRmw8Spec {
            phase: "retry_function",
            register_name: "REG_FWHW_TXQ_CTRL",
            address: REG_FWHW_TXQ_CTRL,
            preserve_mask: !EN_AMPDU_RTY_NEW,
            value_mask: EN_AMPDU_RTY_NEW,
            value: EN_AMPDU_RTY_NEW,
            description: "enable new AMPDU retry behavior",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write8_step(
        registers,
        counters,
        steps,
        QueueWrite8Spec {
            phase: "retry_function",
            register_name: "REG_ACKTO",
            address: REG_ACKTO,
            value: 0x80,
            verify_mask: u8::MAX,
            verify_value: 0x80,
            description: "set MAC ACK timeout",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write8_step(
        registers,
        counters,
        steps,
        QueueWrite8Spec {
            phase: "hw_sequence",
            register_name: "REG_HWSEQ_CTRL",
            address: REG_HWSEQ_CTRL,
            value: 0xff,
            verify_mask: u8::MAX,
            verify_value: 0xff,
            description: "enable hardware sequence number control",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    queue_write32_step(
        registers,
        counters,
        steps,
        QueueWrite32Spec {
            phase: "bar_control",
            register_name: "REG_BAR_MODE_CTRL",
            address: REG_BAR_MODE_CTRL,
            value: BAR_MODE_CTRL_VALUE,
            verify_mask: BAR_MODE_CTRL_READBACK_MASK,
            verify_value: BAR_MODE_CTRL_VALUE,
            description: "disable BAR mode as in upstream init",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    mac_rmw8_step(
        registers,
        counters,
        steps,
        MacRmw8Spec {
            phase: "mac_enable",
            register_name: "REG_CR",
            address: REG_CR,
            preserve_mask: !MAC_TX_RX_ENABLE_MASK,
            value_mask: MAC_TX_RX_ENABLE_MASK,
            value: MAC_TX_RX_ENABLE_MASK,
            description: "enable MAC TX and RX blocks after queue and boundary setup",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )
}

fn mac_preflight<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let cr = read16_with_counter(registers, counters, REG_CR).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_CR preflight read failed: {error}"),
        }
    })?;
    let cr_passed = (cr & CR_ENABLE_BITS) == CR_ENABLE_BITS;
    steps.push(queue_read_report16(QueueRead16Spec {
        phase: "preflight",
        description: "verify command register block enables before MAC programming",
        register_name: "REG_CR",
        address: REG_CR,
        value: cr,
        mask: Some(CR_ENABLE_BITS),
        expected: Some(CR_ENABLE_BITS),
        passed: cr_passed,
    }));
    if !cr_passed {
        return Err(DiagnosticErrorReport {
            code: "mac_not_powered_on",
            message: format!(
                "REG_CR expected block-enable mask {} to be set before MAC programming, got {}",
                format_value(CR_ENABLE_BITS, 4),
                format_value(cr, 4)
            ),
        });
    }

    let mcu = read8_with_counter(registers, counters, REG_MCUFWDL).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_MCUFWDL preflight read failed: {error}"),
        }
    })?;
    let firmware_ready_mask = RAM_DL_SEL | BIT6 | BIT1;
    let firmware_passed = (mcu & firmware_ready_mask) == firmware_ready_mask;
    steps.push(queue_read_report8(QueueRead8Spec {
        phase: "preflight",
        description: "verify firmware is running before MAC programming",
        register_name: "REG_MCUFWDL",
        address: REG_MCUFWDL,
        value: mcu,
        mask: Some(firmware_ready_mask),
        expected: Some(firmware_ready_mask),
        passed: firmware_passed,
    }));
    if !firmware_passed {
        return Err(DiagnosticErrorReport {
            code: "firmware_not_ready",
            message: format!(
                "REG_MCUFWDL expected firmware-ready mask {} before MAC programming, got {}",
                format_value(firmware_ready_mask, 2),
                format_value(mcu, 2)
            ),
        });
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct MacRmw8Spec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    preserve_mask: u8,
    value_mask: u8,
    value: u8,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct MacRmw32Spec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    preserve_mask: u32,
    value_mask: u32,
    value: u32,
    description: &'static str,
    source: &'static str,
}

fn mac_rmw8_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    spec: MacRmw8Spec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    let written = (before & spec.preserve_mask) | (spec.value & spec.value_mask);
    write8_with_counter(registers, counters, spec.address, written).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.value & spec.value_mask;
    let passed = (after & spec.value_mask) == expected;
    steps.push(QueueDmaStepReport {
        phase: spec.phase,
        operation: "rmw8",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u8",
        mask_hex: Some(format_value(spec.value_mask, 2)),
        value_hex: Some(format_value(spec.value, 2)),
        before_hex: Some(format_value(before, 2)),
        written_hex: Some(format_value(written, 2)),
        after_hex: Some(format_value(after, 2)),
        expected_hex: Some(format_value(expected, 2)),
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(queue_readback_error(
            spec.register_name,
            format_value(spec.value_mask, 2),
            format_value(expected, 2),
            format_value(after & spec.value_mask, 2),
        ))
    }
}

fn mac_rmw32_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    spec: MacRmw32Spec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read32_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    let written = (before & spec.preserve_mask) | (spec.value & spec.value_mask);
    write32_with_counter(registers, counters, spec.address, written).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read32_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.value & spec.value_mask;
    let passed = (after & spec.value_mask) == expected;
    steps.push(QueueDmaStepReport {
        phase: spec.phase,
        operation: "rmw32",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u32",
        mask_hex: Some(format_value(spec.value_mask, 8)),
        value_hex: Some(format_value(spec.value, 8)),
        before_hex: Some(format_value(before, 8)),
        written_hex: Some(format_value(written, 8)),
        after_hex: Some(format_value(after, 8)),
        expected_hex: Some(format_value(expected, 8)),
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(queue_readback_error(
            spec.register_name,
            format_value(spec.value_mask, 8),
            format_value(expected, 8),
            format_value(after & spec.value_mask, 8),
        ))
    }
}

fn bb_smoke_report(args: BbSmokeArgs) -> BbSmokeReport {
    let selector = args.adapter.selector();
    let condition_env = args.condition_env();
    let mut setup_steps = Vec::new();
    let mut counters = DiagnosticCounters::default();
    let mut stats = BbSmokeStats::default();

    let (phy_plan, agc_plan) = match load_bb_table_plans(&args.bb_source, condition_env) {
        Ok(plans) => plans,
        Err(error) => {
            return bb_smoke_failure(
                &args,
                BbSmokeFailureInput {
                    condition_env,
                    adapter: None,
                    endpoints: None,
                    setup_steps,
                    phy_plan: None,
                    agc_plan: None,
                    stats,
                    counters,
                    error,
                },
            );
        }
    };

    if !args.i_understand_this_writes_registers {
        return bb_smoke_failure(
            &args,
            BbSmokeFailureInput {
                condition_env,
                adapter: None,
                endpoints: None,
                setup_steps,
                phy_plan: Some(phy_plan),
                agc_plan: Some(agc_plan),
                stats,
                counters,
                error: DiagnosticErrorReport {
                    code: "missing_write_authorization",
                    message: "BB smoke writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
                },
            },
        );
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return bb_smoke_failure(
                &args,
                BbSmokeFailureInput {
                    condition_env,
                    adapter: None,
                    endpoints: None,
                    setup_steps,
                    phy_plan: Some(phy_plan),
                    agc_plan: Some(agc_plan),
                    stats,
                    counters,
                    error,
                },
            );
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return bb_smoke_failure(
                &args,
                BbSmokeFailureInput {
                    condition_env,
                    adapter: Some(selected),
                    endpoints: None,
                    setup_steps,
                    phy_plan: Some(phy_plan),
                    agc_plan: Some(agc_plan),
                    stats,
                    counters,
                    error: DiagnosticErrorReport {
                        code: "usb_claim_failed",
                        message: error.to_string(),
                    },
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let registers =
        Rtl8812auRegisterAccess::new(&claimed).with_timeout(Duration::from_millis(args.timeout_ms));

    if let Err(error) = run_bb_sequence(
        &registers,
        &args,
        &phy_plan,
        &agc_plan,
        &mut counters,
        &mut setup_steps,
        &mut stats,
    ) {
        return bb_smoke_failure(
            &args,
            BbSmokeFailureInput {
                condition_env,
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                setup_steps,
                phy_plan: Some(phy_plan),
                agc_plan: Some(agc_plan),
                stats,
                counters,
                error,
            },
        );
    }

    BbSmokeReport {
        schema_version: 1,
        command: "bb-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        bb_source: args.bb_source,
        condition_env,
        crystal_cap_hex: format_value(args.crystal_cap, 2),
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        setup_steps,
        phy_plan: Some(phy_plan),
        agc_plan: Some(agc_plan),
        phy_writes_applied: stats.phy_writes_applied,
        agc_writes_applied: stats.agc_writes_applied,
        delays_applied: stats.delays_applied,
        counters,
        error: None,
        notes: vec![
            "guarded BB smoke test: RTL8812A PHY_REG and AGC_TAB tables were parsed from external Realtek source and written through vendor control transfers",
            "no RF radio table programming, channel tuning, bulk traffic, RX loop, or TX operation was issued",
        ],
    }
}

fn bb_smoke_failure(args: &BbSmokeArgs, input: BbSmokeFailureInput) -> BbSmokeReport {
    BbSmokeReport {
        schema_version: 1,
        command: "bb-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector: args.adapter.selector(),
        bb_source: args.bb_source.clone(),
        condition_env: input.condition_env,
        crystal_cap_hex: format_value(args.crystal_cap, 2),
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Fail,
        adapter: input.adapter,
        endpoints: input.endpoints,
        setup_steps: input.setup_steps,
        phy_plan: input.phy_plan,
        agc_plan: input.agc_plan,
        phy_writes_applied: input.stats.phy_writes_applied,
        agc_writes_applied: input.stats.agc_writes_applied,
        delays_applied: input.stats.delays_applied,
        counters: input.counters,
        error: Some(input.error),
        notes: vec![
            "guarded BB smoke test stopped before RF radio table programming, channel tuning, bulk traffic, RX loop, or TX operation",
        ],
    }
}

fn load_bb_table_plans(
    source_path: &Path,
    condition_env: RealtekConditionEnv,
) -> std::result::Result<(RealtekTablePlan, RealtekTablePlan), DiagnosticErrorReport> {
    let source = fs::read_to_string(source_path).map_err(|error| DiagnosticErrorReport {
        code: "bb_source_read_failed",
        message: format!("failed to read {}: {error}", source_path.display()),
    })?;

    let phy_values =
        parse_realtek_u32_array(&source, BB_PHY_ARRAY).map_err(|error| DiagnosticErrorReport {
            code: "bb_phy_table_parse_failed",
            message: error.to_string(),
        })?;
    let agc_values =
        parse_realtek_u32_array(&source, BB_AGC_ARRAY).map_err(|error| DiagnosticErrorReport {
            code: "bb_agc_table_parse_failed",
            message: error.to_string(),
        })?;
    let phy_plan = plan_realtek_table(
        BB_PHY_ARRAY,
        RealtekTableKind::BbPhy,
        &phy_values,
        condition_env,
    )
    .map_err(|error| DiagnosticErrorReport {
        code: "bb_phy_table_plan_failed",
        message: error.to_string(),
    })?;
    let agc_plan = plan_realtek_table(
        BB_AGC_ARRAY,
        RealtekTableKind::BbAgc,
        &agc_values,
        condition_env,
    )
    .map_err(|error| DiagnosticErrorReport {
        code: "bb_agc_table_plan_failed",
        message: error.to_string(),
    })?;

    Ok((phy_plan, agc_plan))
}

fn run_bb_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    args: &BbSmokeArgs,
    phy_plan: &RealtekTablePlan,
    agc_plan: &RealtekTablePlan,
    counters: &mut DiagnosticCounters,
    setup_steps: &mut Vec<QueueDmaStepReport>,
    stats: &mut BbSmokeStats,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    bb_preflight(registers, counters, setup_steps)?;
    mac_rmw8_step(
        registers,
        counters,
        setup_steps,
        MacRmw8Spec {
            phase: "bb_power",
            register_name: "REG_SYS_FUNC_EN",
            address: REG_SYS_FUNC_EN,
            preserve_mask: !FEN_USBA,
            value_mask: FEN_USBA,
            value: FEN_USBA,
            description: "enable USB analog path before BB/RF configuration",
            source: BB_SOURCE_PHYCFG,
        },
    )?;
    mac_rmw8_step(
        registers,
        counters,
        setup_steps,
        MacRmw8Spec {
            phase: "bb_power",
            register_name: "REG_SYS_FUNC_EN",
            address: REG_SYS_FUNC_EN,
            preserve_mask: !(FEN_USBA | FEN_BB_GLB_RSTN | FEN_BBRSTB),
            value_mask: FEN_USBA | FEN_BB_GLB_RSTN | FEN_BBRSTB,
            value: FEN_USBA | FEN_BB_GLB_RSTN | FEN_BBRSTB,
            description: "release BB global reset and BB reset before table programming",
            source: BB_SOURCE_PHYCFG,
        },
    )?;
    queue_write8_step(
        registers,
        counters,
        setup_steps,
        QueueWrite8Spec {
            phase: "bb_power",
            register_name: "REG_RF_CTRL",
            address: REG_RF_CTRL,
            value: 0x07,
            verify_mask: u8::MAX,
            verify_value: 0x07,
            description: "power on RF path A gate before BB table programming",
            source: BB_SOURCE_PHYCFG,
        },
    )?;
    queue_write8_step(
        registers,
        counters,
        setup_steps,
        QueueWrite8Spec {
            phase: "bb_power",
            register_name: "REG_RF_B_CTRL_8812",
            address: REG_RF_B_CTRL_8812,
            value: 0x07,
            verify_mask: u8::MAX,
            verify_value: 0x07,
            description: "power on RF path B gate before BB table programming",
            source: BB_SOURCE_PHYCFG,
        },
    )?;

    run_bb_table_plan(registers, counters, stats, phy_plan)?;
    run_bb_table_plan(registers, counters, stats, agc_plan)?;
    bb_masked_write32_step(
        registers,
        counters,
        setup_steps,
        BbMaskedWrite32Spec {
            phase: "bb_crystal_cap",
            register_name: "REG_MAC_PHY_CTRL",
            address: REG_MAC_PHY_CTRL,
            bitmask: RTL8812_CRYSTAL_CAP_MASK,
            data: u32::from((args.crystal_cap & 0x3f) | ((args.crystal_cap & 0x3f) << 6)),
            description: "set RTL8812A crystal-cap bits after BB and AGC table programming",
            source: "aircrack-ng/rtl8812au@7344855:hal/hal_com.c",
        },
    )
}

fn bb_preflight<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let cr = read16_with_counter(registers, counters, REG_CR).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_CR preflight read failed: {error}"),
        }
    })?;
    let cr_passed = (cr & CR_ENABLE_BITS) == CR_ENABLE_BITS;
    steps.push(queue_read_report16(QueueRead16Spec {
        phase: "preflight",
        description: "verify command register block enables before BB programming",
        register_name: "REG_CR",
        address: REG_CR,
        value: cr,
        mask: Some(CR_ENABLE_BITS),
        expected: Some(CR_ENABLE_BITS),
        passed: cr_passed,
    }));
    if !cr_passed {
        return Err(DiagnosticErrorReport {
            code: "mac_not_powered_on",
            message: format!(
                "REG_CR expected block-enable mask {} to be set before BB programming, got {}",
                format_value(CR_ENABLE_BITS, 4),
                format_value(cr, 4)
            ),
        });
    }

    let mcu = read8_with_counter(registers, counters, REG_MCUFWDL).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_MCUFWDL preflight read failed: {error}"),
        }
    })?;
    let firmware_ready_mask = RAM_DL_SEL | BIT6 | BIT1;
    let firmware_passed = (mcu & firmware_ready_mask) == firmware_ready_mask;
    steps.push(queue_read_report8(QueueRead8Spec {
        phase: "preflight",
        description: "verify firmware is running before BB programming",
        register_name: "REG_MCUFWDL",
        address: REG_MCUFWDL,
        value: mcu,
        mask: Some(firmware_ready_mask),
        expected: Some(firmware_ready_mask),
        passed: firmware_passed,
    }));
    if !firmware_passed {
        return Err(DiagnosticErrorReport {
            code: "firmware_not_ready",
            message: format!(
                "REG_MCUFWDL expected firmware-ready mask {} before BB programming, got {}",
                format_value(firmware_ready_mask, 2),
                format_value(mcu, 2)
            ),
        });
    }

    Ok(())
}

fn run_bb_table_plan<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    stats: &mut BbSmokeStats,
    plan: &RealtekTablePlan,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    for action in &plan.actions {
        match action.kind {
            RealtekTableActionKind::Delay => {
                let delay_us = action.delay_us.unwrap_or_default();
                if delay_us > 0 {
                    std::thread::sleep(Duration::from_micros(delay_us));
                }
                stats.delays_applied += 1;
            }
            RealtekTableActionKind::Write => {
                let address = u16::try_from(action.address).map_err(|_| DiagnosticErrorReport {
                    code: "bb_table_address_out_of_range",
                    message: format!(
                        "{} pair {} address {} does not fit a USB register address",
                        plan.array_name, action.pair_index, action.address_hex
                    ),
                })?;
                let bitmask = action.bitmask.unwrap_or(u32::MAX);
                let data = action.data.ok_or_else(|| DiagnosticErrorReport {
                    code: "bb_table_write_missing_data",
                    message: format!(
                        "{} pair {} is a write action with no data",
                        plan.array_name, action.pair_index
                    ),
                })?;
                bb_set_bb_reg(registers, counters, address, bitmask, data).map_err(|error| {
                    DiagnosticErrorReport {
                        code: "bb_table_write_failed",
                        message: format!(
                            "{} pair {} write addr={} data={} failed: {error}",
                            plan.array_name,
                            action.pair_index,
                            action.address_hex,
                            action.data_hex.as_deref().unwrap_or("<missing>")
                        ),
                    }
                })?;
                match plan.kind {
                    RealtekTableKind::BbPhy => stats.phy_writes_applied += 1,
                    RealtekTableKind::BbAgc => stats.agc_writes_applied += 1,
                    RealtekTableKind::RfRadioA | RealtekTableKind::RfRadioB => {}
                }
                std::thread::sleep(Duration::from_micros(1));
            }
        }
    }
    Ok(())
}

fn bb_set_bb_reg<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
    bitmask: u32,
    data: u32,
) -> std::result::Result<(), radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    if bitmask == u32::MAX {
        return write32_with_counter(registers, counters, address, data);
    }

    if bitmask == 0 {
        return Ok(());
    }

    let original = read32_with_counter(registers, counters, address)?;
    let bitshift = bitmask.trailing_zeros();
    let written = (original & !bitmask) | ((data << bitshift) & bitmask);
    write32_with_counter(registers, counters, address, written)
}

#[derive(Debug, Clone, Copy)]
struct BbMaskedWrite32Spec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    bitmask: u32,
    data: u32,
    description: &'static str,
    source: &'static str,
}

fn bb_masked_write32_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    spec: BbMaskedWrite32Spec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read32_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    let bitshift = spec.bitmask.trailing_zeros();
    let written = (before & !spec.bitmask) | ((spec.data << bitshift) & spec.bitmask);
    write32_with_counter(registers, counters, spec.address, written).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read32_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = (spec.data << bitshift) & spec.bitmask;
    let passed = (after & spec.bitmask) == expected;
    steps.push(QueueDmaStepReport {
        phase: spec.phase,
        operation: "masked_write32",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u32",
        mask_hex: Some(format_value(spec.bitmask, 8)),
        value_hex: Some(format_value(spec.data, 8)),
        before_hex: Some(format_value(before, 8)),
        written_hex: Some(format_value(written, 8)),
        after_hex: Some(format_value(after, 8)),
        expected_hex: Some(format_value(expected, 8)),
        passed,
    });

    if passed {
        Ok(())
    } else {
        Err(queue_readback_error(
            spec.register_name,
            format_value(spec.bitmask, 8),
            format_value(expected, 8),
            format_value(after & spec.bitmask, 8),
        ))
    }
}

fn rf_smoke_report(args: RfSmokeArgs) -> RfSmokeReport {
    let selector = args.adapter.selector();
    let condition_env = args.condition_env();
    let mut setup_steps = Vec::new();
    let mut counters = DiagnosticCounters::default();
    let mut stats = RfSmokeStats::default();

    let (radioa_plan, radiob_plan) = match load_rf_table_plans(&args.rf_source, condition_env) {
        Ok(plans) => plans,
        Err(error) => {
            return rf_smoke_failure(
                &args,
                RfSmokeFailureInput {
                    condition_env,
                    adapter: None,
                    endpoints: None,
                    setup_steps,
                    radioa_plan: None,
                    radiob_plan: None,
                    stats,
                    counters,
                    error,
                },
            );
        }
    };

    if !args.i_understand_this_writes_registers {
        return rf_smoke_failure(
            &args,
            RfSmokeFailureInput {
                condition_env,
                adapter: None,
                endpoints: None,
                setup_steps,
                radioa_plan: Some(radioa_plan),
                radiob_plan: Some(radiob_plan),
                stats,
                counters,
                error: DiagnosticErrorReport {
                    code: "missing_write_authorization",
                    message: "RF smoke writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
                },
            },
        );
    }

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return rf_smoke_failure(
                &args,
                RfSmokeFailureInput {
                    condition_env,
                    adapter: None,
                    endpoints: None,
                    setup_steps,
                    radioa_plan: Some(radioa_plan),
                    radiob_plan: Some(radiob_plan),
                    stats,
                    counters,
                    error,
                },
            );
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return rf_smoke_failure(
                &args,
                RfSmokeFailureInput {
                    condition_env,
                    adapter: Some(selected),
                    endpoints: None,
                    setup_steps,
                    radioa_plan: Some(radioa_plan),
                    radiob_plan: Some(radiob_plan),
                    stats,
                    counters,
                    error: DiagnosticErrorReport {
                        code: "usb_claim_failed",
                        message: error.to_string(),
                    },
                },
            );
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let registers =
        Rtl8812auRegisterAccess::new(&claimed).with_timeout(Duration::from_millis(args.timeout_ms));

    if let Err(error) = run_rf_sequence(
        &registers,
        &radioa_plan,
        &radiob_plan,
        &mut counters,
        &mut setup_steps,
        &mut stats,
    ) {
        return rf_smoke_failure(
            &args,
            RfSmokeFailureInput {
                condition_env,
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                setup_steps,
                radioa_plan: Some(radioa_plan),
                radiob_plan: Some(radiob_plan),
                stats,
                counters,
                error,
            },
        );
    }

    RfSmokeReport {
        schema_version: 1,
        command: "rf-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector,
        rf_source: args.rf_source,
        condition_env,
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Pass,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        setup_steps,
        radioa_plan: Some(radioa_plan),
        radiob_plan: Some(radiob_plan),
        radioa_writes_applied: stats.radioa_writes_applied,
        radiob_writes_applied: stats.radiob_writes_applied,
        delays_applied: stats.delays_applied,
        counters,
        error: None,
        notes: vec![
            "guarded RF smoke test: RTL8812A radioA/radioB tables were parsed from external Realtek source and written through RF 3-wire BB registers",
            "no channel tuning, bulk traffic, RX loop, or TX operation was issued",
        ],
    }
}

fn rf_smoke_failure(args: &RfSmokeArgs, input: RfSmokeFailureInput) -> RfSmokeReport {
    RfSmokeReport {
        schema_version: 1,
        command: "rf-smoke",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector: args.adapter.selector(),
        rf_source: args.rf_source.clone(),
        condition_env: input.condition_env,
        timeout_ms: args.timeout_ms,
        result: DiagnosticResult::Fail,
        adapter: input.adapter,
        endpoints: input.endpoints,
        setup_steps: input.setup_steps,
        radioa_plan: input.radioa_plan,
        radiob_plan: input.radiob_plan,
        radioa_writes_applied: input.stats.radioa_writes_applied,
        radiob_writes_applied: input.stats.radiob_writes_applied,
        delays_applied: input.stats.delays_applied,
        counters: input.counters,
        error: Some(input.error),
        notes: vec![
            "guarded RF smoke test stopped before channel tuning, bulk traffic, RX loop, or TX operation",
        ],
    }
}

fn load_rf_table_plans(
    source_path: &Path,
    condition_env: RealtekConditionEnv,
) -> std::result::Result<(RealtekTablePlan, RealtekTablePlan), DiagnosticErrorReport> {
    let source = fs::read_to_string(source_path).map_err(|error| DiagnosticErrorReport {
        code: "rf_source_read_failed",
        message: format!("failed to read {}: {error}", source_path.display()),
    })?;

    let radioa_values = parse_realtek_u32_array(&source, RF_RADIOA_ARRAY).map_err(|error| {
        DiagnosticErrorReport {
            code: "rf_radioa_table_parse_failed",
            message: error.to_string(),
        }
    })?;
    let radiob_values = parse_realtek_u32_array(&source, RF_RADIOB_ARRAY).map_err(|error| {
        DiagnosticErrorReport {
            code: "rf_radiob_table_parse_failed",
            message: error.to_string(),
        }
    })?;
    let radioa_plan = plan_realtek_table(
        RF_RADIOA_ARRAY,
        RealtekTableKind::RfRadioA,
        &radioa_values,
        condition_env,
    )
    .map_err(|error| DiagnosticErrorReport {
        code: "rf_radioa_table_plan_failed",
        message: error.to_string(),
    })?;
    let radiob_plan = plan_realtek_table(
        RF_RADIOB_ARRAY,
        RealtekTableKind::RfRadioB,
        &radiob_values,
        condition_env,
    )
    .map_err(|error| DiagnosticErrorReport {
        code: "rf_radiob_table_plan_failed",
        message: error.to_string(),
    })?;

    Ok((radioa_plan, radiob_plan))
}

fn run_rf_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    radioa_plan: &RealtekTablePlan,
    radiob_plan: &RealtekTablePlan,
    counters: &mut DiagnosticCounters,
    setup_steps: &mut Vec<QueueDmaStepReport>,
    stats: &mut RfSmokeStats,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    rf_preflight(registers, counters, setup_steps)?;
    run_rf_table_plan(registers, counters, stats, radioa_plan)?;
    run_rf_table_plan(registers, counters, stats, radiob_plan)
}

fn rf_preflight<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    bb_preflight(registers, counters, steps)?;
    let sys = read8_with_counter(registers, counters, REG_SYS_FUNC_EN).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_SYS_FUNC_EN RF preflight read failed: {error}"),
        }
    })?;
    let expected = FEN_USBA | FEN_BB_GLB_RSTN | FEN_BBRSTB;
    let passed = (sys & expected) == expected;
    steps.push(QueueDmaStepReport {
        phase: "preflight",
        operation: "read8",
        description: "verify BB reset and USB analog path are enabled before RF 3-wire writes",
        source: RF_SOURCE_PHYCFG,
        register_name: "REG_SYS_FUNC_EN",
        address: REG_SYS_FUNC_EN,
        address_hex: format_address(REG_SYS_FUNC_EN),
        width: "u8",
        mask_hex: Some(format_value(expected, 2)),
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(sys, 2)),
        expected_hex: Some(format_value(expected, 2)),
        passed,
    });
    if !passed {
        return Err(DiagnosticErrorReport {
            code: "bb_not_ready",
            message: format!(
                "REG_SYS_FUNC_EN expected BB-ready mask {} before RF programming, got {}",
                format_value(expected, 2),
                format_value(sys, 2)
            ),
        });
    }
    Ok(())
}

fn run_rf_table_plan<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    stats: &mut RfSmokeStats,
    plan: &RealtekTablePlan,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let rf_3wire_register = match plan.kind {
        RealtekTableKind::RfRadioA => REG_RF_PATH_A_3WIRE,
        RealtekTableKind::RfRadioB => REG_RF_PATH_B_3WIRE,
        other => {
            return Err(DiagnosticErrorReport {
                code: "invalid_rf_table_kind",
                message: format!("{} has non-RF table kind {other:?}", plan.array_name),
            });
        }
    };

    for action in &plan.actions {
        match action.kind {
            RealtekTableActionKind::Delay => {
                let delay_us = action.delay_us.unwrap_or_default();
                if delay_us > 0 {
                    std::thread::sleep(Duration::from_micros(delay_us));
                }
                stats.delays_applied += 1;
            }
            RealtekTableActionKind::Write => {
                let data = action.data.ok_or_else(|| DiagnosticErrorReport {
                    code: "rf_table_write_missing_data",
                    message: format!(
                        "{} pair {} is a write action with no data",
                        plan.array_name, action.pair_index
                    ),
                })?;
                let encoded = encode_rf_serial_write(action.address, data);
                write32_with_counter(registers, counters, rf_3wire_register, encoded).map_err(
                    |error| DiagnosticErrorReport {
                        code: "rf_table_write_failed",
                        message: format!(
                            "{} pair {} RF addr={} data={} via {} failed: {error}",
                            plan.array_name,
                            action.pair_index,
                            action.address_hex,
                            action.data_hex.as_deref().unwrap_or("<missing>"),
                            format_address(rf_3wire_register)
                        ),
                    },
                )?;
                match plan.kind {
                    RealtekTableKind::RfRadioA => stats.radioa_writes_applied += 1,
                    RealtekTableKind::RfRadioB => stats.radiob_writes_applied += 1,
                    RealtekTableKind::BbPhy | RealtekTableKind::BbAgc => {}
                }
                std::thread::sleep(Duration::from_micros(1));
            }
        }
    }

    Ok(())
}

fn encode_rf_serial_write(rf_offset: u32, data: u32) -> u32 {
    (((rf_offset & 0xff) << 20) | (data & 0x000f_ffff)) & 0x0fff_ffff
}

fn run_channel_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    channel: Channel,
    bandwidth: Bandwidth,
    radioa_plan: &RealtekTablePlan,
    radiob_plan: &RealtekTablePlan,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let programming_channel_number = channel_programming_number(channel, bandwidth)?;
    let data_sc = data_secondary_channel_setting(channel, bandwidth)?;

    rf_preflight(registers, counters, steps)?;

    let mut rf_path_a = last_rf_register_data(radioa_plan, RF_CHNLBW_JAGUAR)?;
    let mut rf_path_b = last_rf_register_data(radiob_plan, RF_CHNLBW_JAGUAR)?;

    switch_wireless_band_8812(registers, counters, steps, channel.band)?;

    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "channel",
            register_name: "rFc_area_Jaguar",
            address: REG_FC_AREA_JAGUAR,
            bitmask: 0x1ffe_0000,
            data: fc_area_data(programming_channel_number),
            description: "program RTL8812A fc_area for the selected channel group",
            source: RF_SOURCE_PHYCFG,
        },
    )?;

    rf_path_a = apply_rf_mask(
        rf_path_a,
        RF_CHNLBW_MOD_AG_MASK,
        rf_mod_ag_data(programming_channel_number),
    );
    rf_serial_write_step(
        registers,
        counters,
        steps,
        RfSerialWriteSpec {
            phase: "channel",
            path: "A",
            bb_register_name: "rA_LSSIWrite_Jaguar",
            bb_register: REG_RF_PATH_A_3WIRE,
            rf_register_name: "RF_CHNLBW_Jaguar",
            rf_offset: RF_CHNLBW_JAGUAR,
            value: rf_path_a,
            description: "set RF path A MOD_AG bits for the selected channel group",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    fix_spur_8812a(
        registers,
        counters,
        steps,
        programming_channel_number,
        bandwidth,
    )?;
    rf_path_a = apply_rf_mask(
        rf_path_a,
        RF_CHNLBW_CHANNEL_MASK,
        u32::from(programming_channel_number),
    );
    rf_serial_write_step(
        registers,
        counters,
        steps,
        RfSerialWriteSpec {
            phase: "channel",
            path: "A",
            bb_register_name: "rA_LSSIWrite_Jaguar",
            bb_register: REG_RF_PATH_A_3WIRE,
            rf_register_name: "RF_CHNLBW_Jaguar",
            rf_offset: RF_CHNLBW_JAGUAR,
            value: rf_path_a,
            description: "set RF path A channel byte",
            source: RF_SOURCE_PHYCFG,
        },
    )?;

    rf_path_b = apply_rf_mask(
        rf_path_b,
        RF_CHNLBW_MOD_AG_MASK,
        rf_mod_ag_data(programming_channel_number),
    );
    rf_serial_write_step(
        registers,
        counters,
        steps,
        RfSerialWriteSpec {
            phase: "channel",
            path: "B",
            bb_register_name: "rB_LSSIWrite_Jaguar",
            bb_register: REG_RF_PATH_B_3WIRE,
            rf_register_name: "RF_CHNLBW_Jaguar",
            rf_offset: RF_CHNLBW_JAGUAR,
            value: rf_path_b,
            description: "set RF path B MOD_AG bits for the selected channel group",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    fix_spur_8812a(
        registers,
        counters,
        steps,
        programming_channel_number,
        bandwidth,
    )?;
    rf_path_b = apply_rf_mask(
        rf_path_b,
        RF_CHNLBW_CHANNEL_MASK,
        u32::from(programming_channel_number),
    );
    rf_serial_write_step(
        registers,
        counters,
        steps,
        RfSerialWriteSpec {
            phase: "channel",
            path: "B",
            bb_register_name: "rB_LSSIWrite_Jaguar",
            bb_register: REG_RF_PATH_B_3WIRE,
            rf_register_name: "RF_CHNLBW_Jaguar",
            rf_offset: RF_CHNLBW_JAGUAR,
            value: rf_path_b,
            description: "set RF path B channel byte",
            source: RF_SOURCE_PHYCFG,
        },
    )?;

    let wmac_bandwidth_bits = match bandwidth {
        Bandwidth::Mhz20 => 0x0000,
        Bandwidth::Mhz40 => 0x0080,
        Bandwidth::Mhz80 => 0x0100,
    };

    queue_rmw16_step(
        registers,
        counters,
        steps,
        QueueRmw16Spec {
            phase: "bandwidth",
            register_name: "REG_WMAC_TRXPTCL_CTL",
            address: REG_WMAC_TRXPTCL_CTL,
            preserve_mask: 0xfe7f,
            value_mask: 0x0180,
            value: wmac_bandwidth_bits,
            description: match bandwidth {
                Bandwidth::Mhz20 => "clear WMAC 40/80 MHz bandwidth bits for 20 MHz operation",
                Bandwidth::Mhz40 => "set WMAC 40 MHz bandwidth bit and clear 80 MHz bit",
                Bandwidth::Mhz80 => "set WMAC 80 MHz bandwidth bit and clear 40 MHz bit",
            },
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    queue_write8_step(
        registers,
        counters,
        steps,
        QueueWrite8Spec {
            phase: "bandwidth",
            register_name: "REG_DATA_SC_8812",
            address: REG_DATA_SC_8812,
            value: data_sc,
            verify_mask: u8::MAX,
            verify_value: data_sc,
            description: match bandwidth {
                Bandwidth::Mhz20 => "clear secondary-channel data mapping for 20 MHz operation",
                Bandwidth::Mhz40 => {
                    "program primary 20 MHz subchannel mapping for 40 MHz operation"
                }
                Bandwidth::Mhz80 => {
                    "program primary 40/20 MHz subchannel mapping for 80 MHz operation"
                }
            },
            source: RF_SOURCE_PHYCFG,
        },
    )?;

    let bw_indication = read8_with_counter(registers, counters, REG_BW_INDICATION_JAGUAR + 3)
        .map_err(|error| DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("rBWIndication_Jaguar + 3 read failed: {error}"),
        })?;
    steps.push(QueueDmaStepReport {
        phase: "bandwidth",
        operation: "read8",
        description: "sample rBWIndication_Jaguar + 3, matching upstream post-bandwidth flow",
        source: RF_SOURCE_PHYCFG,
        register_name: "rBWIndication_Jaguar + 3",
        address: REG_BW_INDICATION_JAGUAR + 3,
        address_hex: format_address(REG_BW_INDICATION_JAGUAR + 3),
        width: "u8",
        mask_hex: None,
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(bw_indication, 2)),
        expected_hex: None,
        passed: true,
    });

    let rf_mode_data = match bandwidth {
        Bandwidth::Mhz20 => 0x0030_0200,
        Bandwidth::Mhz40 => 0x0030_0201,
        Bandwidth::Mhz80 => 0x0030_0202,
    };
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "bandwidth",
            register_name: "rRFMOD_Jaguar",
            address: REG_RF_MOD_JAGUAR,
            bitmask: 0x0030_03c3,
            data: rf_mode_data,
            description: match bandwidth {
                Bandwidth::Mhz20 => "program BB RF mode fields for 20 MHz operation",
                Bandwidth::Mhz40 => "program BB RF mode fields for 40 MHz operation",
                Bandwidth::Mhz80 => "program BB RF mode fields for 80 MHz operation",
            },
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "bandwidth",
            register_name: "rADC_Buf_Clk_Jaguar",
            address: REG_ADC_BUF_CLK_JAGUAR,
            bitmask: 1 << 30,
            data: if bandwidth == Bandwidth::Mhz80 { 1 } else { 0 },
            description: match bandwidth {
                Bandwidth::Mhz20 => "select the default ADC buffer clock for 20 MHz operation",
                Bandwidth::Mhz40 => "select the default ADC buffer clock for 40 MHz operation",
                Bandwidth::Mhz80 => "select the 80 MHz ADC buffer clock",
            },
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    if matches!(bandwidth, Bandwidth::Mhz40 | Bandwidth::Mhz80) {
        bb_masked_write32_step(
            registers,
            counters,
            steps,
            BbMaskedWrite32Spec {
                phase: "bandwidth",
                register_name: "rRFMOD_Jaguar",
                address: REG_RF_MOD_JAGUAR,
                bitmask: 0x0000_003c,
                data: u32::from(data_sc),
                description: match bandwidth {
                    Bandwidth::Mhz40 => "mirror 40 MHz primary subchannel mapping into BB RF mode",
                    Bandwidth::Mhz80 => "mirror 80 MHz primary subchannel mapping into BB RF mode",
                    Bandwidth::Mhz20 => unreachable!("guarded by bandwidth match"),
                },
                source: RF_SOURCE_PHYCFG,
            },
        )?;
        bb_masked_write32_step(
            registers,
            counters,
            steps,
            BbMaskedWrite32Spec {
                phase: "bandwidth",
                register_name: "rCCAonSec_Jaguar",
                address: REG_CCA_ON_SEC_JAGUAR,
                bitmask: 0xf000_0000,
                data: u32::from(data_sc),
                description: match bandwidth {
                    Bandwidth::Mhz40 => "program CCA-on-secondary mapping for 40 MHz operation",
                    Bandwidth::Mhz80 => "program CCA-on-secondary mapping for 80 MHz operation",
                    Bandwidth::Mhz20 => unreachable!("guarded by bandwidth match"),
                },
                source: RF_SOURCE_PHYCFG,
            },
        )?;
    }

    let l1_peak = match bandwidth {
        Bandwidth::Mhz20 => 7,
        Bandwidth::Mhz40 if (bw_indication & BIT2) != 0 => 6,
        Bandwidth::Mhz40 => 7,
        Bandwidth::Mhz80 if (bw_indication & BIT2) != 0 => 5,
        Bandwidth::Mhz80 => 6,
    };
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "bandwidth",
            register_name: "rL1PeakTH_Jaguar",
            address: REG_L1_PEAK_TH_JAGUAR,
            bitmask: 0x03c0_0000,
            data: l1_peak,
            description: match bandwidth {
                Bandwidth::Mhz20 => "set 2T2R L1 peak threshold for 20 MHz operation",
                Bandwidth::Mhz40 => "set 2T2R L1 peak threshold for 40 MHz operation",
                Bandwidth::Mhz80 => "set 2T2R L1 peak threshold for 80 MHz operation",
            },
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    if bandwidth == Bandwidth::Mhz40 {
        bb_masked_write32_step(
            registers,
            counters,
            steps,
            BbMaskedWrite32Spec {
                phase: "bandwidth",
                register_name: "rCCK_System_Jaguar",
                address: REG_CCK_SYSTEM_JAGUAR,
                bitmask: 0x10,
                data: if data_sc == VHT_DATA_SC_20_UPPER_OF_80MHZ {
                    1
                } else {
                    0
                },
                description: "program CCK sideband selection for 40 MHz operation",
                source: RF_SOURCE_PHYCFG,
            },
        )?;
    }
    fix_spur_8812a(
        registers,
        counters,
        steps,
        programming_channel_number,
        bandwidth,
    )?;

    let rf_bandwidth_bits = match bandwidth {
        Bandwidth::Mhz20 => 3,
        Bandwidth::Mhz40 => 1,
        Bandwidth::Mhz80 => 0,
    };
    rf_path_a = apply_rf_mask(rf_path_a, RF_CHNLBW_BW_MASK, rf_bandwidth_bits);
    rf_serial_write_step(
        registers,
        counters,
        steps,
        RfSerialWriteSpec {
            phase: "bandwidth",
            path: "A",
            bb_register_name: "rA_LSSIWrite_Jaguar",
            bb_register: REG_RF_PATH_A_3WIRE,
            rf_register_name: "RF_CHNLBW_Jaguar",
            rf_offset: RF_CHNLBW_JAGUAR,
            value: rf_path_a,
            description: match bandwidth {
                Bandwidth::Mhz20 => "set RF path A bandwidth bits to 20 MHz",
                Bandwidth::Mhz40 => "set RF path A bandwidth bits to 40 MHz",
                Bandwidth::Mhz80 => "set RF path A bandwidth bits to 80 MHz",
            },
            source: RF_SOURCE_RF6052,
        },
    )?;
    rf_path_b = apply_rf_mask(rf_path_b, RF_CHNLBW_BW_MASK, rf_bandwidth_bits);
    rf_serial_write_step(
        registers,
        counters,
        steps,
        RfSerialWriteSpec {
            phase: "bandwidth",
            path: "B",
            bb_register_name: "rB_LSSIWrite_Jaguar",
            bb_register: REG_RF_PATH_B_3WIRE,
            rf_register_name: "RF_CHNLBW_Jaguar",
            rf_offset: RF_CHNLBW_JAGUAR,
            value: rf_path_b,
            description: match bandwidth {
                Bandwidth::Mhz20 => "set RF path B bandwidth bits to 20 MHz",
                Bandwidth::Mhz40 => "set RF path B bandwidth bits to 40 MHz",
                Bandwidth::Mhz80 => "set RF path B bandwidth bits to 80 MHz",
            },
            source: RF_SOURCE_RF6052,
        },
    )
}

fn data_secondary_channel_setting(
    channel: Channel,
    bandwidth: Bandwidth,
) -> std::result::Result<u8, DiagnosticErrorReport> {
    match bandwidth {
        Bandwidth::Mhz20 => Ok(0),
        Bandwidth::Mhz40 => match channel.band {
            Band::Ghz5 if channel.number % 8 == 4 => Ok(VHT_DATA_SC_20_LOWER_OF_80MHZ),
            Band::Ghz5 if channel.number % 8 == 0 => Ok(VHT_DATA_SC_20_UPPER_OF_80MHZ),
            Band::Ghz5 => Err(DiagnosticErrorReport {
                code: "channel_bandwidth_not_supported",
                message: format!(
                    "channel {} is not aligned to a 5 GHz 40 MHz channel pair",
                    channel.number
                ),
            }),
            Band::Ghz2 if (1..=7).contains(&channel.number) => Ok(VHT_DATA_SC_20_LOWER_OF_80MHZ),
            Band::Ghz2 if (8..=13).contains(&channel.number) => Ok(VHT_DATA_SC_20_UPPER_OF_80MHZ),
            Band::Ghz2 => Err(DiagnosticErrorReport {
                code: "channel_bandwidth_not_supported",
                message: format!(
                    "channel {} is not supported for 2.4 GHz 40 MHz operation",
                    channel.number
                ),
            }),
        },
        Bandwidth::Mhz80 => {
            let (_center, position) = eighty_mhz_center_and_position(channel.number)?;
            let (sc40, sc20) = match position {
                0 => (
                    VHT_DATA_SC_40_LOWER_OF_80MHZ,
                    VHT_DATA_SC_20_LOWEST_OF_80MHZ,
                ),
                1 => (VHT_DATA_SC_40_LOWER_OF_80MHZ, VHT_DATA_SC_20_LOWER_OF_80MHZ),
                2 => (VHT_DATA_SC_40_UPPER_OF_80MHZ, VHT_DATA_SC_20_UPPER_OF_80MHZ),
                3 => (
                    VHT_DATA_SC_40_UPPER_OF_80MHZ,
                    VHT_DATA_SC_20_UPPERST_OF_80MHZ,
                ),
                _ => unreachable!("80 MHz position is constrained to four primary channels"),
            };
            Ok((sc40 << 4) | sc20)
        }
    }
}

fn channel_programming_number(
    channel: Channel,
    bandwidth: Bandwidth,
) -> std::result::Result<u8, DiagnosticErrorReport> {
    match bandwidth {
        Bandwidth::Mhz20 | Bandwidth::Mhz40 => Ok(channel.number),
        Bandwidth::Mhz80 => {
            eighty_mhz_center_and_position(channel.number).map(|(center, _)| center)
        }
    }
}

fn eighty_mhz_center_and_position(
    primary_channel: u8,
) -> std::result::Result<(u8, u8), DiagnosticErrorReport> {
    match primary_channel {
        36 => Ok((42, 0)),
        40 => Ok((42, 1)),
        44 => Ok((42, 2)),
        48 => Ok((42, 3)),
        52 => Ok((58, 0)),
        56 => Ok((58, 1)),
        60 => Ok((58, 2)),
        64 => Ok((58, 3)),
        100 => Ok((106, 0)),
        104 => Ok((106, 1)),
        108 => Ok((106, 2)),
        112 => Ok((106, 3)),
        116 => Ok((122, 0)),
        120 => Ok((122, 1)),
        124 => Ok((122, 2)),
        128 => Ok((122, 3)),
        132 => Ok((138, 0)),
        136 => Ok((138, 1)),
        140 => Ok((138, 2)),
        144 => Ok((138, 3)),
        149 => Ok((155, 0)),
        153 => Ok((155, 1)),
        157 => Ok((155, 2)),
        161 => Ok((155, 3)),
        _ => Err(DiagnosticErrorReport {
            code: "channel_bandwidth_not_supported",
            message: format!(
                "channel {primary_channel} is not aligned to a supported 5 GHz 80 MHz group"
            ),
        }),
    }
}

fn switch_wireless_band_8812<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    target_band: Band,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let cck_check =
        read8_with_counter(registers, counters, REG_CCK_CHECK_8812).map_err(|error| {
            DiagnosticErrorReport {
                code: "register_read_failed",
                message: format!("REG_CCK_CHECK_8812 read failed: {error}"),
            }
        })?;
    let current_band = if cck_check & BIT7 != 0 {
        Band::Ghz5
    } else {
        Band::Ghz2
    };
    steps.push(QueueDmaStepReport {
        phase: "band_switch",
        operation: "read8",
        description: "infer current band from REG_CCK_CHECK_8812 bit 7",
        source: RF_SOURCE_PHYCFG,
        register_name: "REG_CCK_CHECK_8812",
        address: REG_CCK_CHECK_8812,
        address_hex: format_address(REG_CCK_CHECK_8812),
        width: "u8",
        mask_hex: Some(format_value(BIT7, 2)),
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(cck_check, 2)),
        expected_hex: None,
        passed: true,
    });

    if current_band == target_band {
        return Ok(());
    }

    match target_band {
        Band::Ghz2 => switch_to_2g_band(registers, counters, steps),
        Band::Ghz5 => switch_to_5g_band(registers, counters, steps),
    }
}

fn switch_to_2g_band<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rOFDMCCKEN_Jaguar",
            address: REG_OFDMCCKEN_JAGUAR,
            bitmask: 0x3000_0000,
            data: 0x03,
            description: "enable OFDM and CCK blocks for 2.4 GHz operation",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rBWIndication_Jaguar",
            address: REG_BW_INDICATION_JAGUAR,
            bitmask: 0x0000_0003,
            data: 0x01,
            description: "mark BB band indication as 2.4 GHz",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    set_pwed_thresholds(registers, counters, steps, 0x17)?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rAGC_table_Jaguar",
            address: REG_AGC_TABLE_JAGUAR,
            bitmask: 0x0000_0003,
            data: 0x00,
            description: "select the 2.4 GHz AGC table",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    set_rfe_reg_8812_rfe0(registers, counters, steps, Band::Ghz2)?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rTxPath_Jaguar",
            address: REG_TX_PATH_JAGUAR,
            bitmask: 0x0000_00f0,
            data: 0x01,
            description: "select CCK-capable TX path behavior for 2.4 GHz",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rCCK_RX_Jaguar",
            address: REG_CCK_RX_JAGUAR,
            bitmask: 0x0f00_0000,
            data: 0x01,
            description: "select CCK RX path behavior for 2.4 GHz",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    set_basic_rate(registers, counters, steps, BASIC_RATE_2G)?;
    mac_rmw8_step(
        registers,
        counters,
        steps,
        MacRmw8Spec {
            phase: "band_switch",
            register_name: "REG_CCK_CHECK_8812",
            address: REG_CCK_CHECK_8812,
            preserve_mask: !BIT7,
            value_mask: BIT7,
            value: 0,
            description: "clear CCK_CHECK band bit for 2.4 GHz",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    set_bb_swing_default(registers, counters, steps)
}

fn switch_to_5g_band<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    mac_rmw8_step(
        registers,
        counters,
        steps,
        MacRmw8Spec {
            phase: "band_switch",
            register_name: "REG_CCK_CHECK_8812",
            address: REG_CCK_CHECK_8812,
            preserve_mask: !BIT7,
            value_mask: BIT7,
            value: BIT7,
            description: "set CCK_CHECK band bit for 5 GHz",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    poll_tx_packet_empty(registers, counters, steps)?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rOFDMCCKEN_Jaguar",
            address: REG_OFDMCCKEN_JAGUAR,
            bitmask: 0x3000_0000,
            data: 0x03,
            description: "enable OFDM and CCK blocks before 5 GHz CCK avoidance settings",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rBWIndication_Jaguar",
            address: REG_BW_INDICATION_JAGUAR,
            bitmask: 0x0000_0003,
            data: 0x02,
            description: "mark BB band indication as 5 GHz",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    set_pwed_thresholds(registers, counters, steps, 0x15)?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rAGC_table_Jaguar",
            address: REG_AGC_TABLE_JAGUAR,
            bitmask: 0x0000_0003,
            data: 0x01,
            description: "select the 5 GHz AGC table",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    set_rfe_reg_8812_rfe0(registers, counters, steps, Band::Ghz5)?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rTxPath_Jaguar",
            address: REG_TX_PATH_JAGUAR,
            bitmask: 0x0000_00f0,
            data: 0x00,
            description: "avoid CCK TX path behavior in 5 GHz",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rCCK_RX_Jaguar",
            address: REG_CCK_RX_JAGUAR,
            bitmask: 0x0f00_0000,
            data: 0x0f,
            description: "avoid CCK RX path behavior in 5 GHz",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    set_basic_rate(registers, counters, steps, BASIC_RATE_5G)?;
    set_bb_swing_default(registers, counters, steps)
}

fn set_pwed_thresholds<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    pd_th_20m: u32,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rPwed_TH_Jaguar",
            address: REG_PWED_TH_JAGUAR,
            bitmask: 0x0003_e000,
            data: pd_th_20m,
            description: "set PD_TH_20M for the selected band",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "band_switch",
            register_name: "rPwed_TH_Jaguar",
            address: REG_PWED_TH_JAGUAR,
            bitmask: 0x0000_000e,
            data: 0x04,
            description: "set PWED_TH for 2T2R operation",
            source: RF_SOURCE_PHYCFG,
        },
    )
}

fn set_rfe_reg_8812_rfe0<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    band: Band,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let (pinmux, inv) = match band {
        Band::Ghz2 => (0x7777_7777, 0x000),
        Band::Ghz5 => (0x7733_7717, 0x010),
    };
    for (register_name, address) in [
        ("rA_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_A_JAGUAR),
        ("rB_RFE_Pinmux_Jaguar", REG_RFE_PINMUX_B_JAGUAR),
    ] {
        bb_masked_write32_step(
            registers,
            counters,
            steps,
            BbMaskedWrite32Spec {
                phase: "band_switch",
                register_name,
                address,
                bitmask: u32::MAX,
                data: pinmux,
                description: "program RTL8812A RFE pinmux for default RFE type 0",
                source: RF_SOURCE_PHYCFG,
            },
        )?;
    }
    for (register_name, address) in [
        ("rA_RFE_Inv_Jaguar", REG_RFE_INV_A_JAGUAR),
        ("rB_RFE_Inv_Jaguar", REG_RFE_INV_B_JAGUAR),
    ] {
        bb_masked_write32_step(
            registers,
            counters,
            steps,
            BbMaskedWrite32Spec {
                phase: "band_switch",
                register_name,
                address,
                bitmask: 0x3ff0_0000,
                data: inv,
                description: "program RTL8812A RFE inversion bits for default RFE type 0",
                source: RF_SOURCE_PHYCFG,
            },
        )?;
    }
    Ok(())
}

fn set_basic_rate<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    rate_mask: u16,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    queue_write16_step(
        registers,
        counters,
        steps,
        QueueWrite16Spec {
            phase: "band_switch",
            register_name: "REG_RRSR",
            address: REG_RRSR,
            value: rate_mask,
            verify_mask: u16::MAX,
            verify_value: rate_mask,
            description: "update response-rate set for the selected band",
            source: RF_SOURCE_PHYCFG,
        },
    )?;
    mac_rmw8_step(
        registers,
        counters,
        steps,
        MacRmw8Spec {
            phase: "band_switch",
            register_name: "REG_RRSR + 2",
            address: REG_RRSR + 2,
            preserve_mask: 0xf0,
            value_mask: 0x0f,
            value: 0x00,
            description: "clear high response-rate bits after basic-rate update",
            source: RF_SOURCE_PHYCFG,
        },
    )
}

fn set_bb_swing_default<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    for (register_name, address) in [
        ("rA_TxScale_Jaguar", REG_TX_SCALE_A_JAGUAR),
        ("rB_TxScale_Jaguar", REG_TX_SCALE_B_JAGUAR),
    ] {
        bb_masked_write32_step(
            registers,
            counters,
            steps,
            BbMaskedWrite32Spec {
                phase: "band_switch",
                register_name,
                address,
                bitmask: 0xffe0_0000,
                data: 0x200,
                description: "set default 0 dB BB swing pending EFUSE power-table support",
                source: RF_SOURCE_PHYCFG,
            },
        )?;
    }
    Ok(())
}

fn poll_tx_packet_empty<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let mut observed = 0;
    let mut attempts = 0;
    while attempts < 50 {
        observed = read16_with_counter(registers, counters, REG_TXPKT_EMPTY).map_err(|error| {
            DiagnosticErrorReport {
                code: "register_read_failed",
                message: format!("REG_TXPKT_EMPTY read failed: {error}"),
            }
        })?;
        attempts += 1;
        if observed & 0x0030 == 0x0030 {
            break;
        }
        std::thread::sleep(Duration::from_micros(50));
    }

    let passed = observed & 0x0030 == 0x0030;
    steps.push(QueueDmaStepReport {
        phase: "band_switch",
        operation: "poll16",
        description: "wait for TX packet-empty bits before 5 GHz band switch continues",
        source: RF_SOURCE_PHYCFG,
        register_name: "REG_TXPKT_EMPTY",
        address: REG_TXPKT_EMPTY,
        address_hex: format_address(REG_TXPKT_EMPTY),
        width: "u16",
        mask_hex: Some(format_value(0x0030u16, 4)),
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(observed, 4)),
        expected_hex: Some(format_value(0x0030u16, 4)),
        passed,
    });

    if passed {
        Ok(())
    } else {
        Err(DiagnosticErrorReport {
            code: "tx_packet_empty_poll_failed",
            message: format!(
                "REG_TXPKT_EMPTY did not report mask 0x0030 after {attempts} reads, got {}",
                format_value(observed, 4)
            ),
        })
    }
}

fn fix_spur_8812a<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    channel_number: u8,
    bandwidth: Bandwidth,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    if channel_number > 14 {
        return Ok(());
    }
    let data = if bandwidth == Bandwidth::Mhz20 && matches!(channel_number, 13 | 14) {
        0x03
    } else {
        0x02
    };
    bb_masked_write32_step(
        registers,
        counters,
        steps,
        BbMaskedWrite32Spec {
            phase: "spur",
            register_name: "rRFMOD_Jaguar",
            address: REG_RF_MOD_JAGUAR,
            bitmask: 0x0000_0300,
            data,
            description: "apply RTL8812A 2.4 GHz spur workaround",
            source: RF_SOURCE_PHYCFG,
        },
    )
}

fn last_rf_register_data(
    plan: &RealtekTablePlan,
    rf_offset: u32,
) -> std::result::Result<u32, DiagnosticErrorReport> {
    let mut data = None;
    for action in &plan.actions {
        if action.kind == RealtekTableActionKind::Write && action.address == rf_offset {
            data = action.data;
        }
    }
    data.ok_or_else(|| DiagnosticErrorReport {
        code: "rf_channel_base_missing",
        message: format!(
            "{} did not contain a final RF register 0x{:02x} write to use as channel base",
            plan.array_name, rf_offset
        ),
    })
}

fn fc_area_data(channel: u8) -> u32 {
    match channel {
        36..=48 => 0x494,
        15..=35 => 0x494,
        50..=80 => 0x453,
        82..=116 => 0x452,
        118..=u8::MAX => 0x412,
        _ => 0x96a,
    }
}

fn rf_mod_ag_data(channel: u8) -> u32 {
    match channel {
        36..=80 => 0x101,
        15..=35 => 0x101,
        82..=140 => 0x301,
        141..=u8::MAX => 0x501,
        _ => 0x000,
    }
}

fn apply_rf_mask(original: u32, bitmask: u32, data: u32) -> u32 {
    if bitmask == 0 {
        return original;
    }
    let bitshift = bitmask.trailing_zeros();
    (original & !bitmask) | ((data << bitshift) & bitmask)
}

#[derive(Debug, Clone, Copy)]
struct RfSerialWriteSpec {
    phase: &'static str,
    path: &'static str,
    bb_register_name: &'static str,
    bb_register: u16,
    rf_register_name: &'static str,
    rf_offset: u32,
    value: u32,
    description: &'static str,
    source: &'static str,
}

fn rf_serial_write_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<QueueDmaStepReport>,
    spec: RfSerialWriteSpec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let encoded = encode_rf_serial_write(spec.rf_offset, spec.value);
    write32_with_counter(registers, counters, spec.bb_register, encoded).map_err(|error| {
        DiagnosticErrorReport {
            code: "rf_serial_write_failed",
            message: format!(
                "{} path {} write RF offset 0x{:02x} value {} via {} failed: {error}",
                spec.rf_register_name,
                spec.path,
                spec.rf_offset,
                format_value(spec.value, 5),
                format_address(spec.bb_register)
            ),
        }
    })?;
    steps.push(QueueDmaStepReport {
        phase: spec.phase,
        operation: "rf_serial_write",
        description: spec.description,
        source: spec.source,
        register_name: spec.bb_register_name,
        address: spec.bb_register,
        address_hex: format_address(spec.bb_register),
        width: "u32",
        mask_hex: Some(format!("rf:{}:0x{:02x}", spec.path, spec.rf_offset)),
        value_hex: Some(format_value(spec.value, 5)),
        before_hex: None,
        written_hex: Some(format_value(encoded, 8)),
        after_hex: None,
        expected_hex: Some(format_value(spec.value, 5)),
        passed: true,
    });
    std::thread::sleep(Duration::from_micros(1));
    Ok(())
}

fn run_firmware_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    args: &FirmwareSmokeArgs,
    firmware_payload: &[u8],
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
    stats: &mut FirmwareRunStats,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    firmware_preflight_reset_loaded_code(registers, counters, steps)?;
    firmware_rmw8_step(
        registers,
        counters,
        steps,
        FirmwareRmw8StepSpec {
            phase: "firmware_download_enable",
            register_name: "REG_MCUFWDL",
            address: REG_MCUFWDL,
            mask: MCUFWDL_EN,
            value: MCUFWDL_EN,
            verify_readback: true,
            description: "enable MCU firmware download",
            source: FIRMWARE_SOURCE_HAL_INIT,
            firmware_attempt: None,
            page: None,
        },
    )?;
    firmware_rmw8_step(
        registers,
        counters,
        steps,
        FirmwareRmw8StepSpec {
            phase: "firmware_download_enable",
            register_name: "REG_MCUFWDL + 2",
            address: REG_MCUFWDL_PLUS_2,
            mask: BIT3,
            value: 0,
            verify_readback: true,
            description: "hold 8051 reset before firmware download",
            source: FIRMWARE_SOURCE_HAL_INIT,
            firmware_attempt: None,
            page: None,
        },
    )?;

    let mut download_result = Err(DiagnosticErrorReport {
        code: "firmware_download_not_started",
        message: "firmware download loop did not run".to_string(),
    });
    for attempt in 1..=args.download_attempts {
        firmware_rmw8_step(
            registers,
            counters,
            steps,
            FirmwareRmw8StepSpec {
                phase: "firmware_download",
                register_name: "REG_MCUFWDL",
                address: REG_MCUFWDL,
                mask: FWDL_CHKSUM_RPT_U8,
                value: FWDL_CHKSUM_RPT_U8,
                verify_readback: false,
                description: "reset firmware checksum report before writing image",
                source: FIRMWARE_SOURCE_HAL_INIT,
                firmware_attempt: Some(attempt),
                page: None,
            },
        )?;
        write_firmware_image(registers, counters, steps, stats, firmware_payload, attempt)?;
        match firmware_poll32_step(
            registers,
            counters,
            steps,
            FirmwarePoll32StepSpec {
                phase: "firmware_checksum_poll",
                register_name: "REG_MCUFWDL",
                address: REG_MCUFWDL,
                mask: FWDL_CHKSUM_RPT_U32,
                expected: FWDL_CHKSUM_RPT_U32,
                min_attempts: args.checksum_min_attempts,
                timeout_ms: args.checksum_timeout_ms,
                delay_us: args.poll_delay_us,
                description: "poll firmware checksum report",
                source: FIRMWARE_SOURCE_HAL_INIT,
                firmware_attempt: Some(attempt),
            },
        ) {
            Ok(result) => {
                stats.checksum_poll_attempts = Some(result.attempts);
                stats.final_mcu_status = Some(result.value);
                download_result = Ok(());
                break;
            }
            Err(error) => {
                download_result = Err(error);
            }
        }
    }

    let disable_result = firmware_rmw8_step(
        registers,
        counters,
        steps,
        FirmwareRmw8StepSpec {
            phase: "firmware_download_disable",
            register_name: "REG_MCUFWDL",
            address: REG_MCUFWDL,
            mask: MCUFWDL_EN,
            value: 0,
            verify_readback: true,
            description: "disable MCU firmware download",
            source: FIRMWARE_SOURCE_HAL_INIT,
            firmware_attempt: None,
            page: None,
        },
    );
    download_result?;
    disable_result?;

    let before = read32_with_counter(registers, counters, REG_MCUFWDL).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_MCUFWDL read before firmware ready write failed: {error}"),
        }
    })?;
    let written = (before | MCUFWDL_RDY) & !WINTINI_RDY;
    firmware_write32_step(
        registers,
        counters,
        steps,
        FirmwareWrite32StepSpec {
            phase: "firmware_ready",
            register_name: "REG_MCUFWDL",
            address: REG_MCUFWDL,
            before,
            value: written,
            verify_mask: None,
            verify_value: None,
            description: "set MCUFWDL_RDY and clear WINTINI_RDY before 8051 reset",
            source: FIRMWARE_SOURCE_HAL_INIT,
        },
    )?;
    firmware_8051_reset_8812(registers, counters, steps)?;
    let ready = firmware_poll32_step(
        registers,
        counters,
        steps,
        FirmwarePoll32StepSpec {
            phase: "firmware_ready_poll",
            register_name: "REG_MCUFWDL",
            address: REG_MCUFWDL,
            mask: WINTINI_RDY,
            expected: WINTINI_RDY,
            min_attempts: args.ready_min_attempts,
            timeout_ms: args.ready_timeout_ms,
            delay_us: args.poll_delay_us,
            description: "poll firmware initialization ready bit",
            source: FIRMWARE_SOURCE_HAL_INIT,
            firmware_attempt: None,
        },
    )?;
    stats.ready_poll_attempts = Some(ready.attempts);
    stats.final_mcu_status = Some(ready.value);

    Ok(())
}

fn run_power_on_sequence<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    args: &PowerOnSmokeArgs,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<PowerOnStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    rmw8_step(
        registers,
        counters,
        steps,
        Rmw8StepSpec {
            phase: "cardemu_to_active",
            register_name: "REG_APS_FSMCO + 1",
            address: REG_APS_FSMCO_PLUS_1,
            mask: BIT2,
            value: 0,
            verify_readback: true,
            description: "disable SW low-power state",
            source: POWER_SOURCE_PWRSEQ,
        },
    )?;
    poll8_step(
        registers,
        args,
        counters,
        steps,
        Poll8StepSpec {
            phase: "cardemu_to_active",
            register_name: "REG_APS_FSMCO + 2",
            address: REG_APS_FSMCO_PLUS_2,
            mask: BIT1,
            expected: BIT1,
            description: "poll power-ready bit",
            source: POWER_SOURCE_PWRSEQ,
        },
    )?;
    rmw8_step(
        registers,
        counters,
        steps,
        Rmw8StepSpec {
            phase: "cardemu_to_active",
            register_name: "REG_APS_FSMCO + 1",
            address: REG_APS_FSMCO_PLUS_1,
            mask: BIT3,
            value: 0,
            verify_readback: true,
            description: "disable WLAN suspend",
            source: POWER_SOURCE_PWRSEQ,
        },
    )?;
    rmw8_step(
        registers,
        counters,
        steps,
        Rmw8StepSpec {
            phase: "cardemu_to_active",
            register_name: "REG_APS_FSMCO + 1",
            address: REG_APS_FSMCO_PLUS_1,
            mask: BIT0,
            value: BIT0,
            verify_readback: false,
            description: "request MAC power-on transition",
            source: POWER_SOURCE_PWRSEQ,
        },
    )?;
    poll8_step(
        registers,
        args,
        counters,
        steps,
        Poll8StepSpec {
            phase: "cardemu_to_active",
            register_name: "REG_APS_FSMCO + 1",
            address: REG_APS_FSMCO_PLUS_1,
            mask: BIT0,
            expected: 0,
            description: "poll MAC power-on transition completion",
            source: POWER_SOURCE_PWRSEQ,
        },
    )?;
    rmw8_step(
        registers,
        counters,
        steps,
        Rmw8StepSpec {
            phase: "cardemu_to_active",
            register_name: "REG_AFE_XTAL_CTRL",
            address: REG_AFE_XTAL_CTRL,
            mask: BIT1,
            value: 0,
            verify_readback: true,
            description: "select post-XOSC buffer type",
            source: POWER_SOURCE_PWRSEQ,
        },
    )?;
    rmw8_step(
        registers,
        counters,
        steps,
        Rmw8StepSpec {
            phase: "cardemu_to_active",
            register_name: "REG_AFE_PLL_CTRL",
            address: REG_AFE_PLL_CTRL,
            mask: BIT3,
            value: 0,
            verify_readback: true,
            description: "select post-XOSC PLL buffer type",
            source: POWER_SOURCE_PWRSEQ,
        },
    )?;
    write16_step(
        registers,
        counters,
        steps,
        Write16StepSpec {
            phase: "command_register",
            register_name: "REG_CR",
            address: REG_CR,
            value: 0,
            expected_mask: 0xffff,
            expected_value: 0,
            description: "clear command register before enabling DMA and scheduler blocks",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    let cr_value = read16_with_counter(registers, counters, REG_CR).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_CR read after clear failed: {error}"),
        }
    })?;
    steps.push(PowerOnStepReport {
        phase: "command_register",
        operation: "read16",
        description: "read command register for block-enable update",
        source: POWER_SOURCE_USB_HALINIT,
        register_name: "REG_CR",
        address: REG_CR,
        address_hex: format_address(REG_CR),
        width: "u16",
        mask_hex: None,
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(cr_value, 4)),
        expected_hex: None,
        attempts: None,
        passed: true,
    });
    write16_step(
        registers,
        counters,
        steps,
        Write16StepSpec {
            phase: "command_register",
            register_name: "REG_CR",
            address: REG_CR,
            value: cr_value | CR_ENABLE_BITS,
            expected_mask: CR_ENABLE_BITS,
            expected_value: CR_ENABLE_BITS,
            description:
                "enable HCI DMA, TX/RX DMA, protocol, scheduler, security, and calibration timer",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    write8_step(
        registers,
        counters,
        steps,
        Write8StepSpec {
            phase: "rf_reset",
            register_name: "REG_RF_CTRL",
            address: REG_RF_CTRL,
            value: 0x05,
            description: "reset RF path A after MAC power-on",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    write8_step(
        registers,
        counters,
        steps,
        Write8StepSpec {
            phase: "rf_reset",
            register_name: "REG_RF_CTRL",
            address: REG_RF_CTRL,
            value: 0x07,
            description: "release RF path A reset after MAC power-on",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    write8_step(
        registers,
        counters,
        steps,
        Write8StepSpec {
            phase: "rf_reset",
            register_name: "REG_RF_B_CTRL_8812",
            address: REG_RF_B_CTRL_8812,
            value: 0x05,
            description: "reset RF path B after MAC power-on",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;
    write8_step(
        registers,
        counters,
        steps,
        Write8StepSpec {
            phase: "rf_reset",
            register_name: "REG_RF_B_CTRL_8812",
            address: REG_RF_B_CTRL_8812,
            value: 0x07,
            description: "release RF path B reset after MAC power-on",
            source: POWER_SOURCE_USB_HALINIT,
        },
    )?;

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct FirmwareRmw8StepSpec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    mask: u8,
    value: u8,
    verify_readback: bool,
    description: &'static str,
    source: &'static str,
    firmware_attempt: Option<u32>,
    page: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct FirmwareWrite8StepSpec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    value: u8,
    verify_mask: Option<u8>,
    verify_value: Option<u8>,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct FirmwareWrite32StepSpec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    before: u32,
    value: u32,
    verify_mask: Option<u32>,
    verify_value: Option<u32>,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct FirmwarePoll32StepSpec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    mask: u32,
    expected: u32,
    min_attempts: u32,
    timeout_ms: u64,
    delay_us: u64,
    description: &'static str,
    source: &'static str,
    firmware_attempt: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct FirmwarePollResult {
    attempts: u32,
    value: u32,
}

#[derive(Debug, Clone, Copy)]
struct FirmwareDataWriteSpec {
    firmware_attempt: u32,
    page: usize,
    page_offset: usize,
}

fn firmware_preflight_reset_loaded_code<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let value = read8_with_counter(registers, counters, REG_MCUFWDL).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("REG_MCUFWDL preflight read failed: {error}"),
        }
    })?;
    steps.push(FirmwareStepReport {
        phase: "firmware_preflight",
        operation: "read8",
        description: "check whether 8051 RAM code is already selected",
        source: FIRMWARE_SOURCE_HAL_INIT,
        register_name: Some("REG_MCUFWDL"),
        address: Some(REG_MCUFWDL),
        address_hex: Some(format_address(REG_MCUFWDL)),
        width: Some("u8"),
        firmware_attempt: None,
        page: None,
        page_offset: None,
        length: None,
        mask_hex: Some(format_value(RAM_DL_SEL, 2)),
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(value, 2)),
        expected_hex: None,
        attempts: None,
        passed: true,
    });

    if value & RAM_DL_SEL == 0 {
        return Ok(());
    }

    firmware_write8_step(
        registers,
        counters,
        steps,
        FirmwareWrite8StepSpec {
            phase: "firmware_preflight",
            register_name: "REG_MCUFWDL",
            address: REG_MCUFWDL,
            value: 0,
            verify_mask: Some(RAM_DL_SEL),
            verify_value: Some(0),
            description: "clear RAM code selection before re-downloading firmware",
            source: FIRMWARE_SOURCE_HAL_INIT,
        },
    )?;
    firmware_8051_reset_8812(registers, counters, steps)
}

fn write_firmware_image<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
    stats: &mut FirmwareRunStats,
    firmware_payload: &[u8],
    firmware_attempt: u32,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    for (page, chunk) in firmware_payload.chunks(MAX_DLFW_PAGE_SIZE).enumerate() {
        firmware_rmw8_step(
            registers,
            counters,
            steps,
            FirmwareRmw8StepSpec {
                phase: "firmware_download",
                register_name: "REG_MCUFWDL + 2",
                address: REG_MCUFWDL_PLUS_2,
                mask: 0x07,
                value: (page as u8) & 0x07,
                verify_readback: true,
                description: "select firmware download page",
                source: FIRMWARE_SOURCE_HAL_INIT,
                firmware_attempt: Some(firmware_attempt),
                page: Some(page),
            },
        )?;
        write_firmware_page(
            registers,
            counters,
            steps,
            stats,
            firmware_attempt,
            page,
            chunk,
        )?;
    }

    Ok(())
}

fn write_firmware_page<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
    stats: &mut FirmwareRunStats,
    firmware_attempt: u32,
    page: usize,
    bytes: &[u8],
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let full_block_len = bytes.len() / MAX_REG_BLOCK_SIZE * MAX_REG_BLOCK_SIZE;
    for (index, block) in bytes[..full_block_len]
        .chunks(MAX_REG_BLOCK_SIZE)
        .enumerate()
    {
        let page_offset = index * MAX_REG_BLOCK_SIZE;
        firmware_write_data_step(
            registers,
            counters,
            steps,
            stats,
            FirmwareDataWriteSpec {
                firmware_attempt,
                page,
                page_offset,
            },
            block,
        )?;
    }

    let remainder = &bytes[full_block_len..];
    let remainder_block_len =
        remainder.len() / FIRMWARE_REMAINDER_BLOCK_SIZE * FIRMWARE_REMAINDER_BLOCK_SIZE;
    for (index, block) in remainder[..remainder_block_len]
        .chunks(FIRMWARE_REMAINDER_BLOCK_SIZE)
        .enumerate()
    {
        let page_offset = full_block_len + index * FIRMWARE_REMAINDER_BLOCK_SIZE;
        firmware_write_data_step(
            registers,
            counters,
            steps,
            stats,
            FirmwareDataWriteSpec {
                firmware_attempt,
                page,
                page_offset,
            },
            block,
        )?;
    }

    for (index, byte) in remainder[remainder_block_len..].iter().enumerate() {
        let page_offset = full_block_len + remainder_block_len + index;
        firmware_write_data_step(
            registers,
            counters,
            steps,
            stats,
            FirmwareDataWriteSpec {
                firmware_attempt,
                page,
                page_offset,
            },
            std::slice::from_ref(byte),
        )?;
    }

    Ok(())
}

fn firmware_write_data_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
    stats: &mut FirmwareRunStats,
    spec: FirmwareDataWriteSpec,
    data: &[u8],
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let address = firmware_page_address(spec.page_offset)?;
    let write_result = if data.len() == 1 {
        write8_with_counter(registers, counters, address, data[0])
    } else {
        write_block_with_counter(registers, counters, address, data)
    };
    if let Err(error) = write_result {
        steps.push(FirmwareStepReport {
            phase: "firmware_download",
            operation: if data.len() == 1 {
                "write8"
            } else {
                "write_block"
            },
            description: "write firmware image bytes",
            source: FIRMWARE_SOURCE_HAL_INIT,
            register_name: Some("FW_START_ADDRESS + page_offset"),
            address: Some(address),
            address_hex: Some(format_address(address)),
            width: Some(if data.len() == 1 { "u8" } else { "block" }),
            firmware_attempt: Some(spec.firmware_attempt),
            page: Some(spec.page),
            page_offset: Some(spec.page_offset),
            length: Some(data.len()),
            mask_hex: None,
            value_hex: Some(format_value(byte_sum(data), 8)),
            before_hex: None,
            written_hex: Some(encode_hex(data)),
            after_hex: None,
            expected_hex: None,
            attempts: None,
            passed: false,
        });
        return Err(DiagnosticErrorReport {
            code: "firmware_write_failed",
            message: format!(
                "firmware write page={} offset={} len={} addr={} failed: {error}",
                spec.page,
                spec.page_offset,
                data.len(),
                format_address(address)
            ),
        });
    }

    stats.firmware_bytes_written += data.len() as u64;
    stats.firmware_control_writes += 1;
    steps.push(FirmwareStepReport {
        phase: "firmware_download",
        operation: if data.len() == 1 {
            "write8"
        } else {
            "write_block"
        },
        description: "write firmware image bytes",
        source: FIRMWARE_SOURCE_HAL_INIT,
        register_name: Some("FW_START_ADDRESS + page_offset"),
        address: Some(address),
        address_hex: Some(format_address(address)),
        width: Some(if data.len() == 1 { "u8" } else { "block" }),
        firmware_attempt: Some(spec.firmware_attempt),
        page: Some(spec.page),
        page_offset: Some(spec.page_offset),
        length: Some(data.len()),
        mask_hex: None,
        value_hex: Some(format_value(byte_sum(data), 8)),
        before_hex: None,
        written_hex: Some(encode_hex(data)),
        after_hex: None,
        expected_hex: None,
        attempts: None,
        passed: true,
    });
    Ok(())
}

fn firmware_8051_reset_8812<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    firmware_rmw8_step(
        registers,
        counters,
        steps,
        FirmwareRmw8StepSpec {
            phase: "firmware_8051_reset",
            register_name: "REG_RSV_CTRL",
            address: REG_RSV_CTRL,
            mask: BIT1,
            value: 0,
            verify_readback: true,
            description: "reset MCU IO wrapper for RTL8812A",
            source: FIRMWARE_SOURCE_HAL_INIT,
            firmware_attempt: None,
            page: None,
        },
    )?;
    firmware_rmw8_step(
        registers,
        counters,
        steps,
        FirmwareRmw8StepSpec {
            phase: "firmware_8051_reset",
            register_name: "REG_RSV_CTRL + 1",
            address: REG_RSV_CTRL + 1,
            mask: BIT3,
            value: 0,
            verify_readback: true,
            description: "disable MCU IO wrapper before 8051 reset",
            source: FIRMWARE_SOURCE_HAL_INIT,
            firmware_attempt: None,
            page: None,
        },
    )?;
    let sys_func =
        read8_with_counter(registers, counters, REG_SYS_FUNC_EN_PLUS_1).map_err(|error| {
            DiagnosticErrorReport {
                code: "register_read_failed",
                message: format!("REG_SYS_FUNC_EN + 1 read before 8051 reset failed: {error}"),
            }
        })?;
    firmware_write8_step(
        registers,
        counters,
        steps,
        FirmwareWrite8StepSpec {
            phase: "firmware_8051_reset",
            register_name: "REG_SYS_FUNC_EN + 1",
            address: REG_SYS_FUNC_EN_PLUS_1,
            value: sys_func & !BIT2,
            verify_mask: Some(BIT2),
            verify_value: Some(0),
            description: "assert 8051 reset",
            source: FIRMWARE_SOURCE_HAL_INIT,
        },
    )?;
    firmware_rmw8_step(
        registers,
        counters,
        steps,
        FirmwareRmw8StepSpec {
            phase: "firmware_8051_reset",
            register_name: "REG_RSV_CTRL",
            address: REG_RSV_CTRL,
            mask: BIT1,
            value: 0,
            verify_readback: true,
            description: "keep MCU IO wrapper reset low for RTL8812A",
            source: FIRMWARE_SOURCE_HAL_INIT,
            firmware_attempt: None,
            page: None,
        },
    )?;
    firmware_rmw8_step(
        registers,
        counters,
        steps,
        FirmwareRmw8StepSpec {
            phase: "firmware_8051_reset",
            register_name: "REG_RSV_CTRL + 1",
            address: REG_RSV_CTRL + 1,
            mask: BIT3,
            value: BIT3,
            verify_readback: true,
            description: "enable MCU IO wrapper after reset",
            source: FIRMWARE_SOURCE_HAL_INIT,
            firmware_attempt: None,
            page: None,
        },
    )?;
    firmware_write8_step(
        registers,
        counters,
        steps,
        FirmwareWrite8StepSpec {
            phase: "firmware_8051_reset",
            register_name: "REG_SYS_FUNC_EN + 1",
            address: REG_SYS_FUNC_EN_PLUS_1,
            value: sys_func | BIT2,
            verify_mask: Some(BIT2),
            verify_value: Some(BIT2),
            description: "release 8051 reset",
            source: FIRMWARE_SOURCE_HAL_INIT,
        },
    )
}

fn firmware_rmw8_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
    spec: FirmwareRmw8StepSpec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    let written = (before & !spec.mask) | (spec.value & spec.mask);
    write8_with_counter(registers, counters, spec.address, written).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.value & spec.mask;
    let passed = !spec.verify_readback || (after & spec.mask) == expected;
    steps.push(FirmwareStepReport {
        phase: spec.phase,
        operation: "rmw8",
        description: spec.description,
        source: spec.source,
        register_name: Some(spec.register_name),
        address: Some(spec.address),
        address_hex: Some(format_address(spec.address)),
        width: Some("u8"),
        firmware_attempt: spec.firmware_attempt,
        page: spec.page,
        page_offset: None,
        length: None,
        mask_hex: Some(format_value(spec.mask, 2)),
        value_hex: Some(format_value(spec.value, 2)),
        before_hex: Some(format_value(before, 2)),
        written_hex: Some(format_value(written, 2)),
        after_hex: Some(format_value(after, 2)),
        expected_hex: spec.verify_readback.then(|| format_value(expected, 2)),
        attempts: None,
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(DiagnosticErrorReport {
            code: "register_readback_mismatch",
            message: format!(
                "{} expected mask {} to equal {}, got {}",
                spec.register_name,
                format_value(spec.mask, 2),
                format_value(expected, 2),
                format_value(after & spec.mask, 2)
            ),
        })
    }
}

fn firmware_write8_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
    spec: FirmwareWrite8StepSpec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    write8_with_counter(registers, counters, spec.address, spec.value).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let passed = match (spec.verify_mask, spec.verify_value) {
        (Some(mask), Some(expected)) => (after & mask) == (expected & mask),
        _ => true,
    };
    steps.push(FirmwareStepReport {
        phase: spec.phase,
        operation: "write8",
        description: spec.description,
        source: spec.source,
        register_name: Some(spec.register_name),
        address: Some(spec.address),
        address_hex: Some(format_address(spec.address)),
        width: Some("u8"),
        firmware_attempt: None,
        page: None,
        page_offset: None,
        length: None,
        mask_hex: spec.verify_mask.map(|mask| format_value(mask, 2)),
        value_hex: Some(format_value(spec.value, 2)),
        before_hex: Some(format_value(before, 2)),
        written_hex: Some(format_value(spec.value, 2)),
        after_hex: Some(format_value(after, 2)),
        expected_hex: spec.verify_value.map(|value| format_value(value, 2)),
        attempts: None,
        passed,
    });
    if passed {
        Ok(())
    } else {
        let mask = spec.verify_mask.unwrap_or(0xff);
        let expected = spec.verify_value.unwrap_or(spec.value) & mask;
        Err(DiagnosticErrorReport {
            code: "register_readback_mismatch",
            message: format!(
                "{} expected mask {} to equal {}, got {}",
                spec.register_name,
                format_value(mask, 2),
                format_value(expected, 2),
                format_value(after & mask, 2)
            ),
        })
    }
}

fn firmware_write32_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
    spec: FirmwareWrite32StepSpec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    write32_with_counter(registers, counters, spec.address, spec.value).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read32_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let passed = match (spec.verify_mask, spec.verify_value) {
        (Some(mask), Some(expected)) => (after & mask) == (expected & mask),
        _ => true,
    };
    steps.push(FirmwareStepReport {
        phase: spec.phase,
        operation: "write32",
        description: spec.description,
        source: spec.source,
        register_name: Some(spec.register_name),
        address: Some(spec.address),
        address_hex: Some(format_address(spec.address)),
        width: Some("u32"),
        firmware_attempt: None,
        page: None,
        page_offset: None,
        length: None,
        mask_hex: spec.verify_mask.map(|mask| format_value(mask, 8)),
        value_hex: Some(format_value(spec.value, 8)),
        before_hex: Some(format_value(spec.before, 8)),
        written_hex: Some(format_value(spec.value, 8)),
        after_hex: Some(format_value(after, 8)),
        expected_hex: spec.verify_value.map(|value| format_value(value, 8)),
        attempts: None,
        passed,
    });
    if passed {
        Ok(())
    } else {
        let mask = spec.verify_mask.unwrap_or(u32::MAX);
        let expected = spec.verify_value.unwrap_or(spec.value) & mask;
        Err(DiagnosticErrorReport {
            code: "register_readback_mismatch",
            message: format!(
                "{} expected mask {} to equal {}, got {}",
                spec.register_name,
                format_value(mask, 8),
                format_value(expected, 8),
                format_value(after & mask, 8)
            ),
        })
    }
}

fn firmware_poll32_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<FirmwareStepReport>,
    spec: FirmwarePoll32StepSpec,
) -> std::result::Result<FirmwarePollResult, DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let started = Instant::now();
    let timeout = Duration::from_millis(spec.timeout_ms);
    let mut attempts = 0u32;

    loop {
        attempts = attempts.saturating_add(1);
        let value = read32_with_counter(registers, counters, spec.address).map_err(|error| {
            DiagnosticErrorReport {
                code: "register_poll_failed",
                message: format!("{} poll read failed: {error}", spec.register_name),
            }
        })?;
        if (value & spec.mask) == (spec.expected & spec.mask) {
            steps.push(firmware_poll32_report(spec, attempts, value, true));
            return Ok(FirmwarePollResult { attempts, value });
        }
        if started.elapsed() >= timeout && attempts >= spec.min_attempts {
            steps.push(firmware_poll32_report(spec, attempts, value, false));
            return Err(DiagnosticErrorReport {
                code: "register_poll_timeout",
                message: format!(
                    "{} expected mask {} to equal {}, last value {} after {} attempts",
                    spec.register_name,
                    format_value(spec.mask, 8),
                    format_value(spec.expected & spec.mask, 8),
                    format_value(value, 8),
                    attempts
                ),
            });
        }
        if spec.delay_us > 0 {
            std::thread::sleep(Duration::from_micros(spec.delay_us));
        }
    }
}

fn firmware_poll32_report(
    spec: FirmwarePoll32StepSpec,
    attempts: u32,
    value: u32,
    passed: bool,
) -> FirmwareStepReport {
    FirmwareStepReport {
        phase: spec.phase,
        operation: "poll32",
        description: spec.description,
        source: spec.source,
        register_name: Some(spec.register_name),
        address: Some(spec.address),
        address_hex: Some(format_address(spec.address)),
        width: Some("u32"),
        firmware_attempt: spec.firmware_attempt,
        page: None,
        page_offset: None,
        length: None,
        mask_hex: Some(format_value(spec.mask, 8)),
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(value, 8)),
        expected_hex: Some(format_value(spec.expected & spec.mask, 8)),
        attempts: Some(attempts),
        passed,
    }
}

fn firmware_page_count(byte_len: usize) -> usize {
    byte_len.div_ceil(MAX_DLFW_PAGE_SIZE)
}

fn firmware_page_address(page_offset: usize) -> std::result::Result<u16, DiagnosticErrorReport> {
    let offset = u16::try_from(page_offset).map_err(|_| DiagnosticErrorReport {
        code: "firmware_offset_too_large",
        message: format!("firmware page offset {page_offset} does not fit in a register address"),
    })?;
    FW_START_ADDRESS
        .checked_add(offset)
        .ok_or_else(|| DiagnosticErrorReport {
            code: "firmware_address_overflow",
            message: format!(
                "firmware address overflow: start={} offset={}",
                format_address(FW_START_ADDRESS),
                page_offset
            ),
        })
}

#[derive(Debug, Clone, Copy)]
struct Rmw8StepSpec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    mask: u8,
    value: u8,
    verify_readback: bool,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct Poll8StepSpec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    mask: u8,
    expected: u8,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct Write8StepSpec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    value: u8,
    description: &'static str,
    source: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct Write16StepSpec {
    phase: &'static str,
    register_name: &'static str,
    address: u16,
    value: u16,
    expected_mask: u16,
    expected_value: u16,
    description: &'static str,
    source: &'static str,
}

fn rmw8_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<PowerOnStepReport>,
    spec: Rmw8StepSpec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    let written = (before & !spec.mask) | (spec.value & spec.mask);
    write8_with_counter(registers, counters, spec.address, written).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.value & spec.mask;
    let passed = !spec.verify_readback || (after & spec.mask) == expected;
    steps.push(PowerOnStepReport {
        phase: spec.phase,
        operation: "rmw8",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u8",
        mask_hex: Some(format_value(spec.mask, 2)),
        value_hex: Some(format_value(spec.value, 2)),
        before_hex: Some(format_value(before, 2)),
        written_hex: Some(format_value(written, 2)),
        after_hex: Some(format_value(after, 2)),
        expected_hex: spec.verify_readback.then(|| format_value(expected, 2)),
        attempts: None,
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(DiagnosticErrorReport {
            code: "register_readback_mismatch",
            message: format!(
                "{} expected mask {} to equal {}, got {}",
                spec.register_name,
                format_value(spec.mask, 2),
                format_value(expected, 2),
                format_value(after & spec.mask, 2)
            ),
        })
    }
}

fn poll8_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    args: &PowerOnSmokeArgs,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<PowerOnStepReport>,
    spec: Poll8StepSpec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let mut last = 0u8;
    for attempt in 1..=args.poll_attempts {
        last = read8_with_counter(registers, counters, spec.address).map_err(|error| {
            DiagnosticErrorReport {
                code: "register_poll_failed",
                message: format!("{} poll read failed: {error}", spec.register_name),
            }
        })?;
        if (last & spec.mask) == (spec.expected & spec.mask) {
            steps.push(PowerOnStepReport {
                phase: spec.phase,
                operation: "poll8",
                description: spec.description,
                source: spec.source,
                register_name: spec.register_name,
                address: spec.address,
                address_hex: format_address(spec.address),
                width: "u8",
                mask_hex: Some(format_value(spec.mask, 2)),
                value_hex: None,
                before_hex: None,
                written_hex: None,
                after_hex: Some(format_value(last, 2)),
                expected_hex: Some(format_value(spec.expected & spec.mask, 2)),
                attempts: Some(attempt),
                passed: true,
            });
            return Ok(());
        }
        std::thread::sleep(Duration::from_micros(args.poll_delay_us));
    }

    steps.push(PowerOnStepReport {
        phase: spec.phase,
        operation: "poll8",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u8",
        mask_hex: Some(format_value(spec.mask, 2)),
        value_hex: None,
        before_hex: None,
        written_hex: None,
        after_hex: Some(format_value(last, 2)),
        expected_hex: Some(format_value(spec.expected & spec.mask, 2)),
        attempts: Some(args.poll_attempts),
        passed: false,
    });
    Err(DiagnosticErrorReport {
        code: "register_poll_timeout",
        message: format!(
            "{} expected mask {} to equal {}, last value {}",
            spec.register_name,
            format_value(spec.mask, 2),
            format_value(spec.expected & spec.mask, 2),
            format_value(last, 2)
        ),
    })
}

fn write8_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<PowerOnStepReport>,
    spec: Write8StepSpec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    write8_with_counter(registers, counters, spec.address, spec.value).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read8_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let passed = after == spec.value;
    steps.push(PowerOnStepReport {
        phase: spec.phase,
        operation: "write8",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u8",
        mask_hex: None,
        value_hex: Some(format_value(spec.value, 2)),
        before_hex: Some(format_value(before, 2)),
        written_hex: Some(format_value(spec.value, 2)),
        after_hex: Some(format_value(after, 2)),
        expected_hex: Some(format_value(spec.value, 2)),
        attempts: None,
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(DiagnosticErrorReport {
            code: "register_readback_mismatch",
            message: format!(
                "{} expected {}, got {}",
                spec.register_name,
                format_value(spec.value, 2),
                format_value(after, 2)
            ),
        })
    }
}

fn write16_step<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    steps: &mut Vec<PowerOnStepReport>,
    spec: Write16StepSpec,
) -> std::result::Result<(), DiagnosticErrorReport>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let before = read16_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read before write failed: {error}", spec.register_name),
        }
    })?;
    write16_with_counter(registers, counters, spec.address, spec.value).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_write_failed",
            message: format!("{} write failed: {error}", spec.register_name),
        }
    })?;
    let after = read16_with_counter(registers, counters, spec.address).map_err(|error| {
        DiagnosticErrorReport {
            code: "register_read_failed",
            message: format!("{} read after write failed: {error}", spec.register_name),
        }
    })?;
    let expected = spec.expected_value & spec.expected_mask;
    let passed = (after & spec.expected_mask) == expected;
    steps.push(PowerOnStepReport {
        phase: spec.phase,
        operation: "write16",
        description: spec.description,
        source: spec.source,
        register_name: spec.register_name,
        address: spec.address,
        address_hex: format_address(spec.address),
        width: "u16",
        mask_hex: Some(format_value(spec.expected_mask, 4)),
        value_hex: Some(format_value(spec.value, 4)),
        before_hex: Some(format_value(before, 4)),
        written_hex: Some(format_value(spec.value, 4)),
        after_hex: Some(format_value(after, 4)),
        expected_hex: Some(format_value(expected, 4)),
        attempts: None,
        passed,
    });
    if passed {
        Ok(())
    } else {
        Err(DiagnosticErrorReport {
            code: "register_readback_mismatch",
            message: format!(
                "{} expected mask {} to equal {}, got {}",
                spec.register_name,
                format_value(spec.expected_mask, 4),
                format_value(expected, 4),
                format_value(after & spec.expected_mask, 4)
            ),
        })
    }
}

fn read8_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
) -> std::result::Result<u8, radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let value = registers.read8(address)?;
    counters.usb_control_reads += 1;
    Ok(value)
}

fn read16_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
) -> std::result::Result<u16, radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let value = registers.read16(address)?;
    counters.usb_control_reads += 1;
    Ok(value)
}

fn read32_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
) -> std::result::Result<u32, radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    let value = registers.read32(address)?;
    counters.usb_control_reads += 1;
    Ok(value)
}

fn write8_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
    value: u8,
) -> std::result::Result<(), radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    registers.write8(address, value)?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn write16_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
    value: u16,
) -> std::result::Result<(), radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    registers.write16(address, value)?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn write32_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
    value: u32,
) -> std::result::Result<(), radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    registers.write32(address, value)?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn write_block_with_counter<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut DiagnosticCounters,
    address: u16,
    data: &[u8],
) -> std::result::Result<(), radio_core::Rtl8812auRegisterError>
where
    T: radio_core::rtl8812au::Rtl8812auUsbTransport,
{
    registers.write_block(address, data)?;
    counters.usb_control_writes += 1;
    Ok(())
}

fn format_address(address: u16) -> String {
    format!("0x{address:04x}")
}

fn format_value<T>(value: T, digits: usize) -> String
where
    T: Into<u64>,
{
    format!("0x{:0width$x}", value.into(), width = digits)
}

fn init_report(args: InitArgs) -> PendingDiagnosticReport {
    let selector = args.adapter.selector();
    let (channel, mut result, mut error) = resolve_report_channel(args.channel, args.bandwidth);
    let firmware_path = args.firmware.clone();
    let trace_out = args.trace_out.clone();
    let mut firmware_image = None;
    let mut init_dry_run = None;
    let firmware = match args.firmware.as_deref() {
        Some(path) if result != DiagnosticResult::Fail => match load_firmware_with_report(path) {
            Ok((image, report)) => {
                firmware_image = Some(image);
                Some(report)
            }
            Err(message) => {
                result = DiagnosticResult::Fail;
                error = Some(DiagnosticErrorReport {
                    code: "firmware_load_failed",
                    message,
                });
                None
            }
        },
        _ => None,
    };

    if args.trace_out.is_some() && !args.dry_run && result != DiagnosticResult::Fail {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "trace_out_requires_dry_run",
            message: "--trace-out is only valid with --dry-run".to_string(),
        });
    }

    if args.dry_run && result != DiagnosticResult::Fail {
        match firmware_image.as_ref() {
            Some(image) => match build_init_dry_run_report(image, trace_out.clone()) {
                Ok(report) => {
                    result = DiagnosticResult::Pass;
                    init_dry_run = Some(report);
                }
                Err(report_error) => {
                    result = DiagnosticResult::Fail;
                    error = Some(report_error);
                }
            },
            None => {
                result = DiagnosticResult::Fail;
                error = Some(DiagnosticErrorReport {
                    code: "missing_firmware",
                    message: "--dry-run requires --firmware so the download plan is explicit"
                        .to_string(),
                });
            }
        }
    }

    if args.dry_run {
        let phases = if result == DiagnosticResult::Fail {
            vec![DiagnosticPhase {
                id: "argument_validation",
                status: DiagnosticPhaseStatus::Blocked,
                detail: "init arguments did not pass local validation",
            }]
        } else {
            vec![
                DiagnosticPhase {
                    id: "firmware_plan",
                    status: DiagnosticPhaseStatus::Completed,
                    detail: "loaded firmware and built a source-audited transfer plan",
                },
                DiagnosticPhase {
                    id: "power_on",
                    status: DiagnosticPhaseStatus::Completed,
                    detail: "planned RTL8812AU power-on control transfers from driver audit",
                },
                DiagnosticPhase {
                    id: "firmware",
                    status: DiagnosticPhaseStatus::Completed,
                    detail:
                        "planned firmware download and readiness polling transfers from driver audit",
                },
                DiagnosticPhase {
                    id: "mac_bb_rf",
                    status: DiagnosticPhaseStatus::Completed,
                    detail: "planned LLT, queue, MAC, BB, RF, and channel setup skeleton",
                },
            ]
        };

        return pending_report(PendingReportInput {
            command: "init",
            selector,
            adapter: None,
            endpoints: None,
            channel,
            bandwidth: Some(args.bandwidth),
            firmware_path,
            firmware,
            init_dry_run,
            init_live: None,
            duration_ms: None,
            pcap_path: None,
            tx_frame_len: None,
            tx_frame_source: None,
            tx_dry_run: None,
            tx_live: None,
            rx_fixture: None,
            repeat_tx: None,
            counters: DiagnosticCounters::default(),
            result,
            phases,
            error,
            notes: vec![
                "dry run only: source-audited init transfer plan was emitted and no USB control transfers were issued",
            ],
        });
    }

    if result == DiagnosticResult::Fail {
        return pending_report(PendingReportInput {
            command: "init",
            selector,
            adapter: None,
            endpoints: None,
            channel,
            bandwidth: Some(args.bandwidth),
            firmware_path,
            firmware,
            init_dry_run: None,
            init_live: None,
            duration_ms: None,
            pcap_path: None,
            tx_frame_len: None,
            tx_frame_source: None,
            tx_dry_run: None,
            tx_live: None,
            rx_fixture: None,
            repeat_tx: None,
            counters: DiagnosticCounters::default(),
            result,
            phases: vec![DiagnosticPhase {
                id: "argument_validation",
                status: DiagnosticPhaseStatus::Blocked,
                detail: "init arguments did not pass local validation",
            }],
            error,
            notes: vec!["live init aborted before claiming USB or writing hardware registers"],
        });
    }

    init_live_report(args, channel, firmware_path, firmware_image, firmware)
}

fn init_live_report(
    args: InitArgs,
    channel: Option<Channel>,
    firmware_path: Option<PathBuf>,
    firmware_image: Option<FirmwareImage>,
    firmware: Option<FirmwareReport>,
) -> PendingDiagnosticReport {
    let selector = args.adapter.selector();
    let condition_env = args.condition_env();
    let mut phase_summaries = Vec::new();
    let mut counters = DiagnosticCounters::default();
    let mut adapter = None;
    let mut endpoints = None;
    let mut llt_stats = LltRunStats::default();
    let mut queue_layout = None;
    let mut bb_stats = BbSmokeStats::default();
    let mut rf_stats = RfSmokeStats::default();

    if firmware_image.is_none() {
        push_init_live_phase(
            &mut phase_summaries,
            "argument_validation",
            DiagnosticPhaseStatus::Blocked,
            "live init requires --firmware",
            counters,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len: None,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("argument_validation"),
            error: Some(DiagnosticErrorReport {
                code: "missing_firmware",
                message: "live init requires --firmware with an RTL8812A firmware image"
                    .to_string(),
            }),
            notes: vec!["live init aborted before claiming USB or writing hardware registers"],
        });
    }

    if !args.i_understand_this_writes_registers {
        push_init_live_phase(
            &mut phase_summaries,
            "argument_validation",
            DiagnosticPhaseStatus::Blocked,
            "live init requires explicit hardware-write authorization",
            counters,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len: firmware_image
                .as_ref()
                .map(|image| image.realtek_download_payload().bytes.len()),
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("argument_validation"),
            error: Some(DiagnosticErrorReport {
                code: "missing_write_authorization",
                message: "live init writes hardware registers and requires --i-understand-this-writes-registers".to_string(),
            }),
            notes: vec!["live init aborted before claiming USB or writing hardware registers"],
        });
    }

    let firmware_image = firmware_image.expect("validated firmware image is present");
    let firmware_payload = firmware_image.realtek_download_payload();
    let firmware_payload_len = Some(firmware_payload.bytes.len());
    if firmware_page_count(firmware_payload.bytes.len()) > MAX_FIRMWARE_DOWNLOAD_PAGES {
        push_init_live_phase(
            &mut phase_summaries,
            "argument_validation",
            DiagnosticPhaseStatus::Blocked,
            "firmware payload exceeds the RTL8812A page selector",
            counters,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("argument_validation"),
            error: Some(DiagnosticErrorReport {
                code: "firmware_too_many_pages",
                message: format!(
                    "firmware requires {} 4 KiB pages, but RTL8812A page selector exposes {} pages",
                    firmware_page_count(firmware_payload.bytes.len()),
                    MAX_FIRMWARE_DOWNLOAD_PAGES
                ),
            }),
            notes: vec!["live init aborted before claiming USB or writing hardware registers"],
        });
    }

    let before = counters;
    let (phy_plan, agc_plan) = match load_bb_table_plans(&args.bb_source, condition_env) {
        Ok(plans) => {
            push_init_live_phase(
                &mut phase_summaries,
                "table_plan",
                DiagnosticPhaseStatus::Completed,
                "parsed and planned BB PHY/AGC tables",
                before,
                counters,
            );
            plans
        }
        Err(error) => {
            push_init_live_phase(
                &mut phase_summaries,
                "table_plan",
                DiagnosticPhaseStatus::Blocked,
                format!("BB table planning failed: {}", error.message),
                before,
                counters,
            );
            return init_live_pending_report(InitLivePendingReportInput {
                args: &args,
                selector,
                adapter,
                endpoints,
                channel,
                firmware_path,
                firmware,
                phase_summaries,
                firmware_payload_len,
                llt_stats: &llt_stats,
                queue_layout,
                bb_stats: &bb_stats,
                rf_stats: &rf_stats,
                counters,
                result: DiagnosticResult::Fail,
                phases: init_live_phases("table_plan"),
                error: Some(error),
                notes: vec!["live init aborted before claiming USB or writing hardware registers"],
            });
        }
    };

    let before = counters;
    let (radioa_plan, radiob_plan) = match load_rf_table_plans(&args.rf_source, condition_env) {
        Ok(plans) => {
            push_init_live_phase(
                &mut phase_summaries,
                "table_plan",
                DiagnosticPhaseStatus::Completed,
                "parsed and planned RF radioA/radioB tables",
                before,
                counters,
            );
            plans
        }
        Err(error) => {
            push_init_live_phase(
                &mut phase_summaries,
                "table_plan",
                DiagnosticPhaseStatus::Blocked,
                format!("RF table planning failed: {}", error.message),
                before,
                counters,
            );
            return init_live_pending_report(InitLivePendingReportInput {
                args: &args,
                selector,
                adapter,
                endpoints,
                channel,
                firmware_path,
                firmware,
                phase_summaries,
                firmware_payload_len,
                llt_stats: &llt_stats,
                queue_layout,
                bb_stats: &bb_stats,
                rf_stats: &rf_stats,
                counters,
                result: DiagnosticResult::Fail,
                phases: init_live_phases("table_plan"),
                error: Some(error),
                notes: vec!["live init aborted before claiming USB or writing hardware registers"],
            });
        }
    };

    let before = counters;
    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            push_init_live_phase(
                &mut phase_summaries,
                "usb_claim",
                DiagnosticPhaseStatus::Blocked,
                "no supported adapter matched the selector",
                before,
                counters,
            );
            return init_live_pending_report(InitLivePendingReportInput {
                args: &args,
                selector,
                adapter,
                endpoints,
                channel,
                firmware_path,
                firmware,
                phase_summaries,
                firmware_payload_len,
                llt_stats: &llt_stats,
                queue_layout,
                bb_stats: &bb_stats,
                rf_stats: &rf_stats,
                counters,
                result: DiagnosticResult::Fail,
                phases: init_live_phases("usb_claim"),
                error: Some(error),
                notes: vec!["live init stopped before hardware register writes"],
            });
        }
    };

    let claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            adapter = Some(selected);
            push_init_live_phase(
                &mut phase_summaries,
                "usb_claim",
                DiagnosticPhaseStatus::Blocked,
                format!("USB claim failed: {error}"),
                before,
                counters,
            );
            return init_live_pending_report(InitLivePendingReportInput {
                args: &args,
                selector,
                adapter,
                endpoints,
                channel,
                firmware_path,
                firmware,
                phase_summaries,
                firmware_payload_len,
                llt_stats: &llt_stats,
                queue_layout,
                bb_stats: &bb_stats,
                rf_stats: &rf_stats,
                counters,
                result: DiagnosticResult::Fail,
                phases: init_live_phases("usb_claim"),
                error: Some(DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                }),
                notes: vec!["live init stopped before hardware register writes"],
            });
        }
    };
    adapter = Some(claimed.info.clone());
    endpoints = Some(claimed.endpoints.clone());
    push_init_live_phase(
        &mut phase_summaries,
        "usb_claim",
        DiagnosticPhaseStatus::Completed,
        "claimed adapter interface and discovered bulk endpoints",
        before,
        counters,
    );

    let registers =
        Rtl8812auRegisterAccess::new(&claimed).with_timeout(Duration::from_millis(args.timeout_ms));
    let power_args = PowerOnSmokeArgs {
        adapter: args.adapter.clone(),
        timeout_ms: args.timeout_ms,
        poll_attempts: 200,
        poll_delay_us: 10,
        i_understand_this_writes_registers: true,
    };
    let firmware_args = FirmwareSmokeArgs {
        adapter: args.adapter.clone(),
        firmware: firmware_path
            .clone()
            .expect("live init firmware path is present"),
        timeout_ms: args.timeout_ms,
        download_attempts: 3,
        checksum_min_attempts: 5,
        checksum_timeout_ms: 50,
        ready_min_attempts: 10,
        ready_timeout_ms: 200,
        poll_delay_us: 1000,
        i_understand_this_writes_registers: true,
    };
    let llt_args = LltSmokeArgs {
        adapter: args.adapter.clone(),
        timeout_ms: args.timeout_ms,
        poll_attempts: 25,
        poll_delay_us: 10,
        i_understand_this_writes_registers: true,
    };
    let bb_args = BbSmokeArgs {
        adapter: args.adapter.clone(),
        bb_source: args.bb_source.clone(),
        timeout_ms: args.timeout_ms,
        cut_version: args.cut_version,
        package_type: args.package_type,
        support_interface: args.support_interface,
        support_platform: args.support_platform,
        board_type: args.board_type,
        type_glna: args.type_glna,
        type_gpa: args.type_gpa,
        type_alna: args.type_alna,
        type_apa: args.type_apa,
        crystal_cap: args.crystal_cap,
        i_understand_this_writes_registers: true,
    };

    let mut power_steps = Vec::new();
    let before = counters;
    if let Err(error) =
        run_power_on_sequence(&registers, &power_args, &mut counters, &mut power_steps)
    {
        push_init_live_phase(
            &mut phase_summaries,
            "power_on",
            DiagnosticPhaseStatus::Blocked,
            format!("{} after {} steps", error.message, power_steps.len()),
            before,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("power_on"),
            error: Some(error),
            notes: vec!["live init stopped during RTL8812AU power-on"],
        });
    }
    push_init_live_phase(
        &mut phase_summaries,
        "power_on",
        DiagnosticPhaseStatus::Completed,
        format!("completed {} power-on/RF-reset steps", power_steps.len()),
        before,
        counters,
    );

    let mut firmware_steps = Vec::new();
    let mut firmware_stats = FirmwareRunStats {
        firmware_payload_offset: Some(firmware_payload.offset),
        firmware_payload_len: Some(firmware_payload.bytes.len()),
        firmware_signature: firmware_payload.signature,
        ..FirmwareRunStats::default()
    };
    let before = counters;
    if let Err(error) = run_firmware_sequence(
        &registers,
        &firmware_args,
        firmware_payload.bytes,
        &mut counters,
        &mut firmware_steps,
        &mut firmware_stats,
    ) {
        push_init_live_phase(
            &mut phase_summaries,
            "firmware",
            DiagnosticPhaseStatus::Blocked,
            format!("{} after {} steps", error.message, firmware_steps.len()),
            before,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("firmware"),
            error: Some(error),
            notes: vec!["live init stopped during firmware download/readiness polling"],
        });
    }
    push_init_live_phase(
        &mut phase_summaries,
        "firmware",
        DiagnosticPhaseStatus::Completed,
        format!(
            "downloaded {} payload bytes in {} control writes",
            firmware_payload.bytes.len(),
            firmware_stats.firmware_control_writes
        ),
        before,
        counters,
    );

    let mut llt_steps = Vec::new();
    let before = counters;
    if let Err(error) = run_llt_sequence(
        &registers,
        &llt_args,
        &mut counters,
        &mut llt_steps,
        &mut llt_stats,
    ) {
        push_init_live_phase(
            &mut phase_summaries,
            "llt",
            DiagnosticPhaseStatus::Blocked,
            format!("{} after {} steps", error.message, llt_steps.len()),
            before,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("llt"),
            error: Some(error),
            notes: vec!["live init stopped during LLT programming"],
        });
    }
    push_init_live_phase(
        &mut phase_summaries,
        "llt",
        DiagnosticPhaseStatus::Completed,
        format!("wrote {} LLT entries", llt_stats.entries_written),
        before,
        counters,
    );

    let layout = match endpoints
        .as_ref()
        .and_then(|eps| queue_layout_from_endpoints(eps).ok())
    {
        Some(layout) => layout,
        None => {
            let error = endpoints
                .as_ref()
                .map(queue_layout_from_endpoints)
                .expect("endpoints are present")
                .expect_err("layout error is present");
            push_init_live_phase(
                &mut phase_summaries,
                "queue_dma",
                DiagnosticPhaseStatus::Blocked,
                error.message.clone(),
                counters,
                counters,
            );
            return init_live_pending_report(InitLivePendingReportInput {
                args: &args,
                selector,
                adapter,
                endpoints,
                channel,
                firmware_path,
                firmware,
                phase_summaries,
                firmware_payload_len,
                llt_stats: &llt_stats,
                queue_layout,
                bb_stats: &bb_stats,
                rf_stats: &rf_stats,
                counters,
                result: DiagnosticResult::Fail,
                phases: init_live_phases("queue_dma"),
                error: Some(error),
                notes: vec!["live init stopped before queue/DMA register programming"],
            });
        }
    };
    queue_layout = Some(layout);
    let mut queue_steps = Vec::new();
    let before = counters;
    if let Err(error) = run_queue_dma_sequence(&registers, &layout, &mut counters, &mut queue_steps)
    {
        push_init_live_phase(
            &mut phase_summaries,
            "queue_dma",
            DiagnosticPhaseStatus::Blocked,
            format!("{} after {} steps", error.message, queue_steps.len()),
            before,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("queue_dma"),
            error: Some(error),
            notes: vec!["live init stopped during queue/DMA setup"],
        });
    }
    push_init_live_phase(
        &mut phase_summaries,
        "queue_dma",
        DiagnosticPhaseStatus::Completed,
        format!(
            "programmed queue/DMA layout for {} bulk OUT endpoints",
            layout.bulk_out_endpoint_count
        ),
        before,
        counters,
    );

    let mut mac_steps = Vec::new();
    let before = counters;
    if let Err(error) = run_mac_sequence(&registers, &mut counters, &mut mac_steps) {
        push_init_live_phase(
            &mut phase_summaries,
            "mac",
            DiagnosticPhaseStatus::Blocked,
            format!("{} after {} steps", error.message, mac_steps.len()),
            before,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("mac"),
            error: Some(error),
            notes: vec!["live init stopped during MAC/WMAC setup"],
        });
    }
    push_init_live_phase(
        &mut phase_summaries,
        "mac",
        DiagnosticPhaseStatus::Completed,
        format!("completed {} MAC/WMAC setup steps", mac_steps.len()),
        before,
        counters,
    );

    let mut bb_steps = Vec::new();
    let before = counters;
    if let Err(error) = run_bb_sequence(
        &registers,
        &bb_args,
        &phy_plan,
        &agc_plan,
        &mut counters,
        &mut bb_steps,
        &mut bb_stats,
    ) {
        push_init_live_phase(
            &mut phase_summaries,
            "bb",
            DiagnosticPhaseStatus::Blocked,
            format!("{} after {} setup steps", error.message, bb_steps.len()),
            before,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("bb"),
            error: Some(error),
            notes: vec!["live init stopped during BB PHY/AGC table programming"],
        });
    }
    push_init_live_phase(
        &mut phase_summaries,
        "bb",
        DiagnosticPhaseStatus::Completed,
        format!(
            "applied {} PHY writes, {} AGC writes, and {} delays",
            bb_stats.phy_writes_applied, bb_stats.agc_writes_applied, bb_stats.delays_applied
        ),
        before,
        counters,
    );

    let mut rf_steps = Vec::new();
    let before = counters;
    if let Err(error) = run_rf_sequence(
        &registers,
        &radioa_plan,
        &radiob_plan,
        &mut counters,
        &mut rf_steps,
        &mut rf_stats,
    ) {
        push_init_live_phase(
            &mut phase_summaries,
            "rf",
            DiagnosticPhaseStatus::Blocked,
            format!("{} after {} setup steps", error.message, rf_steps.len()),
            before,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("rf"),
            error: Some(error),
            notes: vec!["live init stopped during RF radio table programming"],
        });
    }
    push_init_live_phase(
        &mut phase_summaries,
        "rf",
        DiagnosticPhaseStatus::Completed,
        format!(
            "applied {} radioA writes, {} radioB writes, and {} delays",
            rf_stats.radioa_writes_applied, rf_stats.radiob_writes_applied, rf_stats.delays_applied
        ),
        before,
        counters,
    );

    let requested_channel =
        channel.expect("live init channel was validated before hardware access");
    let mut channel_steps = Vec::new();
    let before = counters;
    if let Err(error) = run_channel_sequence(
        &registers,
        requested_channel,
        args.bandwidth,
        &radioa_plan,
        &radiob_plan,
        &mut counters,
        &mut channel_steps,
    ) {
        push_init_live_phase(
            &mut phase_summaries,
            "channel",
            DiagnosticPhaseStatus::Blocked,
            format!(
                "{} after {} channel steps",
                error.message,
                channel_steps.len()
            ),
            before,
            counters,
        );
        return init_live_pending_report(InitLivePendingReportInput {
            args: &args,
            selector,
            adapter,
            endpoints,
            channel,
            firmware_path,
            firmware,
            phase_summaries,
            firmware_payload_len,
            llt_stats: &llt_stats,
            queue_layout,
            bb_stats: &bb_stats,
            rf_stats: &rf_stats,
            counters,
            result: DiagnosticResult::Fail,
            phases: init_live_phases("channel"),
            error: Some(error),
            notes: vec!["live init stopped during channel switch"],
        });
    }
    push_init_live_phase(
        &mut phase_summaries,
        "channel",
        DiagnosticPhaseStatus::Completed,
        format!(
            "programmed channel {} ({} MHz, {} MHz bandwidth) in {} steps",
            requested_channel.number,
            requested_channel.frequency_mhz,
            args.bandwidth.mhz(),
            channel_steps.len()
        ),
        before,
        counters,
    );

    init_live_pending_report(InitLivePendingReportInput {
        args: &args,
        selector,
        adapter,
        endpoints,
        channel,
        firmware_path,
        firmware,
        phase_summaries,
        firmware_payload_len,
        llt_stats: &llt_stats,
        queue_layout,
        bb_stats: &bb_stats,
        rf_stats: &rf_stats,
        counters,
        result: DiagnosticResult::Pass,
        phases: init_live_phases(""),
        error: None,
        notes: vec![
            "live init completed power, firmware, LLT, queue/DMA, MAC, BB, RF, and selected channel setup",
            "TX power tables, IQK, bulk IN loop, and TX frame submission remain separate tasks",
            "no bulk IN loop, bulk OUT frame submission, or TX operation was issued",
        ],
    })
}

struct InitLivePendingReportInput<'a> {
    args: &'a InitArgs,
    selector: DeviceSelector,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    channel: Option<Channel>,
    firmware_path: Option<PathBuf>,
    firmware: Option<FirmwareReport>,
    phase_summaries: Vec<InitLivePhaseSummary>,
    firmware_payload_len: Option<usize>,
    llt_stats: &'a LltRunStats,
    queue_layout: Option<QueueLayout>,
    bb_stats: &'a BbSmokeStats,
    rf_stats: &'a RfSmokeStats,
    counters: DiagnosticCounters,
    result: DiagnosticResult,
    phases: Vec<DiagnosticPhase>,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

fn init_live_pending_report(input: InitLivePendingReportInput<'_>) -> PendingDiagnosticReport {
    let init_live = InitLiveReport {
        bb_source: input.args.bb_source.clone(),
        rf_source: input.args.rf_source.clone(),
        condition_env: input.args.condition_env(),
        crystal_cap_hex: format_value(input.args.crystal_cap, 2),
        phase_summaries: input.phase_summaries,
        firmware_payload_len: input.firmware_payload_len,
        llt_entries_written: input.llt_stats.entries_written,
        queue_pages: input.queue_layout.map(queue_page_report),
        bb_phy_writes_applied: input.bb_stats.phy_writes_applied,
        bb_agc_writes_applied: input.bb_stats.agc_writes_applied,
        bb_delays_applied: input.bb_stats.delays_applied,
        rf_radioa_writes_applied: input.rf_stats.radioa_writes_applied,
        rf_radiob_writes_applied: input.rf_stats.radiob_writes_applied,
        rf_delays_applied: input.rf_stats.delays_applied,
        effective_channel: if input.result == DiagnosticResult::Pass {
            input.channel
        } else {
            None
        },
        effective_bandwidth: if input.result == DiagnosticResult::Pass {
            Some(input.args.bandwidth)
        } else {
            None
        },
    };

    pending_report(PendingReportInput {
        command: "init",
        selector: input.selector,
        adapter: input.adapter,
        endpoints: input.endpoints,
        channel: input.channel,
        bandwidth: Some(input.args.bandwidth),
        firmware_path: input.firmware_path,
        firmware: input.firmware,
        init_dry_run: None,
        init_live: Some(init_live),
        duration_ms: None,
        pcap_path: None,
        tx_frame_len: None,
        tx_frame_source: None,
        tx_dry_run: None,
        tx_live: None,
        rx_fixture: None,
        repeat_tx: None,
        counters: input.counters,
        result: input.result,
        phases: input.phases,
        error: input.error,
        notes: input.notes,
    })
}

fn push_init_live_phase(
    phase_summaries: &mut Vec<InitLivePhaseSummary>,
    id: &'static str,
    status: DiagnosticPhaseStatus,
    detail: impl Into<String>,
    before: DiagnosticCounters,
    after: DiagnosticCounters,
) {
    phase_summaries.push(InitLivePhaseSummary {
        id,
        status,
        detail: detail.into(),
        usb_control_reads: after
            .usb_control_reads
            .saturating_sub(before.usb_control_reads),
        usb_control_writes: after
            .usb_control_writes
            .saturating_sub(before.usb_control_writes),
    });
}

fn init_live_phases(blocked_phase: &'static str) -> Vec<DiagnosticPhase> {
    let mut phases = Vec::new();
    let mut blocked_seen = false;
    for (id, detail) in [
        (
            "argument_validation",
            "validate channel, firmware, and operator authorization",
        ),
        (
            "table_plan",
            "parse and plan Realtek BB/RF configuration tables",
        ),
        ("usb_claim", "claim selected adapter and discover endpoints"),
        (
            "power_on",
            "run minimum RTL8812AU power-on and RF reset sequence",
        ),
        ("firmware", "download firmware and poll checksum/readiness"),
        ("llt", "program RTL8812A linked-list table"),
        (
            "queue_dma",
            "program queue, DMA, page boundary, and packet-buffer registers",
        ),
        (
            "mac",
            "program MAC/WMAC raw receive and TX/RX enable registers",
        ),
        ("bb", "program BB PHY/AGC tables and crystal-cap setting"),
        (
            "rf",
            "program RF radioA/radioB tables through 3-wire writes",
        ),
        (
            "channel",
            "program selected channel/bandwidth and report effective channel",
        ),
    ] {
        let status = if blocked_phase.is_empty() {
            DiagnosticPhaseStatus::Completed
        } else if blocked_phase == id {
            blocked_seen = true;
            DiagnosticPhaseStatus::Blocked
        } else if blocked_seen {
            DiagnosticPhaseStatus::Pending
        } else {
            DiagnosticPhaseStatus::Completed
        };
        phases.push(DiagnosticPhase { id, status, detail });
    }
    phases
}

fn rx_scan_report(args: RxScanArgs) -> PendingDiagnosticReport {
    let (channel, result, error) = resolve_report_channel(args.channel, args.bandwidth);
    if !args.fixture_bulk_in.is_empty() && result != DiagnosticResult::Fail {
        return rx_scan_fixture_report(args, channel.expect("channel resolved"));
    }

    if result == DiagnosticResult::Fail {
        return pending_report(PendingReportInput {
            command: "rx-scan",
            selector: args.adapter.selector(),
            adapter: None,
            endpoints: None,
            channel,
            bandwidth: Some(args.bandwidth),
            firmware_path: None,
            firmware: None,
            init_dry_run: None,
            init_live: None,
            duration_ms: Some(args.duration_ms),
            pcap_path: args.pcap,
            tx_frame_len: None,
            tx_frame_source: None,
            tx_dry_run: None,
            tx_live: None,
            rx_fixture: None,
            repeat_tx: None,
            counters: DiagnosticCounters::default(),
            result,
            phases: vec![DiagnosticPhase {
                id: "argument_validation",
                status: DiagnosticPhaseStatus::Blocked,
                detail: "channel or bandwidth arguments are not supported",
            }],
            error,
            notes: vec!["live RX aborted before claiming USB"],
        });
    }

    let selector = args.adapter.selector();
    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return pending_report(PendingReportInput {
                command: "rx-scan",
                selector,
                adapter: None,
                endpoints: None,
                channel,
                bandwidth: Some(args.bandwidth),
                firmware_path: None,
                firmware: None,
                init_dry_run: None,
                init_live: None,
                duration_ms: Some(args.duration_ms),
                pcap_path: args.pcap,
                tx_frame_len: None,
                tx_frame_source: None,
                tx_dry_run: None,
                tx_live: None,
                rx_fixture: None,
                repeat_tx: None,
                counters: DiagnosticCounters::default(),
                result: DiagnosticResult::Fail,
                phases: vec![DiagnosticPhase {
                    id: "usb_claim",
                    status: DiagnosticPhaseStatus::Blocked,
                    detail: "no supported adapter matched the selector",
                }],
                error: Some(error),
                notes: vec!["live RX stopped before USB claim"],
            });
        }
    };

    let mut claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return pending_report(PendingReportInput {
                command: "rx-scan",
                selector,
                adapter: Some(selected),
                endpoints: None,
                channel,
                bandwidth: Some(args.bandwidth),
                firmware_path: None,
                firmware: None,
                init_dry_run: None,
                init_live: None,
                duration_ms: Some(args.duration_ms),
                pcap_path: args.pcap,
                tx_frame_len: None,
                tx_frame_source: None,
                tx_dry_run: None,
                tx_live: None,
                rx_fixture: None,
                repeat_tx: None,
                counters: DiagnosticCounters::default(),
                result: DiagnosticResult::Fail,
                phases: vec![DiagnosticPhase {
                    id: "usb_claim",
                    status: DiagnosticPhaseStatus::Blocked,
                    detail: "USB interface claim failed",
                }],
                error: Some(DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                }),
                notes: vec!["live RX stopped before bulk IN reads"],
            });
        }
    };

    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let bulk_in = match endpoints.bulk_in {
        Some(endpoint) => endpoint,
        None => {
            return pending_report(PendingReportInput {
                command: "rx-scan",
                selector,
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                channel,
                bandwidth: Some(args.bandwidth),
                firmware_path: None,
                firmware: None,
                init_dry_run: None,
                init_live: None,
                duration_ms: Some(args.duration_ms),
                pcap_path: args.pcap,
                tx_frame_len: None,
                tx_frame_source: None,
                tx_dry_run: None,
                tx_live: None,
                rx_fixture: None,
                repeat_tx: None,
                counters: DiagnosticCounters::default(),
                result: DiagnosticResult::Fail,
                phases: vec![DiagnosticPhase {
                    id: "bulk_in_loop",
                    status: DiagnosticPhaseStatus::Blocked,
                    detail: "claimed interface has no bulk IN endpoint",
                }],
                error: Some(DiagnosticErrorReport {
                    code: "missing_bulk_in_endpoint",
                    message: "claimed interface has no bulk IN endpoint".to_string(),
                }),
                notes: vec!["live RX stopped before bulk IN reads"],
            });
        }
    };

    let pcap_path = args.pcap.clone();
    let frame_jsonl_path = args.frame_jsonl.clone();
    let channel = channel.expect("channel resolved before live RX");
    match run_rx_bulk_in_capture(
        &mut claimed,
        bulk_in,
        channel,
        args.duration_ms,
        args.timeout_ms,
        pcap_path.as_deref(),
        frame_jsonl_path.as_deref(),
    ) {
        Ok((rx_fixture, counters)) => pending_report(PendingReportInput {
            command: "rx-scan",
            selector,
            adapter: Some(adapter),
            endpoints: Some(endpoints),
            channel: Some(channel),
            bandwidth: Some(args.bandwidth),
            firmware_path: None,
            firmware: None,
            init_dry_run: None,
            init_live: None,
            duration_ms: Some(args.duration_ms),
            pcap_path,
            tx_frame_len: None,
            tx_frame_source: None,
            tx_dry_run: None,
            tx_live: None,
            rx_fixture: Some(rx_fixture),
            repeat_tx: None,
            counters,
            result: DiagnosticResult::Pass,
            phases: vec![
                DiagnosticPhase {
                    id: "usb_claim",
                    status: DiagnosticPhaseStatus::Completed,
                    detail: "claimed initialized adapter for live RX",
                },
                DiagnosticPhase {
                    id: "bulk_in_loop",
                    status: DiagnosticPhaseStatus::Completed,
                    detail: "read bounded buffers from the RTL8812AU bulk IN endpoint",
                },
                DiagnosticPhase {
                    id: "pcap",
                    status: DiagnosticPhaseStatus::Completed,
                    detail: "optional PCAP output completed",
                },
            ],
            error: None,
            notes: vec![
                "live RX assumes the adapter has already completed init on the requested channel",
                "no control writes, bulk OUT frame submissions, or TX operations were issued",
            ],
        }),
        Err(error) => pending_report(PendingReportInput {
            command: "rx-scan",
            selector,
            adapter: Some(adapter),
            endpoints: Some(endpoints),
            channel: Some(channel),
            bandwidth: Some(args.bandwidth),
            firmware_path: None,
            firmware: None,
            init_dry_run: None,
            init_live: None,
            duration_ms: Some(args.duration_ms),
            pcap_path,
            tx_frame_len: None,
            tx_frame_source: None,
            tx_dry_run: None,
            tx_live: None,
            rx_fixture: None,
            repeat_tx: None,
            counters: DiagnosticCounters::default(),
            result: DiagnosticResult::Fail,
            phases: vec![DiagnosticPhase {
                id: "bulk_in_loop",
                status: DiagnosticPhaseStatus::Blocked,
                detail: "bulk IN capture failed",
            }],
            error: Some(error),
            notes: vec!["live RX stopped during bulk IN capture"],
        }),
    }
}

fn run_rx_bulk_in_capture<T>(
    transport: &mut T,
    bulk_in_endpoint: u8,
    channel: Channel,
    duration_ms: u64,
    timeout_ms: u64,
    pcap_path: Option<&Path>,
    frame_jsonl_path: Option<&Path>,
) -> std::result::Result<(RxFixtureReport, DiagnosticCounters), DiagnosticErrorReport>
where
    T: UsbBulkTransfer,
{
    let mut report = RxFixtureReport {
        frame_jsonl_path: frame_jsonl_path.map(Path::to_path_buf),
        ..RxFixtureReport::default()
    };
    let mut pcap = create_optional_pcap(pcap_path)?;
    let mut frame_jsonl = create_optional_frame_jsonl(frame_jsonl_path)?;
    let timeout_ms = timeout_ms.clamp(1, duration_ms.max(1));
    let per_read_timeout = Duration::from_millis(timeout_ms);
    let deadline = Instant::now() + Duration::from_millis(duration_ms);
    let mut buf = vec![0u8; 16 * 1024];

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let timeout = per_read_timeout.min(remaining);
        match transport.read_bulk_transfer(bulk_in_endpoint, &mut buf, timeout) {
            Ok(0) => {
                report.buffers_read += 1;
            }
            Ok(len) => {
                report.buffers_read += 1;
                report.bulk_bytes += len as u64;
                process_rx_buffer(
                    &mut report,
                    &mut pcap,
                    &mut frame_jsonl,
                    channel,
                    &buf[..len],
                )?;
            }
            Err(error) if error.is_timeout() => {
                report.read_timeouts += 1;
            }
            Err(error) => {
                return Err(DiagnosticErrorReport {
                    code: "bulk_in_read_failed",
                    message: format!(
                        "bulk IN read from endpoint 0x{bulk_in_endpoint:02x} failed: {error}"
                    ),
                });
            }
        }
    }

    flush_optional_pcap(&mut pcap)?;
    flush_optional_frame_jsonl(&mut frame_jsonl)?;
    let counters = DiagnosticCounters {
        usb_bulk_in_reads: report.buffers_read + report.read_timeouts,
        rx_frames: report.parsed_frames,
        dropped_frames: report.dropped_packets,
        ..DiagnosticCounters::default()
    };
    Ok((report, counters))
}

fn create_optional_pcap(
    pcap_path: Option<&Path>,
) -> std::result::Result<Option<PcapWriter<File>>, DiagnosticErrorReport> {
    match pcap_path {
        Some(path) => Ok(Some(
            PcapWriter::new(File::create(path).map_err(|error| DiagnosticErrorReport {
                code: "pcap_create_failed",
                message: format!("failed to create {}: {error}", path.display()),
            })?)
            .map_err(|error| DiagnosticErrorReport {
                code: "pcap_header_write_failed",
                message: format!("failed to write PCAP header: {error}"),
            })?,
        )),
        None => Ok(None),
    }
}

fn create_optional_frame_jsonl(
    frame_jsonl_path: Option<&Path>,
) -> std::result::Result<Option<File>, DiagnosticErrorReport> {
    match frame_jsonl_path {
        Some(path) => File::create(path)
            .map(Some)
            .map_err(|error| DiagnosticErrorReport {
                code: "frame_jsonl_create_failed",
                message: format!("failed to create {}: {error}", path.display()),
            }),
        None => Ok(None),
    }
}

fn process_rx_buffer(
    report: &mut RxFixtureReport,
    pcap: &mut Option<PcapWriter<File>>,
    frame_jsonl: &mut Option<File>,
    channel: Channel,
    buf: &[u8],
) -> std::result::Result<(), DiagnosticErrorReport> {
    let mut offset = 0usize;
    while offset < buf.len() {
        let parsed = parse_rx_packet(&buf[offset..], channel);
        match parsed.outcome {
            RxParseOutcome::Frame => {
                let frame = parsed.frame.expect("frame outcome includes frame");
                report.parsed_frames += 1;
                count_rx_frame_type(report, &frame.data);
                if let Some(writer) = pcap.as_mut() {
                    writer
                        .write_frame(SystemTime::now(), &frame.data)
                        .map_err(|error| DiagnosticErrorReport {
                            code: "pcap_packet_write_failed",
                            message: format!("failed to write PCAP packet: {error}"),
                        })?;
                    report.pcap_frames_written += 1;
                }
                if let Some(writer) = frame_jsonl.as_mut() {
                    write_rx_frame_record(writer, channel, &frame)?;
                    report.frame_records_written += 1;
                }
                advance_fixture_offset(&mut offset, parsed.consumed, buf.len(), report);
            }
            RxParseOutcome::Drop => {
                report.dropped_packets += 1;
                advance_fixture_offset(&mut offset, parsed.consumed, buf.len(), report);
            }
            RxParseOutcome::NeedMoreData => {
                report.need_more_data += 1;
                break;
            }
        }
    }
    Ok(())
}

fn flush_optional_pcap(
    pcap: &mut Option<PcapWriter<File>>,
) -> std::result::Result<(), DiagnosticErrorReport> {
    if let Some(writer) = pcap.as_mut() {
        writer.flush().map_err(|error| DiagnosticErrorReport {
            code: "pcap_flush_failed",
            message: format!("failed to flush PCAP: {error}"),
        })?;
    }
    Ok(())
}

fn flush_optional_frame_jsonl(
    frame_jsonl: &mut Option<File>,
) -> std::result::Result<(), DiagnosticErrorReport> {
    if let Some(writer) = frame_jsonl.as_mut() {
        writer.flush().map_err(|error| DiagnosticErrorReport {
            code: "frame_jsonl_flush_failed",
            message: format!("failed to flush frame JSONL: {error}"),
        })?;
    }
    Ok(())
}

fn write_rx_frame_record(
    writer: &mut File,
    channel: Channel,
    frame: &radio_core::RxFrame,
) -> std::result::Result<(), DiagnosticErrorReport> {
    let frame_type = frame_type(&frame.data)
        .map(|kind| format!("{kind:?}"))
        .unwrap_or_else(|_| "Malformed".to_string());
    let record = RxFrameJsonRecord {
        timestamp_unix_ms: started_at_unix_ms(),
        frame_len: frame.data.len(),
        rssi_dbm: frame.rssi_dbm,
        channel,
        frequency_mhz: channel.frequency_mhz,
        band: channel.band,
        frame_type,
        frame_hex: encode_hex(&frame.data),
    };
    serde_json::to_writer(&mut *writer, &record).map_err(|error| DiagnosticErrorReport {
        code: "frame_jsonl_write_failed",
        message: format!("failed to encode frame JSONL record: {error}"),
    })?;
    writer
        .write_all(b"\n")
        .map_err(|error| DiagnosticErrorReport {
            code: "frame_jsonl_write_failed",
            message: format!("failed to write frame JSONL newline: {error}"),
        })
}

fn rx_scan_fixture_report(args: RxScanArgs, channel: Channel) -> PendingDiagnosticReport {
    let pcap_path = args.pcap.clone();
    let frame_jsonl_path = args.frame_jsonl.clone();
    let (result, rx_fixture, counters, error, phases, notes) = match parse_rx_fixture_files(
        &args.fixture_bulk_in,
        channel,
        pcap_path.as_deref(),
        frame_jsonl_path.as_deref(),
    ) {
        Ok((fixture, counters)) => (
            DiagnosticResult::Pass,
            Some(fixture),
            counters,
            None,
            vec![
                DiagnosticPhase {
                    id: "fixture_bulk_in",
                    status: DiagnosticPhaseStatus::Completed,
                    detail: "parsed raw RTL8812AU bulk-IN fixture buffers",
                },
                DiagnosticPhase {
                    id: "pcap",
                    status: DiagnosticPhaseStatus::Completed,
                    detail: "optional PCAP output completed",
                },
            ],
            vec!["fixture mode only: no USB bulk IN reads were issued"],
        ),
        Err(error) => (
            DiagnosticResult::Fail,
            None,
            DiagnosticCounters::default(),
            Some(error),
            vec![DiagnosticPhase {
                id: "fixture_bulk_in",
                status: DiagnosticPhaseStatus::Blocked,
                detail: "fixture parsing failed before live RX was attempted",
            }],
            vec!["fixture mode only: no USB bulk IN reads were issued"],
        ),
    };

    pending_report(PendingReportInput {
        command: "rx-scan",
        selector: args.adapter.selector(),
        adapter: None,
        endpoints: None,
        channel: Some(channel),
        bandwidth: Some(args.bandwidth),
        firmware_path: None,
        firmware: None,
        init_dry_run: None,
        init_live: None,
        duration_ms: Some(args.duration_ms),
        pcap_path,
        tx_frame_len: None,
        tx_frame_source: None,
        tx_dry_run: None,
        tx_live: None,
        rx_fixture,
        repeat_tx: None,
        counters,
        result,
        phases,
        error,
        notes,
    })
}

fn parse_rx_fixture_files(
    paths: &[PathBuf],
    channel: Channel,
    pcap_path: Option<&Path>,
    frame_jsonl_path: Option<&Path>,
) -> std::result::Result<(RxFixtureReport, DiagnosticCounters), DiagnosticErrorReport> {
    let mut report = RxFixtureReport {
        fixture_paths: paths.to_vec(),
        frame_jsonl_path: frame_jsonl_path.map(Path::to_path_buf),
        ..RxFixtureReport::default()
    };
    let mut pcap = create_optional_pcap(pcap_path)?;
    let mut frame_jsonl = create_optional_frame_jsonl(frame_jsonl_path)?;

    for path in paths {
        let buf = fs::read(path).map_err(|error| DiagnosticErrorReport {
            code: "fixture_read_failed",
            message: format!("failed to read {}: {error}", path.display()),
        })?;
        report.buffers_read += 1;
        report.bulk_bytes += buf.len() as u64;
        process_rx_buffer(&mut report, &mut pcap, &mut frame_jsonl, channel, &buf)?;
    }

    flush_optional_pcap(&mut pcap)?;
    flush_optional_frame_jsonl(&mut frame_jsonl)?;

    let counters = DiagnosticCounters {
        usb_bulk_in_reads: report.buffers_read + report.read_timeouts,
        rx_frames: report.parsed_frames,
        dropped_frames: report.dropped_packets,
        ..DiagnosticCounters::default()
    };
    Ok((report, counters))
}

fn advance_fixture_offset(
    offset: &mut usize,
    consumed: usize,
    buf_len: usize,
    report: &mut RxFixtureReport,
) {
    if consumed == 0 {
        report.need_more_data += 1;
        *offset = buf_len;
        return;
    }
    let remaining = buf_len.saturating_sub(*offset);
    report.bytes_consumed += consumed.min(remaining) as u64;
    *offset = (*offset + consumed).min(buf_len);
}

fn count_rx_frame_type(report: &mut RxFixtureReport, frame: &[u8]) {
    match frame_type(frame) {
        Ok(FrameType::Management) => report.management_frames += 1,
        Ok(FrameType::Control) => report.control_frames += 1,
        Ok(FrameType::Data) => report.data_frames += 1,
        Ok(FrameType::Extension) => report.extension_frames += 1,
        Err(_) => report.dropped_packets += 1,
    }
}

fn tx_once_report(args: TxOnceArgs) -> PendingDiagnosticReport {
    let (channel, mut result, mut error) = resolve_report_channel(args.channel, args.bandwidth);
    let mut tx_dry_run = None;
    let (tx_frame_len, tx_frame_source) =
        validate_tx_frame_arg(args.frame_hex.as_deref(), &mut result, &mut error);

    if args.packet_out.is_some() && !args.dry_run && result != DiagnosticResult::Fail {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "packet_out_requires_dry_run",
            message: "--packet-out is only valid with --dry-run".to_string(),
        });
    }
    if args.tx_led.tx_led && args.dry_run && result != DiagnosticResult::Fail {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "tx_led_requires_live_tx",
            message: "--tx-led is only valid for live TX commands".to_string(),
        });
    }
    if args.tx_status.tx_status && args.dry_run && result != DiagnosticResult::Fail {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "tx_status_requires_live_tx",
            message: "--tx-status is only valid for live TX commands".to_string(),
        });
    }
    if args.tx_led.tx_led
        && args.tx_led.tx_led_hold_ms > MAX_TX_LED_HOLD_MS
        && result != DiagnosticResult::Fail
    {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "invalid_tx_led_hold",
            message: format!("--tx-led-hold-ms must be <= {MAX_TX_LED_HOLD_MS}"),
        });
    }
    if args.tx_status.tx_status
        && args.tx_status.tx_status_delay_ms > MAX_TX_STATUS_DELAY_MS
        && result != DiagnosticResult::Fail
    {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "invalid_tx_status_delay",
            message: format!("--tx-status-delay-ms must be <= {MAX_TX_STATUS_DELAY_MS}"),
        });
    }

    if args.dry_run && result != DiagnosticResult::Fail {
        match args.frame_hex.as_deref() {
            Some(hex) => match build_tx_dry_run_report(
                hex,
                channel.expect("channel resolved before dry run"),
                args.bandwidth,
                &args.tx_options,
                args.packet_out.clone(),
            ) {
                Ok(report) => {
                    result = DiagnosticResult::Pass;
                    tx_dry_run = Some(report);
                }
                Err(report_error) => {
                    result = DiagnosticResult::Fail;
                    error = Some(report_error);
                }
            },
            None => {
                result = DiagnosticResult::Fail;
                error = Some(DiagnosticErrorReport {
                    code: "missing_frame_hex",
                    message: "--dry-run requires --frame-hex so no test frame is invented"
                        .to_string(),
                });
            }
        }
    }

    if !args.dry_run && result != DiagnosticResult::Fail {
        if args.frame_hex.is_none() {
            result = DiagnosticResult::Fail;
            error = Some(DiagnosticErrorReport {
                code: "missing_frame_hex",
                message: "live tx-once requires --frame-hex so no test frame is invented"
                    .to_string(),
            });
        } else if !args.i_understand_this_transmits {
            result = DiagnosticResult::Fail;
            error = Some(DiagnosticErrorReport {
                code: "missing_tx_authorization",
                message: "live tx-once requires --i-understand-this-transmits".to_string(),
            });
        }
    }

    if !args.dry_run && result != DiagnosticResult::Fail {
        return tx_once_live_report(
            args,
            channel.expect("channel resolved before live tx"),
            tx_frame_len,
            tx_frame_source,
        );
    }

    let phases = if result == DiagnosticResult::Fail {
        vec![DiagnosticPhase {
            id: "argument_validation",
            status: DiagnosticPhaseStatus::Blocked,
            detail: "TX arguments did not pass local validation",
        }]
    } else if args.dry_run {
        vec![DiagnosticPhase {
            id: "tx_descriptor",
            status: DiagnosticPhaseStatus::Completed,
            detail: "built descriptor-prefixed packet without touching USB",
        }]
    } else {
        vec![
            DiagnosticPhase {
                id: "init",
                status: DiagnosticPhaseStatus::Pending,
                detail: "requires completed radio initialization",
            },
            DiagnosticPhase {
                id: "tx_descriptor",
                status: DiagnosticPhaseStatus::Pending,
                detail: "build RTL8812AU descriptor for one validated IEEE 802.11 frame",
            },
            DiagnosticPhase {
                id: "bulk_out",
                status: DiagnosticPhaseStatus::Pending,
                detail: "write one descriptor-prefixed frame to the bulk OUT endpoint",
            },
        ]
    };

    pending_report(PendingReportInput {
        command: "tx-once",
        selector: args.adapter.selector(),
        adapter: None,
        endpoints: None,
        channel,
        bandwidth: Some(args.bandwidth),
        firmware_path: None,
        firmware: None,
        init_dry_run: None,
        init_live: None,
        duration_ms: None,
        pcap_path: None,
        tx_frame_len,
        tx_frame_source,
        tx_dry_run,
        tx_live: None,
        rx_fixture: None,
        repeat_tx: None,
        counters: DiagnosticCounters::default(),
        result,
        phases,
        error,
        notes: if args.dry_run {
            vec!["dry run only: no USB bulk OUT write was issued"]
        } else {
            vec!["live TX requires --frame-hex and --i-understand-this-transmits"]
        },
    })
}

fn tx_once_live_report(
    args: TxOnceArgs,
    channel: Channel,
    tx_frame_len: Option<usize>,
    tx_frame_source: Option<&'static str>,
) -> PendingDiagnosticReport {
    let selector = args.adapter.selector();
    let bandwidth = args.bandwidth;
    let frame_hex = args
        .frame_hex
        .as_deref()
        .expect("live tx frame hex was validated");
    let frame = match parse_hex_bytes(frame_hex) {
        Ok(frame) => frame,
        Err(error) => {
            return tx_once_live_failure(TxOnceLiveFailureInput {
                selector,
                adapter: None,
                endpoints: None,
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                counters: DiagnosticCounters::default(),
                phase_id: "argument_validation",
                phase_detail: "TX frame hex could not be parsed",
                error: DiagnosticErrorReport {
                    code: "invalid_frame_hex",
                    message: error,
                },
            });
        }
    };
    let opts = tx_options_from_args(bandwidth, &args.tx_options);
    let packet_len = match build_tx_packet(&frame, channel, opts) {
        Ok(packet) => packet.len(),
        Err(error) => {
            return tx_once_live_failure(TxOnceLiveFailureInput {
                selector,
                adapter: None,
                endpoints: None,
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                counters: DiagnosticCounters::default(),
                phase_id: "tx_descriptor",
                phase_detail: "TX descriptor construction failed",
                error: DiagnosticErrorReport {
                    code: "tx_descriptor_failed",
                    message: error.to_string(),
                },
            });
        }
    };

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return tx_once_live_failure(TxOnceLiveFailureInput {
                selector,
                adapter: None,
                endpoints: None,
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                counters: DiagnosticCounters::default(),
                phase_id: "usb_claim",
                phase_detail: "no supported adapter matched the selector",
                error,
            });
        }
    };
    let mut claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return tx_once_live_failure(TxOnceLiveFailureInput {
                selector,
                adapter: Some(selected),
                endpoints: None,
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                counters: DiagnosticCounters::default(),
                phase_id: "usb_claim",
                phase_detail: "USB interface claim failed",
                error: DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            });
        }
    };
    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let bulk_out = match endpoints.bulk_out {
        Some(endpoint) => endpoint,
        None => {
            return tx_once_live_failure(TxOnceLiveFailureInput {
                selector,
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                counters: DiagnosticCounters::default(),
                phase_id: "bulk_out",
                phase_detail: "claimed interface has no bulk OUT endpoint",
                error: DiagnosticErrorReport {
                    code: "missing_bulk_out_endpoint",
                    message: "claimed interface has no bulk OUT endpoint".to_string(),
                },
            });
        }
    };

    let mut submit_counters = TxSubmitCounters::default();
    let mut tx_activity_led = tx_activity_led_report(&args.tx_led);
    let mut tx_status = tx_status_probe_report(&args.tx_status);
    if tx_activity_led.is_some() || tx_status.is_some() {
        let registers = Rtl8812auRegisterAccess::new(&claimed);
        tx_activity_led_step(&registers, &mut tx_activity_led, LedAction::On);
        tx_status_probe_pre(&registers, &mut tx_status);
    }
    match submit_tx_frame(
        &mut claimed,
        bulk_out,
        &frame,
        channel,
        opts,
        &mut submit_counters,
    ) {
        Ok(bytes_written) => {
            if tx_status.is_some() {
                let registers = Rtl8812auRegisterAccess::new(&claimed);
                tx_status_probe_post(&registers, &mut tx_status);
            }
            tx_activity_led_hold(&tx_activity_led);
            if tx_activity_led.is_some() {
                let registers = Rtl8812auRegisterAccess::new(&claimed);
                tx_activity_led_step(&registers, &mut tx_activity_led, LedAction::Off);
            }
            let mut counters = DiagnosticCounters {
                usb_bulk_out_writes: submit_counters.submitted,
                tx_frames: submit_counters.submitted,
                ..DiagnosticCounters::default()
            };
            add_tx_activity_led_counters(&mut counters, &tx_activity_led);
            add_tx_status_probe_counters(&mut counters, &tx_status);
            pending_report(PendingReportInput {
                command: "tx-once",
                selector,
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                channel: Some(channel),
                bandwidth: Some(bandwidth),
                firmware_path: None,
                firmware: None,
                init_dry_run: None,
                init_live: None,
                duration_ms: None,
                pcap_path: None,
                tx_frame_len,
                tx_frame_source,
                tx_dry_run: None,
                tx_live: Some(TxLiveReport {
                    bulk_out_endpoint: bulk_out,
                    bulk_out_endpoint_hex: format_value(bulk_out, 2),
                    frame_len: frame.len(),
                    packet_len,
                    bytes_written,
                    tx_options: opts,
                    tx_activity_led,
                    tx_status,
                    submit_counters,
                }),
                rx_fixture: None,
                repeat_tx: None,
                counters,
                result: DiagnosticResult::Pass,
                phases: vec![
                    DiagnosticPhase {
                        id: "usb_claim",
                        status: DiagnosticPhaseStatus::Completed,
                        detail: "claimed initialized adapter for live TX",
                    },
                    DiagnosticPhase {
                        id: "tx_descriptor",
                        status: DiagnosticPhaseStatus::Completed,
                        detail: "built RTL8812AU descriptor for one validated IEEE 802.11 frame",
                    },
                    DiagnosticPhase {
                        id: "bulk_out",
                        status: DiagnosticPhaseStatus::Completed,
                        detail: "wrote one descriptor-prefixed frame to the bulk OUT endpoint",
                    },
                ],
                error: None,
                notes: vec![
                    "live TX assumes the adapter has already completed init on the requested channel",
                    "one descriptor-prefixed frame was submitted; no RX loop was started",
                ],
            })
        }
        Err(error) => {
            if tx_status.is_some() {
                let registers = Rtl8812auRegisterAccess::new(&claimed);
                tx_status_probe_post(&registers, &mut tx_status);
            }
            tx_activity_led_hold(&tx_activity_led);
            if tx_activity_led.is_some() {
                let registers = Rtl8812auRegisterAccess::new(&claimed);
                tx_activity_led_step(&registers, &mut tx_activity_led, LedAction::Off);
            }
            let mut counters = DiagnosticCounters {
                usb_bulk_out_writes: submit_counters.submitted + submit_counters.failed,
                tx_frames: submit_counters.submitted,
                ..DiagnosticCounters::default()
            };
            add_tx_activity_led_counters(&mut counters, &tx_activity_led);
            add_tx_status_probe_counters(&mut counters, &tx_status);
            tx_once_live_failure(TxOnceLiveFailureInput {
                selector,
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                counters,
                phase_id: "bulk_out",
                phase_detail: "bulk OUT frame submission failed",
                error: DiagnosticErrorReport {
                    code: "tx_submit_failed",
                    message: error.to_string(),
                },
            })
        }
    }
}

struct TxOnceLiveFailureInput {
    selector: DeviceSelector,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    channel: Channel,
    bandwidth: Bandwidth,
    tx_frame_len: Option<usize>,
    tx_frame_source: Option<&'static str>,
    counters: DiagnosticCounters,
    phase_id: &'static str,
    phase_detail: &'static str,
    error: DiagnosticErrorReport,
}

fn tx_once_live_failure(input: TxOnceLiveFailureInput) -> PendingDiagnosticReport {
    pending_report(PendingReportInput {
        command: "tx-once",
        selector: input.selector,
        adapter: input.adapter,
        endpoints: input.endpoints,
        channel: Some(input.channel),
        bandwidth: Some(input.bandwidth),
        firmware_path: None,
        firmware: None,
        init_dry_run: None,
        init_live: None,
        duration_ms: None,
        pcap_path: None,
        tx_frame_len: input.tx_frame_len,
        tx_frame_source: input.tx_frame_source,
        tx_dry_run: None,
        tx_live: None,
        rx_fixture: None,
        repeat_tx: None,
        counters: input.counters,
        result: DiagnosticResult::Fail,
        phases: vec![DiagnosticPhase {
            id: input.phase_id,
            status: DiagnosticPhaseStatus::Blocked,
            detail: input.phase_detail,
        }],
        error: Some(input.error),
        notes: vec!["live TX stopped before a verified single-frame submission"],
    })
}

fn tx_repeat_report(args: TxRepeatArgs) -> PendingDiagnosticReport {
    let (channel, mut result, mut error) = resolve_report_channel(args.channel, args.bandwidth);
    let (tx_frame_len, tx_frame_source) =
        validate_tx_frame_arg(args.frame_hex.as_deref(), &mut result, &mut error);

    if args.frame_hex.is_none() && result != DiagnosticResult::Fail {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "missing_frame_hex",
            message: "live tx-repeat requires --frame-hex so no test frame is invented".to_string(),
        });
    }
    if !args.i_understand_this_transmits && result != DiagnosticResult::Fail {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "missing_tx_authorization",
            message: "repeated TX requires --i-understand-this-transmits".to_string(),
        });
    }
    if args.count == 0 && result != DiagnosticResult::Fail {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "invalid_repeat_count",
            message: "--count must be greater than zero".to_string(),
        });
    }
    if args.interval_ms == 0 && result != DiagnosticResult::Fail {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "invalid_repeat_interval",
            message: "--interval-ms must be greater than zero".to_string(),
        });
    }
    if args.tx_led.tx_led
        && args.tx_led.tx_led_hold_ms > MAX_TX_LED_HOLD_MS
        && result != DiagnosticResult::Fail
    {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "invalid_tx_led_hold",
            message: format!("--tx-led-hold-ms must be <= {MAX_TX_LED_HOLD_MS}"),
        });
    }
    if args.tx_status.tx_status
        && args.tx_status.tx_status_delay_ms > MAX_TX_STATUS_DELAY_MS
        && result != DiagnosticResult::Fail
    {
        result = DiagnosticResult::Fail;
        error = Some(DiagnosticErrorReport {
            code: "invalid_tx_status_delay",
            message: format!("--tx-status-delay-ms must be <= {MAX_TX_STATUS_DELAY_MS}"),
        });
    }

    if result != DiagnosticResult::Fail {
        return tx_repeat_live_report(
            args,
            channel.expect("channel resolved before live repeated tx"),
            tx_frame_len,
            tx_frame_source,
        );
    }

    let phases = if result == DiagnosticResult::Fail {
        vec![DiagnosticPhase {
            id: "argument_validation",
            status: DiagnosticPhaseStatus::Blocked,
            detail: "repeated TX arguments did not pass local validation",
        }]
    } else {
        vec![
            DiagnosticPhase {
                id: "init",
                status: DiagnosticPhaseStatus::Pending,
                detail: "requires completed radio initialization",
            },
            DiagnosticPhase {
                id: "repeat_gate",
                status: DiagnosticPhaseStatus::Pending,
                detail: "operator supplied explicit count, interval, channel, and authorization",
            },
            DiagnosticPhase {
                id: "bulk_out_loop",
                status: DiagnosticPhaseStatus::Pending,
                detail: "write descriptor-prefixed frame count times with the requested interval",
            },
        ]
    };

    pending_report(PendingReportInput {
        command: "tx-repeat",
        selector: args.adapter.selector(),
        adapter: None,
        endpoints: None,
        channel,
        bandwidth: Some(args.bandwidth),
        firmware_path: None,
        firmware: None,
        init_dry_run: None,
        init_live: None,
        duration_ms: None,
        pcap_path: None,
        tx_frame_len,
        tx_frame_source,
        tx_dry_run: None,
        tx_live: None,
        rx_fixture: None,
        repeat_tx: Some(RepeatTxReport {
            count: args.count,
            interval_ms: args.interval_ms,
            authorized: args.i_understand_this_transmits,
            bulk_out_endpoint: None,
            bulk_out_endpoint_hex: None,
            frame_len: tx_frame_len,
            packet_len: None,
            elapsed_ms: None,
            submitted_per_second: None,
            usb_bytes_per_second: None,
            cpu: None,
            tx_options: None,
            tx_activity_led: None,
            tx_status: None,
            submit_counters: TxSubmitCounters::default(),
        }),
        counters: DiagnosticCounters::default(),
        result,
        phases,
        error,
        notes: vec!["live repeated TX stopped before USB bulk OUT writes"],
    })
}

fn tx_repeat_live_report(
    args: TxRepeatArgs,
    channel: Channel,
    tx_frame_len: Option<usize>,
    tx_frame_source: Option<&'static str>,
) -> PendingDiagnosticReport {
    let selector = args.adapter.selector();
    let bandwidth = args.bandwidth;
    let frame_hex = args
        .frame_hex
        .as_deref()
        .expect("live repeated tx frame hex was validated");
    let frame = match parse_hex_bytes(frame_hex) {
        Ok(frame) => frame,
        Err(error) => {
            return tx_repeat_live_failure(TxRepeatLiveFailureInput {
                selector,
                adapter: None,
                endpoints: None,
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                repeat: repeat_tx_report_base(&args, tx_frame_len, None, None, None, None),
                counters: DiagnosticCounters::default(),
                phase_id: "argument_validation",
                phase_detail: "TX frame hex could not be parsed",
                error: DiagnosticErrorReport {
                    code: "invalid_frame_hex",
                    message: error,
                },
            });
        }
    };
    let opts = tx_options_from_args(bandwidth, &args.tx_options);
    let packet_len = match build_tx_packet(&frame, channel, opts) {
        Ok(packet) => packet.len(),
        Err(error) => {
            return tx_repeat_live_failure(TxRepeatLiveFailureInput {
                selector,
                adapter: None,
                endpoints: None,
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                repeat: repeat_tx_report_base(
                    &args,
                    Some(frame.len()),
                    None,
                    None,
                    None,
                    Some(opts),
                ),
                counters: DiagnosticCounters::default(),
                phase_id: "tx_descriptor",
                phase_detail: "TX descriptor construction failed",
                error: DiagnosticErrorReport {
                    code: "tx_descriptor_failed",
                    message: error.to_string(),
                },
            });
        }
    };

    let selected = match select_supported_adapter(selector) {
        Ok(device) => device,
        Err(error) => {
            return tx_repeat_live_failure(TxRepeatLiveFailureInput {
                selector,
                adapter: None,
                endpoints: None,
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                repeat: repeat_tx_report_base(
                    &args,
                    Some(frame.len()),
                    Some(packet_len),
                    None,
                    None,
                    Some(opts),
                ),
                counters: DiagnosticCounters::default(),
                phase_id: "usb_claim",
                phase_detail: "no supported adapter matched the selector",
                error,
            });
        }
    };
    let mut claimed = match radio_core::usb::claim_usb_device(&selected) {
        Ok(claimed) => claimed,
        Err(error) => {
            return tx_repeat_live_failure(TxRepeatLiveFailureInput {
                selector,
                adapter: Some(selected),
                endpoints: None,
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                repeat: repeat_tx_report_base(
                    &args,
                    Some(frame.len()),
                    Some(packet_len),
                    None,
                    None,
                    Some(opts),
                ),
                counters: DiagnosticCounters::default(),
                phase_id: "usb_claim",
                phase_detail: "USB interface claim failed",
                error: DiagnosticErrorReport {
                    code: "usb_claim_failed",
                    message: error.to_string(),
                },
            });
        }
    };
    let adapter = claimed.info.clone();
    let endpoints = claimed.endpoints.clone();
    let bulk_out = match endpoints.bulk_out {
        Some(endpoint) => endpoint,
        None => {
            return tx_repeat_live_failure(TxRepeatLiveFailureInput {
                selector,
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                repeat: repeat_tx_report_base(
                    &args,
                    Some(frame.len()),
                    Some(packet_len),
                    None,
                    None,
                    Some(opts),
                ),
                counters: DiagnosticCounters::default(),
                phase_id: "bulk_out_loop",
                phase_detail: "claimed interface has no bulk OUT endpoint",
                error: DiagnosticErrorReport {
                    code: "missing_bulk_out_endpoint",
                    message: "claimed interface has no bulk OUT endpoint".to_string(),
                },
            });
        }
    };

    let mut submit_counters = TxSubmitCounters::default();
    let mut tx_activity_led = tx_activity_led_report(&args.tx_led);
    let mut tx_status = tx_status_probe_report(&args.tx_status);
    if tx_activity_led.is_some() || tx_status.is_some() {
        let registers = Rtl8812auRegisterAccess::new(&claimed);
        tx_activity_led_step(&registers, &mut tx_activity_led, LedAction::On);
        tx_status_probe_pre(&registers, &mut tx_status);
    }
    let cpu_started = process_cpu_usage();
    let started = Instant::now();
    for index in 0..args.count {
        if let Err(error) = submit_tx_frame(
            &mut claimed,
            bulk_out,
            &frame,
            channel,
            opts,
            &mut submit_counters,
        ) {
            if tx_status.is_some() {
                let registers = Rtl8812auRegisterAccess::new(&claimed);
                tx_status_probe_post(&registers, &mut tx_status);
            }
            tx_activity_led_hold(&tx_activity_led);
            if tx_activity_led.is_some() {
                let registers = Rtl8812auRegisterAccess::new(&claimed);
                tx_activity_led_step(&registers, &mut tx_activity_led, LedAction::Off);
            }
            let mut counters = DiagnosticCounters {
                usb_bulk_out_writes: submit_counters.submitted + submit_counters.failed,
                tx_frames: submit_counters.submitted,
                dropped_frames: submit_counters.failed + submit_counters.rejected,
                ..DiagnosticCounters::default()
            };
            add_tx_activity_led_counters(&mut counters, &tx_activity_led);
            add_tx_status_probe_counters(&mut counters, &tx_status);
            let elapsed_ms = elapsed_ms_u64(started);
            let cpu = cpu_usage_delta(cpu_started, process_cpu_usage(), elapsed_ms);
            let mut repeat = repeat_tx_report_with_submit(
                &args,
                Some(frame.len()),
                Some(packet_len),
                Some(bulk_out),
                Some(elapsed_ms),
                Some(opts),
                submit_counters,
            );
            repeat.cpu = cpu;
            repeat.tx_activity_led = tx_activity_led;
            repeat.tx_status = tx_status;
            return tx_repeat_live_failure(TxRepeatLiveFailureInput {
                selector,
                adapter: Some(adapter),
                endpoints: Some(endpoints),
                channel,
                bandwidth,
                tx_frame_len,
                tx_frame_source,
                repeat,
                counters,
                phase_id: "bulk_out_loop",
                phase_detail: "bulk OUT frame submission failed during repeated TX",
                error: DiagnosticErrorReport {
                    code: "tx_submit_failed",
                    message: format!("frame {}: {error}", index + 1),
                },
            });
        }

        if index + 1 < args.count {
            std::thread::sleep(Duration::from_millis(args.interval_ms));
        }
    }
    if tx_status.is_some() {
        let registers = Rtl8812auRegisterAccess::new(&claimed);
        tx_status_probe_post(&registers, &mut tx_status);
    }
    tx_activity_led_hold(&tx_activity_led);
    if tx_activity_led.is_some() {
        let registers = Rtl8812auRegisterAccess::new(&claimed);
        tx_activity_led_step(&registers, &mut tx_activity_led, LedAction::Off);
    }
    let elapsed_ms = elapsed_ms_u64(started);
    let mut counters = DiagnosticCounters {
        usb_bulk_out_writes: submit_counters.submitted,
        tx_frames: submit_counters.submitted,
        ..DiagnosticCounters::default()
    };
    add_tx_activity_led_counters(&mut counters, &tx_activity_led);
    add_tx_status_probe_counters(&mut counters, &tx_status);
    let cpu = cpu_usage_delta(cpu_started, process_cpu_usage(), elapsed_ms);
    let mut repeat = repeat_tx_report_with_submit(
        &args,
        Some(frame.len()),
        Some(packet_len),
        Some(bulk_out),
        Some(elapsed_ms),
        Some(opts),
        submit_counters,
    );
    repeat.cpu = cpu;
    repeat.tx_activity_led = tx_activity_led;
    repeat.tx_status = tx_status;

    pending_report(PendingReportInput {
        command: "tx-repeat",
        selector,
        adapter: Some(adapter),
        endpoints: Some(endpoints),
        channel: Some(channel),
        bandwidth: Some(bandwidth),
        firmware_path: None,
        firmware: None,
        init_dry_run: None,
        init_live: None,
        duration_ms: Some(elapsed_ms),
        pcap_path: None,
        tx_frame_len,
        tx_frame_source,
        tx_dry_run: None,
        tx_live: None,
        rx_fixture: None,
        repeat_tx: Some(repeat),
        counters,
        result: DiagnosticResult::Pass,
        phases: vec![
            DiagnosticPhase {
                id: "usb_claim",
                status: DiagnosticPhaseStatus::Completed,
                detail: "claimed initialized adapter for live repeated TX",
            },
            DiagnosticPhase {
                id: "repeat_gate",
                status: DiagnosticPhaseStatus::Completed,
                detail: "operator supplied explicit count, interval, channel, frame, and authorization",
            },
            DiagnosticPhase {
                id: "bulk_out_loop",
                status: DiagnosticPhaseStatus::Completed,
                detail: "submitted descriptor-prefixed frames to bulk OUT with requested pacing",
            },
        ],
        error: None,
        notes: vec![
            "live repeated TX assumes the adapter has already completed init on the requested channel",
            "no RX loop or independent over-the-air verification was started",
        ],
    })
}

struct TxRepeatLiveFailureInput {
    selector: DeviceSelector,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    channel: Channel,
    bandwidth: Bandwidth,
    tx_frame_len: Option<usize>,
    tx_frame_source: Option<&'static str>,
    repeat: RepeatTxReport,
    counters: DiagnosticCounters,
    phase_id: &'static str,
    phase_detail: &'static str,
    error: DiagnosticErrorReport,
}

fn tx_repeat_live_failure(input: TxRepeatLiveFailureInput) -> PendingDiagnosticReport {
    pending_report(PendingReportInput {
        command: "tx-repeat",
        selector: input.selector,
        adapter: input.adapter,
        endpoints: input.endpoints,
        channel: Some(input.channel),
        bandwidth: Some(input.bandwidth),
        firmware_path: None,
        firmware: None,
        init_dry_run: None,
        init_live: None,
        duration_ms: input.repeat.elapsed_ms,
        pcap_path: None,
        tx_frame_len: input.tx_frame_len,
        tx_frame_source: input.tx_frame_source,
        tx_dry_run: None,
        tx_live: None,
        rx_fixture: None,
        repeat_tx: Some(input.repeat),
        counters: input.counters,
        result: DiagnosticResult::Fail,
        phases: vec![DiagnosticPhase {
            id: input.phase_id,
            status: DiagnosticPhaseStatus::Blocked,
            detail: input.phase_detail,
        }],
        error: Some(input.error),
        notes: vec!["live repeated TX stopped before completing the requested frame count"],
    })
}

fn repeat_tx_report_base(
    args: &TxRepeatArgs,
    frame_len: Option<usize>,
    packet_len: Option<usize>,
    bulk_out_endpoint: Option<u8>,
    elapsed_ms: Option<u64>,
    tx_options: Option<TxOptions>,
) -> RepeatTxReport {
    repeat_tx_report_with_submit(
        args,
        frame_len,
        packet_len,
        bulk_out_endpoint,
        elapsed_ms,
        tx_options,
        TxSubmitCounters::default(),
    )
}

fn repeat_tx_report_with_submit(
    args: &TxRepeatArgs,
    frame_len: Option<usize>,
    packet_len: Option<usize>,
    bulk_out_endpoint: Option<u8>,
    elapsed_ms: Option<u64>,
    tx_options: Option<TxOptions>,
    submit_counters: TxSubmitCounters,
) -> RepeatTxReport {
    RepeatTxReport {
        count: args.count,
        interval_ms: args.interval_ms,
        authorized: args.i_understand_this_transmits,
        bulk_out_endpoint,
        bulk_out_endpoint_hex: bulk_out_endpoint.map(|endpoint| format_value(endpoint, 2)),
        frame_len,
        packet_len,
        elapsed_ms,
        submitted_per_second: rate_per_second(submit_counters.submitted, elapsed_ms),
        usb_bytes_per_second: rate_per_second(submit_counters.bytes_written, elapsed_ms),
        cpu: None,
        tx_options,
        tx_activity_led: None,
        tx_status: None,
        submit_counters,
    }
}

fn elapsed_ms_u64(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn rate_per_second(count: u64, elapsed_ms: Option<u64>) -> Option<f64> {
    let elapsed_ms = elapsed_ms?;
    if elapsed_ms == 0 {
        return None;
    }
    Some(count as f64 * 1000.0 / elapsed_ms as f64)
}

#[cfg(unix)]
fn process_cpu_usage() -> Option<CpuUsageSnapshot> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    Some(CpuUsageSnapshot {
        user_us: timeval_to_us(usage.ru_utime),
        system_us: timeval_to_us(usage.ru_stime),
    })
}

#[cfg(not(unix))]
fn process_cpu_usage() -> Option<CpuUsageSnapshot> {
    None
}

#[cfg(unix)]
fn timeval_to_us(value: libc::timeval) -> u64 {
    let seconds = value.tv_sec as i128;
    let micros = value.tv_usec as i128;
    if seconds < 0 || micros < 0 {
        0
    } else {
        (seconds * 1_000_000 + micros) as u64
    }
}

fn cpu_usage_delta(
    before: Option<CpuUsageSnapshot>,
    after: Option<CpuUsageSnapshot>,
    elapsed_ms: u64,
) -> Option<CpuUsageReport> {
    let before = before?;
    let after = after?;
    let user_us = after.user_us.saturating_sub(before.user_us);
    let system_us = after.system_us.saturating_sub(before.system_us);
    let total_us = user_us + system_us;
    Some(CpuUsageReport {
        user_us,
        system_us,
        total_us,
        percent_one_core: if elapsed_ms == 0 {
            None
        } else {
            Some(total_us as f64 * 100.0 / (elapsed_ms as f64 * 1000.0))
        },
    })
}

fn validate_tx_frame_arg(
    frame_hex: Option<&str>,
    result: &mut DiagnosticResult,
    error: &mut Option<DiagnosticErrorReport>,
) -> (Option<usize>, Option<&'static str>) {
    match frame_hex {
        Some(hex) if *result != DiagnosticResult::Fail => match parse_hex_bytes(hex) {
            Ok(frame) => match validate_ieee80211_frame(&frame) {
                Ok(()) => (Some(frame.len()), Some("operator_hex")),
                Err(frame_error) => {
                    *result = DiagnosticResult::Fail;
                    *error = Some(DiagnosticErrorReport {
                        code: "invalid_ieee80211_frame",
                        message: frame_error.to_string(),
                    });
                    (Some(frame.len()), Some("operator_hex"))
                }
            },
            Err(parse_error) => {
                *result = DiagnosticResult::Fail;
                *error = Some(DiagnosticErrorReport {
                    code: "invalid_frame_hex",
                    message: parse_error,
                });
                (None, Some("operator_hex"))
            }
        },
        Some(_) => (None, Some("operator_hex")),
        None => (None, None),
    }
}

fn tx_options_from_args(bandwidth: Bandwidth, args: &TxOptionArgs) -> TxOptions {
    TxOptions {
        rate: args.tx_rate,
        bandwidth,
        short_gi: args.short_gi,
        ldpc: args.ldpc,
        stbc: args.stbc,
        ..TxOptions::default()
    }
}

fn build_tx_dry_run_report(
    frame_hex: &str,
    channel: Channel,
    bandwidth: Bandwidth,
    tx_option_args: &TxOptionArgs,
    packet_out: Option<PathBuf>,
) -> std::result::Result<TxDryRunReport, DiagnosticErrorReport> {
    let frame = parse_hex_bytes(frame_hex).map_err(|message| DiagnosticErrorReport {
        code: "invalid_frame_hex",
        message,
    })?;
    let opts = tx_options_from_args(bandwidth, tx_option_args);
    let packet = build_tx_packet(&frame, channel, opts).map_err(|error| DiagnosticErrorReport {
        code: "tx_descriptor_build_failed",
        message: error.to_string(),
    })?;

    if let Some(path) = &packet_out {
        fs::write(path, &packet).map_err(|error| DiagnosticErrorReport {
            code: "packet_out_write_failed",
            message: format!("failed to write {}: {error}", path.display()),
        })?;
    }

    let descriptor_len = 40;
    Ok(TxDryRunReport {
        descriptor_len,
        frame_len: frame.len(),
        packet_len: packet.len(),
        packet_byte_sum: byte_sum(&packet),
        descriptor_hex: encode_hex(&packet[..descriptor_len]),
        packet_out,
        tx_options: opts,
    })
}

fn build_init_dry_run_report(
    firmware: &FirmwareImage,
    trace_out: Option<PathBuf>,
) -> std::result::Result<InitDryRunReport, DiagnosticErrorReport> {
    let plan = plan_rtl8812au_init(firmware).map_err(|error| DiagnosticErrorReport {
        code: "init_plan_failed",
        message: error.to_string(),
    })?;

    if let Some(path) = &trace_out {
        let events = plan.trace_events();
        let output =
            serde_json::to_string_pretty(&events).map_err(|error| DiagnosticErrorReport {
                code: "init_trace_encode_failed",
                message: error.to_string(),
            })?;
        fs::write(path, output).map_err(|error| DiagnosticErrorReport {
            code: "init_trace_write_failed",
            message: format!("failed to write {}: {error}", path.display()),
        })?;
    }

    Ok(init_dry_run_report_from_plan(plan, trace_out))
}

fn init_dry_run_report_from_plan(
    plan: InitDryRunPlan,
    trace_out: Option<PathBuf>,
) -> InitDryRunReport {
    let phase_counts = plan.phase_counts();
    let planned_transfers = plan.transfers.len();
    InitDryRunReport {
        firmware_len: plan.firmware_len,
        firmware_chunk_size: plan.firmware_chunk_size,
        source_repo: plan.source_repo,
        source_commit: plan.source_commit,
        planned_transfers,
        trace_out,
        phase_counts,
        transfers: plan.transfers,
    }
}

fn load_firmware_with_report(
    path: &Path,
) -> std::result::Result<(FirmwareImage, FirmwareReport), String> {
    let image = FirmwareImage::load_external(path).map_err(|error| error.to_string())?;
    let report = firmware_report_from_image(path, &image)?;
    Ok((image, report))
}

fn firmware_report_from_image(
    path: &Path,
    image: &FirmwareImage,
) -> std::result::Result<FirmwareReport, String> {
    let chunk_size = MAX_DLFW_PAGE_SIZE;
    let chunk_count = image
        .chunks(chunk_size)
        .map_err(|error| error.to_string())?
        .count();
    Ok(FirmwareReport {
        source: path.to_path_buf(),
        len: image.len,
        byte_sum: image.byte_sum,
        chunk_size,
        chunk_count,
    })
}

fn macos_usb_state_report(args: MacosUsbStateArgs) -> MacosUsbStateReport {
    let selector = args.adapter.selector();
    let started_at_unix_ms = started_at_unix_ms();
    let platform = platform_info();

    if platform.os != "macos" {
        return MacosUsbStateReport {
            schema_version: 1,
            command: "macos-usb-state",
            started_at_unix_ms,
            platform,
            selector,
            result: DiagnosticResult::Fail,
            devices: Vec::new(),
            error: Some(DiagnosticErrorReport {
                code: "unsupported_platform",
                message: "macos-usb-state requires macOS ioreg".to_string(),
            }),
            notes: vec![
                "This diagnostic reads the macOS IOUSB registry only; it does not claim USB interfaces or issue control/bulk transfers.",
            ],
        };
    }

    let output = match std::process::Command::new("/usr/sbin/ioreg")
        .args(["-p", "IOUSB", "-l", "-w0"])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return MacosUsbStateReport {
                schema_version: 1,
                command: "macos-usb-state",
                started_at_unix_ms,
                platform,
                selector,
                result: DiagnosticResult::Fail,
                devices: Vec::new(),
                error: Some(DiagnosticErrorReport {
                    code: "ioreg_launch_failed",
                    message: error.to_string(),
                }),
                notes: vec![
                    "This diagnostic reads the macOS IOUSB registry only; it does not claim USB interfaces or issue control/bulk transfers.",
                ],
            };
        }
    };

    if !output.status.success() {
        return MacosUsbStateReport {
            schema_version: 1,
            command: "macos-usb-state",
            started_at_unix_ms,
            platform,
            selector,
            result: DiagnosticResult::Fail,
            devices: Vec::new(),
            error: Some(DiagnosticErrorReport {
                code: "ioreg_failed",
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            }),
            notes: vec![
                "This diagnostic reads the macOS IOUSB registry only; it does not claim USB interfaces or issue control/bulk transfers.",
            ],
        };
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let devices = parse_macos_ioreg_usb_devices(&text, selector);
    let result = if devices.is_empty() {
        DiagnosticResult::Fail
    } else {
        DiagnosticResult::Pass
    };
    let error = if devices.is_empty() {
        Some(DiagnosticErrorReport {
            code: "macos_usb_device_not_found",
            message: "no IOUSBHostDevice entries matched the selected USB identity".to_string(),
        })
    } else {
        None
    };

    MacosUsbStateReport {
        schema_version: 1,
        command: "macos-usb-state",
        started_at_unix_ms,
        platform,
        selector,
        result,
        devices,
        error,
        notes: vec![
            "This diagnostic reads the macOS IOUSB registry only; it does not claim USB interfaces or issue control/bulk transfers.",
            "A device with no current configuration or interface children will not be usable by the rusb radio path until macOS finishes configuring it.",
        ],
    }
}

fn parse_macos_ioreg_usb_devices(
    input: &str,
    selector: DeviceSelector,
) -> Vec<MacosUsbDeviceState> {
    let mut devices = Vec::new();
    let mut current: Option<MacosUsbDeviceState> = None;

    for line in input.lines() {
        if line.contains("<class IOUSBHostDevice") {
            if let Some(device) = current.take() {
                if macos_usb_state_matches_selector(&device, selector) {
                    devices.push(finalize_macos_usb_state(device));
                }
            }
            current = Some(parse_macos_ioreg_device_header(line));
            continue;
        }

        if line.contains("+-o ")
            && line.contains("<class ")
            && !line.contains("<class IOUSBHostInterface")
        {
            if let Some(device) = current.take() {
                if macos_usb_state_matches_selector(&device, selector) {
                    devices.push(finalize_macos_usb_state(device));
                }
            }
            continue;
        }

        let Some(device) = current.as_mut() else {
            continue;
        };

        if line.contains("<class IOUSBHostInterface") {
            device.has_interface_children = true;
        }

        if let Some((key, value)) = parse_macos_ioreg_property(line) {
            apply_macos_ioreg_property(device, key, value);
        }
    }

    if let Some(device) = current.take() {
        if macos_usb_state_matches_selector(&device, selector) {
            devices.push(finalize_macos_usb_state(device));
        }
    }

    devices
}

fn parse_macos_ioreg_device_header(line: &str) -> MacosUsbDeviceState {
    let after_marker = line
        .split_once("+-o ")
        .map(|(_, rest)| rest)
        .unwrap_or(line)
        .trim();
    let (identity, status) = after_marker
        .split_once("  <")
        .map(|(identity, status)| {
            (
                identity.trim(),
                status.trim_end_matches('>').trim().to_string(),
            )
        })
        .unwrap_or((after_marker, String::new()));
    let (name, location_path) = identity
        .rsplit_once('@')
        .map(|(name, location)| (name.trim().to_string(), Some(location.trim().to_string())))
        .unwrap_or((identity.trim().to_string(), None));

    MacosUsbDeviceState {
        name,
        location_path,
        registered: !status.contains("!registered"),
        matched: !status.contains("!matched"),
        active: status.contains("active"),
        status,
        ..MacosUsbDeviceState::default()
    }
}

fn parse_macos_ioreg_property(line: &str) -> Option<(&str, &str)> {
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    let key = &rest[..end];
    let (_, value) = rest[end + 1..].split_once('=')?;
    Some((key, value.trim()))
}

fn apply_macos_ioreg_property(device: &mut MacosUsbDeviceState, key: &str, value: &str) {
    match key {
        "idVendor" => {
            device.vid = parse_ioreg_u64(value).and_then(|value| u16::try_from(value).ok());
            device.vid_hex = device.vid.map(|vid| format_value(vid, 4));
        }
        "idProduct" => {
            device.pid = parse_ioreg_u64(value).and_then(|value| u16::try_from(value).ok());
            device.pid_hex = device.pid.map(|pid| format_value(pid, 4));
        }
        "USB Vendor Name" | "kUSBVendorString" if device.vendor_name.is_none() => {
            device.vendor_name = parse_ioreg_string(value);
        }
        "USB Product Name" | "kUSBProductString" if device.product_name.is_none() => {
            device.product_name = parse_ioreg_string(value);
        }
        "USB Serial Number" | "kUSBSerialNumberString" if device.serial_number.is_none() => {
            device.serial_number = parse_ioreg_string(value);
        }
        "USB Address" | "kUSBAddress" if device.usb_address.is_none() => {
            device.usb_address = parse_ioreg_u64(value);
        }
        "locationID" => {
            device.location_id = parse_ioreg_u64(value);
            device.location_id_hex = device.location_id.map(|location| format_value(location, 8));
        }
        "USBSpeed" => device.usb_speed_code = parse_ioreg_u64(value),
        "UsbLinkSpeed" => device.usb_link_speed_bps = parse_ioreg_u64(value),
        "bNumConfigurations" => device.b_num_configurations = parse_ioreg_u64(value),
        "kUSBCurrentConfiguration" => device.current_configuration = parse_ioreg_u64(value),
        "kUSBPreferredConfiguration" => device.preferred_configuration = parse_ioreg_u64(value),
        "UsbEnumerationState" => device.enumeration_state = parse_ioreg_u64(value),
        _ => {}
    }
}

fn finalize_macos_usb_state(mut device: MacosUsbDeviceState) -> MacosUsbDeviceState {
    device.has_current_configuration = device.current_configuration.is_some();
    device
}

fn macos_usb_state_matches_selector(
    device: &MacosUsbDeviceState,
    selector: DeviceSelector,
) -> bool {
    let vid_matches = match selector.vid {
        Some(vid) => device.vid == Some(vid),
        None => true,
    };
    let pid_matches = match selector.pid {
        Some(pid) => device.pid == Some(pid),
        None => true,
    };
    let address_matches = match selector.address {
        Some(address) => device.usb_address == Some(u64::from(address)),
        None => true,
    };

    selector.bus.is_none() && vid_matches && pid_matches && address_matches
}

fn parse_ioreg_string(value: &str) -> Option<String> {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(ToOwned::to_owned)
}

fn parse_ioreg_u64(value: &str) -> Option<u64> {
    let value = value.trim().trim_end_matches(',');
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn trace_compare_report(args: TraceCompareArgs) -> TraceCompareReport {
    let started_at_unix_ms = started_at_unix_ms();
    let platform = platform_info();
    let expected = read_trace_events(&args.expected);
    let observed = read_trace_events(&args.observed);

    match (expected, observed) {
        (Ok(expected), Ok(observed)) => {
            let comparison = compare_usb_traces(&expected, &observed);
            let result = if comparison.result.is_failure() {
                DiagnosticResult::Fail
            } else {
                DiagnosticResult::Pass
            };
            TraceCompareReport {
                schema_version: 1,
                command: "trace-compare",
                started_at_unix_ms,
                platform,
                expected_path: args.expected,
                observed_path: args.observed,
                result,
                comparison: Some(comparison),
                error: None,
            }
        }
        (Err(message), _) => TraceCompareReport {
            schema_version: 1,
            command: "trace-compare",
            started_at_unix_ms,
            platform,
            expected_path: args.expected,
            observed_path: args.observed,
            result: DiagnosticResult::Fail,
            comparison: None,
            error: Some(DiagnosticErrorReport {
                code: "expected_trace_read_failed",
                message,
            }),
        },
        (_, Err(message)) => TraceCompareReport {
            schema_version: 1,
            command: "trace-compare",
            started_at_unix_ms,
            platform,
            expected_path: args.expected,
            observed_path: args.observed,
            result: DiagnosticResult::Fail,
            comparison: None,
            error: Some(DiagnosticErrorReport {
                code: "observed_trace_read_failed",
                message,
            }),
        },
    }
}

fn trace_import_report(args: TraceImportArgs) -> TraceImportReport {
    let started_at_unix_ms = started_at_unix_ms();
    let platform = platform_info();

    let input = match fs::read_to_string(&args.input) {
        Ok(input) => input,
        Err(error) => {
            return TraceImportReport {
                schema_version: 1,
                command: "trace-import",
                started_at_unix_ms,
                platform,
                input_path: args.input,
                output_path: args.output,
                result: DiagnosticResult::Fail,
                import: None,
                error: Some(DiagnosticErrorReport {
                    code: "trace_input_read_failed",
                    message: error.to_string(),
                }),
            };
        }
    };

    let imported = import_usbmon_text(&input);
    if !imported.errors.is_empty() {
        return TraceImportReport {
            schema_version: 1,
            command: "trace-import",
            started_at_unix_ms,
            platform,
            input_path: args.input,
            output_path: args.output,
            result: DiagnosticResult::Fail,
            import: Some(imported),
            error: Some(DiagnosticErrorReport {
                code: "trace_import_failed",
                message: "usbmon text contained malformed recognized transfer lines".to_string(),
            }),
        };
    }

    let output_json = match serde_json::to_string_pretty(&imported.events) {
        Ok(output_json) => output_json,
        Err(error) => {
            return TraceImportReport {
                schema_version: 1,
                command: "trace-import",
                started_at_unix_ms,
                platform,
                input_path: args.input,
                output_path: args.output,
                result: DiagnosticResult::Fail,
                import: Some(imported),
                error: Some(DiagnosticErrorReport {
                    code: "trace_output_encode_failed",
                    message: error.to_string(),
                }),
            };
        }
    };

    if let Err(error) = fs::write(&args.output, output_json) {
        return TraceImportReport {
            schema_version: 1,
            command: "trace-import",
            started_at_unix_ms,
            platform,
            input_path: args.input,
            output_path: args.output,
            result: DiagnosticResult::Fail,
            import: Some(imported),
            error: Some(DiagnosticErrorReport {
                code: "trace_output_write_failed",
                message: error.to_string(),
            }),
        };
    }

    TraceImportReport {
        schema_version: 1,
        command: "trace-import",
        started_at_unix_ms,
        platform,
        input_path: args.input,
        output_path: args.output,
        result: DiagnosticResult::Pass,
        import: Some(imported),
        error: None,
    }
}

fn read_trace_events(path: &Path) -> std::result::Result<Vec<UsbTraceEvent>, String> {
    let data =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&data).map_err(|error| format!("parse {}: {error}", path.display()))
}

struct PendingReportInput {
    command: &'static str,
    selector: DeviceSelector,
    adapter: Option<UsbDeviceInfo>,
    endpoints: Option<UsbEndpoints>,
    channel: Option<Channel>,
    bandwidth: Option<Bandwidth>,
    firmware_path: Option<PathBuf>,
    firmware: Option<FirmwareReport>,
    init_dry_run: Option<InitDryRunReport>,
    init_live: Option<InitLiveReport>,
    duration_ms: Option<u64>,
    pcap_path: Option<PathBuf>,
    tx_frame_len: Option<usize>,
    tx_frame_source: Option<&'static str>,
    tx_dry_run: Option<TxDryRunReport>,
    tx_live: Option<TxLiveReport>,
    rx_fixture: Option<RxFixtureReport>,
    repeat_tx: Option<RepeatTxReport>,
    counters: DiagnosticCounters,
    result: DiagnosticResult,
    phases: Vec<DiagnosticPhase>,
    error: Option<DiagnosticErrorReport>,
    notes: Vec<&'static str>,
}

fn pending_report(input: PendingReportInput) -> PendingDiagnosticReport {
    PendingDiagnosticReport {
        schema_version: 1,
        command: input.command,
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        selector: input.selector,
        adapter: input.adapter,
        endpoints: input.endpoints,
        channel: input.channel,
        bandwidth: input.bandwidth,
        firmware_path: input.firmware_path,
        firmware: input.firmware,
        init_dry_run: input.init_dry_run,
        init_live: input.init_live,
        duration_ms: input.duration_ms,
        pcap_path: input.pcap_path,
        tx_frame_len: input.tx_frame_len,
        tx_frame_source: input.tx_frame_source,
        tx_dry_run: input.tx_dry_run,
        tx_live: input.tx_live,
        rx_fixture: input.rx_fixture,
        repeat_tx: input.repeat_tx,
        result: input.result,
        phases: input.phases,
        counters: input.counters,
        error: input.error,
        notes: input.notes,
    }
}

fn resolve_report_channel(
    channel_number: u8,
    bandwidth: Bandwidth,
) -> (
    Option<Channel>,
    DiagnosticResult,
    Option<DiagnosticErrorReport>,
) {
    match resolve_channel(channel_number, bandwidth) {
        Ok(channel) => (Some(channel), DiagnosticResult::NotImplemented, None),
        Err(message) => (
            None,
            DiagnosticResult::Fail,
            Some(DiagnosticErrorReport {
                code: "unsupported_channel",
                message,
            }),
        ),
    }
}

fn resolve_channel(
    channel_number: u8,
    bandwidth: Bandwidth,
) -> std::result::Result<Channel, String> {
    let channel = Channel::from_number(channel_number).map_err(|error| error.to_string())?;
    if !channel.supports_bandwidth(bandwidth) {
        return Err(format!(
            "channel {} does not support {} MHz bandwidth",
            channel.number,
            bandwidth.mhz()
        ));
    }
    Ok(channel)
}

fn parse_hex_bytes(input: &str) -> std::result::Result<Vec<u8>, String> {
    let compact: String = input
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != ':' && *ch != '-' && *ch != '_')
        .collect();
    if compact.len() % 2 != 0 {
        return Err("hex string must contain an even number of digits".to_string());
    }

    (0..compact.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&compact[index..index + 2], 16)
                .map_err(|error| format!("invalid hex byte at offset {index}: {error}"))
        })
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn byte_sum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |acc, byte| acc.wrapping_add(u32::from(*byte)))
}

fn started_at_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        family: std::env::consts::FAMILY,
        arch: std::env::consts::ARCH,
    }
}

#[derive(Debug, Serialize)]
struct StagesReport {
    schema_version: u8,
    command: &'static str,
    started_at_unix_ms: u64,
    platform: PlatformInfo,
    result: DiagnosticResult,
    stages: Vec<VerificationStage>,
}

#[derive(Debug, Serialize)]
struct VerificationStage {
    id: &'static str,
    command: &'static str,
    purpose: &'static str,
    prerequisites: &'static [&'static str],
    pass_signal: &'static str,
}

fn stages_report() -> StagesReport {
    StagesReport {
        schema_version: 1,
        command: "stages",
        started_at_unix_ms: started_at_unix_ms(),
        platform: platform_info(),
        result: DiagnosticResult::Pass,
        stages: vec![
            VerificationStage {
                id: "usb-probe",
                command: "wfb-radio-diag usb-probe",
                purpose: "Discover supported RTL8812AU adapters, walk descriptors, and claim/release interface 0.",
                prerequisites: &["Mac host", "optional AWUS036ACH attached"],
                pass_signal: "Supported adapter found and interface claim succeeds.",
            },
            VerificationStage {
                id: "macos-usb-state",
                command: "wfb-radio-diag macos-usb-state --vid 0x0bda --pid 0x8812",
                purpose: "Inspect macOS IOUSB registry state for adapters that IOKit can see before libusb can enumerate or claim them.",
                prerequisites: &["macOS host", "optional AWUS036ACH attached"],
                pass_signal: "The selected IOUSBHostDevice entry is reported with registered/matched/configured/interface state.",
            },
            VerificationStage {
                id: "macos-reg-smoke",
                command: "wfb-radio-diag macos-reg-smoke --vid 0x0bda --pid 0x8812",
                purpose: "Read RTL8812AU registers through macOS IOUSBHost default-control transfers when libusb cannot enumerate the device.",
                prerequisites: &["macOS host", "IOUSBHostDevice visible in macos-usb-state"],
                pass_signal: "The register-smoke read set returns full-width values without interface claim or bulk traffic.",
            },
            VerificationStage {
                id: "macos-efuse-dump",
                command: "wfb-radio-diag macos-efuse-dump --vid 0x0bda --pid 0x8812 --i-understand-this-writes-control-registers",
                purpose: "Read RTL8812AU EFUSE through macOS IOUSBHost default-control transfers when libusb cannot enumerate the device.",
                prerequisites: &[
                    "macos-reg-smoke pass",
                    "bench authorization for EFUSE control-register writes",
                ],
                pass_signal: "Physical EFUSE bytes and the decoded logical map are reported without interface claim or bulk traffic.",
            },
            VerificationStage {
                id: "reg-smoke",
                command: "wfb-radio-diag reg-smoke",
                purpose: "Claim the adapter and perform read-only RTL8812AU vendor control reads.",
                prerequisites: &["usb-probe pass"],
                pass_signal: "A small set of stable chip registers return full-width values without any writes.",
            },
            VerificationStage {
                id: "efuse-dump",
                command: "wfb-radio-diag efuse-dump --i-understand-this-writes-control-registers",
                purpose: "Read RTL8812AU physical EFUSE bytes, decode the logical map, and summarize RFE/TX-power offsets.",
                prerequisites: &[
                    "reg-smoke pass",
                    "bench authorization for EFUSE control-register writes",
                ],
                pass_signal: "Physical EFUSE bytes and the decoded logical map are reported without EFUSE programming or bulk traffic.",
            },
            VerificationStage {
                id: "led-smoke",
                command: "wfb-radio-diag led-smoke --pin led0 --mode normal --action blink --i-understand-this-writes-registers",
                purpose: "Drive RTL8812AU software LED pins through guarded LEDCFG register writes.",
                prerequisites: &[
                    "reg-smoke pass",
                    "operator watching the adapter LED",
                    "bench authorization for hardware register writes",
                ],
                pass_signal: "LEDCFG writes read back and the operator observes the expected enclosure LED state change.",
            },
            VerificationStage {
                id: "power-on-smoke",
                command: "wfb-radio-diag power-on-smoke --i-understand-this-writes-registers",
                purpose: "Run the guarded RTL8812AU card-emulation-to-active power flow and RF reset writes.",
                prerequisites: &["reg-smoke pass", "bench authorization for hardware register writes"],
                pass_signal: "Power-on, command-register, and RF reset writes read back as expected without bulk traffic.",
            },
            VerificationStage {
                id: "firmware-smoke",
                command: "wfb-radio-diag firmware-smoke --firmware <rtl8812aefw.bin> --i-understand-this-writes-registers",
                purpose: "Download RTL8812A firmware through vendor control transfers and poll checksum/readiness.",
                prerequisites: &[
                    "power-on-smoke pass",
                    "RTL8812A firmware image",
                    "bench authorization for hardware register writes",
                ],
                pass_signal: "Checksum and WINTINI ready bits report success without bulk traffic or TX.",
            },
            VerificationStage {
                id: "llt-smoke",
                command: "wfb-radio-diag llt-smoke --i-understand-this-writes-registers",
                purpose: "Program the RTL8812A linked-list table entries and poll each write idle.",
                prerequisites: &[
                    "power-on-smoke pass",
                    "bench authorization for hardware register writes",
                ],
                pass_signal: "All 256 LLT entries are written and every REG_LLT_INIT operation returns idle.",
            },
            VerificationStage {
                id: "queue-dma-smoke",
                command: "wfb-radio-diag queue-dma-smoke --i-understand-this-writes-registers",
                purpose: "Program RTL8812A reserved-page, TX buffer boundary, TXDMA queue map, RX boundary, and page-size registers.",
                prerequisites: &[
                    "firmware-smoke pass",
                    "llt-smoke pass",
                    "bench authorization for hardware register writes",
                ],
                pass_signal: "Queue and DMA registers read back expected values without bulk traffic or TX.",
            },
            VerificationStage {
                id: "mac-smoke",
                command: "wfb-radio-diag mac-smoke --i-understand-this-writes-registers",
                purpose: "Program RTL8812A driver-info, network type, WMAC filter, rate/retry, EDCA, HW sequence, BAR, and MAC TX/RX enable registers.",
                prerequisites: &[
                    "queue-dma-smoke pass",
                    "firmware-smoke pass",
                    "bench authorization for hardware register writes",
                ],
                pass_signal: "MAC/WMAC registers read back expected values without BB/RF setup, bulk traffic, or TX.",
            },
            VerificationStage {
                id: "bb-smoke",
                command: "wfb-radio-diag bb-smoke --i-understand-this-writes-registers",
                purpose: "Parse external RTL8812A PHY_REG and AGC_TAB tables and program BB registers through vendor control writes.",
                prerequisites: &[
                    "mac-smoke pass",
                    "Realtek halhwimg8812a_bb.c source file",
                    "bench authorization for hardware register writes",
                ],
                pass_signal: "BB setup gates, PHY_REG writes, AGC_TAB writes, and crystal-cap update complete without bulk traffic or TX.",
            },
            VerificationStage {
                id: "rf-smoke",
                command: "wfb-radio-diag rf-smoke --i-understand-this-writes-registers",
                purpose: "Parse external RTL8812A radioA/radioB tables and program RF registers through path-specific 3-wire BB writes.",
                prerequisites: &[
                    "bb-smoke pass",
                    "Realtek halhwimg8812a_rf.c source file",
                    "bench authorization for hardware register writes",
                ],
                pass_signal: "RF radioA/radioB table writes and delay markers complete without channel tuning, bulk traffic, or TX.",
            },
            VerificationStage {
                id: "init",
                command: "wfb-radio-diag init --firmware <rtl8812aefw.bin> --i-understand-this-writes-registers",
                purpose: "Power on the chip, load firmware, initialize queues, and enter raw RX/TX ready state.",
                prerequisites: &[
                    "rf-smoke pass",
                    "bb-smoke pass",
                    "mac-smoke pass",
                    "queue-dma-smoke pass",
                    "llt-smoke pass",
                    "firmware-smoke pass",
                    "RTL8812A firmware source",
                    "bench authorization for hardware register writes",
                ],
                pass_signal: "Power, firmware, LLT, queue/DMA, MAC, BB, and RF phases report pass and the adapter remains responsive to register reads.",
            },
            VerificationStage {
                id: "macos-power-on-smoke",
                command: "wfb-radio-diag macos-power-on-smoke --vid <vid> --pid <pid> --i-understand-this-writes-registers",
                purpose: "Run guarded RTL8812AU power-on/RF-reset control writes through IOUSBHost when libusb cannot enumerate the device.",
                prerequisites: &["macos-reg-smoke pass", "operator write acknowledgement"],
                pass_signal: "Power-on/RF-reset register sequence completes through default-control transfers without bulk traffic.",
            },
            VerificationStage {
                id: "rx-scan",
                command: "wfb-radio-diag rx-scan --channel <ch>",
                purpose: "Capture raw 802.11 frames on a selected channel for a bounded interval.",
                prerequisites: &["init pass", "active Wi-Fi traffic on selected channel"],
                pass_signal: "Frames are parsed from bulk-IN buffers with channel/RSSI metadata.",
            },
            VerificationStage {
                id: "tx-once",
                command: "wfb-radio-diag tx-once --channel <ch> --frame-hex <hex> --i-understand-this-transmits",
                purpose: "Transmit one bounded test frame with conservative options.",
                prerequisites: &["init pass", "independent monitor receiver"],
                pass_signal: "USB bulk OUT succeeds and the independent receiver observes the frame.",
            },
            VerificationStage {
                id: "wfb-rx",
                command: "wfb-radio-bridge --rx ...",
                purpose: "Forward matching WFB payloads from radio RX to a stock WFB-ng aggregator.",
                prerequisites: &["rx-scan pass", "Linux WFB peer transmitting test packets"],
                pass_signal: "Aggregator receives payloads and bridge counters show matched/forwarded packets.",
            },
            VerificationStage {
                id: "wfb-tx",
                command: "wfb-radio-bridge --tx ...",
                purpose: "Inject WFB distributor traffic through the userspace USB radio.",
                prerequisites: &["tx-once pass", "stock WFB-ng distributor", "Linux WFB receiver"],
                pass_signal: "Linux peer receives WFB payloads for the configured link ID and radio port.",
            },
            VerificationStage {
                id: "sustained-link",
                command: "wfb-radio-bridge ...",
                purpose: "Run sustained video and telemetry after low-rate RX/TX paths are proven.",
                prerequisites: &["wfb-rx pass", "wfb-tx pass", "bench authorization"],
                pass_signal: "Sustained stream runs with acceptable loss, latency, and CPU usage.",
            },
        ],
    }
}

fn parse_u16(input: &str) -> std::result::Result<u16, String> {
    let trimmed = input.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        trimmed.parse::<u16>().map_err(|e| e.to_string())
    }
}

fn parse_u8(input: &str) -> std::result::Result<u8, String> {
    let value = parse_u16(input)?;
    u8::try_from(value).map_err(|_| format!("{input:?} is outside u8 range"))
}

fn parse_bandwidth(input: &str) -> std::result::Result<Bandwidth, String> {
    match input.trim().to_ascii_lowercase().as_str() {
        "20" | "20mhz" | "mhz20" => Ok(Bandwidth::Mhz20),
        "40" | "40mhz" | "mhz40" => Ok(Bandwidth::Mhz40),
        "80" | "80mhz" | "mhz80" => Ok(Bandwidth::Mhz80),
        other => Err(format!(
            "unsupported bandwidth {other:?}; expected 20, 40, or 80"
        )),
    }
}

fn parse_tx_rate_arg(input: &str) -> std::result::Result<TxRate, String> {
    let normalized = input
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', '.'], "");

    let rate = match normalized.as_str() {
        "cck1m" | "1m" => TxRate::Cck1m,
        "cck2m" | "2m" => TxRate::Cck2m,
        "cck55m" | "55m" | "5m5" => TxRate::Cck5_5m,
        "cck11m" | "11m" => TxRate::Cck11m,
        "ofdm6m" | "6m" => TxRate::Ofdm6m,
        "ofdm9m" | "9m" => TxRate::Ofdm9m,
        "ofdm12m" | "12m" => TxRate::Ofdm12m,
        "ofdm18m" | "18m" => TxRate::Ofdm18m,
        "ofdm24m" | "24m" => TxRate::Ofdm24m,
        "ofdm36m" | "36m" => TxRate::Ofdm36m,
        "ofdm48m" | "48m" => TxRate::Ofdm48m,
        "ofdm54m" | "54m" => TxRate::Ofdm54m,
        _ => {
            if let Some(mcs) = normalized.strip_prefix("mcs") {
                let mcs = parse_mcs_index(input, mcs, 31)?;
                TxRate::Mcs(mcs)
            } else if let Some(vht) = normalized.strip_prefix("vht") {
                let (nss, mcs) = vht.split_once("ssmcs").ok_or_else(|| {
                    format!(
                        "unsupported tx rate {input:?}; expected forms like ofdm6m, mcs7, or vht2ss-mcs9"
                    )
                })?;
                let nss = nss
                    .parse::<u8>()
                    .map_err(|_| format!("invalid VHT spatial stream count in {input:?}"))?;
                if !(1..=4).contains(&nss) {
                    return Err(format!(
                        "unsupported VHT spatial stream count {nss}; expected 1..=4"
                    ));
                }
                let mcs = parse_mcs_index(input, mcs, 9)?;
                TxRate::Vht { mcs, nss }
            } else {
                return Err(format!(
                    "unsupported tx rate {input:?}; expected forms like ofdm6m, mcs7, or vht2ss-mcs9"
                ));
            }
        }
    };

    Ok(rate)
}

fn parse_mcs_index(input: &str, mcs: &str, max: u8) -> std::result::Result<u8, String> {
    let mcs = mcs
        .parse::<u8>()
        .map_err(|_| format!("invalid MCS index in {input:?}"))?;
    if mcs <= max {
        Ok(mcs)
    } else {
        Err(format!(
            "unsupported MCS index {mcs} in {input:?}; expected 0..={max}"
        ))
    }
}

fn print_pending_human(report: &PendingDiagnosticReport) {
    println!("{}: {}", report.command, report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    if let Some(channel) = report.channel {
        println!(
            "Channel: {} ({} MHz, {:?})",
            channel.number, channel.frequency_mhz, channel.band
        );
    }
    if let Some(bandwidth) = report.bandwidth {
        println!("Bandwidth: {} MHz", bandwidth.mhz());
    }
    if let Some(duration_ms) = report.duration_ms {
        println!("Duration: {duration_ms} ms");
    }
    if let Some(path) = &report.firmware_path {
        println!("Firmware: {}", path.display());
    }
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    if let Some(firmware) = &report.firmware {
        println!(
            "Firmware image: {} bytes, byte_sum=0x{:08x}, chunks={}x{}",
            firmware.len, firmware.byte_sum, firmware.chunk_count, firmware.chunk_size
        );
    }
    if let Some(init) = &report.init_dry_run {
        println!(
            "Init dry run: planned_transfers={} firmware={} bytes chunk_size={} source={}@{}",
            init.planned_transfers,
            init.firmware_len,
            init.firmware_chunk_size,
            init.source_repo,
            init.source_commit
        );
        if let Some(path) = &init.trace_out {
            println!("Init trace out: {}", path.display());
        }
        for count in &init.phase_counts {
            println!("  {:?}: {} transfers", count.phase, count.transfers);
        }
    }
    if let Some(init) = &report.init_live {
        println!(
            "Init live: firmware_payload={} bytes LLT={} queue_pages={}",
            init.firmware_payload_len.unwrap_or_default(),
            init.llt_entries_written,
            init.queue_pages
                .as_ref()
                .map(|pages| pages.rqpn_hex.as_str())
                .unwrap_or("-")
        );
        println!(
            "  BB: phy_writes={} agc_writes={} delays={}",
            init.bb_phy_writes_applied, init.bb_agc_writes_applied, init.bb_delays_applied
        );
        println!(
            "  RF: radioa_writes={} radiob_writes={} delays={}",
            init.rf_radioa_writes_applied, init.rf_radiob_writes_applied, init.rf_delays_applied
        );
        if let (Some(channel), Some(bandwidth)) = (init.effective_channel, init.effective_bandwidth)
        {
            println!(
                "  Effective channel: {} ({} MHz, {:?}, {} MHz)",
                channel.number,
                channel.frequency_mhz,
                channel.band,
                bandwidth.mhz()
            );
        }
        for phase in &init.phase_summaries {
            println!(
                "  {}: {:?} reads={} writes={} - {}",
                phase.id,
                phase.status,
                phase.usb_control_reads,
                phase.usb_control_writes,
                phase.detail
            );
        }
    }
    if let Some(path) = &report.pcap_path {
        println!("PCAP: {}", path.display());
    }
    if let Some(frame_len) = report.tx_frame_len {
        println!("TX frame: {frame_len} bytes");
    }
    if let Some(dry_run) = &report.tx_dry_run {
        println!(
            "TX dry run: packet={} bytes descriptor={} bytes byte_sum=0x{:08x}",
            dry_run.packet_len, dry_run.descriptor_len, dry_run.packet_byte_sum
        );
        if let Some(path) = &dry_run.packet_out {
            println!("Packet out: {}", path.display());
        }
    }
    if let Some(live) = &report.tx_live {
        println!(
            "TX live: endpoint={} packet={} bytes written={} submitted={}",
            live.bulk_out_endpoint_hex,
            live.packet_len,
            live.bytes_written,
            live.submit_counters.submitted
        );
        if let Some(led) = &live.tx_activity_led {
            println!(
                "TX LED: pin={:?} mode={:?} hold_ms={} steps={} error={}",
                led.pin,
                led.mode,
                led.hold_ms,
                led.steps.len(),
                led.error.as_ref().map(|error| error.code).unwrap_or("none")
            );
        }
        if let Some(status) = &live.tx_status {
            println!(
                "TX status: delay_ms={} reads={} changed={} error={}",
                status.delay_ms,
                status.counters.usb_control_reads,
                status.changed.len(),
                status
                    .error
                    .as_ref()
                    .map(|error| error.code)
                    .unwrap_or("none")
            );
        }
    }
    if let Some(rx_fixture) = &report.rx_fixture {
        println!(
            "RX capture: buffers={} timeouts={} bytes={} frames={} drops={} need_more={}",
            rx_fixture.buffers_read,
            rx_fixture.read_timeouts,
            rx_fixture.bulk_bytes,
            rx_fixture.parsed_frames,
            rx_fixture.dropped_packets,
            rx_fixture.need_more_data
        );
        println!(
            "RX frame types: mgmt={} control={} data={} extension={}",
            rx_fixture.management_frames,
            rx_fixture.control_frames,
            rx_fixture.data_frames,
            rx_fixture.extension_frames
        );
        if rx_fixture.pcap_frames_written > 0 {
            println!("PCAP frames written: {}", rx_fixture.pcap_frames_written);
        }
    }
    if let Some(repeat) = &report.repeat_tx {
        println!(
            "TX repeat: count={} interval_ms={} authorized={} submitted={} failed={} bytes={}",
            repeat.count,
            repeat.interval_ms,
            repeat.authorized,
            repeat.submit_counters.submitted,
            repeat.submit_counters.failed,
            repeat.submit_counters.bytes_written
        );
        if let Some(endpoint) = &repeat.bulk_out_endpoint_hex {
            println!("TX repeat endpoint: {endpoint}");
        }
        if let (Some(submitted_per_second), Some(usb_bytes_per_second)) =
            (repeat.submitted_per_second, repeat.usb_bytes_per_second)
        {
            println!(
                "TX repeat rate: {:.2} frames/s {:.2} USB bytes/s",
                submitted_per_second, usb_bytes_per_second
            );
        }
        if let Some(led) = &repeat.tx_activity_led {
            println!(
                "TX repeat LED: pin={:?} mode={:?} hold_ms={} steps={} error={}",
                led.pin,
                led.mode,
                led.hold_ms,
                led.steps.len(),
                led.error.as_ref().map(|error| error.code).unwrap_or("none")
            );
        }
        if let Some(status) = &repeat.tx_status {
            println!(
                "TX repeat status: delay_ms={} reads={} changed={} error={}",
                status.delay_ms,
                status.counters.usb_control_reads,
                status.changed.len(),
                status
                    .error
                    .as_ref()
                    .map(|error| error.code)
                    .unwrap_or("none")
            );
        }
    }
    println!("Phases:");
    for phase in &report.phases {
        println!("  - {}: {:?} - {}", phase.id, phase.status, phase.detail);
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={} rx_frames={} tx_frames={} drops={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes,
        report.counters.rx_frames,
        report.counters.tx_frames,
        report.counters.dropped_frames
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_usb_probe_human(report: &radio_core::UsbProbeReport) {
    println!("USB probe: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    println!("Devices: {}", report.devices.len());
    for device in &report.devices {
        let support = device
            .known_adapter
            .as_ref()
            .map(|a| format!("supported {} ({})", a.chipset, a.name))
            .unwrap_or_else(|| "unsupported".to_string());
        println!(
            "- {:04x}:{:04x} bus={} address={} speed={} {}",
            device.vid, device.pid, device.bus, device.address, device.speed, support
        );
        for interface in &device.interfaces {
            println!(
                "  interface {} alt={} class={:02x} subclass={:02x} protocol={:02x}",
                interface.number,
                interface.setting_number,
                interface.class_code,
                interface.sub_class_code,
                interface.protocol_code
            );
            for endpoint in &interface.endpoints {
                println!(
                    "    ep 0x{:02x} {} {} max_packet={}",
                    endpoint.address,
                    endpoint.direction,
                    endpoint.transfer_type,
                    endpoint.max_packet_size
                );
            }
        }
    }

    match &report.claim {
        Some(claim) => {
            println!(
                "Claim: {}",
                if claim.success { "success" } else { "failed" }
            );
            if let Some(device) = &claim.device {
                println!(
                    "  selected {:04x}:{:04x} bus={} address={}",
                    device.vid, device.pid, device.bus, device.address
                );
            }
            if let Some(endpoints) = &claim.endpoints {
                println!(
                    "  interface={} bulk_in={:?} bulk_out={:?}",
                    endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
                );
            }
            if let Some(error) = &claim.error {
                println!("  error: {error}");
            }
        }
        None => println!("Claim: not attempted"),
    }

    if !report.errors.is_empty() {
        println!("Errors:");
        for error in &report.errors {
            println!("- {error}");
        }
    }
}

fn print_macos_usb_state_human(report: &MacosUsbStateReport) {
    println!("macOS USB state: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    println!("Devices: {}", report.devices.len());
    for device in &report.devices {
        println!(
            "- {} {}:{} registered={} matched={} configured={} interfaces={}",
            device.name,
            device.vid_hex.as_deref().unwrap_or("????"),
            device.pid_hex.as_deref().unwrap_or("????"),
            device.registered,
            device.matched,
            device.has_current_configuration,
            device.has_interface_children
        );
        println!(
            "  location={} address={:?} enum_state={:?} current_config={:?} link_speed_bps={:?}",
            device.location_id_hex.as_deref().unwrap_or("-"),
            device.usb_address,
            device.enumeration_state,
            device.current_configuration,
            device.usb_link_speed_bps
        );
        if let Some(status) = (!device.status.is_empty()).then_some(&device.status) {
            println!("  status: {status}");
        }
    }
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_register_smoke_human(report: &RegisterSmokeReport) {
    println!("Register smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    println!("Register reads:");
    for read in &report.reads {
        println!(
            "  {} {} {} = {}",
            read.address_hex, read.width, read.name, read.value_hex
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_efuse_dump_human(report: &EfuseDumpReport) {
    println!("EFUSE dump: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    println!("Length: {} authorized={}", report.length, report.authorized);
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    if let Some(efuse) = &report.efuse {
        println!(
            "EFUSE: raw_len={} logical_len={} packets={} used={} ({:.1}%)",
            efuse.raw_len,
            efuse.logical_map_len,
            efuse.summary.decoded_packet_count,
            efuse.summary.raw_used_bytes,
            efuse.summary.raw_used_percent
        );
        if let Some(mac) = &efuse.summary.mac_address {
            println!("MAC: {mac}");
        }
        if let Some(vid) = &efuse.summary.usb_vid_hex {
            println!("USB VID: {vid}");
        }
        if let Some(pid) = &efuse.summary.usb_pid_hex {
            println!("USB PID: {pid}");
        }
        println!(
            "TX power bytes: start={} len={} non_ff={} all_ff={}",
            format_value(efuse.summary.tx_power.start_offset as u64, 3),
            efuse.summary.tx_power.length,
            efuse.summary.tx_power.non_ff_bytes,
            efuse.summary.tx_power.all_ff
        );
        for byte in &efuse.summary.named_bytes {
            println!(
                "  {} {} = {}{}",
                byte.offset_hex,
                byte.name,
                byte.value_hex,
                if byte.programmed {
                    ""
                } else {
                    " (default/blank)"
                }
            );
        }
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_led_smoke_human(report: &LedSmokeReport) {
    println!("LED smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    println!(
        "LED: pin={:?} mode={:?} action={:?} blink_count={} interval_ms={} authorized={}",
        report.pin,
        report.mode,
        report.action,
        report.blink_count,
        report.interval_ms,
        report.authorized
    );
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    println!("Steps:");
    for step in &report.steps {
        let status = if step.passed { "pass" } else { "fail" };
        println!(
            "  {status} {} {:?}/{:?} {} {} before={} written={} after={} expected_masked={}",
            step.operation,
            step.pin,
            step.mode,
            step.address_hex,
            step.register_name,
            step.before_hex,
            step.written_hex,
            step.after_hex,
            step.expected_hex
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_power_on_smoke_human(report: &PowerOnSmokeReport) {
    println!("Power-on smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    println!("Steps:");
    for step in &report.steps {
        let status = if step.passed { "pass" } else { "fail" };
        let after = step.after_hex.as_deref().unwrap_or("-");
        let written = step.written_hex.as_deref().unwrap_or("-");
        println!(
            "  {status} {} {} {} written={} after={} attempts={:?}",
            step.phase, step.operation, step.register_name, written, after, step.attempts
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_firmware_smoke_human(report: &FirmwareSmokeReport) {
    println!("Firmware smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    println!("Firmware: {}", report.firmware_path.display());
    if let Some(firmware) = &report.firmware {
        println!(
            "Firmware image: {} bytes, byte_sum=0x{:08x}, pages={}x{}",
            firmware.len, firmware.byte_sum, firmware.chunk_count, firmware.chunk_size
        );
    }
    if let Some(payload_len) = report.firmware_payload_len {
        println!(
            "Firmware payload: offset={} len={} signature={}",
            report.firmware_payload_offset.unwrap_or_default(),
            payload_len,
            report.firmware_signature_hex.as_deref().unwrap_or("-")
        );
    }
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    println!(
        "Firmware writes: bytes={} control_writes={} checksum_attempts={:?} ready_attempts={:?} final_mcu={}",
        report.firmware_bytes_written,
        report.firmware_control_writes,
        report.checksum_poll_attempts,
        report.ready_poll_attempts,
        report.final_mcu_status_hex.as_deref().unwrap_or("-")
    );
    println!("Steps:");
    for step in &report.steps {
        let status = if step.passed { "pass" } else { "fail" };
        let register = step.register_name.unwrap_or("-");
        let address = step.address_hex.as_deref().unwrap_or("-");
        let after = step.after_hex.as_deref().unwrap_or("-");
        println!(
            "  {status} {} {} {} addr={} len={:?} after={} attempts={:?}",
            step.phase, step.operation, register, address, step.length, after, step.attempts
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={} tx_frames={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes,
        report.counters.tx_frames
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_llt_smoke_human(report: &LltSmokeReport) {
    println!("LLT smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    println!(
        "LLT: boundary=0x{:02x} last=0x{:02x} entries={} max_poll_attempts={}",
        report.tx_page_boundary,
        report.last_tx_page_entry,
        report.entries_written,
        report.max_poll_attempts_observed
    );
    println!("Steps:");
    for step in &report.steps {
        let status = if step.passed { "pass" } else { "fail" };
        println!(
            "  {status} {} {} llt_addr={:?} data={:?} after={} attempts={:?}",
            step.phase,
            step.operation,
            step.llt_address,
            step.llt_data,
            step.after_hex.as_deref().unwrap_or("-"),
            step.attempts
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={} tx_frames={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes,
        report.counters.tx_frames
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_queue_dma_smoke_human(report: &QueueDmaSmokeReport) {
    println!("Queue/DMA smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?} bulk_out_count={}",
            endpoints.interface_number,
            endpoints.bulk_in,
            endpoints.bulk_out,
            endpoints.bulk_out_all.len()
        );
    }
    println!(
        "Queue layout: out_ep_sel={} tx_total=0x{:02x} tx_boundary=0x{:02x} rx_boundary={}",
        report.out_ep_queue_sel_hex.as_deref().unwrap_or("-"),
        report.tx_total_page_number,
        report.tx_page_boundary,
        report.rx_dma_boundary_hex
    );
    if let Some(queue_pages) = &report.queue_pages {
        println!(
            "Reserved pages: hpq=0x{:02x} lpq=0x{:02x} npq=0x{:02x} pubq=0x{:02x} rqpn={}",
            queue_pages.hpq,
            queue_pages.lpq,
            queue_pages.npq,
            queue_pages.pubq,
            queue_pages.rqpn_hex
        );
    }
    println!("Steps:");
    for step in &report.steps {
        let status = if step.passed { "pass" } else { "fail" };
        println!(
            "  {status} {} {} {} written={} after={}",
            step.phase,
            step.operation,
            step.register_name,
            step.written_hex.as_deref().unwrap_or("-"),
            step.after_hex.as_deref().unwrap_or("-")
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={} tx_frames={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes,
        report.counters.tx_frames
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_mac_smoke_human(report: &MacSmokeReport) {
    println!("MAC smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    println!(
        "MAC: receive_config={} retry_limit={}",
        report.receive_config_hex, report.retry_limit_hex
    );
    println!("Steps:");
    for step in &report.steps {
        let status = if step.passed { "pass" } else { "fail" };
        println!(
            "  {status} {} {} {} written={} after={}",
            step.phase,
            step.operation,
            step.register_name,
            step.written_hex.as_deref().unwrap_or("-"),
            step.after_hex.as_deref().unwrap_or("-")
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={} tx_frames={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes,
        report.counters.tx_frames
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_bb_smoke_human(report: &BbSmokeReport) {
    println!("BB smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    println!("BB source: {}", report.bb_source.display());
    println!(
        "Condition: interface=0x{:02x} platform=0x{:02x} board=0x{:02x} glna=0x{:04x} gpa=0x{:04x} alna=0x{:04x} apa=0x{:04x}",
        report.condition_env.support_interface,
        report.condition_env.support_platform,
        report.condition_env.board_type,
        report.condition_env.type_glna,
        report.condition_env.type_gpa,
        report.condition_env.type_alna,
        report.condition_env.type_apa
    );
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    if let Some(plan) = &report.phy_plan {
        println!(
            "PHY_REG: raw_pairs={} writes={} delays={} condition_markers={} skipped={}",
            plan.raw_pair_count,
            plan.write_count(),
            plan.delay_count(),
            plan.condition_marker_pairs,
            plan.skipped_write_pairs
        );
    }
    if let Some(plan) = &report.agc_plan {
        println!(
            "AGC_TAB: raw_pairs={} writes={} delays={} condition_markers={} skipped={}",
            plan.raw_pair_count,
            plan.write_count(),
            plan.delay_count(),
            plan.condition_marker_pairs,
            plan.skipped_write_pairs
        );
    }
    println!(
        "Applied: phy_writes={} agc_writes={} delays={} crystal_cap={}",
        report.phy_writes_applied,
        report.agc_writes_applied,
        report.delays_applied,
        report.crystal_cap_hex
    );
    println!("Setup steps:");
    for step in &report.setup_steps {
        let status = if step.passed { "pass" } else { "fail" };
        println!(
            "  {status} {} {} {} written={} after={}",
            step.phase,
            step.operation,
            step.register_name,
            step.written_hex.as_deref().unwrap_or("-"),
            step.after_hex.as_deref().unwrap_or("-")
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={} tx_frames={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes,
        report.counters.tx_frames
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_rf_smoke_human(report: &RfSmokeReport) {
    println!("RF smoke: {}", report.result.as_str());
    println!("Platform: {} {}", report.platform.os, report.platform.arch);
    println!("RF source: {}", report.rf_source.display());
    println!(
        "Condition: interface=0x{:02x} platform=0x{:02x} board=0x{:02x} glna=0x{:04x} gpa=0x{:04x} alna=0x{:04x} apa=0x{:04x}",
        report.condition_env.support_interface,
        report.condition_env.support_platform,
        report.condition_env.board_type,
        report.condition_env.type_glna,
        report.condition_env.type_gpa,
        report.condition_env.type_alna,
        report.condition_env.type_apa
    );
    if let Some(adapter) = &report.adapter {
        println!(
            "Adapter: {:04x}:{:04x} bus={} address={} speed={}",
            adapter.vid, adapter.pid, adapter.bus, adapter.address, adapter.speed
        );
    }
    if let Some(endpoints) = &report.endpoints {
        println!(
            "Claim: interface={} bulk_in={:?} bulk_out={:?}",
            endpoints.interface_number, endpoints.bulk_in, endpoints.bulk_out
        );
    }
    if let Some(plan) = &report.radioa_plan {
        println!(
            "RadioA: raw_pairs={} writes={} delays={} condition_markers={} skipped={}",
            plan.raw_pair_count,
            plan.write_count(),
            plan.delay_count(),
            plan.condition_marker_pairs,
            plan.skipped_write_pairs
        );
    }
    if let Some(plan) = &report.radiob_plan {
        println!(
            "RadioB: raw_pairs={} writes={} delays={} condition_markers={} skipped={}",
            plan.raw_pair_count,
            plan.write_count(),
            plan.delay_count(),
            plan.condition_marker_pairs,
            plan.skipped_write_pairs
        );
    }
    println!(
        "Applied: radioa_writes={} radiob_writes={} delays={}",
        report.radioa_writes_applied, report.radiob_writes_applied, report.delays_applied
    );
    println!("Setup steps:");
    for step in &report.setup_steps {
        let status = if step.passed { "pass" } else { "fail" };
        println!(
            "  {status} {} {} {} written={} after={}",
            step.phase,
            step.operation,
            step.register_name,
            step.written_hex.as_deref().unwrap_or("-"),
            step.after_hex.as_deref().unwrap_or("-")
        );
    }
    println!(
        "Counters: control_reads={} control_writes={} bulk_in={} bulk_out={} tx_frames={}",
        report.counters.usb_control_reads,
        report.counters.usb_control_writes,
        report.counters.usb_bulk_in_reads,
        report.counters.usb_bulk_out_writes,
        report.counters.tx_frames
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_stages_human(report: &StagesReport) {
    println!("Verification stages:");
    for stage in &report.stages {
        println!("- {}: {}", stage.id, stage.purpose);
        println!("  command: {}", stage.command);
        println!("  prerequisites: {}", stage.prerequisites.join(", "));
        println!("  pass: {}", stage.pass_signal);
    }
}

fn print_trace_compare_human(report: &TraceCompareReport) {
    println!("Trace compare: {}", report.result.as_str());
    println!("Expected: {}", report.expected_path.display());
    println!("Observed: {}", report.observed_path.display());
    if let Some(comparison) = &report.comparison {
        println!(
            "Events: expected={} observed={} compared={}",
            comparison.expected_len, comparison.observed_len, comparison.compared_len
        );
        if !comparison.mismatches.is_empty() {
            println!("Mismatches:");
            for mismatch in &comparison.mismatches {
                println!(
                    "- event {} {} expected={} observed={}",
                    mismatch.event_index, mismatch.field, mismatch.expected, mismatch.observed
                );
            }
        }
    }
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
}

fn print_trace_import_human(report: &TraceImportReport) {
    println!("Trace import: {}", report.result.as_str());
    println!("Input: {}", report.input_path.display());
    println!("Output: {}", report.output_path.display());
    if let Some(imported) = &report.import {
        println!(
            "Events: imported={} ignored_lines={} errors={}",
            imported.events.len(),
            imported.ignored_lines,
            imported.errors.len()
        );
        if !imported.errors.is_empty() {
            println!("Errors:");
            for error in &imported.errors {
                println!("- line {}: {}", error.line_number, error.message);
            }
        }
    }
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter_args() -> AdapterArgs {
        AdapterArgs {
            vid: None,
            pid: None,
            bus: None,
            address: None,
        }
    }

    fn init_args(
        channel: u8,
        bandwidth: Bandwidth,
        firmware: Option<PathBuf>,
        dry_run: bool,
        trace_out: Option<PathBuf>,
    ) -> InitArgs {
        InitArgs {
            adapter: adapter_args(),
            channel,
            bandwidth,
            firmware,
            timeout_ms: 500,
            bb_source: PathBuf::from(
                "/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c",
            ),
            rf_source: PathBuf::from(
                "/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c",
            ),
            cut_version: 0x00,
            package_type: 0x00,
            support_interface: 0x02,
            support_platform: 0x00,
            board_type: 0xd8,
            type_glna: 0x0000,
            type_gpa: 0x0000,
            type_alna: 0x0000,
            type_apa: 0x0000,
            crystal_cap: 0x20,
            i_understand_this_writes_registers: false,
            dry_run,
            trace_out,
        }
    }

    #[test]
    fn parse_bandwidth_accepts_common_forms() {
        assert_eq!(parse_bandwidth("20").expect("20"), Bandwidth::Mhz20);
        assert_eq!(parse_bandwidth("40MHz").expect("40"), Bandwidth::Mhz40);
        assert_eq!(parse_bandwidth("mhz80").expect("80"), Bandwidth::Mhz80);
    }

    #[test]
    fn parse_tx_rate_accepts_legacy_ht_and_vht_forms() {
        assert_eq!(parse_tx_rate_arg("ofdm6m").expect("ofdm"), TxRate::Ofdm6m);
        assert_eq!(parse_tx_rate_arg("6m").expect("ofdm short"), TxRate::Ofdm6m);
        assert_eq!(parse_tx_rate_arg("cck5.5m").expect("cck"), TxRate::Cck5_5m);
        assert_eq!(parse_tx_rate_arg("mcs7").expect("ht"), TxRate::Mcs(7));
        assert_eq!(
            parse_tx_rate_arg("vht2ss-mcs9").expect("vht"),
            TxRate::Vht { mcs: 9, nss: 2 }
        );
        assert_eq!(
            parse_tx_rate_arg("vht4ss_mcs0").expect("vht underscore"),
            TxRate::Vht { mcs: 0, nss: 4 }
        );
        assert!(parse_tx_rate_arg("mcs32").is_err());
        assert!(parse_tx_rate_arg("vht2ss-mcs10").is_err());
        assert!(parse_tx_rate_arg("vht5ss-mcs0").is_err());
    }

    #[test]
    fn macos_ioreg_parser_reports_unconfigured_realtek_device() {
        let ioreg = r#"
  | +-o 802.11n NIC@01100000  <class IOUSBHostDevice, id 0x1000196a7, !registered, !matched, active, busy 0, retain 13>
  |     {
  |       "USBSpeed" = 3
  |       "UsbLinkSpeed" = 480000000
  |       "idProduct" = 34834
  |       "bNumConfigurations" = 1
  |       "USB Product Name" = "802.11n NIC"
  |       "locationID" = 17825792
  |       "kUSBSerialNumberString" = "123456"
  |       "USB Address" = 1
  |       "USB Vendor Name" = "Realtek"
  |       "idVendor" = 3034
  |       "UsbEnumerationState" = 2
  |     }
  +-o AppleT8112USBXHCI@00000000  <class AppleT8112USBXHCI, id 0x1000005d1, registered, matched, active, busy 0, retain 62>
    | {
    |   "locationID" = 0
    | }
"#;

        let devices = parse_macos_ioreg_usb_devices(
            ioreg,
            DeviceSelector {
                vid: Some(0x0bda),
                pid: Some(0x8812),
                bus: None,
                address: None,
            },
        );

        assert_eq!(devices.len(), 1);
        let device = &devices[0];
        assert_eq!(device.name, "802.11n NIC");
        assert_eq!(device.location_path.as_deref(), Some("01100000"));
        assert_eq!(device.vid_hex.as_deref(), Some("0x0bda"));
        assert_eq!(device.pid_hex.as_deref(), Some("0x8812"));
        assert_eq!(device.vendor_name.as_deref(), Some("Realtek"));
        assert_eq!(device.product_name.as_deref(), Some("802.11n NIC"));
        assert_eq!(device.serial_number.as_deref(), Some("123456"));
        assert!(!device.registered);
        assert!(!device.matched);
        assert!(device.active);
        assert_eq!(device.usb_address, Some(1));
        assert_eq!(device.location_id_hex.as_deref(), Some("0x01100000"));
        assert_eq!(device.usb_link_speed_bps, Some(480_000_000));
        assert_eq!(device.enumeration_state, Some(2));
        assert!(!device.has_current_configuration);
        assert!(!device.has_interface_children);
    }

    #[test]
    fn macos_ioreg_parser_reports_configured_device_interfaces() {
        let ioreg = r#"
    +-o AX88179B@00200000  <class IOUSBHostDevice, id 0x100000bb2, registered, matched, active, busy 0 (225 ms), retain 37>
      {
        "idVendor" = 2965
        "idProduct" = 6032
        "kUSBCurrentConfiguration" = 2
        "kUSBPreferredConfiguration" = 2
        "USB Address" = 1
      }
      +-o IOUSBHostInterface@0  <class IOUSBHostInterface, id 0x100000bb3, registered, matched, active, busy 0, retain 8>
"#;

        let devices = parse_macos_ioreg_usb_devices(ioreg, DeviceSelector::default());

        assert_eq!(devices.len(), 1);
        let device = &devices[0];
        assert_eq!(device.name, "AX88179B");
        assert_eq!(device.vid_hex.as_deref(), Some("0x0b95"));
        assert_eq!(device.pid_hex.as_deref(), Some("0x1790"));
        assert!(device.registered);
        assert!(device.matched);
        assert_eq!(device.current_configuration, Some(2));
        assert!(device.has_current_configuration);
        assert!(device.has_interface_children);
    }

    #[test]
    fn efuse_logical_decoder_handles_normal_and_extended_headers() {
        let mut raw = vec![0xff; RTL8812AU_EFUSE_REAL_CONTENT_LEN];
        raw[0] = 0x2e;
        raw[1] = 0xaa;
        raw[2] = 0xbb;
        raw[3] = 0x4f;
        raw[4] = 0x2d;
        raw[5] = 0xcc;
        raw[6] = 0xdd;

        let decoded = decode_efuse_logical_map(&raw);

        assert_eq!(decoded.logical_map[0x10], 0xaa);
        assert_eq!(decoded.logical_map[0x11], 0xbb);
        assert_eq!(decoded.logical_map[0x92], 0xcc);
        assert_eq!(decoded.logical_map[0x93], 0xdd);
        assert_eq!(decoded.packets.len(), 2);
        assert_eq!(decoded.packets[0].section, 2);
        assert_eq!(decoded.packets[1].section, 18);
        assert_eq!(decoded.raw_used_bytes, 7);
        assert_eq!(decoded.terminating_offset, Some(7));
    }

    #[test]
    fn efuse_summary_extracts_usb_identity_mac_and_tx_power_region() {
        let raw = vec![0x00, 0xff];
        let mut logical = vec![0xff; RTL8812AU_EFUSE_LOGICAL_MAP_LEN];
        logical[0xd0] = 0xda;
        logical[0xd1] = 0x0b;
        logical[0xd2] = 0x12;
        logical[0xd3] = 0x88;
        logical[0xd7..0xdd].copy_from_slice(&[0x57, 0x42, 0x00, 0x00, 0x01, 0x23]);
        logical[RTL8812AU_EFUSE_TX_POWER_START] = 0x2a;

        let decoded = EfuseLogicalDecode {
            logical_map: logical,
            packets: Vec::new(),
            raw_used_bytes: 1,
            terminating_offset: Some(1),
        };
        let summary = summarize_efuse(&raw, &decoded);

        assert_eq!(summary.raw_used_bytes, 1);
        assert_eq!(summary.terminating_offset, Some(1));
        assert_eq!(summary.usb_vid_hex.as_deref(), Some("0x0bda"));
        assert_eq!(summary.usb_pid_hex.as_deref(), Some("0x8812"));
        assert_eq!(summary.mac_address.as_deref(), Some("57:42:00:00:01:23"));
        assert_eq!(summary.tx_power.non_ff_bytes, 1);
        assert!(!summary.tx_power.all_ff);
    }

    #[test]
    fn init_report_rejects_invalid_channel_bandwidth() {
        let report = init_report(init_args(6, Bandwidth::Mhz80, None, false, None));

        assert_eq!(report.result, DiagnosticResult::Fail);
        assert_eq!(
            report.error.as_ref().expect("error").code,
            "unsupported_channel"
        );
    }

    #[test]
    fn tx_once_report_validates_operator_hex_frame() {
        let report = tx_once_report(TxOnceArgs {
            adapter: adapter_args(),
            channel: 36,
            bandwidth: Bandwidth::Mhz20,
            frame_hex: Some("0011".to_string()),
            i_understand_this_transmits: false,
            dry_run: false,
            packet_out: None,
            tx_options: TxOptionArgs::default(),
            tx_led: TxActivityLedArgs::default(),
            tx_status: TxStatusProbeArgs::default(),
        });

        assert_eq!(report.result, DiagnosticResult::Fail);
        assert_eq!(
            report.error.as_ref().expect("error").code,
            "invalid_ieee80211_frame"
        );
    }

    #[test]
    fn tx_once_dry_run_builds_descriptor_packet() {
        let report = tx_once_report(TxOnceArgs {
            adapter: adapter_args(),
            channel: 36,
            bandwidth: Bandwidth::Mhz20,
            frame_hex: Some(encode_hex(&sample_data_frame())),
            i_understand_this_transmits: false,
            dry_run: true,
            packet_out: None,
            tx_options: TxOptionArgs {
                tx_rate: TxRate::Ofdm6m,
                short_gi: true,
                ldpc: true,
                stbc: true,
            },
            tx_led: TxActivityLedArgs::default(),
            tx_status: TxStatusProbeArgs::default(),
        });

        assert_eq!(report.result, DiagnosticResult::Pass);
        let dry_run = report.tx_dry_run.expect("dry run");
        assert_eq!(dry_run.descriptor_len, 40);
        assert!(dry_run.packet_len > dry_run.frame_len);
        assert!(dry_run.tx_options.short_gi);
        assert!(dry_run.tx_options.ldpc);
        assert!(dry_run.tx_options.stbc);
    }

    #[test]
    fn tx_once_dry_run_reports_selected_vht_tx_rate() {
        let report = tx_once_report(TxOnceArgs {
            adapter: adapter_args(),
            channel: 36,
            bandwidth: Bandwidth::Mhz80,
            frame_hex: Some(encode_hex(&sample_data_frame())),
            i_understand_this_transmits: false,
            dry_run: true,
            packet_out: None,
            tx_options: TxOptionArgs {
                tx_rate: TxRate::Vht { mcs: 9, nss: 2 },
                short_gi: true,
                ..TxOptionArgs::default()
            },
            tx_led: TxActivityLedArgs::default(),
            tx_status: TxStatusProbeArgs::default(),
        });

        assert_eq!(report.result, DiagnosticResult::Pass);
        let dry_run = report.tx_dry_run.expect("dry run");
        assert_eq!(dry_run.tx_options.rate, TxRate::Vht { mcs: 9, nss: 2 });
        assert_eq!(dry_run.tx_options.bandwidth, Bandwidth::Mhz80);
        assert!(dry_run.tx_options.short_gi);
    }

    #[test]
    fn tx_once_dry_run_rejects_tx_led() {
        let report = tx_once_report(TxOnceArgs {
            adapter: adapter_args(),
            channel: 36,
            bandwidth: Bandwidth::Mhz20,
            frame_hex: Some(encode_hex(&sample_data_frame())),
            i_understand_this_transmits: false,
            dry_run: true,
            packet_out: None,
            tx_options: TxOptionArgs::default(),
            tx_led: TxActivityLedArgs {
                tx_led: true,
                ..TxActivityLedArgs::default()
            },
            tx_status: TxStatusProbeArgs::default(),
        });

        assert_eq!(report.result, DiagnosticResult::Fail);
        assert_eq!(
            report.error.as_ref().expect("error").code,
            "tx_led_requires_live_tx"
        );
    }

    #[test]
    fn tx_once_dry_run_rejects_tx_status() {
        let report = tx_once_report(TxOnceArgs {
            adapter: adapter_args(),
            channel: 36,
            bandwidth: Bandwidth::Mhz20,
            frame_hex: Some(encode_hex(&sample_data_frame())),
            i_understand_this_transmits: false,
            dry_run: true,
            packet_out: None,
            tx_options: TxOptionArgs::default(),
            tx_led: TxActivityLedArgs::default(),
            tx_status: TxStatusProbeArgs {
                tx_status: true,
                ..TxStatusProbeArgs::default()
            },
        });

        assert_eq!(report.result, DiagnosticResult::Fail);
        assert_eq!(
            report.error.as_ref().expect("error").code,
            "tx_status_requires_live_tx"
        );
    }

    #[test]
    fn tx_status_deltas_report_only_changed_registers() {
        let before = vec![
            TxStatusRegisterReport {
                name: "REG_A",
                address: 0x0010,
                address_hex: "0x0010".to_string(),
                width: "u8",
                value: 0x12,
                value_hex: "0x12".to_string(),
            },
            TxStatusRegisterReport {
                name: "REG_B",
                address: 0x0020,
                address_hex: "0x0020".to_string(),
                width: "u16",
                value: 0x0030,
                value_hex: "0x0030".to_string(),
            },
        ];
        let after = vec![
            TxStatusRegisterReport {
                name: "REG_A",
                address: 0x0010,
                address_hex: "0x0010".to_string(),
                width: "u8",
                value: 0x12,
                value_hex: "0x12".to_string(),
            },
            TxStatusRegisterReport {
                name: "REG_B",
                address: 0x0020,
                address_hex: "0x0020".to_string(),
                width: "u16",
                value: 0x0031,
                value_hex: "0x0031".to_string(),
            },
        ];

        let deltas = tx_status_deltas(&before, &after);

        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].name, "REG_B");
        assert_eq!(deltas[0].xor_hex, "0x0001");
    }

    #[test]
    fn tx_once_live_requires_authorization() {
        let report = tx_once_report(TxOnceArgs {
            adapter: adapter_args(),
            channel: 36,
            bandwidth: Bandwidth::Mhz20,
            frame_hex: Some(encode_hex(&sample_data_frame())),
            i_understand_this_transmits: false,
            dry_run: false,
            packet_out: None,
            tx_options: TxOptionArgs::default(),
            tx_led: TxActivityLedArgs::default(),
            tx_status: TxStatusProbeArgs::default(),
        });

        assert_eq!(report.result, DiagnosticResult::Fail);
        assert_eq!(
            report.error.as_ref().expect("error").code,
            "missing_tx_authorization"
        );
    }

    #[test]
    fn tx_repeat_requires_authorization() {
        let report = tx_repeat_report(TxRepeatArgs {
            adapter: adapter_args(),
            channel: 36,
            bandwidth: Bandwidth::Mhz20,
            count: 2,
            interval_ms: 100,
            frame_hex: Some(encode_hex(&sample_data_frame())),
            i_understand_this_transmits: false,
            tx_options: TxOptionArgs::default(),
            tx_led: TxActivityLedArgs::default(),
            tx_status: TxStatusProbeArgs::default(),
        });

        assert_eq!(report.result, DiagnosticResult::Fail);
        assert_eq!(
            report.error.as_ref().expect("error").code,
            "missing_tx_authorization"
        );
    }

    #[test]
    fn tx_rate_helper_handles_elapsed_time() {
        assert_eq!(rate_per_second(50, Some(100)), Some(500.0));
        assert_eq!(rate_per_second(50, Some(0)), None);
        assert_eq!(rate_per_second(50, None), None);
    }

    #[test]
    fn cpu_usage_delta_reports_one_core_percent() {
        let before = CpuUsageSnapshot {
            user_us: 1_000,
            system_us: 2_000,
        };
        let after = CpuUsageSnapshot {
            user_us: 2_500,
            system_us: 2_500,
        };
        let report = cpu_usage_delta(Some(before), Some(after), 10).expect("cpu");

        assert_eq!(report.user_us, 1_500);
        assert_eq!(report.system_us, 500);
        assert_eq!(report.total_us, 2_000);
        assert_eq!(report.percent_one_core, Some(20.0));
    }

    #[test]
    fn led_value_helpers_match_rtl8812au_normal_usb_path() {
        assert_eq!(led_register(LedPin::Led0), ("REG_LEDCFG0", REG_LEDCFG0));
        assert_eq!(led_register(LedPin::Led1), ("REG_LEDCFG1", REG_LEDCFG1));
        assert_eq!(led_register(LedPin::Led2), ("REG_LEDCFG2", REG_LEDCFG2));
        assert_eq!(led_on_value(0x52), 0x70);
        assert_eq!(led_off_value(0x52), 0x78);
        assert_eq!(led_on_value(0x00), 0x20);
        assert_eq!(led_off_value(0x00), 0x28);

        let antdiv_on =
            led_write_plan(LedPin::Led0, LedMode::Antdiv, LedAction::On, 0x02).expect("antdiv on");
        assert_eq!(antdiv_on.written, 0xe0);
        assert_eq!(antdiv_on.verify_mask, 0xe0);
        let antdiv_off = led_write_plan(LedPin::Led0, LedMode::Antdiv, LedAction::Off, 0x20)
            .expect("antdiv off");
        assert_eq!(antdiv_off.written, 0xe8);
        assert_eq!(antdiv_off.verify_mask, 0xe8);
    }

    #[test]
    fn led_smoke_requires_authorization_before_usb_claim() {
        let report = led_smoke_report(LedSmokeArgs {
            adapter: adapter_args(),
            timeout_ms: 500,
            pin: LedPin::Led0,
            mode: LedMode::Normal,
            action: LedAction::On,
            blink_count: 1,
            interval_ms: 250,
            i_understand_this_writes_registers: false,
        });

        assert_eq!(report.result, DiagnosticResult::Fail);
        assert_eq!(
            report.error.as_ref().expect("error").code,
            "missing_write_authorization"
        );
        assert!(report.adapter.is_none());
        assert_eq!(report.counters.usb_control_writes, 0);
    }

    #[test]
    fn rx_scan_fixture_parses_bulk_in_and_writes_pcap() {
        let stamp = started_at_unix_ms();
        let fixture_path = std::env::temp_dir().join(format!(
            "wfb-radio-diag-rx-fixture-{}-{stamp}.bin",
            std::process::id()
        ));
        let pcap_path = std::env::temp_dir().join(format!(
            "wfb-radio-diag-rx-fixture-{}-{stamp}.pcap",
            std::process::id()
        ));
        let frame_jsonl_path = std::env::temp_dir().join(format!(
            "wfb-radio-diag-rx-fixture-{}-{stamp}.jsonl",
            std::process::id()
        ));
        fs::write(&fixture_path, sample_rx_bulk_in()).expect("write fixture");

        let report = rx_scan_report(RxScanArgs {
            adapter: adapter_args(),
            channel: 36,
            bandwidth: Bandwidth::Mhz20,
            duration_ms: 1,
            timeout_ms: 100,
            pcap: Some(pcap_path.clone()),
            frame_jsonl: Some(frame_jsonl_path.clone()),
            fixture_bulk_in: vec![fixture_path.clone()],
        });

        let _ = fs::remove_file(fixture_path);
        let pcap_len = fs::metadata(&pcap_path).expect("pcap metadata").len();
        let frame_jsonl = fs::read_to_string(&frame_jsonl_path).expect("frame jsonl");
        let _ = fs::remove_file(pcap_path);
        let _ = fs::remove_file(frame_jsonl_path);

        assert_eq!(report.result, DiagnosticResult::Pass);
        let rx = report.rx_fixture.expect("rx fixture");
        assert_eq!(rx.buffers_read, 1);
        assert_eq!(rx.parsed_frames, 1);
        assert_eq!(rx.data_frames, 1);
        assert_eq!(rx.pcap_frames_written, 1);
        assert_eq!(rx.frame_records_written, 1);
        assert!(pcap_len > 24);
        assert!(frame_jsonl.contains("\"frame_type\":\"Data\""));
        assert!(frame_jsonl.contains("\"rssi_dbm\":-80"));
    }

    #[test]
    fn init_report_loads_firmware_summary() {
        let path = std::env::temp_dir().join(format!(
            "wfb-radio-diag-firmware-{}-{}.bin",
            std::process::id(),
            started_at_unix_ms()
        ));
        fs::write(&path, [1u8, 2, 3, 4]).expect("write firmware");

        let report = init_report(init_args(
            36,
            Bandwidth::Mhz20,
            Some(path.clone()),
            false,
            None,
        ));
        let _ = fs::remove_file(path);

        assert_eq!(report.result, DiagnosticResult::Fail);
        assert_eq!(
            report.error.as_ref().expect("error").code,
            "missing_write_authorization"
        );
        let firmware = report.firmware.expect("firmware report");
        assert_eq!(firmware.len, 4);
        assert_eq!(firmware.byte_sum, 10);
    }

    #[test]
    fn init_dry_run_writes_planned_trace() {
        let stamp = started_at_unix_ms();
        let firmware_path = std::env::temp_dir().join(format!(
            "wfb-radio-diag-init-fw-{}-{stamp}.bin",
            std::process::id()
        ));
        let trace_path = std::env::temp_dir().join(format!(
            "wfb-radio-diag-init-trace-{}-{stamp}.json",
            std::process::id()
        ));
        fs::write(
            &firmware_path,
            vec![0xa5; radio_core::INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE + 1],
        )
        .expect("write firmware");

        let report = init_report(init_args(
            36,
            Bandwidth::Mhz20,
            Some(firmware_path.clone()),
            true,
            Some(trace_path.clone()),
        ));

        let trace_json = fs::read_to_string(&trace_path).expect("trace");
        let events: Vec<UsbTraceEvent> = serde_json::from_str(&trace_json).expect("trace json");
        let _ = fs::remove_file(firmware_path);
        let _ = fs::remove_file(trace_path);

        assert_eq!(report.result, DiagnosticResult::Pass);
        let dry_run = report.init_dry_run.expect("init dry run");
        assert_eq!(
            dry_run.firmware_len,
            radio_core::INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE + 1
        );
        assert_eq!(
            dry_run.source_commit,
            radio_core::INIT_PLAN_REFERENCE_COMMIT
        );
        assert_eq!(dry_run.planned_transfers, events.len());
        assert!(dry_run.phase_counts.iter().any(|count| count.phase
            == radio_core::InitPhase::FirmwareDownload
            && count.transfers > 2));
        assert!(dry_run
            .phase_counts
            .iter()
            .any(|count| count.phase == radio_core::InitPhase::Llt && count.transfers == 512));
        assert!(events
            .iter()
            .any(|event| event.kind == radio_core::UsbTraceKind::ControlWrite));
    }

    #[test]
    fn trace_import_writes_normalized_events() {
        let stamp = started_at_unix_ms();
        let input_path = std::env::temp_dir().join(format!(
            "wfb-radio-diag-usbmon-{}-{stamp}.txt",
            std::process::id()
        ));
        let output_path = std::env::temp_dir().join(format!(
            "wfb-radio-diag-usbmon-{}-{stamp}.json",
            std::process::id()
        ));
        fs::write(
            &input_path,
            "\
ffff 0 S Co:1:004:0 s 40 05 0002 0000 0001 1 = 00
ffff 1 S Bi:1:004:1 -115 512 <
",
        )
        .expect("write usbmon");

        let report = trace_import_report(TraceImportArgs {
            input: input_path.clone(),
            output: output_path.clone(),
        });

        let output = fs::read_to_string(&output_path).expect("output");
        let _ = fs::remove_file(input_path);
        let _ = fs::remove_file(output_path);

        assert_eq!(report.result, DiagnosticResult::Pass);
        assert!(output.contains("\"control_write\""));
        assert!(output.contains("\"bulk_in\""));
    }

    #[test]
    fn encodes_llt_write_operation() {
        assert_eq!(encode_llt_write(0x12, 0x34), 0x4000_1234);
        assert_eq!(llt_op_value(0x4000_1234), LLT_WRITE_ACCESS);
        assert_eq!(llt_op_value(0x0000_1234), LLT_NO_ACTIVE);
    }

    #[test]
    fn queue_map_matches_upstream_endpoint_layouts() {
        assert_eq!(queue_map_for_endpoint_count(2), 0xfaf0);
        assert_eq!(queue_map_for_endpoint_count(3), 0xf5b0);
        assert_eq!(queue_map_for_endpoint_count(4), 0xc5a0);
        assert_eq!(queue_map_for_endpoint_count(1), 0x0000);
    }

    #[test]
    fn queue_layout_for_three_out_endpoints_matches_awus036ach_path() {
        let endpoints = UsbEndpoints {
            interface_number: 0,
            bulk_in: Some(0x81),
            bulk_out: Some(0x02),
            bulk_in_all: vec![0x81],
            bulk_out_all: vec![0x02, 0x03, 0x04],
        };

        let layout = queue_layout_from_endpoints(&endpoints).expect("layout");

        assert_eq!(layout.bulk_out_endpoint_count, 3);
        assert_eq!(
            layout.out_ep_queue_sel,
            TX_SELE_HQ | TX_SELE_LQ | TX_SELE_NQ
        );
        assert_eq!(layout.hpq, 0x10);
        assert_eq!(layout.lpq, 0x10);
        assert_eq!(layout.npq, 0x00);
        assert_eq!(layout.pubq, 0xd8);
        assert_eq!(layout.rqpn_npq, 0x00);
        assert_eq!(layout.rqpn, 0x80d8_1010);
        assert_eq!(layout.queue_map, 0xf5b0);
    }

    #[test]
    fn mac_receive_config_matches_upstream_wmac_bits() {
        assert_eq!(MAC_RECEIVE_CONFIG, 0x7400_60ce);
        assert_eq!(NETTYPE_LINK_AP, 0x0002_0000);
        assert_eq!(MAC_TX_RX_ENABLE_MASK, 0xc0);
    }

    #[test]
    fn mac_retry_limit_matches_upstream_sta_value() {
        assert_eq!(RETRY_LIMIT_STA, 0x3030);
        assert_eq!(RATE_RRSR_CCK_ONLY_1M & RATE_BITMAP_ALL, 0x000f_fff1);
        assert_eq!(
            BAR_MODE_CTRL_VALUE & BAR_MODE_CTRL_READBACK_MASK,
            0x0201_ff7f
        );
    }

    #[test]
    fn encodes_rf_serial_write_data_and_address() {
        assert_eq!(encode_rf_serial_write(0x018, 0x0001_712a), 0x0181_712a);
        assert_eq!(encode_rf_serial_write(0x1ff, 0xffff_ffff), 0x0fff_ffff);
    }

    #[test]
    fn channel_group_helpers_match_rtl8812a_source_ranges() {
        assert_eq!(fc_area_data(6), 0x96a);
        assert_eq!(fc_area_data(36), 0x494);
        assert_eq!(fc_area_data(64), 0x453);
        assert_eq!(fc_area_data(100), 0x452);
        assert_eq!(fc_area_data(149), 0x412);

        assert_eq!(rf_mod_ag_data(6), 0x000);
        assert_eq!(rf_mod_ag_data(36), 0x101);
        assert_eq!(rf_mod_ag_data(100), 0x301);
        assert_eq!(rf_mod_ag_data(149), 0x501);
    }

    #[test]
    fn forty_mhz_secondary_channel_mapping_matches_primary_side() {
        assert_eq!(
            data_secondary_channel_setting(
                Channel::from_number(36).expect("channel 36"),
                Bandwidth::Mhz40
            )
            .expect("36/40"),
            VHT_DATA_SC_20_LOWER_OF_80MHZ
        );
        assert_eq!(
            data_secondary_channel_setting(
                Channel::from_number(40).expect("channel 40"),
                Bandwidth::Mhz40
            )
            .expect("40/40"),
            VHT_DATA_SC_20_UPPER_OF_80MHZ
        );
        assert_eq!(
            data_secondary_channel_setting(
                Channel::from_number(6).expect("channel 6"),
                Bandwidth::Mhz40
            )
            .expect("6/40"),
            VHT_DATA_SC_20_LOWER_OF_80MHZ
        );
    }

    #[test]
    fn eighty_mhz_channel_plan_matches_primary_subchannel() {
        assert_eq!(
            channel_programming_number(
                Channel::from_number(36).expect("channel 36"),
                Bandwidth::Mhz80
            )
            .expect("36/80 center"),
            42
        );
        assert_eq!(
            channel_programming_number(
                Channel::from_number(149).expect("channel 149"),
                Bandwidth::Mhz80
            )
            .expect("149/80 center"),
            155
        );
        assert_eq!(
            data_secondary_channel_setting(
                Channel::from_number(36).expect("channel 36"),
                Bandwidth::Mhz80
            )
            .expect("36/80 data sc"),
            0xa4
        );
        assert_eq!(
            data_secondary_channel_setting(
                Channel::from_number(40).expect("channel 40"),
                Bandwidth::Mhz80
            )
            .expect("40/80 data sc"),
            0xa2
        );
        assert_eq!(
            data_secondary_channel_setting(
                Channel::from_number(44).expect("channel 44"),
                Bandwidth::Mhz80
            )
            .expect("44/80 data sc"),
            0x91
        );
        assert_eq!(
            data_secondary_channel_setting(
                Channel::from_number(48).expect("channel 48"),
                Bandwidth::Mhz80
            )
            .expect("48/80 data sc"),
            0x93
        );
    }

    #[test]
    fn apply_rf_mask_preserves_unmasked_rf_bits() {
        let base = 0x0001_712a;
        let changed = apply_rf_mask(base, RF_CHNLBW_CHANNEL_MASK, 36);
        assert_eq!(changed, 0x0001_7124);
        let changed = apply_rf_mask(changed, RF_CHNLBW_BW_MASK, 3);
        assert_eq!(changed & RF_CHNLBW_BW_MASK, 0x0000_0c00);
        assert_eq!(
            changed & !RF_CHNLBW_CHANNEL_MASK & !RF_CHNLBW_BW_MASK,
            0x0001_7100
        );
    }

    fn sample_data_frame() -> Vec<u8> {
        vec![
            0x08, 0x01, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x57, 0x42, 0x00, 0x00,
            0x01, 0x23, 0x57, 0x42, 0x00, 0x00, 0x01, 0x23, 0x10, 0x00,
        ]
    }

    fn sample_rx_bulk_in() -> Vec<u8> {
        const RX_DESC_SIZE: usize = 24;
        const RX_ALIGNMENT: usize = 128;

        let frame = sample_data_frame();
        let mut payload = frame.clone();
        payload.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let mut bulk = vec![0u8; RX_ALIGNMENT];
        let dw0 = payload.len() as u32;
        bulk[0..4].copy_from_slice(&dw0.to_le_bytes());
        bulk[RX_DESC_SIZE..RX_DESC_SIZE + payload.len()].copy_from_slice(&payload);
        bulk
    }
}
