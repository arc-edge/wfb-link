use std::{error::Error, time::Duration};

use wfb_link::{LinkBackend, LinkConfig, MacosUserspaceRadioBackend, MacosUserspaceRadioConfig};

fn main() -> Result<(), Box<dyn Error>> {
    let config_path = std::env::args_os()
        .nth(1)
        .ok_or("usage: embed-radio-service <wfb-radio-service.toml>")?;
    let wait_ready_timeout = std::env::var("WFB_LINK_READY_TIMEOUT_S")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(60));

    let radio = MacosUserspaceRadioConfig::from_service_config_path(config_path)?;
    let mut backend = MacosUserspaceRadioBackend::default();
    let handle = backend.start(LinkConfig::macos_userspace_radio(radio))?;

    let ready = handle.wait_ready(wait_ready_timeout)?;
    println!("{}", serde_json::to_string_pretty(&ready)?);

    let health = handle.health()?;
    println!("{}", serde_json::to_string_pretty(&health)?);

    handle.request_stop()?;
    let report = handle.join()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
