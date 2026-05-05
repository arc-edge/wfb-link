use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use clap::{Parser, ValueEnum};
use radio_core::Bandwidth;
use serde::Deserialize;
use wfb_radio_runtime::{RuntimeRadioError, DEFAULT_HEARTBEAT_HALF_PERIOD_MS};

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
