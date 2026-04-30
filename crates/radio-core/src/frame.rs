use thiserror::Error;

pub const IEEE80211_MIN_HEADER_LEN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Management,
    Control,
    Data,
    Extension,
}

#[derive(Debug, Error)]
pub enum Ieee80211FrameError {
    #[error("IEEE 802.11 frame too short: expected at least {min_len} bytes, got {actual_len}")]
    TooShort { min_len: usize, actual_len: usize },
}

pub fn validate_ieee80211_frame(frame: &[u8]) -> Result<(), Ieee80211FrameError> {
    if frame.len() < IEEE80211_MIN_HEADER_LEN {
        return Err(Ieee80211FrameError::TooShort {
            min_len: IEEE80211_MIN_HEADER_LEN,
            actual_len: frame.len(),
        });
    }
    Ok(())
}

pub fn frame_type(frame: &[u8]) -> Result<FrameType, Ieee80211FrameError> {
    validate_ieee80211_frame(frame)?;
    Ok(match frame[0] & 0x0c {
        0x00 => FrameType::Management,
        0x04 => FrameType::Control,
        0x08 => FrameType::Data,
        _ => FrameType::Extension,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_frame() {
        assert!(matches!(
            validate_ieee80211_frame(&[0; 9]),
            Err(Ieee80211FrameError::TooShort { actual_len: 9, .. })
        ));
    }

    #[test]
    fn detects_frame_type() {
        assert_eq!(
            frame_type(&[0x80; 10]).expect("mgmt"),
            FrameType::Management
        );
        assert_eq!(
            frame_type(&[0xb4; 10]).expect("control"),
            FrameType::Control
        );
        assert_eq!(frame_type(&[0x08; 10]).expect("data"), FrameType::Data);
    }
}
