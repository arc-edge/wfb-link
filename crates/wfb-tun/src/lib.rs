//! WFB-NG tunnel packet bridge helpers.
//!
//! The production macOS tunnel bridge lives here so adopters do not need a
//! Python helper in the critical data path. The wire behavior intentionally
//! matches the original development script: each UDP tunnel message is a
//! sequence of `u16be length + IP packet` records, and each utun frame carries
//! the standard four-byte address-family header before the IP packet.

use std::{
    ffi::c_void,
    fs, io,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    os::fd::{AsRawFd, RawFd},
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{json, Map, Value};
use thiserror::Error;

pub const DEFAULT_RADIO_MTU: usize = 1445;
pub const DEFAULT_TUN_MTU: usize = DEFAULT_RADIO_MTU - 2;
pub const SUMMARY_SCHEMA: &str = "wfb_mac_wf_tun_summary/v1";

const CTLIOCGINFO: libc::c_ulong = 0xC0644E03;
const UTUN_OPT_IFNAME: libc::c_int = 2;
const UTUN_CONTROL_NAME: &[u8] = b"com.apple.net.utun_control\0";

static SIGNAL_STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
static SIGNAL_NUMBER: AtomicI32 = AtomicI32::new(0);

#[derive(Debug, Error)]
pub enum TunError {
    #[error("{context}: {source}")]
    Io {
        context: &'static str,
        source: io::Error,
    },
    #[error("{context}: command exited with status {status}: stdout={stdout:?} stderr={stderr:?}")]
    Command {
        context: &'static str,
        status: i32,
        stdout: String,
        stderr: String,
    },
    #[error("invalid IPv4 prefix length: {0}")]
    InvalidPrefixLen(u8),
    #[error("configure requires IPv4 local and peer addresses: local={local_ip} peer={peer_ip}")]
    ConfigureRequiresIpv4 { local_ip: IpAddr, peer_ip: IpAddr },
    #[error("unsupported IP version {0}")]
    UnsupportedIpVersion(u8),
    #[error("unsupported utun address family {0}")]
    UnsupportedAddressFamily(u32),
    #[error("packet record exceeds aggregate MTU: {record_len} > {max_size}")]
    PacketRecordTooLarge { record_len: usize, max_size: usize },
    #[error("utun frame too short")]
    UtunFrameTooShort,
    #[error("empty IP packet")]
    EmptyIpPacket,
    #[error("partial utun write: wrote {wrote} of {expected} bytes")]
    PartialUtunWrite { wrote: usize, expected: usize },
    #[error("wfb-tun currently supports macOS utun only")]
    UnsupportedPlatform,
    #[error("{0}")]
    Other(String),
}

impl TunError {
    fn io(context: &'static str, source: io::Error) -> Self {
        Self::Io { context, source }
    }
}

pub type Result<T> = std::result::Result<T, TunError>;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Counters {
    pub tun_packets_in: u64,
    pub tun_bytes_in: u64,
    pub tunnel_datagrams_out: u64,
    pub tunnel_bytes_out: u64,
    pub tunnel_datagrams_in: u64,
    pub tunnel_bytes_in: u64,
    pub tun_packets_out: u64,
    pub tun_bytes_out: u64,
    pub keepalives_out: u64,
    pub corrupt_messages: u64,
    pub truncated_messages: u64,
    pub dropped_packets: u64,
}

#[derive(Debug, Clone)]
pub struct TunBridgeConfig {
    pub utun_unit: u32,
    pub local_ip: IpAddr,
    pub peer_ip: IpAddr,
    pub prefix_len: u8,
    pub tun_mtu: usize,
    pub radio_mtu: usize,
    pub tx_peer: SocketAddr,
    pub rx_bind: SocketAddr,
    pub agg_timeout: Duration,
    pub keepalive_interval: Duration,
    pub stats_interval: Option<Duration>,
    pub summary_file: Option<PathBuf>,
    pub configure: bool,
    pub stop_requested: Option<Arc<AtomicBool>>,
}

impl Default for TunBridgeConfig {
    fn default() -> Self {
        Self {
            utun_unit: 0,
            local_ip: IpAddr::V4(Ipv4Addr::new(10, 5, 0, 1)),
            peer_ip: IpAddr::V4(Ipv4Addr::new(10, 5, 0, 2)),
            prefix_len: 24,
            tun_mtu: DEFAULT_TUN_MTU,
            radio_mtu: DEFAULT_RADIO_MTU,
            tx_peer: SocketAddr::from(([127, 0, 0, 1], 56020)),
            rx_bind: SocketAddr::from(([127, 0, 0, 1], 56021)),
            agg_timeout: Duration::from_millis(5),
            keepalive_interval: Duration::from_millis(500),
            stats_interval: Some(Duration::from_secs(5)),
            summary_file: None,
            configure: true,
            stop_requested: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TunBridgeSummary {
    pub schema: &'static str,
    pub result: String,
    pub stop_reason: String,
    pub error: Option<String>,
    pub started_at_unix: f64,
    pub ended_at_unix: f64,
    pub duration_s: f64,
    pub ifname: Option<String>,
    pub local_ip: String,
    pub peer_ip: String,
    pub prefix_len: u8,
    pub tun_mtu: usize,
    pub radio_mtu: usize,
    pub agg_timeout_ms: f64,
    pub keepalive_interval_s: f64,
    pub stats_interval_s: f64,
    pub tx_peer: String,
    pub rx_bind: String,
    pub counters: Counters,
}

#[derive(Debug, Clone)]
struct Aggregator {
    max_size: usize,
    timeout: Duration,
    parts: Vec<u8>,
    deadline: Option<std::time::Instant>,
}

impl Aggregator {
    fn new(max_size: usize, timeout: Duration) -> Self {
        Self {
            max_size,
            timeout,
            parts: Vec::new(),
            deadline: None,
        }
    }

    fn add(&mut self, packet: &[u8], now: std::time::Instant) -> Result<Vec<Vec<u8>>> {
        let record_len = packet.len().saturating_add(2);
        if record_len > self.max_size {
            return Err(TunError::PacketRecordTooLarge {
                record_len,
                max_size: self.max_size,
            });
        }

        let mut out = Vec::new();
        if !self.parts.is_empty() && self.parts.len().saturating_add(record_len) > self.max_size {
            out.push(self.flush());
        }

        let packet_len =
            u16::try_from(packet.len()).map_err(|_| TunError::PacketRecordTooLarge {
                record_len,
                max_size: self.max_size,
            })?;
        self.parts.extend_from_slice(&packet_len.to_be_bytes());
        self.parts.extend_from_slice(packet);
        if self.timeout.is_zero() {
            out.push(self.flush());
        } else if self.deadline.is_none() {
            self.deadline = Some(now + self.timeout);
        }
        Ok(out
            .into_iter()
            .filter(|message| !message.is_empty())
            .collect())
    }

    fn flush_due(&mut self, now: std::time::Instant) -> Option<Vec<u8>> {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            let data = self.flush();
            if data.is_empty() {
                None
            } else {
                Some(data)
            }
        } else {
            None
        }
    }

    fn flush(&mut self) -> Vec<u8> {
        self.deadline = None;
        std::mem::take(&mut self.parts)
    }

    fn deadline(&self) -> Option<std::time::Instant> {
        self.deadline
    }
}

pub fn parse_tunnel_message(message: &[u8], counters: &mut Counters) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    let mut offset = 0usize;
    while offset < message.len() {
        if message.len().saturating_sub(offset) < 2 {
            counters.corrupt_messages = counters.corrupt_messages.saturating_add(1);
            return packets;
        }
        let packet_len = u16::from_be_bytes([message[offset], message[offset + 1]]) as usize;
        offset += 2;
        if message.len().saturating_sub(offset) < packet_len {
            counters.truncated_messages = counters.truncated_messages.saturating_add(1);
            return packets;
        }
        packets.push(message[offset..offset + packet_len].to_vec());
        offset += packet_len;
    }
    packets
}

pub fn utun_af_header(packet: &[u8]) -> Result<[u8; 4]> {
    let Some(first) = packet.first() else {
        return Err(TunError::EmptyIpPacket);
    };
    let family = match first >> 4 {
        4 => libc::AF_INET as u32,
        6 => libc::AF_INET6 as u32,
        version => return Err(TunError::UnsupportedIpVersion(version)),
    };
    Ok(family.to_be_bytes())
}

pub fn strip_utun_header(frame: &[u8]) -> Result<&[u8]> {
    if frame.len() < 5 {
        return Err(TunError::UtunFrameTooShort);
    }
    let family = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
    if family != libc::AF_INET as u32 && family != libc::AF_INET6 as u32 {
        return Err(TunError::UnsupportedAddressFamily(family));
    }
    Ok(&frame[4..])
}

pub fn install_signal_handlers() -> Result<()> {
    unsafe extern "C" fn handle_signal(signal: libc::c_int) {
        SIGNAL_NUMBER.store(signal, Ordering::SeqCst);
        SIGNAL_STOP_REQUESTED.store(true, Ordering::SeqCst);
    }

    unsafe {
        if libc::signal(
            libc::SIGINT,
            handle_signal as *const () as libc::sighandler_t,
        ) == libc::SIG_ERR
        {
            return Err(TunError::io(
                "install SIGINT handler",
                io::Error::last_os_error(),
            ));
        }
        if libc::signal(
            libc::SIGTERM,
            handle_signal as *const () as libc::sighandler_t,
        ) == libc::SIG_ERR
        {
            return Err(TunError::io(
                "install SIGTERM handler",
                io::Error::last_os_error(),
            ));
        }
    }
    Ok(())
}

pub fn reset_signal_state() {
    SIGNAL_STOP_REQUESTED.store(false, Ordering::SeqCst);
    SIGNAL_NUMBER.store(0, Ordering::SeqCst);
}

pub fn self_test() -> Result<()> {
    let mut counters = Counters::default();
    let msg = b"\x00\x03abc\x00\x02de";
    assert_eq!(
        parse_tunnel_message(msg, &mut counters),
        vec![b"abc".to_vec(), b"de".to_vec()]
    );
    assert_eq!(counters.corrupt_messages, 0);
    assert_eq!(counters.truncated_messages, 0);

    assert!(parse_tunnel_message(b"\x00\x04abc", &mut counters).is_empty());
    assert_eq!(counters.truncated_messages, 1);

    let packet = [0x45, 0, 0, 20]
        .into_iter()
        .chain([0u8; 16])
        .collect::<Vec<_>>();
    let mut frame = Vec::from(utun_af_header(&packet)?);
    frame.extend_from_slice(&packet);
    assert_eq!(strip_utun_header(&frame)?, packet);

    let mut agg = Aggregator::new(12, Duration::ZERO);
    assert_eq!(
        agg.add(b"abc", std::time::Instant::now())?,
        vec![b"\x00\x03abc".to_vec()]
    );
    let mut agg = Aggregator::new(12, Duration::from_millis(5));
    let now = std::time::Instant::now();
    assert!(agg.add(b"abc", now)?.is_empty());
    assert!(agg.add(b"de", now)?.is_empty());
    assert_eq!(agg.flush(), b"\x00\x03abc\x00\x02de");
    Ok(())
}

pub fn run_tun_bridge(config: TunBridgeConfig) -> Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = config;
        Err(TunError::UnsupportedPlatform)
    }

    #[cfg(target_os = "macos")]
    {
        run_tun_bridge_macos(config)
    }
}

