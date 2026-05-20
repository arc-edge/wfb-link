use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Ghz2,
    Ghz5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bandwidth {
    Mhz20,
    Mhz40,
    Mhz80,
}

impl Bandwidth {
    pub fn mhz(self) -> u16 {
        match self {
            Bandwidth::Mhz20 => 20,
            Bandwidth::Mhz40 => 40,
            Bandwidth::Mhz80 => 80,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub number: u8,
    pub frequency_mhz: u16,
    pub band: Band,
}

impl Channel {
    pub fn from_number(number: u8) -> Result<Self, ChannelError> {
        supported_channels()
            .iter()
            .copied()
            .find(|channel| channel.number == number)
            .ok_or(ChannelError::UnsupportedChannel { number })
    }

    pub fn supports_bandwidth(self, bandwidth: Bandwidth) -> bool {
        match (self.band, bandwidth) {
            (_, Bandwidth::Mhz20) => true,
            (Band::Ghz2, Bandwidth::Mhz40) => (1..=13).contains(&self.number),
            (Band::Ghz2, Bandwidth::Mhz80) => false,
            (Band::Ghz5, Bandwidth::Mhz40) => true,
            (Band::Ghz5, Bandwidth::Mhz80) => matches!(
                self.number,
                36 | 40
                    | 44
                    | 48
                    | 52
                    | 56
                    | 60
                    | 64
                    | 100
                    | 104
                    | 108
                    | 112
                    | 116
                    | 120
                    | 124
                    | 128
                    | 132
                    | 136
                    | 140
                    | 144
                    | 149
                    | 153
                    | 157
                    | 161
            ),
        }
    }
}

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("unsupported Wi-Fi channel {number}")]
    UnsupportedChannel { number: u8 },
    #[error("channel {number} does not support {bandwidth_mhz} MHz bandwidth")]
    UnsupportedBandwidth { number: u8, bandwidth_mhz: u16 },
}

pub const SUPPORTED_2GHZ_CHANNELS: &[Channel] = &[
    Channel {
        number: 1,
        frequency_mhz: 2412,
        band: Band::Ghz2,
    },
    Channel {
        number: 2,
        frequency_mhz: 2417,
        band: Band::Ghz2,
    },
    Channel {
        number: 3,
        frequency_mhz: 2422,
        band: Band::Ghz2,
    },
    Channel {
        number: 4,
        frequency_mhz: 2427,
        band: Band::Ghz2,
    },
    Channel {
        number: 5,
        frequency_mhz: 2432,
        band: Band::Ghz2,
    },
    Channel {
        number: 6,
        frequency_mhz: 2437,
        band: Band::Ghz2,
    },
    Channel {
        number: 7,
        frequency_mhz: 2442,
        band: Band::Ghz2,
    },
    Channel {
        number: 8,
        frequency_mhz: 2447,
        band: Band::Ghz2,
    },
    Channel {
        number: 9,
        frequency_mhz: 2452,
        band: Band::Ghz2,
    },
    Channel {
        number: 10,
        frequency_mhz: 2457,
        band: Band::Ghz2,
    },
    Channel {
        number: 11,
        frequency_mhz: 2462,
        band: Band::Ghz2,
    },
    Channel {
        number: 12,
        frequency_mhz: 2467,
        band: Band::Ghz2,
    },
    Channel {
        number: 13,
        frequency_mhz: 2472,
        band: Band::Ghz2,
    },
    Channel {
        number: 14,
        frequency_mhz: 2484,
        band: Band::Ghz2,
    },
];

pub const SUPPORTED_5GHZ_CHANNELS: &[Channel] = &[
    Channel {
        number: 36,
        frequency_mhz: 5180,
        band: Band::Ghz5,
    },
    Channel {
        number: 40,
        frequency_mhz: 5200,
        band: Band::Ghz5,
    },
    Channel {
        number: 44,
        frequency_mhz: 5220,
        band: Band::Ghz5,
    },
    Channel {
        number: 48,
        frequency_mhz: 5240,
        band: Band::Ghz5,
    },
    Channel {
        number: 52,
        frequency_mhz: 5260,
        band: Band::Ghz5,
    },
    Channel {
        number: 56,
        frequency_mhz: 5280,
        band: Band::Ghz5,
    },
    Channel {
        number: 60,
        frequency_mhz: 5300,
        band: Band::Ghz5,
    },
    Channel {
        number: 64,
        frequency_mhz: 5320,
        band: Band::Ghz5,
    },
    Channel {
        number: 100,
        frequency_mhz: 5500,
        band: Band::Ghz5,
    },
    Channel {
        number: 104,
        frequency_mhz: 5520,
        band: Band::Ghz5,
    },
    Channel {
        number: 108,
        frequency_mhz: 5540,
        band: Band::Ghz5,
    },
    Channel {
        number: 112,
        frequency_mhz: 5560,
        band: Band::Ghz5,
    },
    Channel {
        number: 116,
        frequency_mhz: 5580,
        band: Band::Ghz5,
    },
    Channel {
        number: 120,
        frequency_mhz: 5600,
        band: Band::Ghz5,
    },
    Channel {
        number: 124,
        frequency_mhz: 5620,
        band: Band::Ghz5,
    },
    Channel {
        number: 128,
        frequency_mhz: 5640,
        band: Band::Ghz5,
    },
    Channel {
        number: 132,
        frequency_mhz: 5660,
        band: Band::Ghz5,
    },
    Channel {
        number: 136,
        frequency_mhz: 5680,
        band: Band::Ghz5,
    },
    Channel {
        number: 140,
        frequency_mhz: 5700,
        band: Band::Ghz5,
    },
    Channel {
        number: 144,
        frequency_mhz: 5720,
        band: Band::Ghz5,
    },
    Channel {
        number: 149,
        frequency_mhz: 5745,
        band: Band::Ghz5,
    },
    Channel {
        number: 153,
        frequency_mhz: 5765,
        band: Band::Ghz5,
    },
    Channel {
        number: 157,
        frequency_mhz: 5785,
        band: Band::Ghz5,
    },
    Channel {
        number: 161,
        frequency_mhz: 5805,
        band: Band::Ghz5,
    },
    Channel {
        number: 165,
        frequency_mhz: 5825,
        band: Band::Ghz5,
    },
];

pub fn supported_channels() -> Vec<Channel> {
    SUPPORTED_2GHZ_CHANNELS
        .iter()
        .chain(SUPPORTED_5GHZ_CHANNELS.iter())
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_channels_to_frequency() {
        assert_eq!(
            Channel::from_number(1).expect("channel 1").frequency_mhz,
            2412
        );
        assert_eq!(
            Channel::from_number(36).expect("channel 36").frequency_mhz,
            5180
        );
        assert_eq!(
            Channel::from_number(165)
                .expect("channel 165")
                .frequency_mhz,
            5825
        );
    }

    #[test]
    fn rejects_unsupported_channel() {
        assert!(matches!(
            Channel::from_number(15),
            Err(ChannelError::UnsupportedChannel { number: 15 })
        ));
    }

    #[test]
    fn validates_conservative_bandwidth_support() {
        assert!(Channel::from_number(6)
            .expect("channel 6")
            .supports_bandwidth(Bandwidth::Mhz20));
        assert!(!Channel::from_number(6)
            .expect("channel 6")
            .supports_bandwidth(Bandwidth::Mhz80));
        assert!(Channel::from_number(149)
            .expect("channel 149")
            .supports_bandwidth(Bandwidth::Mhz80));
        assert!(!Channel::from_number(165)
            .expect("channel 165")
            .supports_bandwidth(Bandwidth::Mhz80));
    }
}
