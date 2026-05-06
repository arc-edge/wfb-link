use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use clap::{Parser, ValueEnum};
use radio_core::{
    parse_realtek_u32_array, plan_realtek_table, Bandwidth, Channel, DeviceSelector, FirmwareImage,
    RealtekConditionEnv, RealtekTableKind, RealtekTablePlan,
};
use serde::Deserialize;
use wfb_radio_runtime::{
    plan_rtl8812au_efuse_tx_power, run_production_runtime_flow, LedHeartbeatConfig,
    MacosUsbHostConfig, ProductionRuntimeFlowConfig, ProductionRuntimeFlowExecutionInputs,
    ProductionRuntimeFlowReport, ProductionRuntimePrimaryRxForwardConfig,
    ProductionRuntimeRtl8812auInitInputs, ProductionRuntimeRxForwardConfig,
    ProductionRuntimeTxPowerControlInput, ProductionRuntimeUsbConfig, Rtl8812auInitOrder,
    Rtl8812auRfPath, Rtl8812auTxPowerEfuseSourceReport, Rtl8812auTxPowerSafetyProfile,
    RuntimeRadioError, TxCalibrationProfile, DEFAULT_HEARTBEAT_HALF_PERIOD_MS,
    RTL8812AU_EFUSE_TX_POWER_LEN, RTL8812AU_EFUSE_TX_POWER_START, RTL8812AU_TX_POWER_INDEX_MAX,
};

const MAC_REG_ARRAY: &str = "array_mp_8812a_mac_reg";
const BB_PHY_ARRAY: &str = "array_mp_8812a_phy_reg";
const BB_AGC_ARRAY: &str = "array_mp_8812a_agc_tab";
const RF_RADIOA_ARRAY: &str = "array_mp_8812a_radioa";
const RF_RADIOB_ARRAY: &str = "array_mp_8812a_radiob";
const DEFAULT_RFE_TYPE: u8 = 0x03;
const DEFAULT_INIT_TIMEOUT_MS: u64 = 500;

#[derive(Debug, Parser, Clone)]
#[command(name = "wfb-radio-service")]
#[command(about = "Production service entry point for the native WFB USB radio runtime")]
pub struct ServiceCli {
    /// Emit JSON report to stdout.
    #[arg(long)]
    pub json: bool,

    /// Write the command's JSON report to a file.
    #[arg(long, value_name = "PATH")]
    pub report: Option<PathBuf>,

    /// Service-oriented TOML config file for production radio settings.
    #[arg(long, value_name = "PATH")]
    pub config: PathBuf,

    /// USB vendor ID.
    #[arg(long, value_parser = parse_u16)]
    pub vid: Option<u16>,

    /// USB product ID.
    #[arg(long, value_parser = parse_u16)]
    pub pid: Option<u16>,

    /// USB bus number.
    #[arg(long)]
    pub bus: Option<u8>,

    /// USB device address.
    #[arg(long)]
    pub address: Option<u8>,

    /// Open through macOS IOUSBHost retained-session transport.
    #[arg(long)]
    pub macos_usbhost: bool,

    /// IOUSBHost configuration value.
    #[arg(long)]
    pub macos_configuration_value: Option<u8>,

    /// IOUSBHost interface number.
    #[arg(long)]
    pub macos_interface_number: Option<u8>,

    /// IOUSBHost bulk IN endpoint.
    #[arg(long)]
    pub macos_bulk_in_endpoint: Option<u8>,

    /// IOUSBHost selected bulk OUT endpoint.
    #[arg(long)]
    pub macos_bulk_out_endpoint: Option<u8>,

    /// IOUSBHost bulk OUT endpoint count.
    #[arg(long)]
    pub macos_bulk_out_endpoint_count: Option<usize>,

    /// IOUSBHost pipe polling attempts.
    #[arg(long)]
    pub macos_poll_attempts: Option<u32>,

    /// IOUSBHost pipe polling delay in milliseconds.
    #[arg(long)]
    pub macos_poll_delay_ms: Option<u64>,

    /// Channel to run the WFB radio on.
    #[arg(long)]
    pub channel: Option<u8>,

    /// Channel bandwidth used for init, RX metadata, and TX descriptors.
    #[arg(long, value_parser = parse_bandwidth)]
    pub bandwidth: Option<Bandwidth>,

    /// RTL8812A firmware image path.
    #[arg(long)]
    pub firmware: Option<PathBuf>,

    /// UDP address to bind for WFB distributor/injector datagrams.
    #[arg(long)]
    pub bind: Option<SocketAddr>,

    /// Additional UDP address to bind for WFB distributor/injector datagrams.
    #[arg(long = "tx-bind", value_name = "ADDR")]
    pub tx_binds: Vec<SocketAddr>,

    /// Bounded production runtime in milliseconds after init completes; 0 runs without a time bound.
    #[arg(long)]
    pub duration_ms: Option<u64>,

    /// Per bulk-IN read timeout while interleaving RX and TX.
    #[arg(long)]
    pub rx_timeout_ms: Option<u64>,

    /// Maximum TX datagrams to drain before returning to one bulk-IN RX read.
    #[arg(long)]
    pub tx_burst_limit: Option<u32>,

    /// Maximum datagrams to receive before exiting; 0 is unlimited.
    #[arg(long)]
    pub max_datagrams: Option<u32>,

    /// Write this JSON marker after init/calibration and immediately before runtime loops.
    #[arg(long, value_name = "PATH")]
    pub ready_file: Option<PathBuf>,

    /// Write this JSON health artifact at production service lifecycle boundaries.
    #[arg(long, value_name = "PATH")]
    pub health_file: Option<PathBuf>,

    /// Required acknowledgement for live RF TX submission.
    #[arg(long)]
    pub i_understand_this_transmits: bool,

    /// Required acknowledgement for runtime calibration profiles that write RF/BB registers.
    #[arg(long)]
    pub i_understand_this_writes_registers: bool,

    /// Explicit RTL8812AU TXAGC power index to write to all per-rate TX power registers.
    #[arg(long, value_parser = parse_tx_power_index)]
    pub tx_power_index: Option<u8>,

    /// TX power programming mode. Manual mode requires --tx-power-index; EFUSE mode requires an EFUSE source.
    #[arg(long, value_enum)]
    pub tx_power_mode: Option<ServiceTxPowerControlMode>,

    /// RF path set affected by TX power programming.
    #[arg(long, value_enum)]
    pub tx_power_path: Option<ServiceTxPowerPath>,

    /// efuse-dump JSON report used by --tx-power-mode efuse-derived.
    #[arg(long, value_name = "PATH")]
    pub tx_power_efuse_report: Option<PathBuf>,

