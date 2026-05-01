use radio_core::{Bandwidth, TxOptions, TxRate};
use thiserror::Error;

const RADIOTAP_MIN_LEN: usize = 8;
const WFB_HT_RADIOTAP_LEN: usize = 13;
const WFB_VHT_RADIOTAP_LEN: usize = 22;
const WFB_HT_PRESENT: u32 = 0x0008_8000;
const WFB_VHT_PRESENT: u32 = 0x0020_8000;
const TX_FLAGS_OFFSET: usize = 8;
const TX_FLAGS_NO_ACK: u16 = 0x0008;

const MCS_KNOWN_OFFSET: usize = 10;
const MCS_FLAGS_OFFSET: usize = 11;
const MCS_INDEX_OFFSET: usize = 12;
const MCS_HAVE_BW: u8 = 0x01;
const MCS_HAVE_MCS: u8 = 0x02;
const MCS_HAVE_GI: u8 = 0x04;
const MCS_HAVE_FEC: u8 = 0x10;
const MCS_HAVE_STBC: u8 = 0x20;
const MCS_BW_MASK: u8 = 0x03;
const MCS_BW_20: u8 = 0x00;
const MCS_BW_40: u8 = 0x01;
const MCS_SGI: u8 = 0x04;
const MCS_FEC_LDPC: u8 = 0x10;
const MCS_STBC_MASK: u8 = 0x60;

