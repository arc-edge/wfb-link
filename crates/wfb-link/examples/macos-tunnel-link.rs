use std::{
    error::Error,
    net::IpAddr,
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use radio_core::Channel;
use wfb_link::{
    LinkBackend, LinkConfig, MacosUserspaceRadioConfig, MacosWfbTunnelBackend, MacosWfbTunnelConfig,
};
use wfb_radio_runtime::{ProductionRuntimeAirtimeSchedule, ProductionRuntimeTddWindow};

fn main() -> Result<(), Box<dyn Error>> {
    let config_path = std::env::args_os()
        .nth(1)
        .ok_or("usage: macos-tunnel-link <wfb-radio-service.toml>")?;
    let wfb_key = env_path("WFB_KEY")?;
    let out_dir = env_path("OUT_DIR").unwrap_or_else(|_| default_out_dir());
    let wait_ready_timeout = env_duration_s("WFB_LINK_READY_TIMEOUT_S", 90);

    let mut radio = MacosUserspaceRadioConfig::from_service_config_path(config_path)?;
    apply_radio_overrides(&mut radio)?;

    let mut tunnel = MacosWfbTunnelConfig::from_radio_config(radio, wfb_key)
        .with_artifact_dir(&out_dir)
        .with_bins(
            env_path("WFB_TX_BIN").unwrap_or_else(|_| "target/wfb-ng-macos/bin/wfb_tx".into()),
            env_path("WFB_RX_BIN").unwrap_or_else(|_| "target/wfb-ng-macos/bin/wfb_rx".into()),
            env_path("TUN_BIN").unwrap_or_else(|_| "target/debug/wfb-tun-macos".into()),
        )
        .with_tunnel_streams(
            env_u32("LINK_ID", 0),
            env_u8("TUN_RX_RADIO_PORT", 3),
            env_u8("TUN_TX_RADIO_PORT", 4),
        )
        .with_tx_profile(
            env_u16("BANDWIDTH_MHZ", 20),
            env_u8("MCS", 1),
            env_u8("FEC_K", 2),
            env_u8("FEC_N", 4),
        )
        .with_sudo_for_tun(env_bool("TUN_USE_SUDO", true));
    tunnel.radio.runtime_config.tx_min_interval_us = env_u64("TX_MIN_INTERVAL_US", 700);
    tunnel.radio.runtime_config.duration_ms = 0;
    tunnel.radio.runtime_config.max_datagrams = 0;
    if let (Ok(local_ip), Ok(peer_ip)) = (env_ip("LOCAL_IP"), env_ip("PEER_IP")) {
        tunnel = tunnel.with_tunnel_ips(local_ip, peer_ip);
    }

    let mut backend = MacosWfbTunnelBackend::default();
    let handle = backend.start(LinkConfig::macos_wfb_tunnel(tunnel))?;
    let ready = handle.wait_ready(wait_ready_timeout)?;
    println!("{}", serde_json::to_string_pretty(&ready)?);

    let mut probe_status = 0;
    if let Ok(probe_command) = std::env::var("WFB_LINK_PROBE_COMMAND") {
        let status = Command::new("bash")
            .arg("-lc")
            .arg(&probe_command)
            .status()?;
        probe_status = status.code().unwrap_or(1);
    } else if let Ok(hold_s) = std::env::var("WFB_LINK_HOLD_SECONDS") {
        std::thread::sleep(Duration::from_secs(hold_s.parse()?));
    }

    let health = handle.health()?;
    println!("{}", serde_json::to_string_pretty(&health)?);
    handle.request_stop()?;
    let report = handle.join()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    std::process::exit(probe_status);
}

fn apply_radio_overrides(radio: &mut MacosUserspaceRadioConfig) -> Result<(), Box<dyn Error>> {
    if let Ok(channel) = std::env::var("CHANNEL") {
        radio.runtime_config.channel = Channel::from_number(channel.parse()?)?;
    }
    if let Ok(bind) = std::env::var("RADIO_BIND") {
        radio.runtime_config.bind_addr = bind.parse()?;
    }
    if let Ok(aggregator) = std::env::var("AGGREGATOR") {
        radio.runtime_config.primary_rx_forward.aggregator = Some(aggregator.parse()?);
    }
    if std::env::var("AIRTIME_MODE")
        .map(|value| value == "tdd")
        .unwrap_or(false)
    {
        let first_window = match std::env::var("AIRTIME_TDD_FIRST_WINDOW")
            .unwrap_or_else(|_| "rx".to_string())
            .as_str()
        {
            "tx" => ProductionRuntimeTddWindow::Tx,
            _ => ProductionRuntimeTddWindow::Rx,
        };
        radio.runtime_config.airtime_schedule = ProductionRuntimeAirtimeSchedule::tdd(
            first_window,
            env_u64("AIRTIME_TDD_RX_WINDOW_MS", 1000),
            env_u64("AIRTIME_TDD_TX_WINDOW_MS", 1000),
            env_u64("AIRTIME_TDD_GUARD_MS", 100),
            env_u64("AIRTIME_TDD_START_DELAY_MS", 0),
        );
    }
    Ok(())
}

fn env_path(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    Ok(PathBuf::from(
        std::env::var_os(name).ok_or_else(|| format!("{name} is required"))?,
    ))
}

fn default_out_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("wfb-link-tunnel-{stamp}"))
}

fn env_duration_s(name: &str, default: u64) -> Duration {
    Duration::from_secs(env_u64(name, default))
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn env_ip(name: &str) -> Result<IpAddr, Box<dyn Error>> {
    Ok(std::env::var(name)?.parse()?)
}

fn env_u8(name: &str, default: u8) -> u8 {
    env_u64(name, default.into()).try_into().unwrap_or(default)
}

fn env_u16(name: &str, default: u16) -> u16 {
    env_u64(name, default.into()).try_into().unwrap_or(default)
}

fn env_u32(name: &str, default: u32) -> u32 {
    env_u64(name, default.into()).try_into().unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| parse_u64(&value).ok())
        .unwrap_or(default)
}

fn parse_u64(value: &str) -> Result<u64, std::num::ParseIntError> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        value.parse()
    }
}