    /// Binary EFUSE logical map or 84-byte TX-power region used by --tx-power-mode efuse-derived.
    #[arg(long, value_name = "PATH")]
    pub tx_power_efuse_logical_map: Option<PathBuf>,

    /// Safety clamp profile for EFUSE-derived TXAGC indexes.
    #[arg(long, value_enum)]
    pub tx_power_safety_profile: Option<ServiceTxPowerSafetyProfile>,

    /// Absolute maximum RTL8812AU TX power index allowed after EFUSE calculation.
    #[arg(long, value_parser = parse_tx_power_index)]
    pub tx_power_max_index: Option<u8>,

    /// Guarded RF/TX calibration profile applied after init and before TX.
    #[arg(long, value_enum)]
    pub tx_calibration_profile: Option<ServiceTxCalibrationProfile>,

    /// Disable the heartbeat LED.
    #[arg(long)]
    pub no_heartbeat_led: bool,

    /// Half-period in milliseconds for the heartbeat LED toggle.
    #[arg(long)]
    pub heartbeat_led_half_period_ms: Option<u64>,

    /// WFB link ID to match and optionally forward during RX.
    #[arg(long, value_parser = parse_u32)]
    pub wfb_link_id: Option<u32>,

    /// WFB radio port to match and optionally forward during RX.
    #[arg(long, value_parser = parse_u8)]
    pub wfb_radio_port: Option<u8>,

    /// UDP aggregator address for matching WFB RX forwarding.
    #[arg(long)]
    pub rx_aggregator: Option<SocketAddr>,

    /// Additional WFB RX forwarding target as LINK_ID:RADIO_PORT=HOST:PORT.
    #[arg(long = "rx-forward", value_name = "LINK_ID:RADIO_PORT=HOST:PORT")]
    pub rx_forwards: Vec<String>,

    /// WFB forwarding WLAN index metadata.
    #[arg(long)]
    pub rx_wlan_idx: Option<u8>,

