use serde::Serialize;

pub const RX_ANT_MAX: usize = 4;
pub const WFB_FORWARD_HEADER_LEN: usize = 17;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WfbForwardHeader {
    pub wlan_idx: u8,
    pub antenna: [u8; RX_ANT_MAX],
    pub rssi: [i8; RX_ANT_MAX],
    pub noise: [i8; RX_ANT_MAX],
    pub freq_mhz: u16,
    pub mcs_index: u8,
    pub bandwidth_mhz: u8,
}

impl WfbForwardHeader {
    pub fn single_antenna(
        wlan_idx: u8,
        rssi_dbm: i8,
        freq_mhz: u16,
        mcs_index: u8,
        bandwidth_mhz: u8,
    ) -> Self {
        let mut antenna = [0xff; RX_ANT_MAX];
        let mut rssi = [i8::MIN; RX_ANT_MAX];
        let noise = [i8::MAX; RX_ANT_MAX];
        antenna[0] = 0;
        rssi[0] = rssi_dbm;
        Self {
            wlan_idx,
            antenna,
            rssi,
            noise,
            freq_mhz,
            mcs_index,
            bandwidth_mhz,
        }
    }

    pub fn to_bytes(&self) -> [u8; WFB_FORWARD_HEADER_LEN] {
        let mut out = [0u8; WFB_FORWARD_HEADER_LEN];
        out[0] = self.wlan_idx;
        out[1..5].copy_from_slice(&self.antenna);
        for (idx, value) in self.rssi.iter().enumerate() {
            out[5 + idx] = *value as u8;
        }
        for (idx, value) in self.noise.iter().enumerate() {
            out[9 + idx] = *value as u8;
        }
        out[13..15].copy_from_slice(&self.freq_mhz.to_be_bytes());
        out[15] = self.mcs_index;
        out[16] = self.bandwidth_mhz;
        out
    }

    pub fn prepend_to_payload(&self, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(WFB_FORWARD_HEADER_LEN + payload.len());
        out.extend_from_slice(&self.to_bytes());
        out.extend_from_slice(payload);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_forward_header_with_network_order_frequency() {
        let header = WfbForwardHeader::single_antenna(2, -42, 5745, 4, 20);
        let bytes = header.to_bytes();

        assert_eq!(bytes.len(), WFB_FORWARD_HEADER_LEN);
        assert_eq!(bytes[0], 2);
        assert_eq!(&bytes[1..5], &[0, 0xff, 0xff, 0xff]);
        assert_eq!(bytes[5], (-42i8) as u8);
        assert_eq!(&bytes[13..15], &5745u16.to_be_bytes());
        assert_eq!(bytes[15], 4);
        assert_eq!(bytes[16], 20);
    }

    #[test]
    fn prepends_header_to_payload() {
        let header = WfbForwardHeader::single_antenna(0, -80, 5180, 0, 20);
        let packet = header.prepend_to_payload(b"abc");

        assert_eq!(packet.len(), WFB_FORWARD_HEADER_LEN + 3);
        assert_eq!(&packet[WFB_FORWARD_HEADER_LEN..], b"abc");
    }
}
