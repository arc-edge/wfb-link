use std::{error::Error, thread, time::Duration};

use wfb_link::{
    LinkBackend, LinkConfig, LinkEndpointsBuilder, MacosUserspaceRadioBackend,
    MacosUserspaceRadioConfig, PayloadKind, StreamCriticality,
};

fn main() -> Result<(), Box<dyn Error>> {
    let config_path = std::env::args_os()
        .nth(1)
        .ok_or("usage: multi-stream-link <wfb-radio-service.toml>")?;
    let wait_ready_timeout = env_duration_s("WFB_LINK_READY_TIMEOUT_S", 90);

    let mut radio = MacosUserspaceRadioConfig::from_service_config_path(config_path)?;
    radio.endpoints = LinkEndpointsBuilder::new()
        .rx_stream_with_criticality(
            "s0",
            0,
            "127.0.0.1:5800",
            PayloadKind::RawApplicationDatagram,
            StreamCriticality::Required,
        )
        .rx_stream_with_criticality(
            "s1",
            1,
            "127.0.0.1:5801",
            PayloadKind::RawApplicationDatagram,
            StreamCriticality::BestEffort,
        )
        .tx_stream_with_criticality(
            "s2",
            2,
            "127.0.0.1:5802",
            PayloadKind::RawApplicationDatagram,
            StreamCriticality::Required,
        )
        .with_tunnel("10.5.0.1", "10.5.0.2")
        .build()?;

    let mut backend = MacosUserspaceRadioBackend::default();
    let handle = backend.start(LinkConfig::macos_userspace_radio(radio))?;
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

fn hold_duration() -> Result<Option<Duration>, Box<dyn Error>> {
    if let Some(ms) = env_optional_u64("WFB_LINK_HOLD_MS")? {
        return Ok(Some(Duration::from_millis(ms)));
    }
    Ok(env_optional_u64("WFB_LINK_HOLD_SECONDS")?.map(Duration::from_secs))
}

fn env_duration_s(name: &str, default: u64) -> Duration {
    Duration::from_secs(env_optional_u64(name).ok().flatten().unwrap_or(default))
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