    /// WFB forwarding MCS metadata.
    #[arg(long)]
    pub rx_mcs_index: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfigFile {
    pub adapter: Option<ServiceAdapterConfig>,
    pub macos_usbhost: Option<ServiceMacosUsbHostConfig>,
    pub radio: Option<ServiceRadioConfig>,
    pub wfb: Option<ServiceWfbConfig>,
    pub tx_power: Option<ServiceTxPowerConfig>,
    pub calibration: Option<ServiceCalibrationConfig>,
    pub heartbeat: Option<ServiceHeartbeatConfig>,
    pub authorization: Option<ServiceAuthorizationConfig>,
    pub artifacts: Option<ServiceArtifactConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceAdapterConfig {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub bus: Option<u8>,
    pub address: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceMacosUsbHostConfig {
    pub enabled: Option<bool>,
    pub configuration_value: Option<u8>,
    pub interface_number: Option<u8>,
    pub bulk_in_endpoint: Option<u8>,
    pub bulk_out_endpoint: Option<u8>,
    pub bulk_out_endpoint_count: Option<usize>,
    pub poll_attempts: Option<u32>,
    pub poll_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceRadioConfig {
    pub channel: Option<u8>,
    pub bandwidth_mhz: Option<u16>,
    pub firmware: Option<PathBuf>,
    pub duration_ms: Option<u64>,
    pub rx_timeout_ms: Option<u64>,
    pub tx_burst_limit: Option<u32>,
    pub max_datagrams: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceWfbConfig {
    pub bind: Option<SocketAddr>,
    pub tx_binds: Option<Vec<SocketAddr>>,
    pub link_id: Option<u32>,
    pub radio_port: Option<u8>,
    pub rx_aggregator: Option<SocketAddr>,
    pub rx_forwards: Option<Vec<String>>,
    pub rx_wlan_idx: Option<u8>,
    pub rx_mcs_index: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceTxPowerConfig {
    pub mode: Option<ServiceTxPowerControlMode>,
    pub index: Option<u8>,
    pub path: Option<ServiceTxPowerPath>,
    pub efuse_report: Option<PathBuf>,
    pub efuse_logical_map: Option<PathBuf>,
    pub safety_profile: Option<ServiceTxPowerSafetyProfile>,
    pub max_index: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceCalibrationConfig {
    pub profile: Option<ServiceTxCalibrationProfile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceHeartbeatConfig {
    pub enabled: Option<bool>,
    pub half_period_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceAuthorizationConfig {
    pub transmit: Option<bool>,
    pub live_register_writes: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceArtifactConfig {
    pub ready_file: Option<PathBuf>,
    pub health_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTxPowerControlMode {
    ManualIndex,
    EfuseDerived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTxPowerSafetyProfile {
    MaxIndex,
    LinuxCh36Ht20,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTxPowerPath {
    A,
    B,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTxCalibrationProfile {
    CurrentDefault,
    LinuxParityCh36Ht20,
    Rtl8812aLck,
    Rtl8812aIqkProbe,
    Rtl8812aRuntimeIqk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedServiceRun {
    pub report: Option<PathBuf>,
    pub adapter: ServiceAdapterConfig,
    pub macos_usbhost: ServiceMacosUsbHostConfig,
    pub channel: u8,
    pub bandwidth: Bandwidth,
    pub firmware: PathBuf,
    pub bind: SocketAddr,
    pub tx_binds: Vec<SocketAddr>,
    pub duration_ms: u64,
    pub rx_timeout_ms: u64,
    pub tx_burst_limit: u32,
    pub max_datagrams: u32,
    pub ready_file: Option<PathBuf>,
    pub health_file: Option<PathBuf>,
    pub tx_authorized: bool,
    pub live_register_write_authorized: bool,
    pub tx_power: ResolvedServiceTxPowerControl,
    pub calibration_profile: ServiceTxCalibrationProfile,
    pub heartbeat_enabled: bool,
    pub heartbeat_half_period_ms: u64,
    pub wfb_link_id: Option<u32>,
    pub wfb_radio_port: Option<u8>,
    pub rx_aggregator: Option<SocketAddr>,
    pub rx_forwards: Vec<String>,
    pub rx_wlan_idx: u8,
    pub rx_mcs_index: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedServiceTxPowerControl {
    pub mode: Option<ServiceTxPowerControlMode>,
    pub index: Option<u8>,
    pub path: ServiceTxPowerPath,
    pub efuse_report: Option<PathBuf>,
    pub efuse_logical_map: Option<PathBuf>,
    pub safety_profile: ServiceTxPowerSafetyProfile,
    pub max_index: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceRxForwardArg {
    pub link_id: Option<u32>,
    pub radio_port: u8,
    pub aggregator: SocketAddr,
}

impl ServiceAdapterConfig {
    pub fn selector(&self) -> DeviceSelector {
        DeviceSelector {
            vid: self.vid,
            pid: self.pid,
            bus: self.bus,
            address: self.address,
        }
    }
}

pub fn load_service_config_file(
    path: &Path,
) -> std::result::Result<ServiceConfigFile, RuntimeRadioError> {
    let input = fs::read_to_string(path).map_err(|error| {
        RuntimeRadioError::new(
            "service_config_read_failed",
            format!("{}: {error}", path.display()),
        )
    })?;
    toml::from_str(&input).map_err(|error| {
        RuntimeRadioError::new(
            "service_config_parse_failed",
            format!("{}: {error}", path.display()),
        )
    })
}

pub fn resolve_service_run(
    cli: &ServiceCli,
) -> std::result::Result<ResolvedServiceRun, RuntimeRadioError> {
    let file = load_service_config_file(&cli.config)?;
    let adapter = file.adapter.as_ref();
    let macos = file.macos_usbhost.as_ref();
    let radio = file.radio.as_ref();
    let wfb = file.wfb.as_ref();
    let tx_power = file.tx_power.as_ref();
    let calibration = file.calibration.as_ref();
    let heartbeat = file.heartbeat.as_ref();
    let authorization = file.authorization.as_ref();
    let artifacts = file.artifacts.as_ref();

    let channel = cli
        .channel
        .or_else(|| radio.and_then(|radio| radio.channel))
        .ok_or_else(|| service_missing_required("radio.channel"))?;
    let bandwidth = cli
        .bandwidth
        .or(service_config_bandwidth(
            radio.and_then(|radio| radio.bandwidth_mhz),
        )?)
        .unwrap_or(Bandwidth::Mhz20);
    let firmware = cli
        .firmware
        .clone()
        .or_else(|| radio.and_then(|radio| radio.firmware.clone()))
        .ok_or_else(|| service_missing_required("radio.firmware"))?;
    let rx_forwards = if cli.rx_forwards.is_empty() {
        wfb.and_then(|wfb| wfb.rx_forwards.clone())
            .unwrap_or_default()
    } else {
        cli.rx_forwards.clone()
    };

    Ok(ResolvedServiceRun {
        report: cli.report.clone(),
        adapter: ServiceAdapterConfig {
            vid: cli.vid.or_else(|| adapter.and_then(|adapter| adapter.vid)),
            pid: cli.pid.or_else(|| adapter.and_then(|adapter| adapter.pid)),
            bus: cli.bus.or_else(|| adapter.and_then(|adapter| adapter.bus)),
            address: cli
                .address
                .or_else(|| adapter.and_then(|adapter| adapter.address)),
        },
        macos_usbhost: ServiceMacosUsbHostConfig {
            enabled: Some(
                cli.macos_usbhost || macos.and_then(|macos| macos.enabled).unwrap_or(false),
            ),
            configuration_value: cli
                .macos_configuration_value
                .or_else(|| macos.and_then(|macos| macos.configuration_value))
                .or(Some(1)),
            interface_number: cli
                .macos_interface_number
                .or_else(|| macos.and_then(|macos| macos.interface_number))
                .or(Some(0)),
            bulk_in_endpoint: cli
                .macos_bulk_in_endpoint
                .or_else(|| macos.and_then(|macos| macos.bulk_in_endpoint))
                .or(Some(0x81)),
            bulk_out_endpoint: cli
                .macos_bulk_out_endpoint
                .or_else(|| macos.and_then(|macos| macos.bulk_out_endpoint))
                .or(Some(0x02)),
            bulk_out_endpoint_count: cli
                .macos_bulk_out_endpoint_count
                .or_else(|| macos.and_then(|macos| macos.bulk_out_endpoint_count))
                .or(Some(3)),
            poll_attempts: cli
                .macos_poll_attempts
                .or_else(|| macos.and_then(|macos| macos.poll_attempts))
                .or(Some(25)),
            poll_delay_ms: cli
                .macos_poll_delay_ms
                .or_else(|| macos.and_then(|macos| macos.poll_delay_ms))
                .or(Some(100)),
        },
        channel,
        bandwidth,
        firmware,
        bind: cli
            .bind
            .or_else(|| wfb.and_then(|wfb| wfb.bind))
            .unwrap_or_else(service_default_bind),
        tx_binds: if cli.tx_binds.is_empty() {
            wfb.and_then(|wfb| wfb.tx_binds.clone()).unwrap_or_default()
        } else {
            cli.tx_binds.clone()
        },
        duration_ms: cli
            .duration_ms
            .or_else(|| radio.and_then(|radio| radio.duration_ms))
            .unwrap_or(10_000),
        rx_timeout_ms: cli
            .rx_timeout_ms
            .or_else(|| radio.and_then(|radio| radio.rx_timeout_ms))
            .unwrap_or(20),
        tx_burst_limit: cli
            .tx_burst_limit
            .or_else(|| radio.and_then(|radio| radio.tx_burst_limit))
            .unwrap_or(8),
        max_datagrams: cli
            .max_datagrams
            .or_else(|| radio.and_then(|radio| radio.max_datagrams))
            .unwrap_or(0),
        ready_file: cli
            .ready_file
            .clone()
            .or_else(|| artifacts.and_then(|artifacts| artifacts.ready_file.clone())),
        health_file: cli
            .health_file
            .clone()
            .or_else(|| artifacts.and_then(|artifacts| artifacts.health_file.clone())),
        tx_authorized: cli.i_understand_this_transmits
            || authorization
                .and_then(|authorization| authorization.transmit)
                .unwrap_or(false),
        live_register_write_authorized: cli.i_understand_this_writes_registers
            || authorization
                .and_then(|authorization| authorization.live_register_writes)
                .unwrap_or(false),
        tx_power: ResolvedServiceTxPowerControl {
            mode: cli
                .tx_power_mode
                .or_else(|| tx_power.and_then(|tx_power| tx_power.mode)),
            index: cli
                .tx_power_index
                .or_else(|| tx_power.and_then(|tx_power| tx_power.index)),
            path: cli
                .tx_power_path
                .or_else(|| tx_power.and_then(|tx_power| tx_power.path))
                .unwrap_or(ServiceTxPowerPath::Both),
            efuse_report: cli
                .tx_power_efuse_report
                .clone()
                .or_else(|| tx_power.and_then(|tx_power| tx_power.efuse_report.clone())),
            efuse_logical_map: cli
                .tx_power_efuse_logical_map
                .clone()
                .or_else(|| tx_power.and_then(|tx_power| tx_power.efuse_logical_map.clone())),
            safety_profile: cli
                .tx_power_safety_profile
                .or_else(|| tx_power.and_then(|tx_power| tx_power.safety_profile))
                .unwrap_or(ServiceTxPowerSafetyProfile::LinuxCh36Ht20),
            max_index: cli
                .tx_power_max_index
                .or_else(|| tx_power.and_then(|tx_power| tx_power.max_index))
                .unwrap_or(RTL8812AU_TX_POWER_INDEX_MAX),
        },
        calibration_profile: cli
            .tx_calibration_profile
            .or_else(|| calibration.and_then(|calibration| calibration.profile))
            .unwrap_or(ServiceTxCalibrationProfile::CurrentDefault),
        heartbeat_enabled: !(cli.no_heartbeat_led
            || heartbeat
                .and_then(|heartbeat| heartbeat.enabled)
                .map(|enabled| !enabled)
                .unwrap_or(false)),
        heartbeat_half_period_ms: cli
            .heartbeat_led_half_period_ms
            .or_else(|| heartbeat.and_then(|heartbeat| heartbeat.half_period_ms))
            .unwrap_or(DEFAULT_HEARTBEAT_HALF_PERIOD_MS),
        wfb_link_id: cli.wfb_link_id.or_else(|| wfb.and_then(|wfb| wfb.link_id)),
        wfb_radio_port: cli
            .wfb_radio_port
            .or_else(|| wfb.and_then(|wfb| wfb.radio_port)),
        rx_aggregator: cli
            .rx_aggregator
            .or_else(|| wfb.and_then(|wfb| wfb.rx_aggregator)),
        rx_forwards,
        rx_wlan_idx: cli
            .rx_wlan_idx
            .or_else(|| wfb.and_then(|wfb| wfb.rx_wlan_idx))
            .unwrap_or(0),
        rx_mcs_index: cli
            .rx_mcs_index
            .or_else(|| wfb.and_then(|wfb| wfb.rx_mcs_index))
            .unwrap_or(0),
    })
}

pub fn run_service(
    cli: &ServiceCli,
) -> std::result::Result<ProductionRuntimeFlowReport, RuntimeRadioError> {
    let resolved = resolve_service_run(cli)?;
    let config = service_runtime_config_from_resolved(&resolved)?;
    let inputs = match service_runtime_inputs_from_resolved(&resolved, config.channel) {
        Ok(inputs) => inputs,
        Err(error) => return Ok(ProductionRuntimeFlowReport::not_started(&config, error)),
    };
    Ok(run_production_runtime_flow(config, inputs))
}

pub fn service_runtime_config_from_resolved(
    resolved: &ResolvedServiceRun,
) -> std::result::Result<ProductionRuntimeFlowConfig, RuntimeRadioError> {
    let channel = Channel::from_number(resolved.channel).map_err(|error| {
        RuntimeRadioError::new(
            "invalid_channel",
            format!("invalid production service channel: {error}"),
        )
    })?;
    let usb = if resolved.macos_usbhost.enabled.unwrap_or(false) {
        ProductionRuntimeUsbConfig::macos_usbhost(
            resolved.adapter.selector(),
            service_macos_usbhost_config(&resolved.macos_usbhost),
        )
    } else {
        ProductionRuntimeUsbConfig::libusb(resolved.adapter.selector())
    };
    let rx_forwards = service_runtime_rx_forwards(resolved)?;

    Ok(ProductionRuntimeFlowConfig {
        usb,
        channel,
        bandwidth: resolved.bandwidth,
        firmware: Some(resolved.firmware.clone()),
        bind_addr: resolved.bind,
        tx_binds: resolved.tx_binds.clone(),
        duration_ms: resolved.duration_ms,
        rx_timeout_ms: resolved.rx_timeout_ms,
        tx_burst_limit: resolved.tx_burst_limit,
        max_datagrams: resolved.max_datagrams,
        ready_file: resolved.ready_file.clone(),
        health_file: resolved.health_file.clone(),
        tx_authorized: resolved.tx_authorized,
        live_register_write_authorized: resolved.live_register_write_authorized,
        calibration_profile: TxCalibrationProfile::from(resolved.calibration_profile),
        captured_tail_applied: service_should_apply_captured_tx_bringup_tail(
            channel,
            resolved.bandwidth,
        ),
        primary_rx_forward: ProductionRuntimePrimaryRxForwardConfig {
            link_id: resolved.wfb_link_id,
            radio_port: resolved.wfb_radio_port,
            aggregator: resolved.rx_aggregator,
        },
        rx_forwards,
        rx_wlan_idx: resolved.rx_wlan_idx,
        rx_mcs_index: resolved.rx_mcs_index,
    })
}

pub fn service_runtime_inputs_from_resolved(
    resolved: &ResolvedServiceRun,
    channel: Channel,
) -> std::result::Result<ProductionRuntimeFlowExecutionInputs, RuntimeRadioError> {
    let firmware_image = FirmwareImage::load_external(&resolved.firmware).map_err(|error| {
        RuntimeRadioError::new(
            "service_firmware_load_failed",
            format!("{}: {error}", resolved.firmware.display()),
        )
    })?;
    let condition_env = RealtekConditionEnv::rtl8812au_awus036ach_default();
    let mac_plan = load_service_realtek_table_plan(
        &rtl8812a_mac_source_default(),
        MAC_REG_ARRAY,
        RealtekTableKind::Mac,
        condition_env,
    )?;
    let phy_plan = load_service_realtek_table_plan(
        &rtl8812a_bb_source_default(),
        BB_PHY_ARRAY,
        RealtekTableKind::BbPhy,
        condition_env,
    )?;
    let agc_plan = load_service_realtek_table_plan(
        &rtl8812a_bb_source_default(),
        BB_AGC_ARRAY,
        RealtekTableKind::BbAgc,
        condition_env,
    )?;
    let radioa_plan = load_service_realtek_table_plan(
        &rtl8812a_rf_source_default(),
        RF_RADIOA_ARRAY,
        RealtekTableKind::RfRadioA,
        condition_env,
    )?;
    let radiob_plan = load_service_realtek_table_plan(
        &rtl8812a_rf_source_default(),
        RF_RADIOB_ARRAY,
        RealtekTableKind::RfRadioB,
        condition_env,
    )?;

    Ok(ProductionRuntimeFlowExecutionInputs {
        rtl8812au_init: Some(ProductionRuntimeRtl8812auInitInputs {
            firmware_image,
            mac_plan,
            phy_plan,
            agc_plan,
            radioa_plan,
            radiob_plan,
            init_order: Rtl8812auInitOrder::Linux,
            rfe_type: DEFAULT_RFE_TYPE,
            init_timeout: Duration::from_millis(DEFAULT_INIT_TIMEOUT_MS),
        }),
        tx_power_control: service_runtime_tx_power_input(
            &resolved.tx_power,
            channel,
            resolved.bandwidth,
        )?,
        heartbeat_led: LedHeartbeatConfig {
            enabled: resolved.heartbeat_enabled,
            half_period_ms: resolved.heartbeat_half_period_ms,
        },
    })
}

pub fn parse_bandwidth(input: &str) -> std::result::Result<Bandwidth, String> {
    let normalized = input
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "");
    match normalized.trim_start_matches("mhz").trim_end_matches("mhz") {
        "20" => Ok(Bandwidth::Mhz20),
        "40" => Ok(Bandwidth::Mhz40),
        "80" => Ok(Bandwidth::Mhz80),
        _ => Err("expected 20, 40, or 80 MHz".to_string()),
    }
}

fn service_config_bandwidth(
    bandwidth_mhz: Option<u16>,
) -> std::result::Result<Option<Bandwidth>, RuntimeRadioError> {
    bandwidth_mhz
        .map(|mhz| {
            parse_bandwidth(&mhz.to_string()).map_err(|error| {
                RuntimeRadioError::new(
                    "service_config_invalid_field",
                    format!("radio.bandwidth_mhz: {error}"),
                )
            })
        })
        .transpose()
}

fn service_missing_required(field: &'static str) -> RuntimeRadioError {
    RuntimeRadioError::new(
        "service_config_missing_required",
        format!("wfb-radio-service requires {field} from CLI or --config"),
    )
}

fn service_default_bind() -> SocketAddr {
    "127.0.0.1:5600"
        .parse()
        .expect("service default bind address")
}

fn service_runtime_rx_forwards(
    resolved: &ResolvedServiceRun,
) -> std::result::Result<Vec<ProductionRuntimeRxForwardConfig>, RuntimeRadioError> {
    resolved
        .rx_forwards
        .iter()
        .map(|forward| {
            parse_service_rx_forward_arg(forward).map(|forward| ProductionRuntimeRxForwardConfig {
                link_id: forward.link_id.or(resolved.wfb_link_id),
                radio_port: forward.radio_port,
                aggregator: Some(forward.aggregator),
            })
        })
        .collect()
}

fn service_macos_usbhost_config(config: &ServiceMacosUsbHostConfig) -> MacosUsbHostConfig {
    MacosUsbHostConfig {
        configuration_value: config.configuration_value.unwrap_or(1),
        interface_number: config.interface_number.unwrap_or(0),
        bulk_in_endpoint: config.bulk_in_endpoint.unwrap_or(0x81),
        bulk_out_endpoint: config.bulk_out_endpoint.unwrap_or(0x02),
        bulk_out_endpoint_count: config.bulk_out_endpoint_count.unwrap_or(3),
        poll_attempts: config.poll_attempts.unwrap_or(25),
        poll_delay: Duration::from_millis(config.poll_delay_ms.unwrap_or(100)),
    }
}

fn service_should_apply_captured_tx_bringup_tail(channel: Channel, bandwidth: Bandwidth) -> bool {
    channel.number == 36 && matches!(bandwidth, Bandwidth::Mhz20 | Bandwidth::Mhz40)
}

fn service_runtime_tx_power_input(
    tx_power: &ResolvedServiceTxPowerControl,
    channel: Channel,
    bandwidth: Bandwidth,
) -> std::result::Result<ProductionRuntimeTxPowerControlInput, RuntimeRadioError> {
    let Some(mode) = service_tx_power_control_mode(tx_power)? else {
        return Ok(ProductionRuntimeTxPowerControlInput::None);
    };

    match mode {
        ServiceTxPowerControlMode::ManualIndex => {
            let index = tx_power.index.ok_or_else(|| {
                RuntimeRadioError::new(
                    "service_tx_power_manual_index_missing",
                    "--tx-power-mode manual-index requires --tx-power-index",
                )
            })?;
            Ok(ProductionRuntimeTxPowerControlInput::ManualIndex {
                path: Rtl8812auRfPath::from(tx_power.path),
                index,
            })
        }
        ServiceTxPowerControlMode::EfuseDerived => {
            let source = service_load_tx_power_efuse_source(tx_power)?;
            let plan = plan_rtl8812au_efuse_tx_power(
                &source.tx_power_data,
                channel,
                bandwidth,
                Rtl8812auRfPath::from(tx_power.path),
                Rtl8812auTxPowerSafetyProfile::from(tx_power.safety_profile),
                tx_power.max_index,
            )?;
            Ok(ProductionRuntimeTxPowerControlInput::EfuseDerived {
                source: source.report,
                plan,
            })
        }
    }
}

fn service_tx_power_control_mode(
    tx_power: &ResolvedServiceTxPowerControl,
) -> std::result::Result<Option<ServiceTxPowerControlMode>, RuntimeRadioError> {
    match (tx_power.mode, tx_power.index) {
        (Some(ServiceTxPowerControlMode::EfuseDerived), Some(_)) => Err(RuntimeRadioError::new(
            "service_tx_power_mode_conflict",
            "--tx-power-mode efuse-derived cannot be combined with --tx-power-index; use one explicit mode",
        )),
        (Some(ServiceTxPowerControlMode::ManualIndex), None) => Err(RuntimeRadioError::new(
            "service_tx_power_manual_index_missing",
            "--tx-power-mode manual-index requires --tx-power-index",
        )),
        (Some(mode), _) => Ok(Some(mode)),
        (None, Some(_)) => Ok(Some(ServiceTxPowerControlMode::ManualIndex)),
        (None, None) => Ok(None),
    }
}

#[derive(Debug, Clone)]
struct ServiceTxPowerEfuseSource {
    report: Rtl8812auTxPowerEfuseSourceReport,
    tx_power_data: Vec<u8>,
}

fn service_load_tx_power_efuse_source(
    tx_power: &ResolvedServiceTxPowerControl,
) -> std::result::Result<ServiceTxPowerEfuseSource, RuntimeRadioError> {
    match (
        tx_power.efuse_report.as_deref(),
        tx_power.efuse_logical_map.as_deref(),
    ) {
        (Some(_), Some(_)) => Err(RuntimeRadioError::new(
            "service_tx_power_efuse_source_conflict",
            "use only one of --tx-power-efuse-report or --tx-power-efuse-logical-map",
        )),
        (Some(path), None) => service_load_tx_power_efuse_report(path),
        (None, Some(path)) => {
            let bytes = fs::read(path).map_err(|error| {
                RuntimeRadioError::new(
                    "service_tx_power_efuse_logical_map_read_failed",
                    format!("{}: {error}", path.display()),
                )
            })?;
            service_tx_power_efuse_source_from_bytes(
                bytes,
                "efuse_logical_map_or_tx_power_region_binary",
                Some(path.to_path_buf()),
            )
        }
        (None, None) => Err(RuntimeRadioError::new(
            "service_tx_power_efuse_source_missing",
            "--tx-power-mode efuse-derived requires --tx-power-efuse-report or --tx-power-efuse-logical-map",
        )),
    }
}

fn service_load_tx_power_efuse_report(
    path: &Path,
) -> std::result::Result<ServiceTxPowerEfuseSource, RuntimeRadioError> {
    let input = fs::read_to_string(path).map_err(|error| {
        RuntimeRadioError::new(
            "service_tx_power_efuse_report_read_failed",
            format!("{}: {error}", path.display()),
        )
    })?;
    let json: serde_json::Value = serde_json::from_str(&input).map_err(|error| {
        RuntimeRadioError::new(
            "service_tx_power_efuse_report_parse_failed",
            format!("{}: {error}", path.display()),
        )
    })?;
    let (source_kind, hex) = service_json_string(&json, &["efuse", "logical_map_hex"])
        .map(|hex| ("efuse_report_logical_map", hex))
        .or_else(|| {
            service_json_string(&json, &["efuse", "summary", "tx_power", "data_hex"])
                .map(|hex| ("efuse_report_tx_power_region", hex))
        })
        .or_else(|| {
            service_json_string(&json, &["tx_power_data_hex"])
                .map(|hex| ("efuse_report_tx_power_region", hex))
        })
        .ok_or_else(|| {
            RuntimeRadioError::new(
                "service_tx_power_efuse_report_missing_hex",
                format!(
                    "{} does not contain efuse.logical_map_hex, efuse.summary.tx_power.data_hex, or tx_power_data_hex",
                    path.display()
                ),
            )
        })?;
    let bytes = parse_service_hex_bytes(hex).map_err(|message| {
        RuntimeRadioError::new(
            "service_tx_power_efuse_report_hex_invalid",
            format!("{}: {message}", path.display()),
        )
    })?;
    service_tx_power_efuse_source_from_bytes(bytes, source_kind, Some(path.to_path_buf()))
}

fn service_tx_power_efuse_source_from_bytes(
    bytes: Vec<u8>,
    source_kind: &'static str,
    source_path: Option<PathBuf>,
) -> std::result::Result<ServiceTxPowerEfuseSource, RuntimeRadioError> {
    let tx_power_data = if bytes.len() == RTL8812AU_EFUSE_TX_POWER_LEN {
        bytes
    } else if bytes.len() >= RTL8812AU_EFUSE_TX_POWER_START + RTL8812AU_EFUSE_TX_POWER_LEN {
        bytes[RTL8812AU_EFUSE_TX_POWER_START
            ..RTL8812AU_EFUSE_TX_POWER_START + RTL8812AU_EFUSE_TX_POWER_LEN]
            .to_vec()
    } else {
        return Err(RuntimeRadioError::new(
                "service_tx_power_efuse_source_too_short",
                format!(
                    "EFUSE source has {} bytes; expected an 84-byte TX-power region or a logical map at least {} bytes long",
                    bytes.len(),
                    RTL8812AU_EFUSE_TX_POWER_START + RTL8812AU_EFUSE_TX_POWER_LEN
                ),
            ));
    };
    let non_ff_bytes = tx_power_data.iter().filter(|byte| **byte != 0xff).count();
    Ok(ServiceTxPowerEfuseSource {
        report: Rtl8812auTxPowerEfuseSourceReport {
            source_kind,
            source_path,
            tx_power_start_offset: RTL8812AU_EFUSE_TX_POWER_START,
            tx_power_length: tx_power_data.len(),
            tx_power_data_hex: encode_service_hex(&tx_power_data),
            non_ff_bytes,
        },
        tx_power_data,
    })
}

fn service_json_string<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value, |cursor, key| cursor.get(*key))
        .and_then(serde_json::Value::as_str)
}

fn parse_service_hex_bytes(input: &str) -> std::result::Result<Vec<u8>, String> {
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

fn encode_service_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn load_service_realtek_table_plan(
    source_path: &Path,
    array_name: &str,
    kind: RealtekTableKind,
    condition_env: RealtekConditionEnv,
) -> std::result::Result<RealtekTablePlan, RuntimeRadioError> {
    let source = fs::read_to_string(source_path).map_err(|error| {
        RuntimeRadioError::new(
            "service_realtek_table_source_read_failed",
            format!("failed to read {}: {error}", source_path.display()),
        )
    })?;
    let values = parse_realtek_u32_array(&source, array_name).map_err(|error| {
        RuntimeRadioError::new("service_realtek_table_parse_failed", error.to_string())
    })?;
    plan_realtek_table(array_name, kind, &values, condition_env).map_err(|error| {
        RuntimeRadioError::new("service_realtek_table_plan_failed", error.to_string())
    })
}

fn rtl8812a_mac_source_default() -> PathBuf {
    PathBuf::from("/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_mac.c")
}

fn rtl8812a_bb_source_default() -> PathBuf {
    PathBuf::from("/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c")
}

fn rtl8812a_rf_source_default() -> PathBuf {
    PathBuf::from("/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c")
}

fn parse_service_rx_forward_arg(
    input: &str,
) -> std::result::Result<ServiceRxForwardArg, RuntimeRadioError> {
    let (lhs, rhs) = input.split_once('=').ok_or_else(|| {
        RuntimeRadioError::new(
            "service_rx_forward_parse_failed",
            "expected LINK_ID:RADIO_PORT=HOST:PORT or RADIO_PORT=HOST:PORT",
        )
    })?;
    let aggregator = rhs.parse::<SocketAddr>().map_err(|error| {
        RuntimeRadioError::new(
            "service_rx_forward_parse_failed",
            format!("invalid RX forward aggregator {rhs:?}: {error}"),
        )
    })?;
    let parts = lhs.split(':').collect::<Vec<_>>();
    let (link_id, radio_port) = match parts.as_slice() {
        [radio_port] => (None, parse_service_prefixed_u8(radio_port)?),
        [link_id, radio_port] => (
            Some(parse_service_prefixed_u32(link_id)?),
            parse_service_prefixed_u8(radio_port)?,
        ),
        _ => {
            return Err(RuntimeRadioError::new(
                "service_rx_forward_parse_failed",
                "expected at most one ':' before '='",
            ))
        }
    };
    Ok(ServiceRxForwardArg {
        link_id,
        radio_port,
        aggregator,
    })
}

fn parse_service_prefixed_u32(input: &str) -> std::result::Result<u32, RuntimeRadioError> {
    parse_prefixed_int(input)
        .and_then(|value| u32::try_from(value).map_err(|_| format!("{input} does not fit in u32")))
        .map_err(|error| RuntimeRadioError::new("service_rx_forward_parse_failed", error))
}

fn parse_service_prefixed_u8(input: &str) -> std::result::Result<u8, RuntimeRadioError> {
    parse_prefixed_int(input)
        .and_then(|value| u8::try_from(value).map_err(|_| format!("{input} does not fit in u8")))
        .map_err(|error| RuntimeRadioError::new("service_rx_forward_parse_failed", error))
}

impl From<ServiceTxCalibrationProfile> for TxCalibrationProfile {
    fn from(profile: ServiceTxCalibrationProfile) -> Self {
        match profile {
            ServiceTxCalibrationProfile::CurrentDefault => Self::CurrentDefault,
            ServiceTxCalibrationProfile::LinuxParityCh36Ht20 => Self::LinuxParityCh36Ht20,
            ServiceTxCalibrationProfile::Rtl8812aLck => Self::Rtl8812aLck,
            ServiceTxCalibrationProfile::Rtl8812aIqkProbe => Self::Rtl8812aIqkProbe,
            ServiceTxCalibrationProfile::Rtl8812aRuntimeIqk => Self::Rtl8812aRuntimeIqk,
        }
    }
}

impl From<ServiceTxPowerPath> for Rtl8812auRfPath {
    fn from(path: ServiceTxPowerPath) -> Self {
        match path {
            ServiceTxPowerPath::A => Self::A,
            ServiceTxPowerPath::B => Self::B,
            ServiceTxPowerPath::Both => Self::Both,
        }
    }
}

impl From<ServiceTxPowerSafetyProfile> for Rtl8812auTxPowerSafetyProfile {
    fn from(profile: ServiceTxPowerSafetyProfile) -> Self {
        match profile {
            ServiceTxPowerSafetyProfile::MaxIndex => Self::MaxIndex,
            ServiceTxPowerSafetyProfile::LinuxCh36Ht20 => Self::LinuxCh36Ht20,
        }
    }
}

fn parse_tx_power_index(input: &str) -> std::result::Result<u8, String> {
    let value = parse_u8(input)?;
    if value > RTL8812AU_TX_POWER_INDEX_MAX {
        return Err(format!(
            "TX power index {value} exceeds RTL8812AU maximum {RTL8812AU_TX_POWER_INDEX_MAX}"
        ));
    }
    Ok(value)
}

fn parse_u16(input: &str) -> std::result::Result<u16, String> {
    parse_prefixed_int(input)
        .and_then(|value| u16::try_from(value).map_err(|_| format!("{input} does not fit in u16")))
}

fn parse_u32(input: &str) -> std::result::Result<u32, String> {
    parse_prefixed_int(input)
        .and_then(|value| u32::try_from(value).map_err(|_| format!("{input} does not fit in u32")))
}

fn parse_u8(input: &str) -> std::result::Result<u8, String> {
    parse_prefixed_int(input)
        .and_then(|value| u8::try_from(value).map_err(|_| format!("{input} does not fit in u8")))
}

fn parse_prefixed_int(input: &str) -> std::result::Result<u64, String> {
    let trimmed = input.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|error| error.to_string())
    } else {
        trimmed.parse::<u64>().map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_config(name: &str, contents: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "wfb-radio-service-{name}-{}-{unique}.toml",
            std::process::id()
        ));
        fs::write(&path, contents).expect("write config");
        path
    }

    fn write_temp_file(name: &str, contents: &str) -> PathBuf {
        write_config(name, contents)
    }

    #[test]
    fn service_config_only_resolves_runtime_profile() {
        let path = write_config(
            "config-only",
            r#"
[adapter]
vid = 3034
pid = 34834

[macos_usbhost]
enabled = true

[radio]
channel = 36
bandwidth_mhz = 20
firmware = "/tmp/config-fw.bin"
duration_ms = 2500
rx_timeout_ms = 15
tx_burst_limit = 3
max_datagrams = 4

[wfb]
bind = "127.0.0.1:5610"
tx_binds = ["127.0.0.1:5611"]
link_id = 1
radio_port = 35
rx_aggregator = "127.0.0.1:5801"
rx_wlan_idx = 2
rx_mcs_index = 1

[authorization]
transmit = true

[tx_power]
mode = "efuse_derived"
path = "b"
efuse_report = "/tmp/config-efuse.json"
safety_profile = "max_index"
max_index = 42

[calibration]
profile = "rtl8812a_lck"

[artifacts]
ready_file = "/tmp/config-ready.json"
health_file = "/tmp/config-health.json"
"#,
        );
        let cli = ServiceCli::try_parse_from([
            "wfb-radio-service",
            "--config",
            path.to_string_lossy().as_ref(),
        ])
        .expect("parse service");

        let resolved = resolve_service_run(&cli).expect("resolve");
        let _ = fs::remove_file(path);

        assert_eq!(resolved.channel, 36);
        assert_eq!(resolved.bandwidth, Bandwidth::Mhz20);
        assert_eq!(resolved.firmware, Path::new("/tmp/config-fw.bin"));
        assert_eq!(resolved.bind, "127.0.0.1:5610".parse().unwrap());
        assert_eq!(resolved.tx_binds.len(), 1);
        assert_eq!(resolved.duration_ms, 2500);
        assert_eq!(resolved.rx_timeout_ms, 15);
        assert_eq!(resolved.tx_burst_limit, 3);
        assert_eq!(resolved.max_datagrams, 4);
        assert_eq!(resolved.rx_wlan_idx, 2);
        assert_eq!(resolved.rx_mcs_index, 1);
        assert!(resolved.tx_authorized);
        assert!(resolved.heartbeat_enabled);
        assert_eq!(
            resolved.tx_power.mode,
            Some(ServiceTxPowerControlMode::EfuseDerived)
        );
        assert_eq!(resolved.tx_power.path, ServiceTxPowerPath::B);
        assert_eq!(
            resolved.tx_power.efuse_report.as_deref(),
            Some(Path::new("/tmp/config-efuse.json"))
        );
        assert_eq!(
            resolved.tx_power.safety_profile,
            ServiceTxPowerSafetyProfile::MaxIndex
        );
        assert_eq!(resolved.tx_power.max_index, 42);
        assert_eq!(
            resolved.calibration_profile,
            ServiceTxCalibrationProfile::Rtl8812aLck
        );
    }

    #[test]
    fn service_cli_overrides_config() {
        let path = write_config(
            "overrides",
            r#"
[radio]
channel = 149
bandwidth_mhz = 40
firmware = "/tmp/config-fw.bin"
duration_ms = 5000
rx_timeout_ms = 40
tx_burst_limit = 8
max_datagrams = 0

[wfb]
bind = "127.0.0.1:5610"
tx_binds = ["127.0.0.1:5611"]

[authorization]
transmit = false

[tx_power]
mode = "efuse_derived"
path = "b"
efuse_report = "/tmp/config-efuse.json"
safety_profile = "linux_ch36_ht20"
max_index = 63

[calibration]
profile = "current_default"
"#,
        );
        let cli = ServiceCli::try_parse_from([
            "wfb-radio-service",
            "--config",
            path.to_string_lossy().as_ref(),
            "--channel",
            "36",
            "--bandwidth",
            "20",
            "--duration-ms",
            "25",
            "--rx-timeout-ms",
            "10",
            "--tx-burst-limit",
            "4",
            "--max-datagrams",
            "2",
            "--tx-bind",
            "127.0.0.1:5701",
            "--firmware",
            "/tmp/cli-fw.bin",
            "--ready-file",
            "/tmp/cli-ready.json",
            "--health-file",
            "/tmp/cli-health.json",
            "--tx-power-mode",
            "manual-index",
            "--tx-power-index",
            "0x1a",
            "--tx-power-path",
            "a",
            "--tx-power-safety-profile",
            "max-index",
            "--tx-power-max-index",
            "0x2a",
            "--tx-calibration-profile",
            "linux-parity-ch36-ht20",
            "--i-understand-this-transmits",
        ])
        .expect("parse service");

        let resolved = resolve_service_run(&cli).expect("resolve");
        let _ = fs::remove_file(path);

        assert_eq!(resolved.channel, 36);
        assert_eq!(resolved.bandwidth, Bandwidth::Mhz20);
        assert_eq!(resolved.firmware, Path::new("/tmp/cli-fw.bin"));
        assert_eq!(resolved.duration_ms, 25);
        assert_eq!(resolved.rx_timeout_ms, 10);
        assert_eq!(resolved.tx_burst_limit, 4);
        assert_eq!(resolved.max_datagrams, 2);
        assert_eq!(resolved.tx_binds.len(), 1);
        assert_eq!(
            resolved.ready_file.as_deref(),
            Some(Path::new("/tmp/cli-ready.json"))
        );
        assert_eq!(
            resolved.health_file.as_deref(),
            Some(Path::new("/tmp/cli-health.json"))
        );
        assert!(resolved.tx_authorized);
        assert_eq!(
            resolved.tx_power.mode,
            Some(ServiceTxPowerControlMode::ManualIndex)
        );
        assert_eq!(resolved.tx_power.index, Some(0x1a));
        assert_eq!(resolved.tx_power.path, ServiceTxPowerPath::A);
        assert_eq!(
            resolved.tx_power.safety_profile,
            ServiceTxPowerSafetyProfile::MaxIndex
        );
        assert_eq!(resolved.tx_power.max_index, 0x2a);
        assert_eq!(
            resolved.calibration_profile,
            ServiceTxCalibrationProfile::LinuxParityCh36Ht20
        );
    }

    #[test]
    fn service_efuse_tx_power_input_plans_before_usb_open() {
        let efuse_hex =
            include_str!("../../../fixtures/rf-quality/awus036ach-ch36-efuse-tx-power.hex").trim();
        let efuse_path = write_temp_file(
            "efuse-report",
            &format!(r#"{{"tx_power_data_hex":"{efuse_hex}"}}"#),
        );
        let tx_power = ResolvedServiceTxPowerControl {
            mode: Some(ServiceTxPowerControlMode::EfuseDerived),
            index: None,
            path: ServiceTxPowerPath::Both,
            efuse_report: Some(efuse_path.clone()),
            efuse_logical_map: None,
            safety_profile: ServiceTxPowerSafetyProfile::LinuxCh36Ht20,
            max_index: RTL8812AU_TX_POWER_INDEX_MAX,
        };
        let input = service_runtime_tx_power_input(
            &tx_power,
            Channel::from_number(36).expect("channel"),
            Bandwidth::Mhz20,
        )
        .expect("tx power input");
        let _ = fs::remove_file(efuse_path);

        match input {
            ProductionRuntimeTxPowerControlInput::EfuseDerived { source, plan } => {
                assert_eq!(source.tx_power_length, RTL8812AU_EFUSE_TX_POWER_LEN);
                assert_eq!(plan.channel, 36);
                assert_eq!(plan.bandwidth_mhz, 20);
                assert_eq!(plan.writes.len(), 22);
            }
            other => panic!("expected efuse-derived input, got {other:?}"),
        }
    }

    #[test]
    fn service_tx_power_mode_conflict_is_rejected_before_usb_open() {
        let tx_power = ResolvedServiceTxPowerControl {
            mode: Some(ServiceTxPowerControlMode::EfuseDerived),
            index: Some(0x1a),
            path: ServiceTxPowerPath::Both,
            efuse_report: None,
            efuse_logical_map: None,
            safety_profile: ServiceTxPowerSafetyProfile::LinuxCh36Ht20,
            max_index: RTL8812AU_TX_POWER_INDEX_MAX,
        };

        let error = service_runtime_tx_power_input(
            &tx_power,
            Channel::from_number(36).expect("channel"),
            Bandwidth::Mhz20,
        )
        .expect_err("mode conflict");

        assert_eq!(error.code, "service_tx_power_mode_conflict");
    }

    #[test]
    fn service_config_rejects_invalid_profile_names() {
        let path = write_config(
            "bad-profile",
            r#"
[radio]
channel = 36
firmware = "/tmp/rtl8812aefw.bin"

[calibration]
profile = "does_not_exist"
"#,
        );

        let error = load_service_config_file(&path).expect_err("invalid profile rejected");
        let _ = fs::remove_file(path);

        assert_eq!(error.code, "service_config_parse_failed");
        assert!(error.message.contains("does_not_exist"));
    }

    #[test]
    fn service_config_rejects_diagnostic_only_fields() {
        let path = write_config(
            "bad",
            r#"
[radio]
channel = 36
firmware = "/tmp/rtl8812aefw.bin"

[diagnostic]
pre_tx_write32 = ["0x0522=0x00000000"]
"#,
        );

        let error = load_service_config_file(&path).expect_err("unknown field rejected");
        let _ = fs::remove_file(path);

        assert_eq!(error.code, "service_config_parse_failed");
        assert!(error.message.contains("unknown field"));
    }

    #[test]
    fn service_cli_omits_diagnostic_register_experiments() {
        let help = ServiceCli::command().render_long_help().to_string();
        for flag in [
            "--pre-tx-write8",
            "--pre-tx-write16",
            "--pre-tx-write32",
            "--pre-tx-rmw32",
            "--pre-tx-rf-write",
            "--tx-status",
            "--clear-txdma-status-before-tx",
            "--rx-pcap",
            "--rx-frame-jsonl",
        ] {
            assert!(!help.contains(flag), "service CLI leaked {flag}");
        }
    }
}
