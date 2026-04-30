use std::net::SocketAddr;

use radio_core::RxFrame;
use thiserror::Error;
use tokio::net::UdpSocket;

use crate::{
    counters::RxCounters,
    forward::WfbForwardHeader,
    frame::{extract_wfb_payload, WfbChannelId, WfbFrameError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxForwardConfig {
    pub channel_id: WfbChannelId,
    pub wlan_idx: u8,
    pub mcs_index: u8,
    pub bandwidth_mhz: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RxForwardOutcome {
    Forwarded { bytes: usize },
    Filtered,
    Malformed,
}

#[derive(Debug, Error)]
pub enum RxBridgeError {
    #[error("failed to send RX payload to aggregator: {0}")]
    Send(#[from] std::io::Error),
}

pub fn build_rx_forward_datagram(
    frame: &RxFrame,
    config: RxForwardConfig,
    counters: &mut RxCounters,
) -> Option<Vec<u8>> {
    counters.received += 1;

    let payload = match extract_wfb_payload(&frame.data, config.channel_id) {
        Ok(payload) => payload,
        Err(WfbFrameError::PrefixMismatch | WfbFrameError::ChannelIdMismatch) => {
            counters.filtered += 1;
            return None;
        }
        Err(WfbFrameError::TooShort { .. } | WfbFrameError::InvalidLinkId { .. }) => {
            counters.malformed += 1;
            return None;
        }
    };

    counters.matched += 1;
    let header = WfbForwardHeader::single_antenna(
        config.wlan_idx,
        frame.rssi_dbm,
        frame.channel.frequency_mhz,
        config.mcs_index,
        config.bandwidth_mhz,
    );
    Some(header.prepend_to_payload(payload))
}

pub async fn forward_rx_frame_udp(
    socket: &UdpSocket,
    aggregator: SocketAddr,
    frame: &RxFrame,
    config: RxForwardConfig,
    counters: &mut RxCounters,
) -> Result<RxForwardOutcome, RxBridgeError> {
    let malformed_before = counters.malformed;
    let Some(packet) = build_rx_forward_datagram(frame, config, counters) else {
        return Ok(if counters.malformed > malformed_before {
            RxForwardOutcome::Malformed
        } else {
            RxForwardOutcome::Filtered
        });
    };

    match socket.send_to(&packet, aggregator).await {
        Ok(bytes) => {
            counters.forwarded += 1;
            Ok(RxForwardOutcome::Forwarded { bytes })
        }
        Err(error) => {
            counters.send_failed += 1;
            Err(RxBridgeError::Send(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use radio_core::Channel;

    use super::*;
    use crate::{build_wfb_data_header, forward::WFB_FORWARD_HEADER_LEN};

    fn rx_frame(data: Vec<u8>) -> RxFrame {
        RxFrame {
            data,
            rssi_dbm: -47,
            channel: Channel::from_number(149).expect("channel"),
            crc_error: false,
        }
    }

    #[test]
    fn builds_forward_datagram_and_counts_match() {
        let channel_id = WfbChannelId::new(0x000102, 3).expect("channel id");
        let mut frame = build_wfb_data_header(channel_id, 0).to_vec();
        frame.extend_from_slice(b"payload");
        let mut counters = RxCounters::default();

        let packet = build_rx_forward_datagram(
            &rx_frame(frame),
            RxForwardConfig {
                channel_id,
                wlan_idx: 1,
                mcs_index: 0,
                bandwidth_mhz: 20,
            },
            &mut counters,
        )
        .expect("forward packet");

        assert_eq!(&packet[WFB_FORWARD_HEADER_LEN..], b"payload");
        assert_eq!(counters.received, 1);
        assert_eq!(counters.matched, 1);
        assert_eq!(counters.filtered, 0);
        assert_eq!(counters.malformed, 0);
    }

    #[test]
    fn filters_non_matching_frames() {
        let expected = WfbChannelId::new(0x000102, 3).expect("expected");
        let other = WfbChannelId::new(0x000102, 4).expect("other");
        let mut counters = RxCounters::default();

        let packet = build_rx_forward_datagram(
            &rx_frame(build_wfb_data_header(other, 0).to_vec()),
            RxForwardConfig {
                channel_id: expected,
                wlan_idx: 0,
                mcs_index: 0,
                bandwidth_mhz: 20,
            },
            &mut counters,
        );

        assert!(packet.is_none());
        assert_eq!(counters.received, 1);
        assert_eq!(counters.filtered, 1);
    }

    #[tokio::test]
    async fn forwards_packet_to_udp_aggregator() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.expect("receiver");
        let sender = UdpSocket::bind("127.0.0.1:0").await.expect("sender");
        let aggregator = receiver.local_addr().expect("addr");
        let channel_id = WfbChannelId::new(0x000102, 3).expect("channel id");
        let mut frame = build_wfb_data_header(channel_id, 0).to_vec();
        frame.extend_from_slice(b"payload");
        let mut counters = RxCounters::default();

        let outcome = forward_rx_frame_udp(
            &sender,
            aggregator,
            &rx_frame(frame),
            RxForwardConfig {
                channel_id,
                wlan_idx: 0,
                mcs_index: 0,
                bandwidth_mhz: 20,
            },
            &mut counters,
        )
        .await
        .expect("forward");

        assert!(matches!(outcome, RxForwardOutcome::Forwarded { .. }));
        assert_eq!(counters.forwarded, 1);

        let mut buf = [0u8; 128];
        let (len, _) = receiver.recv_from(&mut buf).await.expect("datagram");
        assert_eq!(&buf[WFB_FORWARD_HEADER_LEN..len], b"payload");
    }
}