const VHT_KNOWN_OFFSET: usize = 10;
const VHT_FLAGS_OFFSET: usize = 12;
const VHT_BW_OFFSET: usize = 13;
const VHT_MCSNSS0_OFFSET: usize = 14;
const VHT_CODING_OFFSET: usize = 18;
const VHT_FLAG_STBC: u8 = 0x01;
const VHT_FLAG_SGI: u8 = 0x04;
const VHT_BW_20: u8 = 0x00;
const VHT_BW_40: u8 = 0x01;
const VHT_BW_80: u8 = 0x04;
const VHT_CODING_LDPC_USER0: u8 = 0x01;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRadiotapTx {
    pub header_len: usize,
    pub is_vht: bool,
    pub no_ack: bool,
    pub options: TxOptions,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RadiotapError {
    #[error("radiotap header too short: expected at least {min_len}, got {actual_len}")]
    TooShort { min_len: usize, actual_len: usize },
    #[error("unsupported radiotap version {version}")]
    UnsupportedVersion { version: u8 },
    #[error("radiotap length {header_len} exceeds packet length {packet_len}")]
    LengthExceedsPacket {
        header_len: usize,
        packet_len: usize,
    },
    #[error("unsupported WFB radiotap present flags 0x{present:08x}")]
    UnsupportedPresentFlags { present: u32 },
    #[error("unsupported HT bandwidth bits 0x{bits:02x}")]
    UnsupportedHtBandwidth { bits: u8 },
    #[error("unsupported VHT bandwidth value 0x{value:02x}")]
    UnsupportedVhtBandwidth { value: u8 },
}

pub fn parse_wfb_radiotap_tx(packet: &[u8]) -> Result<ParsedRadiotapTx, RadiotapError> {
    if packet.len() < RADIOTAP_MIN_LEN {
        return Err(RadiotapError::TooShort {
            min_len: RADIOTAP_MIN_LEN,
            actual_len: packet.len(),
        });
    }
    let version = packet[0];
    if version != 0 {
        return Err(RadiotapError::UnsupportedVersion { version });
    }

    let header_len = u16::from_le_bytes([packet[2], packet[3]]) as usize;
    if header_len > packet.len() {
        return Err(RadiotapError::LengthExceedsPacket {
            header_len,
            packet_len: packet.len(),
        });
    }
    let present = u32::from_le_bytes([packet[4], packet[5], packet[6], packet[7]]);
    match (header_len, present) {
        (WFB_HT_RADIOTAP_LEN, WFB_HT_PRESENT) => parse_ht(packet),
        (WFB_VHT_RADIOTAP_LEN, WFB_VHT_PRESENT) => parse_vht(packet),
        _ => Err(RadiotapError::UnsupportedPresentFlags { present }),
    }
}

fn parse_ht(packet: &[u8]) -> Result<ParsedRadiotapTx, RadiotapError> {
    let tx_flags = u16::from_le_bytes([packet[TX_FLAGS_OFFSET], packet[TX_FLAGS_OFFSET + 1]]);
    let known = packet[MCS_KNOWN_OFFSET];
    let flags = packet[MCS_FLAGS_OFFSET];
    let mcs = packet[MCS_INDEX_OFFSET];
    let bw = if known & MCS_HAVE_BW != 0 {
        match flags & MCS_BW_MASK {
            MCS_BW_20 => Bandwidth::Mhz20,
            MCS_BW_40 => Bandwidth::Mhz40,
            bits => return Err(RadiotapError::UnsupportedHtBandwidth { bits }),
        }
    } else {
        Bandwidth::Mhz20
    };

    Ok(ParsedRadiotapTx {
        header_len: WFB_HT_RADIOTAP_LEN,
        is_vht: false,
        no_ack: tx_flags & TX_FLAGS_NO_ACK != 0,
        options: TxOptions {
            rate: if known & MCS_HAVE_MCS != 0 {
                TxRate::Mcs(mcs)
            } else {
                TxRate::Ofdm6m
            },
            bandwidth: bw,
            channel_bandwidth: None,
            queue: Default::default(),
            mac_id: 0,
            rate_id: None,
            retries: 12,
            hardware_sequence: true,
            first_segment: true,
            disable_rate_fallback: true,
            rate_fallback_limit: 0x1f,
            aggregate_break: true,
            short_gi: known & MCS_HAVE_GI != 0 && flags & MCS_SGI != 0,
            ldpc: known & MCS_HAVE_FEC != 0 && flags & MCS_FEC_LDPC != 0,
            stbc: known & MCS_HAVE_STBC != 0 && flags & MCS_STBC_MASK != 0,
            protect: false,
            no_retry: tx_flags & TX_FLAGS_NO_ACK != 0,
        },
    })
}

fn parse_vht(packet: &[u8]) -> Result<ParsedRadiotapTx, RadiotapError> {
    let tx_flags = u16::from_le_bytes([packet[TX_FLAGS_OFFSET], packet[TX_FLAGS_OFFSET + 1]]);
    let _known = u16::from_le_bytes([packet[VHT_KNOWN_OFFSET], packet[VHT_KNOWN_OFFSET + 1]]);
    let flags = packet[VHT_FLAGS_OFFSET];
    let bw = match packet[VHT_BW_OFFSET] {
        VHT_BW_20 => Bandwidth::Mhz20,
        VHT_BW_40 => Bandwidth::Mhz40,
        VHT_BW_80 => Bandwidth::Mhz80,
        value => return Err(RadiotapError::UnsupportedVhtBandwidth { value }),
    };
    let mcs_nss = packet[VHT_MCSNSS0_OFFSET];
    let mcs = (mcs_nss >> 4) & 0x0f;
    let nss = mcs_nss & 0x0f;
    let coding = packet[VHT_CODING_OFFSET];

    Ok(ParsedRadiotapTx {
        header_len: WFB_VHT_RADIOTAP_LEN,
        is_vht: true,
        no_ack: tx_flags & TX_FLAGS_NO_ACK != 0,
        options: TxOptions {
            rate: TxRate::Vht { mcs, nss },
            bandwidth: bw,
            channel_bandwidth: None,
            queue: Default::default(),
            mac_id: 0,
            rate_id: None,
            retries: 12,
            hardware_sequence: true,
            first_segment: true,
            disable_rate_fallback: true,
            rate_fallback_limit: 0x1f,
            aggregate_break: true,
            short_gi: flags & VHT_FLAG_SGI != 0,
            ldpc: coding & VHT_CODING_LDPC_USER0 != 0,
            stbc: flags & VHT_FLAG_STBC != 0,
            protect: false,
            no_retry: tx_flags & TX_FLAGS_NO_ACK != 0,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wfb_ht_radiotap() {
        let mut packet = vec![
            0x00, 0x00, 0x0d, 0x00, 0x00, 0x80, 0x08, 0x00, 0x08, 0x00, 0x37, 0x15, 0x04,
        ];
        packet.extend_from_slice(&[0; 24]);

        let parsed = parse_wfb_radiotap_tx(&packet).expect("ht radiotap");

        assert_eq!(parsed.header_len, 13);
        assert!(!parsed.is_vht);
        assert!(parsed.no_ack);
        assert_eq!(parsed.options.rate, TxRate::Mcs(4));
        assert_eq!(parsed.options.bandwidth, Bandwidth::Mhz40);
        assert!(parsed.options.short_gi);
        assert!(parsed.options.ldpc);
    }

    #[test]
    fn parses_wfb_vht_radiotap() {
        let mut packet = vec![
            0x00, 0x00, 0x16, 0x00, 0x00, 0x80, 0x20, 0x00, 0x08, 0x00, 0x45, 0x00, 0x05, 0x04,
            0x92, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        ];
        packet.extend_from_slice(&[0; 24]);

        let parsed = parse_wfb_radiotap_tx(&packet).expect("vht radiotap");

        assert_eq!(parsed.header_len, 22);
        assert!(parsed.is_vht);
        assert_eq!(parsed.options.rate, TxRate::Vht { mcs: 9, nss: 2 });
        assert_eq!(parsed.options.bandwidth, Bandwidth::Mhz80);
        assert!(parsed.options.short_gi);
        assert!(parsed.options.stbc);
        assert!(parsed.options.ldpc);
    }

    #[test]
    fn rejects_unknown_present_flags() {
        let packet = [0x00, 0x00, 0x08, 0x00, 0xef, 0xbe, 0xad, 0xde];

        assert!(matches!(
            parse_wfb_radiotap_tx(&packet),
            Err(RadiotapError::UnsupportedPresentFlags {
                present: 0xdead_beef
            })
        ));
    }
}
