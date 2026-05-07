use std::{
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use anyhow::{anyhow, Result};
use clap::Parser;
use wfb_tun::{
    install_signal_handlers, reset_signal_state, run_tun_bridge, self_test, TunBridgeConfig,
    DEFAULT_RADIO_MTU, DEFAULT_TUN_MTU,
};

#[derive(Debug, Parser)]
#[command(about = "Bridge macOS utun IP packets to WFB-NG tunnel UDP messages.")]
struct Cli {
    #[arg(long)]
    self_test: bool,

    #[arg(long, default_value_t = 0)]
    utun_unit: u32,

    #[arg(long, default_value = "10.5.0.1")]
    local_ip: std::net::IpAddr,

    #[arg(long, default_value = "10.5.0.2")]
    peer_ip: std::net::IpAddr,

    #[arg(long, default_value_t = 24)]
    prefix_len: u8,

    #[arg(long, default_value_t = DEFAULT_TUN_MTU)]
    tun_mtu: usize,

    #[arg(long, default_value_t = DEFAULT_RADIO_MTU)]
    radio_mtu: usize,

    #[arg(long, value_parser = parse_endpoint, default_value = "127.0.0.1:56020")]
    tx_peer: SocketAddr,

    #[arg(long, value_parser = parse_endpoint, default_value = "127.0.0.1:56021")]
    rx_bind: SocketAddr,

    #[arg(long, default_value_t = 5.0)]
    agg_timeout_ms: f64,

    #[arg(long, default_value_t = 0.5)]
    keepalive_interval_s: f64,

    #[arg(long, default_value_t = 5.0)]
    stats_interval_s: f64,

    #[arg(long)]
    summary_file: Option<PathBuf>,

    #[arg(long = "no-configure", default_value_t = false)]
    no_configure: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.self_test {
        self_test()?;
        println!("self-test ok");
        return Ok(());
    }

    reset_signal_state();
    install_signal_handlers()?;
    let stop_requested = Arc::new(AtomicBool::new(false));
    run_tun_bridge(TunBridgeConfig {
        utun_unit: cli.utun_unit,
        local_ip: cli.local_ip,
        peer_ip: cli.peer_ip,
        prefix_len: cli.prefix_len,
        tun_mtu: cli.tun_mtu,
        radio_mtu: cli.radio_mtu,
        tx_peer: cli.tx_peer,
        rx_bind: cli.rx_bind,
        agg_timeout: duration_from_millis(cli.agg_timeout_ms)?,
        keepalive_interval: duration_from_secs(cli.keepalive_interval_s)?,
        stats_interval: if cli.stats_interval_s <= 0.0 {
            None
        } else {
            Some(duration_from_secs(cli.stats_interval_s)?)
        },
        summary_file: cli.summary_file,
        configure: !cli.no_configure,
        stop_requested: Some(stop_requested),
    })?;
    Ok(())
}

fn parse_endpoint(value: &str) -> std::result::Result<SocketAddr, String> {
    value
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| format!("could not resolve endpoint {value:?}"))
}

fn duration_from_millis(ms: f64) -> Result<Duration> {
    if !ms.is_finite() || ms < 0.0 {
        return Err(anyhow!("duration must be a non-negative finite value"));
    }
    Ok(Duration::from_secs_f64(ms / 1000.0))
}

fn duration_from_secs(seconds: f64) -> Result<Duration> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(anyhow!("duration must be a non-negative finite value"));
    }
    Ok(Duration::from_secs_f64(seconds))
}
