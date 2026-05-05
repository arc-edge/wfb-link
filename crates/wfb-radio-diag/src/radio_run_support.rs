use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use radio_core::Bandwidth;
use serde::Deserialize;
use wfb_radio_runtime::RuntimeRadioError;

use crate::{
    parse_bandwidth, parse_bridge_run_rx_forward_arg, AdapterArgs, BridgeRunRxForwardArg,
    HeartbeatLedArgs, MacosUsbHostArgs, RadioRunArgs, TxCalibrationProfileArg,
    TxCalibrationProfileArgs, TxPowerControlArgs, TxPowerControlModeArg, TxPowerPathArg,
    TxPowerSafetyProfileArg,
};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunConfigFile {
    pub(crate) adapter: Option<RadioRunAdapterConfig>,
    pub(crate) macos_usbhost: Option<RadioRunMacosUsbHostConfig>,
    pub(crate) radio: Option<RadioRunRadioConfig>,
    pub(crate) wfb: Option<RadioRunWfbConfig>,
    pub(crate) tx_power: Option<RadioRunTxPowerConfig>,
    pub(crate) calibration: Option<RadioRunCalibrationConfig>,
    pub(crate) heartbeat: Option<RadioRunHeartbeatConfig>,
    pub(crate) authorization: Option<RadioRunAuthorizationConfig>,
    pub(crate) artifacts: Option<RadioRunArtifactConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunAdapterConfig {
    pub(crate) vid: Option<u16>,
    pub(crate) pid: Option<u16>,
    pub(crate) bus: Option<u8>,
    pub(crate) address: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunMacosUsbHostConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) configuration_value: Option<u8>,
    pub(crate) interface_number: Option<u8>,
    pub(crate) bulk_in_endpoint: Option<u8>,
    pub(crate) bulk_out_endpoint: Option<u8>,
    pub(crate) bulk_out_endpoint_count: Option<usize>,
    pub(crate) poll_attempts: Option<u32>,
    pub(crate) poll_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunRadioConfig {
    pub(crate) channel: Option<u8>,
    pub(crate) bandwidth_mhz: Option<u16>,
    pub(crate) firmware: Option<PathBuf>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) rx_timeout_ms: Option<u64>,
    pub(crate) tx_burst_limit: Option<u32>,
    pub(crate) max_datagrams: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunWfbConfig {
    pub(crate) bind: Option<SocketAddr>,
    pub(crate) tx_binds: Option<Vec<SocketAddr>>,
    pub(crate) link_id: Option<u32>,
    pub(crate) radio_port: Option<u8>,
    pub(crate) rx_aggregator: Option<SocketAddr>,
    pub(crate) rx_forwards: Option<Vec<String>>,
    pub(crate) rx_wlan_idx: Option<u8>,
    pub(crate) rx_mcs_index: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunTxPowerConfig {
    pub(crate) mode: Option<TxPowerControlModeArg>,
    pub(crate) index: Option<u8>,
    pub(crate) path: Option<TxPowerPathArg>,
    pub(crate) efuse_report: Option<PathBuf>,
    pub(crate) efuse_logical_map: Option<PathBuf>,
    pub(crate) safety_profile: Option<TxPowerSafetyProfileArg>,
    pub(crate) max_index: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunCalibrationConfig {
    pub(crate) profile: Option<TxCalibrationProfileArg>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunHeartbeatConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) half_period_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunAuthorizationConfig {
    pub(crate) transmit: Option<bool>,
    pub(crate) live_register_writes: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct RadioRunArtifactConfig {
    pub(crate) ready_file: Option<PathBuf>,
    pub(crate) health_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedRadioRunArgs {
    pub(crate) adapter: AdapterArgs,
    pub(crate) macos_usbhost: MacosUsbHostArgs,
    pub(crate) tx_power: TxPowerControlArgs,
    pub(crate) tx_calibration: TxCalibrationProfileArgs,
    pub(crate) heartbeat_led: HeartbeatLedArgs,
    pub(crate) channel: u8,
    pub(crate) bandwidth: Bandwidth,
    pub(crate) firmware: PathBuf,
    pub(crate) bind: SocketAddr,
    pub(crate) tx_binds: Vec<SocketAddr>,
    pub(crate) duration_ms: u64,
    pub(crate) rx_timeout_ms: u64,
    pub(crate) tx_burst_limit: u32,
    pub(crate) max_datagrams: u32,
    pub(crate) ready_file: Option<PathBuf>,
    pub(crate) health_file: Option<PathBuf>,
    pub(crate) i_understand_this_transmits: bool,
    pub(crate) i_understand_this_writes_registers: bool,
    pub(crate) wfb_link_id: Option<u32>,
    pub(crate) wfb_radio_port: Option<u8>,
    pub(crate) rx_aggregator: Option<SocketAddr>,
    pub(crate) rx_forwards: Vec<BridgeRunRxForwardArg>,
    pub(crate) rx_wlan_idx: u8,
    pub(crate) rx_mcs_index: u8,
}

pub(crate) fn load_radio_run_config_file(
    path: &Path,
) -> std::result::Result<RadioRunConfigFile, RuntimeRadioError> {
    let input = fs::read_to_string(path).map_err(|error| {
        RuntimeRadioError::new(
            "radio_run_config_read_failed",
            format!("{}: {error}", path.display()),
        )
    })?;
    toml::from_str(&input).map_err(|error| {
        RuntimeRadioError::new(
            "radio_run_config_parse_failed",
            format!("{}: {error}", path.display()),
        )
    })
}

pub(crate) fn radio_run_resolved_args(
    args: &RadioRunArgs,
) -> std::result::Result<ResolvedRadioRunArgs, RuntimeRadioError> {
    let file = match args.config.as_deref() {
        Some(path) => load_radio_run_config_file(path)?,
        None => RadioRunConfigFile::default(),
    };
    let adapter = file.adapter.as_ref();
    let macos = file.macos_usbhost.as_ref();
    let radio = file.radio.as_ref();
    let wfb = file.wfb.as_ref();
    let tx_power = file.tx_power.as_ref();
    let calibration = file.calibration.as_ref();
    let heartbeat = file.heartbeat.as_ref();
    let authorization = file.authorization.as_ref();
    let artifacts = file.artifacts.as_ref();
    let default_macos = MacosUsbHostArgs::default();
    let default_tx_power = TxPowerControlArgs::default();
    let default_heartbeat = HeartbeatLedArgs::default();

    let channel = args
        .channel
        .or_else(|| radio.and_then(|radio| radio.channel))
        .ok_or_else(|| radio_run_missing_required("radio.channel"))?;
    let bandwidth = args
        .bandwidth
        .or(radio_run_config_bandwidth(
            radio.and_then(|radio| radio.bandwidth_mhz),
        )?)
        .unwrap_or(Bandwidth::Mhz20);
    let firmware = args
        .firmware
        .clone()
        .or_else(|| radio.and_then(|radio| radio.firmware.clone()))
        .ok_or_else(|| radio_run_missing_required("radio.firmware"))?;
    let rx_forwards = if args.rx_forwards.is_empty() {
        radio_run_config_rx_forwards(wfb.and_then(|wfb| wfb.rx_forwards.as_ref()))?
    } else {
        args.rx_forwards.clone()
    };

    Ok(ResolvedRadioRunArgs {
        adapter: AdapterArgs {
            vid: args
                .adapter
                .vid
                .or_else(|| adapter.and_then(|adapter| adapter.vid)),
            pid: args
                .adapter
                .pid
                .or_else(|| adapter.and_then(|adapter| adapter.pid)),
            bus: args
                .adapter
                .bus
                .or_else(|| adapter.and_then(|adapter| adapter.bus)),
            address: args
                .adapter
                .address
                .or_else(|| adapter.and_then(|adapter| adapter.address)),
        },
        macos_usbhost: MacosUsbHostArgs {
            enabled: args.macos_usbhost.enabled
                || macos.and_then(|macos| macos.enabled).unwrap_or(false),
            configuration_value: cli_or_config_or_default(
                args.macos_usbhost.configuration_value,
                default_macos.configuration_value,
                macos.and_then(|macos| macos.configuration_value),
            ),
            interface_number: cli_or_config_or_default(
                args.macos_usbhost.interface_number,
                default_macos.interface_number,
                macos.and_then(|macos| macos.interface_number),
            ),
            bulk_in_endpoint: cli_or_config_or_default(
                args.macos_usbhost.bulk_in_endpoint,
                default_macos.bulk_in_endpoint,
                macos.and_then(|macos| macos.bulk_in_endpoint),
            ),
            bulk_out_endpoint: cli_or_config_or_default(
                args.macos_usbhost.bulk_out_endpoint,
                default_macos.bulk_out_endpoint,
                macos.and_then(|macos| macos.bulk_out_endpoint),
            ),
            bulk_out_endpoint_count: cli_or_config_or_default(
                args.macos_usbhost.bulk_out_endpoint_count,
                default_macos.bulk_out_endpoint_count,
                macos.and_then(|macos| macos.bulk_out_endpoint_count),
            ),
            poll_attempts: cli_or_config_or_default(
                args.macos_usbhost.poll_attempts,
                default_macos.poll_attempts,
                macos.and_then(|macos| macos.poll_attempts),
            ),
            poll_delay_ms: cli_or_config_or_default(
                args.macos_usbhost.poll_delay_ms,
                default_macos.poll_delay_ms,
                macos.and_then(|macos| macos.poll_delay_ms),
            ),
        },
        tx_power: TxPowerControlArgs {
            tx_power_index: args
                .tx_power
                .tx_power_index
                .or_else(|| tx_power.and_then(|tx_power| tx_power.index)),
            tx_power_mode: args
                .tx_power
                .tx_power_mode
                .or_else(|| tx_power.and_then(|tx_power| tx_power.mode)),
            tx_power_path: cli_or_config_or_default(
                args.tx_power.tx_power_path,
                default_tx_power.tx_power_path,
                tx_power.and_then(|tx_power| tx_power.path),
            ),
            tx_power_efuse_report: args
                .tx_power
                .tx_power_efuse_report
                .clone()
                .or_else(|| tx_power.and_then(|tx_power| tx_power.efuse_report.clone())),
            tx_power_efuse_logical_map: args
                .tx_power
                .tx_power_efuse_logical_map
                .clone()
                .or_else(|| tx_power.and_then(|tx_power| tx_power.efuse_logical_map.clone())),
            tx_power_safety_profile: cli_or_config_or_default(
                args.tx_power.tx_power_safety_profile,
                default_tx_power.tx_power_safety_profile,
                tx_power.and_then(|tx_power| tx_power.safety_profile),
            ),
            tx_power_max_index: cli_or_config_or_default(
                args.tx_power.tx_power_max_index,
                default_tx_power.tx_power_max_index,
                tx_power.and_then(|tx_power| tx_power.max_index),
            ),
        },
        tx_calibration: TxCalibrationProfileArgs {
            tx_calibration_profile: cli_or_config_or_default(
                args.tx_calibration.tx_calibration_profile,
                TxCalibrationProfileArg::CurrentDefault,
                calibration.and_then(|calibration| calibration.profile),
            ),
        },
        heartbeat_led: HeartbeatLedArgs {
            no_heartbeat_led: args.heartbeat_led.no_heartbeat_led
                || heartbeat
                    .and_then(|heartbeat| heartbeat.enabled)
                    .map(|enabled| !enabled)
                    .unwrap_or(false),
            heartbeat_led_half_period_ms: cli_or_config_or_default(
                args.heartbeat_led.heartbeat_led_half_period_ms,
                default_heartbeat.heartbeat_led_half_period_ms,
                heartbeat.and_then(|heartbeat| heartbeat.half_period_ms),
            ),
        },
        channel,
        bandwidth,
        firmware,
        bind: args
            .bind
            .or_else(|| wfb.and_then(|wfb| wfb.bind))
            .unwrap_or_else(radio_run_default_bind),
        tx_binds: if args.tx_binds.is_empty() {
            wfb.and_then(|wfb| wfb.tx_binds.clone()).unwrap_or_default()
        } else {
            args.tx_binds.clone()
        },
        duration_ms: args
            .duration_ms
            .or_else(|| radio.and_then(|radio| radio.duration_ms))
            .unwrap_or(10_000),
        rx_timeout_ms: args
            .rx_timeout_ms
            .or_else(|| radio.and_then(|radio| radio.rx_timeout_ms))
            .unwrap_or(20),
        tx_burst_limit: args
            .tx_burst_limit
            .or_else(|| radio.and_then(|radio| radio.tx_burst_limit))
            .unwrap_or(8),
        max_datagrams: args
            .max_datagrams
            .or_else(|| radio.and_then(|radio| radio.max_datagrams))
            .unwrap_or(0),
        ready_file: args
            .ready_file
            .clone()
            .or_else(|| artifacts.and_then(|artifacts| artifacts.ready_file.clone())),
        health_file: args
            .health_file
            .clone()
            .or_else(|| artifacts.and_then(|artifacts| artifacts.health_file.clone())),
        i_understand_this_transmits: args.i_understand_this_transmits
            || authorization
                .and_then(|authorization| authorization.transmit)
                .unwrap_or(false),
        i_understand_this_writes_registers: args.i_understand_this_writes_registers
            || authorization
                .and_then(|authorization| authorization.live_register_writes)
                .unwrap_or(false),
        wfb_link_id: args.wfb_link_id.or_else(|| wfb.and_then(|wfb| wfb.link_id)),
        wfb_radio_port: args
            .wfb_radio_port
            .or_else(|| wfb.and_then(|wfb| wfb.radio_port)),
        rx_aggregator: args
            .rx_aggregator
            .or_else(|| wfb.and_then(|wfb| wfb.rx_aggregator)),
        rx_forwards,
        rx_wlan_idx: args
            .rx_wlan_idx
            .or_else(|| wfb.and_then(|wfb| wfb.rx_wlan_idx))
            .unwrap_or(0),
        rx_mcs_index: args
            .rx_mcs_index
            .or_else(|| wfb.and_then(|wfb| wfb.rx_mcs_index))
            .unwrap_or(0),
    })
}

fn radio_run_missing_required(field: &'static str) -> RuntimeRadioError {
    RuntimeRadioError::new(
        "radio_run_config_missing_required",
        format!("radio-run requires {field} from CLI or --config"),
    )
}

fn radio_run_invalid_config_field(
    field: &'static str,
    message: impl Into<String>,
) -> RuntimeRadioError {
    RuntimeRadioError::new(
        "radio_run_config_invalid_field",
        format!("{field}: {}", message.into()),
    )
}

fn radio_run_default_bind() -> SocketAddr {
    "127.0.0.1:5600"
        .parse()
        .expect("radio-run default bind address")
}

fn radio_run_config_bandwidth(
    bandwidth_mhz: Option<u16>,
) -> std::result::Result<Option<Bandwidth>, RuntimeRadioError> {
    bandwidth_mhz
        .map(|mhz| {
            parse_bandwidth(&mhz.to_string())
                .map_err(|error| radio_run_invalid_config_field("radio.bandwidth_mhz", error))
        })
        .transpose()
}

fn radio_run_config_rx_forwards(
    forwards: Option<&Vec<String>>,
) -> std::result::Result<Vec<BridgeRunRxForwardArg>, RuntimeRadioError> {
    forwards
        .into_iter()
        .flatten()
        .map(|forward| {
            parse_bridge_run_rx_forward_arg(forward)
                .map_err(|error| radio_run_invalid_config_field("wfb.rx_forwards", error))
        })
        .collect()
}

fn cli_or_config_or_default<T: Copy + PartialEq>(cli: T, default: T, config: Option<T>) -> T {
    if cli != default {
        cli
    } else {
        config.unwrap_or(default)
    }
}
