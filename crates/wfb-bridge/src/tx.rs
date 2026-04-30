use radio_core::{validate_ieee80211_frame, Ieee80211FrameError, TxOptions};
use thiserror::Error;

use crate::counters::TxCounters;
use crate::radiotap::{parse_wfb_radiotap_tx, RadiotapError};

const FWMARK_LEN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTxDatagram<'a> {
    pub fwmark: u32,
    pub radiotap_len: usize,
    pub tx_options: TxOptions,
    pub ieee80211_frame: &'a [u8],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TxDatagramError {
    #[error("TX datagram too short: expected at least {min_len} bytes, got {actual_len}")]
    TooShort { min_len: usize, actual_len: usize },
    #[error(transparent)]
    Radiotap(#[from] RadiotapError),
    #[error("TX datagram contains no IEEE 802.11 frame after radiotap header")]
    MissingFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxBridgeOutcome {
    Injected,
}

pub trait RadioTx {
    fn submit_80211(&mut self, frame: &[u8], options: TxOptions) -> Result<(), String>;
}

#[derive(Debug, Error)]
pub enum TxBridgeError {
    #[error(transparent)]
    Datagram(#[from] TxDatagramError),
    #[error(transparent)]
    Frame(#[from] Ieee80211FrameError),
    #[error("radio TX failed: {0}")]
    Radio(String),
}

pub fn parse_tx_datagram(packet: &[u8]) -> Result<ParsedTxDatagram<'_>, TxDatagramError> {
    if packet.len() < FWMARK_LEN + 8 {
        return Err(TxDatagramError::TooShort {
            min_len: FWMARK_LEN + 8,
            actual_len: packet.len(),
        });
    }
    let fwmark = u32::from_be_bytes([packet[0], packet[1], packet[2], packet[3]]);
    let radio_packet = &packet[FWMARK_LEN..];
    let radiotap = parse_wfb_radiotap_tx(radio_packet)?;
    if radio_packet.len() == radiotap.header_len {
        return Err(TxDatagramError::MissingFrame);
    }
    Ok(ParsedTxDatagram {
        fwmark,
        radiotap_len: radiotap.header_len,
        tx_options: radiotap.options,
        ieee80211_frame: &radio_packet[radiotap.header_len..],
    })
}

pub fn submit_tx_datagram<R: RadioTx>(
    packet: &[u8],
    radio: &mut R,
    counters: &mut TxCounters,
) -> Result<TxBridgeOutcome, TxBridgeError> {
    counters.incoming += 1;

    let parsed = match parse_tx_datagram(packet) {
        Ok(parsed) => parsed,
        Err(error) => {
            counters.dropped += 1;
            counters.malformed += 1;
            if is_unsupported_radiotap(&error) {
                counters.unsupported_radiotap += 1;
            }
            return Err(TxBridgeError::Datagram(error));
        }
    };

    if let Err(error) = validate_ieee80211_frame(parsed.ieee80211_frame) {
        counters.dropped += 1;
        counters.malformed += 1;
        return Err(TxBridgeError::Frame(error));
    }

    if let Err(error) = radio.submit_80211(parsed.ieee80211_frame, parsed.tx_options) {
        counters.dropped += 1;
        return Err(TxBridgeError::Radio(error));
    }

    counters.injected += 1;
    Ok(TxBridgeOutcome::Injected)
}

fn is_unsupported_radiotap(error: &TxDatagramError) -> bool {
    matches!(
        error,
        TxDatagramError::Radiotap(
            RadiotapError::UnsupportedPresentFlags { .. }
                | RadiotapError::UnsupportedHtBandwidth { .. }
                | RadiotapError::UnsupportedVhtBandwidth { .. }
        )
    )
}

#[cfg(test)]
mod tests {
    use radio_core::{Bandwidth, TxRate};

    use super::*;

    #[test]
    fn parses_distributor_datagram() {
        let mut packet = Vec::new();
        packet.extend_from_slice(&0x0102_0304u32.to_be_bytes());
        packet.extend_from_slice(&[
            0x00, 0x00, 0x0d, 0x00, 0x00, 0x80, 0x08, 0x00, 0x08, 0x00, 0x37, 0x01, 0x03,
        ]);
        packet.extend_from_slice(&[0x08; 24]);

        let parsed = parse_tx_datagram(&packet).expect("tx datagram");

        assert_eq!(parsed.fwmark, 0x0102_0304);
        assert_eq!(parsed.radiotap_len, 13);
        assert_eq!(parsed.tx_options.rate, TxRate::Mcs(3));
        assert_eq!(parsed.tx_options.bandwidth, Bandwidth::Mhz40);
        assert_eq!(parsed.ieee80211_frame, &[0x08; 24]);
    }

    #[test]
    fn rejects_missing_frame() {
        let mut packet = Vec::new();
        packet.extend_from_slice(&0u32.to_be_bytes());
        packet.extend_from_slice(&[
            0x00, 0x00, 0x0d, 0x00, 0x00, 0x80, 0x08, 0x00, 0x08, 0x00, 0x37, 0x00, 0x00,
        ]);

        assert!(matches!(
            parse_tx_datagram(&packet),
            Err(TxDatagramError::MissingFrame)
        ));
    }

    #[derive(Default)]
    struct FakeRadio {
        submitted: Vec<(Vec<u8>, TxOptions)>,
        fail: bool,
    }

    impl RadioTx for FakeRadio {
        fn submit_80211(&mut self, frame: &[u8], options: TxOptions) -> Result<(), String> {
            if self.fail {
                return Err("fake failure".to_string());
            }
            self.submitted.push((frame.to_vec(), options));
            Ok(())
        }
    }

    fn valid_datagram() -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&0u32.to_be_bytes());
        packet.extend_from_slice(&[
            0x00, 0x00, 0x0d, 0x00, 0x00, 0x80, 0x08, 0x00, 0x08, 0x00, 0x37, 0x00, 0x00,
        ]);
        packet.extend_from_slice(&[0x08, 0x01, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        packet
    }

    #[test]
    fn submits_parsed_frame_to_radio_sink() {
        let mut radio = FakeRadio::default();
        let mut counters = TxCounters::default();

        let outcome =
            submit_tx_datagram(&valid_datagram(), &mut radio, &mut counters).expect("submit");

        assert_eq!(outcome, TxBridgeOutcome::Injected);
        assert_eq!(radio.submitted.len(), 1);
        assert_eq!(radio.submitted[0].0.len(), 10);
        assert_eq!(counters.incoming, 1);
        assert_eq!(counters.injected, 1);
        assert_eq!(counters.dropped, 0);
    }

    #[test]
    fn counts_unsupported_radiotap() {
        let mut radio = FakeRadio::default();
        let mut counters = TxCounters::default();
        let mut packet = Vec::new();
        packet.extend_from_slice(&0u32.to_be_bytes());
        packet.extend_from_slice(&[0x00, 0x00, 0x08, 0x00, 0xef, 0xbe, 0xad, 0xde]);

        let err = submit_tx_datagram(&packet, &mut radio, &mut counters).expect_err("error");

        assert!(matches!(err, TxBridgeError::Datagram(_)));
        assert_eq!(counters.incoming, 1);
        assert_eq!(counters.dropped, 1);
        assert_eq!(counters.malformed, 1);
        assert_eq!(counters.unsupported_radiotap, 1);
        assert!(radio.submitted.is_empty());
    }

    #[test]
    fn counts_radio_submit_failure() {
        let mut radio = FakeRadio {
            fail: true,
            ..FakeRadio::default()
        };
        let mut counters = TxCounters::default();

        let err = submit_tx_datagram(&valid_datagram(), &mut radio, &mut counters)
            .expect_err("submit fails");

        assert!(matches!(err, TxBridgeError::Radio(_)));
        assert_eq!(counters.incoming, 1);
        assert_eq!(counters.injected, 0);
        assert_eq!(counters.dropped, 1);
    }
}
