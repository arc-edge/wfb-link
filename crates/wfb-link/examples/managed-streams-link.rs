use std::{error::Error, net::SocketAddr, path::PathBuf, thread, time::Duration};

use wfb_link::{
    LinkBackend, LinkConfig, ManagedWfbStreamConfig, ManagedWfbStreamsBackend,
    ManagedWfbStreamsConfig, ManagedWfbTxProfile,
};

fn main() -> Result<(), Box<dyn Error>> {
    let config_path = std::env::args_os()
        .nth(1)
        .ok_or("usage: managed-streams-link <wfb-radio-service.toml>")?;
    let wfb_key = env_path("WFB_KEY")?;
    let out_dir = env_path("OUT_DIR")
        .unwrap_or_else(|_| std::env::temp_dir().join("wfb-link-managed-streams"));
    let wait_ready_timeout = env_duration_s("WFB_LINK_READY_TIMEOUT_S", 90);

    let radio = wfb_link::UserspaceRadioConfig::from_service_config_path(config_path)?;
    let config = ManagedWfbStreamsConfig::from_radio_config(radio, wfb_key)
        .with_artifact_dir(out_dir)
        .with_bins(
            env_path("WFB_TX_BIN").unwrap_or_else(|_| "target/wfb-ng-macos/bin/wfb_tx".into()),
            env_path("WFB_RX_BIN").unwrap_or_else(|_| "target/wfb-ng-macos/bin/wfb_rx".into()),
        )
        .with_stream(
            ManagedWfbStreamConfig::rx(
                "video-down",
                4,
                env_socket("VIDEO_DOWN_UDP", "127.0.0.1:5804")?,
            )
            .with_link_id(env_u32("LINK_ID", 1)),
        )
        .with_stream(
            ManagedWfbStreamConfig::rx(
                "telemetry-down",
                5,
                env_socket("TELEMETRY_DOWN_UDP", "127.0.0.1:5805")?,
            )
            .with_link_id(env_u32("LINK_ID", 1)),
        )
        .with_stream(
            ManagedWfbStreamConfig::tx(
                "control-up",
                6,
                env_socket("CONTROL_UP_UDP", "127.0.0.1:5606")?,
            )
            .with_link_id(env_u32("LINK_ID", 1))
            .with_tx_profile(ManagedWfbTxProfile {
                bandwidth_mhz: env_u16("CONTROL_BANDWIDTH_MHZ", 20),
                mcs: env_u8("CONTROL_MCS", 0),
                fec_k: env_u8("CONTROL_FEC_K", 2),
                fec_n: env_u8("CONTROL_FEC_N", 16),
            }),
        );

    let mut backend = ManagedWfbStreamsBackend::default();
    let handle = backend.start(LinkConfig::managed_wfb_streams(config))?;
    let ready = handle.wait_ready(wait_ready_timeout)?;
    println!("{}", serde_json::to_string_pretty(&ready)?);

    if let Some(hold) = hold_duration()? {
        thread::sleep(hold);
    }

    let health = handle.health()?;
    println!("{}", serde_json::to_string_pretty(&health)?);
    handle.request_stop()?;
    let report = handle.join()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn env_path(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    Ok(PathBuf::from(
        std::env::var_os(name).ok_or_else(|| format!("{name} is required"))?,
    ))
}

fn env_socket(name: &str, default: &str) -> Result<SocketAddr, Box<dyn Error>> {
    Ok(std::env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse()?)
}

fn hold_duration() -> Result<Option<Duration>, Box<dyn Error>> {
    if let Some(ms) = env_optional_u64("WFB_LINK_HOLD_MS")? {
        return Ok(Some(Duration::from_millis(ms)));
    }
    Ok(env_optional_u64("WFB_LINK_HOLD_SECONDS")?.map(Duration::from_secs))
}

fn env_duration_s(name: &str, default: u64) -> Duration {
    Duration::from_secs(env_optional_u64(name).ok().flatten().unwrap_or(default))
}

fn env_u32(name: &str, default: u32) -> u32 {
    env_optional_u64(name)
        .ok()
        .flatten()
        .unwrap_or(default.into()) as u32
}

fn env_u16(name: &str, default: u16) -> u16 {
    env_optional_u64(name)
        .ok()
        .flatten()
        .unwrap_or(default.into()) as u16
}

fn env_u8(name: &str, default: u8) -> u8 {
    env_optional_u64(name)
        .ok()
        .flatten()
        .unwrap_or(default.into()) as u8
}

fn env_optional_u64(name: &str) -> Result<Option<u64>, Box<dyn Error>> {
    std::env::var(name)
        .ok()
        .map(|value| parse_u64(&value).map_err(Into::into))
        .transpose()
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
