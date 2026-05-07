use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    thread,
    time::Duration,
};

use wfb_link::{
    LinkBackend, LinkConfig, LinkDirection, LinkEndpoints, MacosUserspaceRadioBackend,
    MacosUserspaceRadioConfig, PayloadKind,
};

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

    let tx_count = env_u64("WFB_LINK_TX_DATAGRAMS", 0)?;
    if tx_count > 0 {
        inject_tx_datagrams(handle.endpoints(), tx_count)?;
    }
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

fn inject_tx_datagrams(endpoints: &LinkEndpoints, count: u64) -> Result<(), Box<dyn Error>> {
    let target = match std::env::var("WFB_LINK_TX_TARGET") {
        Ok(value) => normalize_loopback_target(value.parse()?),
        Err(_) => endpoints
            .streams
            .iter()
            .find(|stream| {
                stream.direction == LinkDirection::Tx
                    && stream.payload_kind == PayloadKind::WfbDistributorDatagram
            })
            .map(|stream| normalize_loopback_target(stream.local_udp))
            .ok_or("no WFB distributor TX endpoint available")?,
    };
    let interval = Duration::from_micros(env_u64("WFB_LINK_TX_INTERVAL_US", 1_000)?);
    let link_id = env_u64("WFB_LINK_TX_LINK_ID", 1)? as u32 & 0x00ff_ffff;
    let radio_port = env_u64("WFB_LINK_TX_RADIO_PORT", 1)? as u8;
    let fwmark = env_u64("WFB_LINK_TX_FWMARK", 0)? as u32;
    let mcs = env_u64("WFB_LINK_TX_MCS", 2)? as u8;
    let bandwidth_mhz = env_u64("WFB_LINK_TX_BANDWIDTH_MHZ", 20)?;
    let payload_len = env_u64("WFB_LINK_TX_PAYLOAD_LEN", 256)? as usize;
    let marker = std::env::var("WFB_LINK_TX_MARKER").unwrap_or_else(|_| "WFLINK".to_string());
    let marker = marker.as_bytes();
    if marker.len() + 4 > payload_len {
        return Err("WFB_LINK_TX_MARKER plus sequence does not fit payload".into());
    }

    let socket = UdpSocket::bind(match target {
        SocketAddr::V4(_) => "127.0.0.1:0",
        SocketAddr::V6(_) => "[::1]:0",
    })?;
    for sequence in 0..count {
        let datagram = build_tx_datagram(
            link_id,
            radio_port,
            fwmark,
            mcs,
            bandwidth_mhz,
            payload_len,
            marker,
            sequence,
        );
        socket.send_to(&datagram, target)?;
        if !interval.is_zero() {
            thread::sleep(interval);
        }
    }
    Ok(())
}

fn normalize_loopback_target(target: SocketAddr) -> SocketAddr {
    match target {
        SocketAddr::V4(addr) if addr.ip().is_unspecified() => {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
        }
        SocketAddr::V6(addr) if addr.ip().is_unspecified() => {
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), addr.port())
        }
        other => other,
    }
}

fn build_tx_datagram(
    link_id: u32,
    radio_port: u8,
    fwmark: u32,
    mcs: u8,
    bandwidth_mhz: u64,
    payload_len: usize,
    marker: &[u8],
    sequence: u64,
) -> Vec<u8> {
    let bandwidth_flag = if bandwidth_mhz == 40 { 0x01 } else { 0x00 };
    let channel = ((link_id & 0x00ff_ffff) << 8) | u32::from(radio_port);
    let mut datagram = Vec::with_capacity(4 + 13 + 24 + payload_len);
    datagram.extend_from_slice(&fwmark.to_be_bytes());
    datagram.extend_from_slice(&[
        0x00,
        0x00,
        0x0d,
        0x00,
        0x00,
        0x80,
        0x08,
        0x00,
        0x08,
        0x00,
        0x37,
        bandwidth_flag,
        mcs,
    ]);
    let mut header = [
        0x08, 0x01, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x57, 0x42, 0x00, 0x00, 0x00,
        0x00, 0x57, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    header[12..16].copy_from_slice(&channel.to_be_bytes());
    header[18..22].copy_from_slice(&channel.to_be_bytes());
    header[22..24].copy_from_slice(&(sequence as u16).to_le_bytes());
    datagram.extend_from_slice(&header);

    let mut payload = Vec::with_capacity(payload_len);
    payload.extend_from_slice(marker);
    payload.extend_from_slice(&(sequence as u32).to_be_bytes());
    while payload.len() < payload_len {
        payload.push(((payload.len() as u64 + sequence) % 251) as u8);
    }
    datagram.extend_from_slice(&payload[..payload_len]);
    datagram
}

fn env_optional_u64(name: &str) -> Result<Option<u64>, Box<dyn Error>> {
    std::env::var(name)
        .ok()
        .map(|value| parse_u64(&value).map_err(Into::into))
        .transpose()
}

fn env_u64(name: &str, default: u64) -> Result<u64, Box<dyn Error>> {
    Ok(env_optional_u64(name)?.unwrap_or(default))
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
