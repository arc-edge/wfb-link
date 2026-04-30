use serde::Serialize;
use thiserror::Error;

pub const WFB_IEEE80211_HEADER_LEN: usize = 24;
const WFB_MAC_PREFIX: [u8; 2] = [0x57, 0x42];
const SRC_MAC_PREFIX_OFFSET: usize = 10;
const SRC_CHANNEL_ID_OFFSET: usize = 12;
const DST_MAC_PREFIX_OFFSET: usize = 16;
const DST_CHANNEL_ID_OFFSET: usize = 18;
const FRAME_SEQ_OFFSET: usize = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WfbChannelId {
    pub link_id: u32,
    pub radio_port: u8,
}

impl WfbChannelId {
    pub fn new(link_id: u32, radio_port: u8) -> Result<Self, WfbFrameError> {
        if link_id > 0x00ff_ffff {
            return Err(WfbFrameError::InvalidLinkId { link_id });
        }
        Ok(Self {
            link_id,
            radio_port,
        })
    }

    pub fn raw(self) -> u32 {
        (self.link_id << 8) | u32::from(self.radio_port)
    }

    pub fn raw_be_bytes(self) -> [u8; 4] {
        self.raw().to_be_bytes()
    }
}

#[derive(Debug, Error)]
pub enum WfbFrameError {
    #[error("WFB link_id must fit in 24 bits, got 0x{link_id:08x}")]
    InvalidLinkId { link_id: u32 },
    #[error(
        "IEEE 802.11 frame too short for WFB header: expected at least 24 bytes, got {actual_len}"
    )]
    TooShort { actual_len: usize },
    #[error("frame does not use the WFB MAC prefix")]
    PrefixMismatch,
    #[error("frame channel id does not match configured link/radio port")]
    ChannelIdMismatch,
}

pub fn extract_wfb_payload(frame: &[u8], expected: WfbChannelId) -> Result<&[u8], WfbFrameError> {
    validate_wfb_header(frame, expected)?;
    Ok(&frame[WFB_IEEE80211_HEADER_LEN..])
}

pub fn build_wfb_data_header(
    channel_id: WfbChannelId,
    sequence: u16,
) -> [u8; WFB_IEEE80211_HEADER_LEN] {
    let mut header = [
        0x08, 0x01, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x57, 0x42, 0x00, 0x00, 0x00,
        0x00, 0x57, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let channel = channel_id.raw_be_bytes();
    header[SRC_CHANNEL_ID_OFFSET..SRC_CHANNEL_ID_OFFSET + 4].copy_from_slice(&channel);
    header[DST_CHANNEL_ID_OFFSET..DST_CHANNEL_ID_OFFSET + 4].copy_from_slice(&channel);
    header[FRAME_SEQ_OFFSET..FRAME_SEQ_OFFSET + 2].copy_from_slice(&sequence.to_le_bytes());
    header
}

fn validate_wfb_header(frame: &[u8], expected: WfbChannelId) -> Result<(), WfbFrameError> {
    if frame.len() < WFB_IEEE80211_HEADER_LEN {
        return Err(WfbFrameError::TooShort {
            actual_len: frame.len(),
        });
    }
    if frame[SRC_MAC_PREFIX_OFFSET..SRC_MAC_PREFIX_OFFSET + 2] != WFB_MAC_PREFIX
        || frame[DST_MAC_PREFIX_OFFSET..DST_MAC_PREFIX_OFFSET + 2] != WFB_MAC_PREFIX
    {
        return Err(WfbFrameError::PrefixMismatch);
    }

    let expected_bytes = expected.raw_be_bytes();
    if frame[SRC_CHANNEL_ID_OFFSET..SRC_CHANNEL_ID_OFFSET + 4] != expected_bytes
        || frame[DST_CHANNEL_ID_OFFSET..DST_CHANNEL_ID_OFFSET + 4] != expected_bytes
    {
        return Err(WfbFrameError::ChannelIdMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_id_encodes_link_and_radio_port() {
        let channel = WfbChannelId::new(0x00aa_bbcc, 0xdd).expect("channel id");
        assert_eq!(channel.raw(), 0xaabb_ccdd);
        assert_eq!(channel.raw_be_bytes(), [0xaa, 0xbb, 0xcc, 0xdd]);
    }

    #[test]
    fn channel_id_rejects_wide_link_id() {
        assert!(matches!(
            WfbChannelId::new(0x0100_0000, 0),
            Err(WfbFrameError::InvalidLinkId { .. })
        ));
    }

    #[test]
    fn extracts_payload_from_matching_frame() {
        let channel = WfbChannelId::new(0x000102, 0x03).expect("channel");
        let mut frame = build_wfb_data_header(channel, 0x10).to_vec();
        frame.extend_from_slice(b"payload");

        assert_eq!(
            extract_wfb_payload(&frame, channel).expect("payload"),
            b"payload"
        );
    }

    #[test]
    fn rejects_non_matching_channel_id() {
        let channel = WfbChannelId::new(0x000102, 0x03).expect("channel");
        let other = WfbChannelId::new(0x000102, 0x04).expect("other");
        let frame = build_wfb_data_header(channel, 0x10);

        assert!(matches!(
            extract_wfb_payload(&frame, other),
            Err(WfbFrameError::ChannelIdMismatch)
        ));
    }
}
