use std::time::Duration;

use serde::Serialize;
use thiserror::Error;

use crate::channel::{Band, Bandwidth, Channel};
use crate::frame::{frame_type, validate_ieee80211_frame, FrameType, Ieee80211FrameError};
use crate::usb::{ClaimedUsbDevice, UsbBulkTransfer, UsbError};

const RTL_USB_REQ: u8 = 0x05;
const RTL_READ_REQUEST_TYPE: u8 = 0xc0;
const RTL_WRITE_REQUEST_TYPE: u8 = 0x40;
const RTL_USB_INDEX: u16 = 0;
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);
pub const TX_DESC_SIZE: usize = 40;
pub const RX_DESC_SIZE: usize = 24;
const USB2_BULK_PACKET_SIZE: usize = 512;
const RX_AGGREGATION_ALIGNMENT: usize = 128;
const RTL8812_PHY_STATUS_1ST_TYPE_MIN_LEN: usize = 17;

const QSLT_BE: u8 = 0x00;
const QSLT_BK: u8 = 0x02;
const QSLT_VI: u8 = 0x05;
const QSLT_VO: u8 = 0x07;
const QSLT_HIGH: u8 = 0x11;
const QSLT_MGNT: u8 = 0x12;
const VHT_DATA_SC_DONOT_CARE: u8 = 0;
const VHT_DATA_SC_20_UPPER_OF_80MHZ: u8 = 1;
const VHT_DATA_SC_20_LOWER_OF_80MHZ: u8 = 2;
const VHT_DATA_SC_20_UPPERST_OF_80MHZ: u8 = 3;
const VHT_DATA_SC_20_LOWEST_OF_80MHZ: u8 = 4;
const VHT_DATA_SC_40_UPPER_OF_80MHZ: u8 = 9;
const VHT_DATA_SC_40_LOWER_OF_80MHZ: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterWidth {
    U8,
    U16,
    U32,
    Block(usize),
}

impl RegisterWidth {
    fn expected_len(self) -> usize {
        match self {
            RegisterWidth::U8 => 1,
            RegisterWidth::U16 => 2,
            RegisterWidth::U32 => 4,
            RegisterWidth::Block(len) => len,
        }
    }
}