#[cfg(target_os = "macos")]
fn run_tun_bridge_macos(config: TunBridgeConfig) -> Result<()> {
    if config.prefix_len > 32 {
        return Err(TunError::InvalidPrefixLen(config.prefix_len));
    }

    let mut counters = Counters::default();
    let started_wall = unix_seconds();
    let started_mono = std::time::Instant::now();
    let mut ifname: Option<String> = None;
    let mut result = "error".to_string();
    let mut stop_reason = "startup".to_string();
    let mut error: Option<String> = None;
    let run_result = (|| -> Result<()> {
        let tun = open_utun(config.utun_unit)?;
        ifname = Some(tun.ifname.clone());
        if config.configure {
            configure_interface(
                &tun.ifname,
                config.local_ip,
                config.peer_ip,
                config.prefix_len,
                config.tun_mtu,
            )?;
        }

        let tx_sock = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
            .map_err(|source| TunError::io("bind UDP TX socket", source))?;
        let rx_sock = UdpSocket::bind(config.rx_bind)
            .map_err(|source| TunError::io("bind UDP RX socket", source))?;
        rx_sock
            .set_nonblocking(true)
            .map_err(|source| TunError::io("set UDP RX nonblocking", source))?;

        let mut aggregator = Aggregator::new(config.radio_mtu, config.agg_timeout);
        let mut next_keepalive = std::time::Instant::now() + config.keepalive_interval;
        let mut next_stats = config
            .stats_interval
            .map(|interval| std::time::Instant::now() + interval);
        let mut tun_buf = vec![0u8; config.tun_mtu.saturating_add(4)];
        let mut udp_buf = vec![0u8; config.radio_mtu.saturating_add(256)];
        let mut signal_logged = false;

        log_event(
            "started",
            json!({
                "ifname": tun.ifname,
                "local_ip": config.local_ip.to_string(),
                "peer_ip": config.peer_ip.to_string(),
                "tun_mtu": config.tun_mtu,
                "radio_mtu": config.radio_mtu,
                "tx_peer": config.tx_peer.to_string(),
                "rx_bind": config.rx_bind.to_string(),
            }),
        );

        while !stop_requested(&config) {
            let now = std::time::Instant::now();
            let mut timeout_at = next_keepalive;
            if let Some(deadline) = aggregator.deadline() {
                timeout_at = timeout_at.min(deadline);
            }
            if let Some(deadline) = next_stats {
                timeout_at = timeout_at.min(deadline);
            }
            let timeout = timeout_at.saturating_duration_since(now);

            poll_two(
                tun.fd,
                rx_sock.as_raw_fd(),
                timeout,
                |ready| -> Result<()> {
                    if ready.tun {
                        match tun.read(&mut tun_buf) {
                            Ok(frame_len) => {
                                let packet = strip_utun_header(&tun_buf[..frame_len])?.to_vec();
                                counters.tun_packets_in = counters.tun_packets_in.saturating_add(1);
                                counters.tun_bytes_in = counters.tun_bytes_in.saturating_add(
                                    u64::try_from(packet.len()).unwrap_or(u64::MAX),
                                );
                                for message in aggregator.add(&packet, std::time::Instant::now())? {
                                    tx_sock.send_to(&message, config.tx_peer).map_err(
                                        |source| TunError::io("send tunnel UDP", source),
                                    )?;
                                    counters.tunnel_datagrams_out =
                                        counters.tunnel_datagrams_out.saturating_add(1);
                                    counters.tunnel_bytes_out =
                                        counters.tunnel_bytes_out.saturating_add(
                                            u64::try_from(message.len()).unwrap_or(u64::MAX),
                                        );
                                }
                            }
                            Err(source) if source.kind() == io::ErrorKind::WouldBlock => {}
                            Err(source) => return Err(TunError::io("read utun", source)),
                        }
                    }

                    if ready.udp {
                        match rx_sock.recv_from(&mut udp_buf) {
                            Ok((message_len, _addr)) => {
                                let message = &udp_buf[..message_len];
                                counters.tunnel_datagrams_in =
                                    counters.tunnel_datagrams_in.saturating_add(1);
                                counters.tunnel_bytes_in = counters.tunnel_bytes_in.saturating_add(
                                    u64::try_from(message.len()).unwrap_or(u64::MAX),
                                );
                                for packet in parse_tunnel_message(message, &mut counters) {
                                    let header = utun_af_header(&packet)?;
                                    tun.write_all(&header, &packet)?;
                                    counters.tun_packets_out =
                                        counters.tun_packets_out.saturating_add(1);
                                    counters.tun_bytes_out = counters.tun_bytes_out.saturating_add(
                                        u64::try_from(packet.len()).unwrap_or(u64::MAX),
                                    );
                                }
                            }
                            Err(source) if source.kind() == io::ErrorKind::WouldBlock => {}
                            Err(source) => return Err(TunError::io("receive tunnel UDP", source)),
                        }
                    }
                    Ok(())
                },
            )?;

            let now = std::time::Instant::now();
            if let Some(message) = aggregator.flush_due(now) {
                tx_sock
                    .send_to(&message, config.tx_peer)
                    .map_err(|source| TunError::io("send due aggregate", source))?;
                counters.tunnel_datagrams_out = counters.tunnel_datagrams_out.saturating_add(1);
                counters.tunnel_bytes_out = counters
                    .tunnel_bytes_out
                    .saturating_add(u64::try_from(message.len()).unwrap_or(u64::MAX));
            }

            if now >= next_keepalive {
                tx_sock
                    .send_to(&[], config.tx_peer)
                    .map_err(|source| TunError::io("send keepalive", source))?;
                counters.keepalives_out = counters.keepalives_out.saturating_add(1);
                next_keepalive = now + config.keepalive_interval;
            }

            if let (Some(interval), Some(deadline)) = (config.stats_interval, next_stats) {
                if now >= deadline {
                    log_counters("stats", &counters);
                    next_stats = Some(now + interval);
                }
            }

            if SIGNAL_STOP_REQUESTED.load(Ordering::SeqCst) && !signal_logged {
                let signal = SIGNAL_NUMBER.load(Ordering::SeqCst);
                let signal_name = signal_name(signal);
                log_event("signal_received", json!({ "signal": signal_name }));
                signal_logged = true;
            }
        }

        if let Some(message) = nonempty(aggregator.flush()) {
            tx_sock
                .send_to(&message, config.tx_peer)
                .map_err(|source| TunError::io("send final aggregate", source))?;
            counters.tunnel_datagrams_out = counters.tunnel_datagrams_out.saturating_add(1);
            counters.tunnel_bytes_out = counters
                .tunnel_bytes_out
                .saturating_add(u64::try_from(message.len()).unwrap_or(u64::MAX));
        }

        result = "pass".to_string();
        stop_reason = current_stop_reason().unwrap_or_else(|| "requested".to_string());
        log_counters_with(
            "stopped",
            &counters,
            json!({
                "result": result,
                "stop_reason": stop_reason,
            }),
        );
        Ok(())
    })();

    if let Err(run_error) = &run_result {
        error = Some(run_error.to_string());
        log_counters_with("fatal", &counters, json!({ "error": error }));
    }
    write_summary(
        config.summary_file.as_ref(),
        &config,
        ifname.as_deref(),
        counters,
        started_wall,
        started_mono,
        &result,
        &stop_reason,
        error.as_deref(),
    )?;
    run_result
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct Utun {
    fd: RawFd,
    ifname: String,
}

#[cfg(target_os = "macos")]
impl Utun {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let rc = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast::<c_void>(), buf.len()) };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }

    fn write_all(&self, header: &[u8; 4], packet: &[u8]) -> Result<()> {
        let mut frame = Vec::with_capacity(4 + packet.len());
        frame.extend_from_slice(header);
        frame.extend_from_slice(packet);
        let rc = unsafe { libc::write(self.fd, frame.as_ptr().cast::<c_void>(), frame.len()) };
        if rc < 0 {
            return Err(TunError::io("write utun", io::Error::last_os_error()));
        }
        let wrote = rc as usize;
        if wrote != frame.len() {
            return Err(TunError::PartialUtunWrite {
                wrote,
                expected: frame.len(),
            });
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
impl Drop for Utun {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(target_os = "macos")]
fn open_utun(unit: u32) -> Result<Utun> {
    let fd = unsafe { libc::socket(libc::PF_SYSTEM, libc::SOCK_DGRAM, libc::SYSPROTO_CONTROL) };
    if fd < 0 {
        return Err(TunError::io(
            "open PF_SYSTEM control socket",
            io::Error::last_os_error(),
        ));
    }

    let open_result = (|| -> Result<Utun> {
        let mut info: libc::ctl_info = unsafe { std::mem::zeroed() };
        for (dst, src) in info
            .ctl_name
            .iter_mut()
            .zip(UTUN_CONTROL_NAME.iter().copied())
        {
            *dst = src as libc::c_char;
        }
        let rc = unsafe { libc::ioctl(fd, CTLIOCGINFO, &mut info) };
        if rc < 0 {
            return Err(TunError::io(
                "lookup utun control",
                io::Error::last_os_error(),
            ));
        }

        let addr = libc::sockaddr_ctl {
            sc_len: std::mem::size_of::<libc::sockaddr_ctl>() as libc::c_uchar,
            sc_family: libc::AF_SYSTEM as libc::c_uchar,
            ss_sysaddr: libc::AF_SYS_CONTROL as u16,
            sc_id: info.ctl_id,
            sc_unit: unit,
            sc_reserved: [0; 5],
        };
        let rc = unsafe {
            libc::connect(
                fd,
                (&addr as *const libc::sockaddr_ctl).cast::<libc::sockaddr>(),
                std::mem::size_of::<libc::sockaddr_ctl>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(TunError::io(
                "connect utun control",
                io::Error::last_os_error(),
            ));
        }

        let mut ifname_buf = [0u8; 64];
        let mut len = ifname_buf.len() as libc::socklen_t;
        let rc = unsafe {
            libc::getsockopt(
                fd,
                libc::SYSPROTO_CONTROL,
                UTUN_OPT_IFNAME,
                ifname_buf.as_mut_ptr().cast::<c_void>(),
                &mut len,
            )
        };
        if rc < 0 {
            return Err(TunError::io("read utun ifname", io::Error::last_os_error()));
        }
        set_nonblocking(fd)?;
        let nul = ifname_buf
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(len as usize);
        let ifname = String::from_utf8_lossy(&ifname_buf[..nul]).to_string();
        Ok(Utun { fd, ifname })
    })();

    match open_result {
        Ok(utun) => Ok(utun),
        Err(error) => {
            unsafe {
                libc::close(fd);
            }
            Err(error)
        }
    }
}

#[cfg(target_os = "macos")]
fn set_nonblocking(fd: RawFd) -> Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
    if flags < 0 {
        return Err(TunError::io("read fd flags", io::Error::last_os_error()));
    }
    let rc = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if rc < 0 {
        return Err(TunError::io(
            "set fd nonblocking",
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
struct Ready {
    tun: bool,
    udp: bool,
}

#[cfg(target_os = "macos")]
fn poll_two<F>(tun_fd: RawFd, udp_fd: RawFd, timeout: Duration, mut handle: F) -> Result<()>
where
    F: FnMut(Ready) -> Result<()>,
{
    let timeout_ms = timeout
        .as_millis()
        .min(i32::MAX as u128)
        .try_into()
        .unwrap_or(i32::MAX);
    let mut fds = [
        libc::pollfd {
            fd: tun_fd,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: udp_fd,
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    let rc = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
    if rc < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            return Ok(());
        }
        return Err(TunError::io("poll tunnel sockets", error));
    }
    if rc == 0 {
        return Ok(());
    }
    handle(Ready {
        tun: fds[0].revents & libc::POLLIN != 0,
        udp: fds[1].revents & libc::POLLIN != 0,
    })
}

fn configure_interface(
    ifname: &str,
    local_ip: IpAddr,
    peer_ip: IpAddr,
    prefix_len: u8,
    mtu: usize,
) -> Result<()> {
    let (IpAddr::V4(local_ip), IpAddr::V4(peer_ip)) = (local_ip, peer_ip) else {
        return Err(TunError::ConfigureRequiresIpv4 { local_ip, peer_ip });
    };
    let mask = netmask(prefix_len)?;
    run_command(
        "configure ifconfig",
        Command::new("/sbin/ifconfig")
            .arg(ifname)
            .arg("inet")
            .arg(local_ip.to_string())
            .arg(peer_ip.to_string())
            .arg("netmask")
            .arg(mask.to_string())
            .arg("mtu")
            .arg(mtu.to_string())
            .arg("up"),
    )?;
    log_event(
        "ifconfig_configured",
        json!({
            "ifname": ifname,
            "local_ip": local_ip.to_string(),
            "peer_ip": peer_ip.to_string(),
            "prefix_len": prefix_len,
            "mtu": mtu,
        }),
    );

    let route = command_output(
        "add host route",
        Command::new("/sbin/route")
            .arg("-n")
            .arg("add")
            .arg("-host")
            .arg(peer_ip.to_string())
            .arg("-interface")
            .arg(ifname),
        false,
    )?;
    let stdout = String::from_utf8_lossy(&route.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&route.stderr).trim().to_string();
    let output_text = format!("{stdout}\n{stderr}");
    if output_text.contains("File exists") {
        let current_ifname = current_route_interface(peer_ip)?;
        if current_ifname.as_deref() == Some(ifname) {
            log_event(
                "route_exists",
                json!({ "ifname": ifname, "peer_ip": peer_ip.to_string(), "stdout": stdout, "stderr": stderr }),
            );
            return Ok(());
        }
        log_event(
            "route_exists_stale",
            json!({
                "ifname": ifname,
                "current_ifname": current_ifname,
                "peer_ip": peer_ip.to_string(),
                "stdout": stdout,
                "stderr": stderr,
            }),
        );
        let changed = command_output(
            "change host route",
            Command::new("/sbin/route")
                .arg("-n")
                .arg("change")
                .arg("-host")
                .arg(peer_ip.to_string())
                .arg("-interface")
                .arg(ifname),
            false,
        )?;
        let changed_stdout = String::from_utf8_lossy(&changed.stdout).trim().to_string();
        let changed_stderr = String::from_utf8_lossy(&changed.stderr).trim().to_string();
        if changed.status.success() {
            log_event(
                "route_changed",
                json!({
                    "ifname": ifname,
                    "peer_ip": peer_ip.to_string(),
                    "stdout": changed_stdout,
                    "stderr": changed_stderr,
                }),
            );
            return Ok(());
        }
        log_event(
            "route_change_failed",
            json!({
                "ifname": ifname,
                "peer_ip": peer_ip.to_string(),
                "returncode": changed.status.code(),
                "stdout": changed_stdout,
                "stderr": changed_stderr,
            }),
        );
        return Err(command_error("change host route", changed));
    }

    if route.status.success() {
        log_event(
            "route_added",
            json!({ "ifname": ifname, "peer_ip": peer_ip.to_string(), "stdout": stdout, "stderr": stderr }),
        );
        return Ok(());
    }

    log_event(
        "route_add_failed",
        json!({
            "ifname": ifname,
            "peer_ip": peer_ip.to_string(),
            "returncode": route.status.code(),
            "stdout": stdout,
            "stderr": stderr,
        }),
    );
    Err(command_error("add host route", route))
}

fn current_route_interface(peer_ip: Ipv4Addr) -> Result<Option<String>> {
    let output = command_output(
        "route get",
        Command::new("/sbin/route")
            .arg("-n")
            .arg("get")
            .arg(peer_ip.to_string()),
        false,
    )?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("interface:") {
            return Ok(Some(value.trim().to_string()));
        }
    }
    Ok(None)
}

fn netmask(prefix_len: u8) -> Result<Ipv4Addr> {
    if prefix_len > 32 {
        return Err(TunError::InvalidPrefixLen(prefix_len));
    }
    let mask = if prefix_len == 0 {
        0
    } else {
        u32::MAX << (32 - prefix_len)
    };
    Ok(Ipv4Addr::from(mask))
}

fn run_command(context: &'static str, command: &mut Command) -> Result<()> {
    let output = command_output(context, command, true)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(context, output))
    }
}

fn command_output(
    context: &'static str,
    command: &mut Command,
    fail_on_spawn: bool,
) -> Result<std::process::Output> {
    command.output().map_err(|source| {
        if fail_on_spawn {
            TunError::io(context, source)
        } else {
            TunError::io(context, source)
        }
    })
}

fn command_error(context: &'static str, output: std::process::Output) -> TunError {
    TunError::Command {
        context,
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    }
}

fn stop_requested(config: &TunBridgeConfig) -> bool {
    SIGNAL_STOP_REQUESTED.load(Ordering::SeqCst)
        || config
            .stop_requested
            .as_ref()
            .is_some_and(|stop| stop.load(Ordering::SeqCst))
}

fn current_stop_reason() -> Option<String> {
    if SIGNAL_STOP_REQUESTED.load(Ordering::SeqCst) {
        Some(format!(
            "signal:{}",
            signal_name(SIGNAL_NUMBER.load(Ordering::SeqCst))
        ))
    } else {
        None
    }
}

fn signal_name(signal: i32) -> String {
    match signal {
        libc::SIGINT => "SIGINT".to_string(),
        libc::SIGTERM => "SIGTERM".to_string(),
        0 => "requested".to_string(),
        other => other.to_string(),
    }
}

fn write_summary(
    path: Option<&PathBuf>,
    config: &TunBridgeConfig,
    ifname: Option<&str>,
    counters: Counters,
    started_wall: f64,
    started_mono: std::time::Instant,
    result: &str,
    stop_reason: &str,
    error: Option<&str>,
) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    let summary = TunBridgeSummary {
        schema: SUMMARY_SCHEMA,
        result: result.to_string(),
        stop_reason: stop_reason.to_string(),
        error: error.map(ToString::to_string),
        started_at_unix: started_wall,
        ended_at_unix: unix_seconds(),
        duration_s: started_mono.elapsed().as_secs_f64().max(0.0),
        ifname: ifname.map(ToString::to_string),
        local_ip: config.local_ip.to_string(),
        peer_ip: config.peer_ip.to_string(),
        prefix_len: config.prefix_len,
        tun_mtu: config.tun_mtu,
        radio_mtu: config.radio_mtu,
        agg_timeout_ms: duration_ms(config.agg_timeout),
        keepalive_interval_s: config.keepalive_interval.as_secs_f64(),
        stats_interval_s: config
            .stats_interval
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0),
        tx_peer: config.tx_peer.to_string(),
        rx_bind: config.rx_bind.to_string(),
        counters,
    };
    let mut bytes = serde_json::to_vec_pretty(&summary)
        .map_err(|source| TunError::Other(format!("serialize summary: {source}")))?;
    bytes.push(b'\n');
    let tmp = path.with_extension(format!(
        "{}tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ));
    fs::write(&tmp, bytes).map_err(|source| TunError::io("write summary temp", source))?;
    fs::rename(&tmp, path).map_err(|source| TunError::io("replace summary", source))?;
    log_event(
        "summary_written",
        json!({ "path": path, "result": result, "stop_reason": stop_reason }),
    );
    Ok(())
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn unix_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

fn log_event(event: &str, fields: Value) {
    let mut object = match fields {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    object.insert("ts".to_string(), json!(unix_seconds()));
    object.insert("event".to_string(), json!(event));
    eprintln!("{}", Value::Object(object));
}

fn log_counters(event: &str, counters: &Counters) {
    log_counters_with(event, counters, Value::Object(Map::new()));
}

fn log_counters_with(event: &str, counters: &Counters, fields: Value) {
    let mut object = match serde_json::to_value(counters).unwrap_or(Value::Null) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    if let Value::Object(extra) = fields {
        object.extend(extra);
    }
    log_event(event, Value::Object(object));
}

fn nonempty(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tunnel_records_and_counts_corruption() {
        let mut counters = Counters::default();
        assert_eq!(
            parse_tunnel_message(b"\x00\x03abc\x00\x02de", &mut counters),
            vec![b"abc".to_vec(), b"de".to_vec()]
        );
        assert!(parse_tunnel_message(b"\x00", &mut counters).is_empty());
        assert_eq!(counters.corrupt_messages, 1);
        assert!(parse_tunnel_message(b"\x00\x04abc", &mut counters).is_empty());
        assert_eq!(counters.truncated_messages, 1);
    }

    #[test]
    fn utun_headers_round_trip_ipv4() {
        let packet = [0x45, 0, 0, 20]
            .into_iter()
            .chain([0u8; 16])
            .collect::<Vec<_>>();
        let mut frame = Vec::from(utun_af_header(&packet).expect("header"));
        frame.extend_from_slice(&packet);
        assert_eq!(strip_utun_header(&frame).expect("packet"), packet);
    }

    #[test]
    fn aggregator_flushes_on_timeout_zero_and_capacity() {
        let now = std::time::Instant::now();
        let mut agg = Aggregator::new(12, Duration::ZERO);
        assert_eq!(
            agg.add(b"abc", now).expect("add"),
            vec![b"\x00\x03abc".to_vec()]
        );

        let mut agg = Aggregator::new(8, Duration::from_millis(5));
        assert!(agg.add(b"abc", now).expect("add").is_empty());
        assert_eq!(
            agg.add(b"de", now).expect("add"),
            vec![b"\x00\x03abc".to_vec()]
        );
        assert_eq!(agg.flush(), b"\x00\x02de");
    }
}