#[derive(Debug, Error)]
pub enum Rtl8812auRegisterError {
    #[error(transparent)]
    Usb(#[from] UsbError),
    #[error(
        "short register read addr=0x{addr:04x} width={width:?} expected={expected} actual={actual}"
    )]
    ShortRead {
        addr: u16,
        width: RegisterWidth,
        expected: usize,
        actual: usize,
    },
    #[error(
        "short register write addr=0x{addr:04x} width={width:?} expected={expected} actual={actual}"
    )]
    ShortWrite {
        addr: u16,
        width: RegisterWidth,
        expected: usize,
        actual: usize,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TxRate {
    Cck1m,
    Cck2m,
    Cck5_5m,
    Cck11m,
    #[default]
    Ofdm6m,
    Ofdm9m,
    Ofdm12m,
    Ofdm18m,
    Ofdm24m,
    Ofdm36m,
    Ofdm48m,
    Ofdm54m,
    Mcs(u8),
    Vht {
        mcs: u8,
        nss: u8,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TxQueue {
    #[default]
    Auto,
    Be,
    Bk,
    Vi,
    Vo,
    High,
    Mgnt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TxOptions {
    pub rate: TxRate,
    pub bandwidth: Bandwidth,
    pub channel_bandwidth: Option<Bandwidth>,
    pub queue: TxQueue,
    pub mac_id: u8,
    pub rate_id: Option<u8>,
    pub retries: u8,
    pub hardware_sequence: bool,
    pub first_segment: bool,
    pub disable_rate_fallback: bool,
    pub rate_fallback_limit: u8,
    pub aggregate_break: bool,
    pub short_gi: bool,
    pub ldpc: bool,
    pub stbc: bool,
    pub protect: bool,
    pub no_retry: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TxSubmitCounters {
    pub attempted: u64,
    pub submitted: u64,
    pub rejected: u64,
    pub failed: u64,
    pub short_writes: u64,
    pub bytes_written: u64,
}

impl Default for TxOptions {
    fn default() -> Self {
        Self {
            rate: TxRate::Ofdm6m,
            bandwidth: Bandwidth::Mhz20,
            channel_bandwidth: None,
            queue: TxQueue::Auto,
            mac_id: 0,
            rate_id: None,
            retries: 12,
            hardware_sequence: true,
            first_segment: true,
            disable_rate_fallback: true,
            rate_fallback_limit: 0x1f,
            aggregate_break: true,
            short_gi: false,
            ldpc: false,
            stbc: false,
            protect: false,
            no_retry: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RxRssiSource {
    Rtl8812PhyStatusBestPath,
    PhyStatusFirstByte,
    FallbackNoPhyStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RxSnrSource {
    Rtl8812PhyStatusBestPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RxFrame {
    pub data: Vec<u8>,
    pub rssi_dbm: i8,
    pub rssi_dbm_valid: bool,
    pub rssi_dbm_source: RxRssiSource,
    pub noise_dbm: Option<i8>,
    pub snr_db: Option<i8>,
    pub snr_db_source: Option<RxSnrSource>,
    pub channel: Channel,
    pub phy_status: bool,
    pub driver_info_size: usize,
    pub rx_shift: usize,
    pub raw_phy_status: Vec<u8>,
    pub rx_rate_raw: u8,
    pub rx_rate: Option<TxRate>,
    pub rx_bandwidth_raw: u8,
    pub rx_bandwidth: Option<Bandwidth>,
    pub short_gi: bool,
    pub ldpc: bool,
    pub stbc: bool,
    pub crc_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxParseOutcome {
    Frame,
    Drop,
    NeedMoreData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRxPacket {
    pub consumed: usize,
    pub outcome: RxParseOutcome,
    pub frame: Option<RxFrame>,
}

#[derive(Debug, Error)]
pub enum Rtl8812auTxError {
    #[error(transparent)]
    Frame(#[from] Ieee80211FrameError),
    #[error("channel {channel} does not support {bandwidth_mhz} MHz TX bandwidth")]
    UnsupportedBandwidth { channel: u8, bandwidth_mhz: u16 },
    #[error("unsupported TX rate {rate}")]
    UnsupportedRate { rate: String },
}

#[derive(Debug, Error)]
pub enum Rtl8812auTxSubmitError {
    #[error(transparent)]
    Build(#[from] Rtl8812auTxError),
    #[error(transparent)]
    Usb(#[from] UsbError),
    #[error("short bulk OUT write to endpoint 0x{endpoint:02x}: expected {expected} bytes, wrote {actual}")]
    ShortWrite {
        endpoint: u8,
        expected: usize,
        actual: usize,
    },
}

pub fn build_tx_packet(
    frame: &[u8],
    channel: Channel,
    opts: TxOptions,
) -> Result<Vec<u8>, Rtl8812auTxError> {
    validate_ieee80211_frame(frame)?;
    if !channel.supports_bandwidth(opts.bandwidth) {
        return Err(Rtl8812auTxError::UnsupportedBandwidth {
            channel: channel.number,
            bandwidth_mhz: opts.bandwidth.mhz(),
        });
    }

    let rate = tx_rate_to_hw(opts.rate, channel)?;
    let retries = if opts.no_retry {
        0
    } else {
        opts.retries.min(63)
    };
    let bmc = frame[4] & 0x01 != 0;
    let frame_type = frame_type(frame)?;
    let auto_qsel = match frame_type {
        FrameType::Management => QSLT_MGNT,
        FrameType::Control => QSLT_VO,
        FrameType::Data => QSLT_BE,
        FrameType::Extension => QSLT_MGNT,
    };
    let qsel = match opts.queue {
        TxQueue::Auto => auto_qsel,
        TxQueue::Be => QSLT_BE,
        TxQueue::Bk => QSLT_BK,
        TxQueue::Vi => QSLT_VI,
        TxQueue::Vo => QSLT_VO,
        TxQueue::High => QSLT_HIGH,
        TxQueue::Mgnt => QSLT_MGNT,
    };

    let frame_len_for_usb = if (frame.len() + TX_DESC_SIZE) % USB2_BULK_PACKET_SIZE == 0 {
        frame.len() + 1
    } else {
        frame.len()
    };
    let mut packet = vec![0u8; TX_DESC_SIZE + frame_len_for_usb];

    packet[0x00] = (frame_len_for_usb & 0xff) as u8;
    packet[0x01] = ((frame_len_for_usb >> 8) & 0xff) as u8;
    packet[0x02] = TX_DESC_SIZE as u8;
    packet[0x03] = (1 << 2) | (1 << 7);
    if opts.first_segment {
        packet[0x03] |= 1 << 3;
    }
    if bmc {
        packet[0x03] |= 1;
    }

    packet[0x04] = 0x00;
    packet[0x04] = opts.mac_id & 0x7f;
    packet[0x05] = qsel & 0x1f;
    packet[0x06] = opts.rate_id.unwrap_or_else(|| default_rate_id(opts.rate)) & 0x1f;
    if matches!(frame_type, FrameType::Data) && opts.aggregate_break {
        packet[0x0a] |= 1;
    }

    packet[0x0d] = 1 << 0;
    if opts.disable_rate_fallback {
        packet[0x0d] |= 1 << 2;
    }
    if opts.protect {
        packet[0x0d] |= (1 << 4) | (1 << 5);
        packet[0x13] = 0x04;
    }

    packet[0x10] = rate & 0x7f;
    packet[0x11] = opts.rate_fallback_limit & 0x1f;
    packet[0x12] = (1 << 1) | ((retries & 0x3f) << 2);

    let channel_bandwidth = opts.channel_bandwidth.unwrap_or(opts.bandwidth);
    packet[0x14] |= tx_data_secondary_channel(channel, channel_bandwidth, opts.bandwidth)?;
    match opts.bandwidth {
        Bandwidth::Mhz20 => {}
        Bandwidth::Mhz40 => packet[0x14] |= 1 << 5,
        Bandwidth::Mhz80 => packet[0x14] |= 2 << 5,
    }
    if opts.short_gi {
        packet[0x14] |= 1 << 4;
    }
    if opts.ldpc {
        packet[0x14] |= 1 << 7;
    }
    if opts.stbc {
        packet[0x15] |= 1;
    }

    if opts.hardware_sequence {
        packet[0x21] = 1 << 7;
    } else if frame.len() >= 24 {
        let sequence = (u16::from_le_bytes([frame[22], frame[23]]) >> 4) & 0x0fff;
        packet[0x25] |= ((sequence & 0x000f) << 4) as u8;
        packet[0x26] = (sequence >> 4) as u8;
    }
    let checksum = tx_descriptor_checksum(&packet[..32]);
    packet[0x1c..0x1e].copy_from_slice(&checksum.to_le_bytes());
    packet[TX_DESC_SIZE..TX_DESC_SIZE + frame.len()].copy_from_slice(frame);

    Ok(packet)
}

fn tx_data_secondary_channel(
    channel: Channel,
    channel_bandwidth: Bandwidth,
    frame_bandwidth: Bandwidth,
) -> Result<u8, Rtl8812auTxError> {
    match (channel_bandwidth, frame_bandwidth) {
        (Bandwidth::Mhz20, Bandwidth::Mhz20)
        | (Bandwidth::Mhz40, Bandwidth::Mhz40)
        | (Bandwidth::Mhz80, Bandwidth::Mhz80) => Ok(VHT_DATA_SC_DONOT_CARE),
        (Bandwidth::Mhz40, Bandwidth::Mhz20) => tx_primary_20_of_40(channel),
        (Bandwidth::Mhz80, Bandwidth::Mhz40) => match channel.number {
            36 | 40 | 52 | 56 | 100 | 104 | 116 | 120 | 132 | 136 | 149 | 153 => {
                Ok(VHT_DATA_SC_40_LOWER_OF_80MHZ)
            }
            44 | 48 | 60 | 64 | 108 | 112 | 124 | 128 | 140 | 144 | 157 | 161 => {
                Ok(VHT_DATA_SC_40_UPPER_OF_80MHZ)
            }
            _ => Err(Rtl8812auTxError::UnsupportedBandwidth {
                channel: channel.number,
                bandwidth_mhz: channel_bandwidth.mhz(),
            }),
        },
        (Bandwidth::Mhz80, Bandwidth::Mhz20) => match channel.number {
            36 | 52 | 100 | 116 | 132 | 149 => Ok(VHT_DATA_SC_20_LOWEST_OF_80MHZ),
            40 | 56 | 104 | 120 | 136 | 153 => Ok(VHT_DATA_SC_20_LOWER_OF_80MHZ),
            44 | 60 | 108 | 124 | 140 | 157 => Ok(VHT_DATA_SC_20_UPPER_OF_80MHZ),
            48 | 64 | 112 | 128 | 144 | 161 => Ok(VHT_DATA_SC_20_UPPERST_OF_80MHZ),
            _ => Err(Rtl8812auTxError::UnsupportedBandwidth {
                channel: channel.number,
                bandwidth_mhz: channel_bandwidth.mhz(),
            }),
        },
        _ => Err(Rtl8812auTxError::UnsupportedBandwidth {
            channel: channel.number,
            bandwidth_mhz: frame_bandwidth.mhz(),
        }),
    }
}

fn tx_primary_20_of_40(channel: Channel) -> Result<u8, Rtl8812auTxError> {
    match channel.band {
        Band::Ghz5 if channel.number % 8 == 4 => Ok(VHT_DATA_SC_20_LOWER_OF_80MHZ),
        Band::Ghz5 if channel.number % 8 == 0 => Ok(VHT_DATA_SC_20_UPPER_OF_80MHZ),
        Band::Ghz2 if (1..=7).contains(&channel.number) => Ok(VHT_DATA_SC_20_LOWER_OF_80MHZ),
        Band::Ghz2 if (8..=13).contains(&channel.number) => Ok(VHT_DATA_SC_20_UPPER_OF_80MHZ),
        _ => Err(Rtl8812auTxError::UnsupportedBandwidth {
            channel: channel.number,
            bandwidth_mhz: Bandwidth::Mhz40.mhz(),
        }),
    }
}

pub fn submit_tx_frame<T: UsbBulkTransfer>(
    transport: &mut T,
    bulk_out_endpoint: u8,
    frame: &[u8],
    channel: Channel,
    opts: TxOptions,
    counters: &mut TxSubmitCounters,
) -> Result<usize, Rtl8812auTxSubmitError> {
    counters.attempted += 1;
    let packet = match build_tx_packet(frame, channel, opts) {
        Ok(packet) => packet,
        Err(error) => {
            counters.rejected += 1;
            return Err(Rtl8812auTxSubmitError::Build(error));
        }
    };

    match transport.write_bulk_transfer(bulk_out_endpoint, &packet, DEFAULT_TIMEOUT) {
        Ok(written) if written == packet.len() => {
            counters.submitted += 1;
            counters.bytes_written += written as u64;
            Ok(written)
        }
        Ok(written) => {
            counters.failed += 1;
            counters.short_writes += 1;
            counters.bytes_written += written as u64;
            Err(Rtl8812auTxSubmitError::ShortWrite {
                endpoint: bulk_out_endpoint,
                expected: packet.len(),
                actual: written,
            })
        }
        Err(error) => {
            counters.failed += 1;
            Err(Rtl8812auTxSubmitError::Usb(error))
        }
    }
}

pub fn parse_rx_packet(buf: &[u8], channel: Channel) -> ParsedRxPacket {
    if buf.len() < RX_DESC_SIZE {
        return ParsedRxPacket {
            consumed: 0,
            outcome: RxParseOutcome::NeedMoreData,
            frame: None,
        };
    }

    let dw0 = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let dw3 = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let dw4 = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let packet_len = (dw0 & 0x3fff) as usize;
    let crc_error = ((dw0 >> 14) & 1) != 0;
    let drvinfo_size = ((dw0 >> 16) & 0x0f) as usize * 8;
    let shift = ((dw0 >> 24) & 0x03) as usize;
    let phy_status = ((dw0 >> 26) & 1) != 0;
    let rx_rate_raw = (dw3 & 0x7f) as u8;
    let rx_bandwidth_raw = ((dw4 >> 4) & 0x03) as u8;

    if packet_len == 0 || packet_len > 4096 {
        return ParsedRxPacket {
            consumed: RX_DESC_SIZE.min(buf.len()),
            outcome: RxParseOutcome::Drop,
            frame: None,
        };
    }

    let data_start = RX_DESC_SIZE + drvinfo_size + shift;
    let data_end = data_start + packet_len;
    let consumed = align_up(data_end, RX_AGGREGATION_ALIGNMENT);
    if data_end > buf.len() {
        return ParsedRxPacket {
            consumed: 0,
            outcome: RxParseOutcome::NeedMoreData,
            frame: None,
        };
    }

    let frame_len = packet_len.saturating_sub(4);
    if crc_error || frame_len < crate::frame::IEEE80211_MIN_HEADER_LEN {
        return ParsedRxPacket {
            consumed,
            outcome: RxParseOutcome::Drop,
            frame: None,
        };
    }

    let raw_phy_status = if phy_status && drvinfo_size > 0 {
        buf[RX_DESC_SIZE..RX_DESC_SIZE + drvinfo_size.min(32)].to_vec()
    } else {
        Vec::new()
    };
    let phy_metrics = rtl8812_phy_status_metrics(&raw_phy_status, rx_rate_raw);
    let (rssi_dbm, rssi_dbm_valid, rssi_dbm_source) = if let Some(metrics) = phy_metrics {
        (
            metrics.rssi_dbm,
            true,
            RxRssiSource::Rtl8812PhyStatusBestPath,
        )
    } else if let Some(raw) = raw_phy_status.first() {
        (
            ((*raw as i16) - 110).clamp(-127, 0) as i8,
            true,
            RxRssiSource::PhyStatusFirstByte,
        )
    } else {
        (-80, false, RxRssiSource::FallbackNoPhyStatus)
    };
    let (noise_dbm, snr_db, snr_db_source) = if let Some(metrics) = phy_metrics {
        (
            Some(metrics.noise_dbm),
            Some(metrics.snr_db),
            Some(RxSnrSource::Rtl8812PhyStatusBestPath),
        )
    } else {
        (None, None, None)
    };

    ParsedRxPacket {
        consumed,
        outcome: RxParseOutcome::Frame,
        frame: Some(RxFrame {
            data: buf[data_start..data_start + frame_len].to_vec(),
            rssi_dbm,
            rssi_dbm_valid,
            rssi_dbm_source,
            noise_dbm,
            snr_db,
            snr_db_source,
            channel,
            phy_status,
            driver_info_size: drvinfo_size,
            rx_shift: shift,
            raw_phy_status,
            rx_rate_raw,
            rx_rate: rx_rate_from_hw(rx_rate_raw),
            rx_bandwidth_raw,
            rx_bandwidth: rx_bandwidth_from_raw(rx_bandwidth_raw),
            short_gi: (dw4 & 1) != 0,
            ldpc: ((dw4 >> 1) & 1) != 0,
            stbc: ((dw4 >> 2) & 1) != 0,
            crc_error,
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rtl8812PhyStatusMetrics {
    rssi_dbm: i8,
    snr_db: i8,
    noise_dbm: i8,
}

fn rtl8812_phy_status_metrics(raw: &[u8], rx_rate_raw: u8) -> Option<Rtl8812PhyStatusMetrics> {
    if raw.len() < RTL8812_PHY_STATUS_1ST_TYPE_MIN_LEN || rx_rate_raw <= 0x03 {
        return None;
    }

    // RTL8812AU uses PHYDM's 1st-type 11ac PHY status layout. For OFDM/HT/VHT
    // packets, bytes 0/1 are {TRSW,gain[6:0]} for paths A/B and bytes 15/16
    // are signed S(8,1) RX SNR for paths A/B.
    let path_a = rtl8812_phy_status_path(raw[0], raw[15]);
    let path_b = rtl8812_phy_status_path(raw[1], raw[16]);
    Some(if path_b.rssi_dbm > path_a.rssi_dbm {
        path_b
    } else {
        path_a
    })
}

fn rtl8812_phy_status_path(gain_trsw: u8, raw_snr: u8) -> Rtl8812PhyStatusMetrics {
    let rssi_dbm = (((gain_trsw & 0x7f) as i16) - 110).clamp(-127, 0) as i8;
    let snr_db = (raw_snr as i8) >> 1;
    let noise_dbm = ((rssi_dbm as i16) - (snr_db as i16)).clamp(-127, 0) as i8;
    Rtl8812PhyStatusMetrics {
        rssi_dbm,
        snr_db,
        noise_dbm,
    }
}

fn rx_rate_from_hw(raw: u8) -> Option<TxRate> {
    match raw {
        0x00 => Some(TxRate::Cck1m),
        0x01 => Some(TxRate::Cck2m),
        0x02 => Some(TxRate::Cck5_5m),
        0x03 => Some(TxRate::Cck11m),
        0x04 => Some(TxRate::Ofdm6m),
        0x05 => Some(TxRate::Ofdm9m),
        0x06 => Some(TxRate::Ofdm12m),
        0x07 => Some(TxRate::Ofdm18m),
        0x08 => Some(TxRate::Ofdm24m),
        0x09 => Some(TxRate::Ofdm36m),
        0x0a => Some(TxRate::Ofdm48m),
        0x0b => Some(TxRate::Ofdm54m),
        0x0c..=0x2b => Some(TxRate::Mcs(raw - 0x0c)),
        0x2c..=0x53 => {
            let index = raw - 0x2c;
            Some(TxRate::Vht {
                mcs: index % 10,
                nss: (index / 10) + 1,
            })
        }
        _ => None,
    }
}

fn rx_bandwidth_from_raw(raw: u8) -> Option<Bandwidth> {
    match raw {
        0 => Some(Bandwidth::Mhz20),
        1 => Some(Bandwidth::Mhz40),
        2 => Some(Bandwidth::Mhz80),
        _ => None,
    }
}

fn tx_rate_to_hw(rate: TxRate, channel: Channel) -> Result<u8, Rtl8812auTxError> {
    let hw = match rate {
        TxRate::Cck1m => 0x00,
        TxRate::Cck2m => 0x01,
        TxRate::Cck5_5m => 0x02,
        TxRate::Cck11m => 0x03,
        TxRate::Ofdm6m => 0x04,
        TxRate::Ofdm9m => 0x05,
        TxRate::Ofdm12m => 0x06,
        TxRate::Ofdm18m => 0x07,
        TxRate::Ofdm24m => 0x08,
        TxRate::Ofdm36m => 0x09,
        TxRate::Ofdm48m => 0x0a,
        TxRate::Ofdm54m => 0x0b,
        TxRate::Mcs(mcs) if mcs <= 31 => 0x0c + mcs,
        TxRate::Mcs(mcs) => {
            return Err(Rtl8812auTxError::UnsupportedRate {
                rate: format!("mcs{mcs}"),
            });
        }
        TxRate::Vht { mcs, nss } if mcs <= 9 && (1..=4).contains(&nss) => {
            0x2c + ((nss - 1) * 10) + mcs
        }
        TxRate::Vht { mcs, nss } => {
            return Err(Rtl8812auTxError::UnsupportedRate {
                rate: format!("vht{nss}ss-mcs{mcs}"),
            });
        }
    };
    if matches!(channel.band, Band::Ghz5) && hw <= 0x03 {
        Ok(0x04)
    } else {
        Ok(hw)
    }
}

fn default_rate_id(rate: TxRate) -> u8 {
    match rate {
        TxRate::Cck1m | TxRate::Cck2m | TxRate::Cck5_5m | TxRate::Cck11m => 8,
        TxRate::Mcs(mcs) if mcs <= 7 => 3,
        TxRate::Mcs(mcs) if mcs <= 15 => 2,
        TxRate::Mcs(_) => 14,
        TxRate::Vht { nss: 1, .. } => 10,
        TxRate::Vht { nss: 2, .. } => 9,
        TxRate::Vht { nss: 3, .. } => 13,
        TxRate::Vht { .. } => 13,
        _ => 7,
    }
}

fn tx_descriptor_checksum(first_32_bytes: &[u8]) -> u16 {
    first_32_bytes
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .fold(0u16, |acc, word| acc ^ word)
}

fn align_up(value: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (value + alignment - 1) & !(alignment - 1)
}

pub trait Rtl8812auUsbTransport {
    fn read_vendor(
        &self,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, UsbError>;

    fn write_vendor(
        &self,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, UsbError>;
}

impl Rtl8812auUsbTransport for &ClaimedUsbDevice {
    fn read_vendor(
        &self,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        self.read_control(
            RTL_READ_REQUEST_TYPE,
            RTL_USB_REQ,
            value,
            index,
            data,
            timeout,
        )
    }

    fn write_vendor(
        &self,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, UsbError> {
        self.write_control(
            RTL_WRITE_REQUEST_TYPE,
            RTL_USB_REQ,
            value,
            index,
            data,
            timeout,
        )
    }
}

pub struct Rtl8812auRegisterAccess<T> {
    transport: T,
    timeout: Duration,
}

impl<T> Rtl8812auRegisterAccess<T>
where
    T: Rtl8812auUsbTransport,
{
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn read8(&self, addr: u16) -> Result<u8, Rtl8812auRegisterError> {
        let mut buf = [0u8; 1];
        self.read_exact(addr, RegisterWidth::U8, &mut buf)?;
        Ok(buf[0])
    }

    pub fn read16(&self, addr: u16) -> Result<u16, Rtl8812auRegisterError> {
        let mut buf = [0u8; 2];
        self.read_exact(addr, RegisterWidth::U16, &mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    pub fn read32(&self, addr: u16) -> Result<u32, Rtl8812auRegisterError> {
        let mut buf = [0u8; 4];
        self.read_exact(addr, RegisterWidth::U32, &mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn write8(&self, addr: u16, value: u8) -> Result<(), Rtl8812auRegisterError> {
        self.write_all(addr, RegisterWidth::U8, &[value])
    }

    pub fn write16(&self, addr: u16, value: u16) -> Result<(), Rtl8812auRegisterError> {
        self.write_all(addr, RegisterWidth::U16, &value.to_le_bytes())
    }

    pub fn write32(&self, addr: u16, value: u32) -> Result<(), Rtl8812auRegisterError> {
        self.write_all(addr, RegisterWidth::U32, &value.to_le_bytes())
    }

    pub fn write_block(&self, addr: u16, data: &[u8]) -> Result<(), Rtl8812auRegisterError> {
        self.write_all(addr, RegisterWidth::Block(data.len()), data)
    }

    fn read_exact(
        &self,
        addr: u16,
        width: RegisterWidth,
        data: &mut [u8],
    ) -> Result<(), Rtl8812auRegisterError> {
        let actual = self
            .transport
            .read_vendor(addr, RTL_USB_INDEX, data, self.timeout)?;
        let expected = width.expected_len();
        if actual != expected {
            return Err(Rtl8812auRegisterError::ShortRead {
                addr,
                width,
                expected,
                actual,
            });
        }
        Ok(())
    }

    fn write_all(
        &self,
        addr: u16,
        width: RegisterWidth,
        data: &[u8],
    ) -> Result<(), Rtl8812auRegisterError> {
        let actual = self
            .transport
            .write_vendor(addr, RTL_USB_INDEX, data, self.timeout)?;
        let expected = width.expected_len();
        if actual != expected {
            return Err(Rtl8812auRegisterError::ShortWrite {
                addr,
                width,
                expected,
                actual,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MockTransport {
        reads: RefCell<HashMap<u16, Vec<u8>>>,
        writes: RefCell<Vec<(u16, Vec<u8>)>>,
    }

    impl MockTransport {
        fn with_read(self, addr: u16, data: &[u8]) -> Self {
            self.reads.borrow_mut().insert(addr, data.to_vec());
            self
        }
    }

    impl Rtl8812auUsbTransport for &MockTransport {
        fn read_vendor(
            &self,
            value: u16,
            _index: u16,
            data: &mut [u8],
            _timeout: Duration,
        ) -> Result<usize, UsbError> {
            let Some(bytes) = self.reads.borrow().get(&value).cloned() else {
                return Ok(0);
            };
            let actual = bytes.len().min(data.len());
            data[..actual].copy_from_slice(&bytes[..actual]);
            Ok(actual)
        }

        fn write_vendor(
            &self,
            value: u16,
            _index: u16,
            data: &[u8],
            _timeout: Duration,
        ) -> Result<usize, UsbError> {
            self.writes.borrow_mut().push((value, data.to_vec()));
            Ok(data.len())
        }
    }

    #[derive(Default)]
    struct MockBulkTransport {
        writes: Vec<(u8, Vec<u8>)>,
        short_write: Option<usize>,
    }

    impl UsbBulkTransfer for MockBulkTransport {
        fn read_bulk_transfer(
            &mut self,
            _endpoint: u8,
            _data: &mut [u8],
            _timeout: Duration,
        ) -> Result<usize, UsbError> {
            Ok(0)
        }

        fn write_bulk_transfer(
            &mut self,
            endpoint: u8,
            data: &[u8],
            _timeout: Duration,
        ) -> Result<usize, UsbError> {
            let written = self.short_write.unwrap_or(data.len());
            self.writes.push((endpoint, data[..written].to_vec()));
            Ok(written)
        }
    }

    #[test]
    fn read16_uses_little_endian() {
        let transport = MockTransport::default().with_read(0x0100, &[0x34, 0x12]);
        let regs = Rtl8812auRegisterAccess::new(&transport);

        assert_eq!(regs.read16(0x0100).expect("read16"), 0x1234);
    }

    #[test]
    fn read32_uses_little_endian() {
        let transport = MockTransport::default().with_read(0x0100, &[0x78, 0x56, 0x34, 0x12]);
        let regs = Rtl8812auRegisterAccess::new(&transport);

        assert_eq!(regs.read32(0x0100).expect("read32"), 0x1234_5678);
    }

    #[test]
    fn write32_uses_little_endian() {
        let transport = MockTransport::default();
        let regs = Rtl8812auRegisterAccess::new(&transport);

        regs.write32(0x0100, 0x1234_5678).expect("write32");

        assert_eq!(
            transport.writes.borrow().as_slice(),
            &[(0x0100, vec![0x78, 0x56, 0x34, 0x12])]
        );
    }

    #[test]
    fn short_read_is_reported() {
        let transport = MockTransport::default().with_read(0x0100, &[0x12]);
        let regs = Rtl8812auRegisterAccess::new(&transport);

        let error = regs.read16(0x0100).expect_err("short read");
        assert!(matches!(
            error,
            Rtl8812auRegisterError::ShortRead {
                addr: 0x0100,
                expected: 2,
                actual: 1,
                ..
            }
        ));
    }

    #[test]
    fn tx_packet_contains_descriptor_and_frame() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(149).expect("channel 149");
        let packet = build_tx_packet(&frame, channel, TxOptions::default()).expect("tx packet");

        assert_eq!(packet.len(), TX_DESC_SIZE + frame.len());
        assert_eq!(&packet[TX_DESC_SIZE..], frame.as_slice());
        assert_eq!(packet[0x02], TX_DESC_SIZE as u8);
        assert_eq!(packet[0x03] & ((1 << 2) | (1 << 3) | (1 << 7)), 0x8c);
        assert_eq!(packet[0x05], QSLT_BE);
        assert_eq!(packet[0x0a] & 0x01, 0x01);
        assert_eq!(packet[0x10], 0x04);
        assert_eq!(packet[0x21], 0x80);

        let mut desc = packet[..TX_DESC_SIZE].to_vec();
        desc[0x1c] = 0;
        desc[0x1d] = 0;
        let expected = tx_descriptor_checksum(&desc[..32]);
        let actual = u16::from_le_bytes([packet[0x1c], packet[0x1d]]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn tx_packet_rejects_short_frame() {
        let channel = Channel::from_number(1).expect("channel 1");
        let error = build_tx_packet(&[0; 9], channel, TxOptions::default()).expect_err("short");
        assert!(matches!(
            error,
            Rtl8812auTxError::Frame(Ieee80211FrameError::TooShort { .. })
        ));
    }

    #[test]
    fn tx_packet_rejects_unsupported_bandwidth() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(6).expect("channel 6");
        let opts = TxOptions {
            bandwidth: Bandwidth::Mhz80,
            ..TxOptions::default()
        };

        let error = build_tx_packet(&frame, channel, opts).expect_err("unsupported bw");
        assert!(matches!(
            error,
            Rtl8812auTxError::UnsupportedBandwidth {
                channel: 6,
                bandwidth_mhz: 80
            }
        ));
    }

    #[test]
    fn tx_packet_encodes_ht_and_vht_rates() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(36).expect("channel 36");
        let ht = build_tx_packet(
            &frame,
            channel,
            TxOptions {
                rate: TxRate::Mcs(7),
                ..TxOptions::default()
            },
        )
        .expect("ht tx packet");
        let vht = build_tx_packet(
            &frame,
            channel,
            TxOptions {
                rate: TxRate::Vht { mcs: 9, nss: 2 },
                bandwidth: Bandwidth::Mhz80,
                ..TxOptions::default()
            },
        )
        .expect("vht tx packet");

        assert_eq!(ht[0x10], 0x13);
        assert_eq!(vht[0x10], 0x3f);
        assert_eq!(vht[0x14] & 0x60, 0x40);
    }

    #[test]
    fn tx_packet_encodes_sgi_ldpc_and_stbc_in_data_bw_word() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(36).expect("channel 36");
        let packet = build_tx_packet(
            &frame,
            channel,
            TxOptions {
                rate: TxRate::Mcs(0),
                bandwidth: Bandwidth::Mhz40,
                short_gi: true,
                ldpc: true,
                stbc: true,
                ..TxOptions::default()
            },
        )
        .expect("tx packet");

        assert_eq!(packet[0x14] & 0x10, 0x10);
        assert_eq!(packet[0x14] & 0x60, 0x20);
        assert_eq!(packet[0x14] & 0x80, 0x80);
        assert_eq!(packet[0x14] & 0x0f, VHT_DATA_SC_DONOT_CARE);
        assert_eq!(packet[0x15] & 0x03, 0x01);
    }

    #[test]
    fn tx_packet_uses_dont_care_data_secondary_channel_for_full_width_frames() {
        let frame = sample_data_frame();
        let packet_36 = build_tx_packet(
            &frame,
            Channel::from_number(36).expect("channel 36"),
            TxOptions {
                bandwidth: Bandwidth::Mhz40,
                ..TxOptions::default()
            },
        )
        .expect("36/40 tx packet");
        let packet_40 = build_tx_packet(
            &frame,
            Channel::from_number(40).expect("channel 40"),
            TxOptions {
                bandwidth: Bandwidth::Mhz40,
                ..TxOptions::default()
            },
        )
        .expect("40/40 tx packet");
        let packet_44_vht = build_tx_packet(
            &frame,
            Channel::from_number(44).expect("channel 44"),
            TxOptions {
                rate: TxRate::Vht { mcs: 1, nss: 1 },
                bandwidth: Bandwidth::Mhz80,
                ..TxOptions::default()
            },
        )
        .expect("44/80 tx packet");

        assert_eq!(packet_36[0x14] & 0x0f, VHT_DATA_SC_DONOT_CARE);
        assert_eq!(packet_40[0x14] & 0x0f, VHT_DATA_SC_DONOT_CARE);
        assert_eq!(packet_44_vht[0x14] & 0x0f, VHT_DATA_SC_DONOT_CARE);
    }

    #[test]
    fn tx_packet_encodes_data_secondary_channel_for_narrow_frames_on_wide_channels() {
        let frame = sample_data_frame();
        let packet_20_on_40_lower = build_tx_packet(
            &frame,
            Channel::from_number(36).expect("channel 36"),
            TxOptions {
                bandwidth: Bandwidth::Mhz20,
                channel_bandwidth: Some(Bandwidth::Mhz40),
                ..TxOptions::default()
            },
        )
        .expect("20 on 36/40 tx packet");
        let packet_20_on_40_upper = build_tx_packet(
            &frame,
            Channel::from_number(40).expect("channel 40"),
            TxOptions {
                bandwidth: Bandwidth::Mhz20,
                channel_bandwidth: Some(Bandwidth::Mhz40),
                ..TxOptions::default()
            },
        )
        .expect("20 on 40/40 tx packet");
        let packet_40_on_80_lower = build_tx_packet(
            &frame,
            Channel::from_number(36).expect("channel 36"),
            TxOptions {
                bandwidth: Bandwidth::Mhz40,
                channel_bandwidth: Some(Bandwidth::Mhz80),
                ..TxOptions::default()
            },
        )
        .expect("40 on 36/80 tx packet");
        let packet_20_on_80_upper = build_tx_packet(
            &frame,
            Channel::from_number(44).expect("channel 44"),
            TxOptions {
                bandwidth: Bandwidth::Mhz20,
                channel_bandwidth: Some(Bandwidth::Mhz80),
                ..TxOptions::default()
            },
        )
        .expect("20 on 44/80 tx packet");

        assert_eq!(
            packet_20_on_40_lower[0x14] & 0x0f,
            VHT_DATA_SC_20_LOWER_OF_80MHZ
        );
        assert_eq!(
            packet_20_on_40_upper[0x14] & 0x0f,
            VHT_DATA_SC_20_UPPER_OF_80MHZ
        );
        assert_eq!(
            packet_40_on_80_lower[0x14] & 0x0f,
            VHT_DATA_SC_40_LOWER_OF_80MHZ
        );
        assert_eq!(
            packet_20_on_80_upper[0x14] & 0x0f,
            VHT_DATA_SC_20_UPPER_OF_80MHZ
        );
    }

    #[test]
    fn tx_packet_allows_queue_override() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(36).expect("channel 36");
        let packet = build_tx_packet(
            &frame,
            channel,
            TxOptions {
                queue: TxQueue::Mgnt,
                ..TxOptions::default()
            },
        )
        .expect("tx packet");

        assert_eq!(packet[0x05], QSLT_MGNT);
    }

    #[test]
    fn tx_packet_allows_mac_id_override() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(36).expect("channel 36");
        let packet = build_tx_packet(
            &frame,
            channel,
            TxOptions {
                mac_id: 1,
                ..TxOptions::default()
            },
        )
        .expect("tx packet");

        assert_eq!(packet[0x04] & 0x7f, 1);
    }

    #[test]
    fn tx_packet_can_preserve_injected_sequence_and_rate_fallback() {
        let mut frame = sample_data_frame();
        frame[22..24].copy_from_slice(&0x1230u16.to_le_bytes());
        let channel = Channel::from_number(36).expect("channel 36");
        let packet = build_tx_packet(
            &frame,
            channel,
            TxOptions {
                retries: 0,
                rate_fallback_limit: 0,
                hardware_sequence: false,
                disable_rate_fallback: false,
                ..TxOptions::default()
            },
        )
        .expect("tx packet");

        assert_eq!(packet[0x0d] & (1 << 2), 0);
        assert_eq!(packet[0x11], 0);
        assert_eq!(packet[0x12] & 0xfe, 0x02);
        assert_eq!(packet[0x21] & 0x80, 0);
        assert_eq!(packet[0x25] >> 4, 0x03);
        assert_eq!(packet[0x26], 0x12);
    }

    #[test]
    fn tx_packet_rejects_unsupported_vht_rate() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(36).expect("channel 36");
        let error = build_tx_packet(
            &frame,
            channel,
            TxOptions {
                rate: TxRate::Vht { mcs: 10, nss: 2 },
                bandwidth: Bandwidth::Mhz80,
                ..TxOptions::default()
            },
        )
        .expect_err("unsupported rate");

        assert!(matches!(error, Rtl8812auTxError::UnsupportedRate { .. }));
    }

    #[test]
    fn submit_tx_frame_writes_descriptor_packet_to_bulk_out() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(149).expect("channel 149");
        let mut transport = MockBulkTransport::default();
        let mut counters = TxSubmitCounters::default();

        let written = submit_tx_frame(
            &mut transport,
            0x02,
            &frame,
            channel,
            TxOptions::default(),
            &mut counters,
        )
        .expect("submit");

        assert_eq!(transport.writes.len(), 1);
        assert_eq!(transport.writes[0].0, 0x02);
        assert_eq!(written, TX_DESC_SIZE + frame.len());
        assert_eq!(&transport.writes[0].1[TX_DESC_SIZE..], frame.as_slice());
        assert_eq!(counters.attempted, 1);
        assert_eq!(counters.submitted, 1);
        assert_eq!(counters.bytes_written, written as u64);
    }

    #[test]
    fn submit_tx_frame_counts_short_bulk_write() {
        let frame = sample_data_frame();
        let channel = Channel::from_number(149).expect("channel 149");
        let mut transport = MockBulkTransport {
            short_write: Some(12),
            ..MockBulkTransport::default()
        };
        let mut counters = TxSubmitCounters::default();

        let error = submit_tx_frame(
            &mut transport,
            0x02,
            &frame,
            channel,
            TxOptions::default(),
            &mut counters,
        )
        .expect_err("short write");

        assert!(matches!(
            error,
            Rtl8812auTxSubmitError::ShortWrite {
                endpoint: 0x02,
                expected: _,
                actual: 12
            }
        ));
        assert_eq!(counters.attempted, 1);
        assert_eq!(counters.submitted, 0);
        assert_eq!(counters.failed, 1);
        assert_eq!(counters.short_writes, 1);
        assert_eq!(counters.bytes_written, 12);
    }

    #[test]
    fn submit_tx_frame_counts_rejected_frame() {
        let channel = Channel::from_number(149).expect("channel 149");
        let mut transport = MockBulkTransport::default();
        let mut counters = TxSubmitCounters::default();

        let error = submit_tx_frame(
            &mut transport,
            0x02,
            &[0; 9],
            channel,
            TxOptions::default(),
            &mut counters,
        )
        .expect_err("rejected");

        assert!(matches!(error, Rtl8812auTxSubmitError::Build(_)));
        assert!(transport.writes.is_empty());
        assert_eq!(counters.attempted, 1);
        assert_eq!(counters.rejected, 1);
    }

    #[test]
    fn rx_parser_extracts_frame_and_strips_fcs() {
        let channel = Channel::from_number(36).expect("channel 36");
        let frame = sample_data_frame();
        let mut payload = frame.clone();
        payload.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let mut bulk = vec![0u8; RX_DESC_SIZE + payload.len()];
        let dw0 = payload.len() as u32;
        bulk[0..4].copy_from_slice(&dw0.to_le_bytes());
        let dw3 = 0x0d_u32;
        let dw4 = 0x17_u32;
        bulk[12..16].copy_from_slice(&dw3.to_le_bytes());
        bulk[16..20].copy_from_slice(&dw4.to_le_bytes());
        bulk[RX_DESC_SIZE..RX_DESC_SIZE + payload.len()].copy_from_slice(&payload);

        let parsed = parse_rx_packet(&bulk, channel);

        assert_eq!(parsed.outcome, RxParseOutcome::Frame);
        assert_eq!(parsed.consumed, 128);
        let rx = parsed.frame.expect("frame");
        assert_eq!(rx.data, frame);
        assert_eq!(rx.rssi_dbm, -80);
        assert!(!rx.rssi_dbm_valid);
        assert_eq!(rx.rssi_dbm_source, RxRssiSource::FallbackNoPhyStatus);
        assert_eq!(rx.noise_dbm, None);
        assert_eq!(rx.snr_db, None);
        assert_eq!(rx.snr_db_source, None);
        assert!(!rx.phy_status);
        assert_eq!(rx.driver_info_size, 0);
        assert_eq!(rx.rx_shift, 0);
        assert!(rx.raw_phy_status.is_empty());
        assert_eq!(rx.rx_rate_raw, 0x0d);
        assert_eq!(rx.rx_rate, Some(TxRate::Mcs(1)));
        assert_eq!(rx.rx_bandwidth_raw, 1);
        assert_eq!(rx.rx_bandwidth, Some(Bandwidth::Mhz40));
        assert!(rx.short_gi);
        assert!(rx.ldpc);
        assert!(rx.stbc);
        assert!(!rx.crc_error);
    }

    #[test]
    fn rx_parser_labels_phy_status_rssi_as_measured() {
        let channel = Channel::from_number(36).expect("channel 36");
        let frame = sample_data_frame();
        let mut payload = frame.clone();
        payload.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let drvinfo_size = 8usize;
        let mut bulk = vec![0u8; RX_DESC_SIZE + drvinfo_size + payload.len()];
        let dw0 = (payload.len() as u32) | (1 << 16) | (1 << 26);
        bulk[0..4].copy_from_slice(&dw0.to_le_bytes());
        bulk[RX_DESC_SIZE] = 72;
        bulk[RX_DESC_SIZE + drvinfo_size..RX_DESC_SIZE + drvinfo_size + payload.len()]
            .copy_from_slice(&payload);

        let parsed = parse_rx_packet(&bulk, channel);

        assert_eq!(parsed.outcome, RxParseOutcome::Frame);
        let rx = parsed.frame.expect("frame");
        assert_eq!(rx.rssi_dbm, -38);
        assert!(rx.rssi_dbm_valid);
        assert_eq!(rx.rssi_dbm_source, RxRssiSource::PhyStatusFirstByte);
        assert_eq!(rx.noise_dbm, None);
        assert_eq!(rx.snr_db, None);
        assert_eq!(rx.snr_db_source, None);
        assert!(rx.phy_status);
        assert_eq!(rx.driver_info_size, drvinfo_size);
        assert_eq!(rx.raw_phy_status, vec![72, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn rx_parser_extracts_rtl8812_ofdm_phy_status_snr() {
        let channel = Channel::from_number(36).expect("channel 36");
        let frame = sample_data_frame();
        let mut payload = frame.clone();
        payload.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let drvinfo_size = 24usize;
        let mut bulk = vec![0u8; RX_DESC_SIZE + drvinfo_size + payload.len()];
        let dw0 = (payload.len() as u32) | (3 << 16) | (1 << 26);
        let dw3 = 0x0d_u32;
        bulk[0..4].copy_from_slice(&dw0.to_le_bytes());
        bulk[12..16].copy_from_slice(&dw3.to_le_bytes());
        bulk[RX_DESC_SIZE] = 74;
        bulk[RX_DESC_SIZE + 1] = 70;
        bulk[RX_DESC_SIZE + 15] = 44;
        bulk[RX_DESC_SIZE + 16] = 36;
        bulk[RX_DESC_SIZE + drvinfo_size..RX_DESC_SIZE + drvinfo_size + payload.len()]
            .copy_from_slice(&payload);

        let parsed = parse_rx_packet(&bulk, channel);

        assert_eq!(parsed.outcome, RxParseOutcome::Frame);
        let rx = parsed.frame.expect("frame");
        assert_eq!(rx.rssi_dbm, -36);
        assert!(rx.rssi_dbm_valid);
        assert_eq!(rx.rssi_dbm_source, RxRssiSource::Rtl8812PhyStatusBestPath);
        assert_eq!(rx.snr_db, Some(22));
        assert_eq!(
            rx.snr_db_source,
            Some(RxSnrSource::Rtl8812PhyStatusBestPath)
        );
        assert_eq!(rx.noise_dbm, Some(-58));
        assert_eq!(rx.rx_rate, Some(TxRate::Mcs(1)));
        assert_eq!(rx.driver_info_size, drvinfo_size);
        assert_eq!(rx.raw_phy_status.len(), drvinfo_size);
    }

    #[test]
    fn rx_parser_drops_crc_error() {
        let channel = Channel::from_number(36).expect("channel 36");
        let payload = vec![0u8; 32];
        let mut bulk = vec![0u8; RX_DESC_SIZE + payload.len()];
        let dw0 = (payload.len() as u32) | (1 << 14);
        bulk[0..4].copy_from_slice(&dw0.to_le_bytes());
        bulk[RX_DESC_SIZE..RX_DESC_SIZE + payload.len()].copy_from_slice(&payload);

        let parsed = parse_rx_packet(&bulk, channel);

        assert_eq!(parsed.outcome, RxParseOutcome::Drop);
        assert!(parsed.frame.is_none());
    }

    fn sample_data_frame() -> Vec<u8> {
        vec![
            0x08, 0x01, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x57, 0x42, 0x00, 0x00,
            0x01, 0x23, 0x57, 0x42, 0x00, 0x00, 0x01, 0x23, 0x10, 0x00,
        ]
    }
}
