use super::*;

pub type Rtl8812auTxPowerAgcRegister = (&'static str, u16);

const TX_POWER_AGC_PATH_A_REGISTERS: &[Rtl8812auTxPowerAgcRegister] = &[
    ("rA_TxAGC_CCK", REG_TX_AGC_A_CCK_JAGUAR),
    ("rA_TxAGC_OFDM18_OFDM6", REG_TX_AGC_A_OFDM18_OFDM6_JAGUAR),
    ("rA_TxAGC_OFDM54_OFDM24", REG_TX_AGC_A_OFDM54_OFDM24_JAGUAR),
    ("rA_TxAGC_MCS3_MCS0", REG_TX_AGC_A_MCS3_MCS0_JAGUAR),
    ("rA_TxAGC_MCS7_MCS4", REG_TX_AGC_A_MCS7_MCS4_JAGUAR),
    ("rA_TxAGC_NSS1_7_NSS1_4", REG_TX_AGC_A_NSS1_7_NSS1_4_JAGUAR),
    (
        "rA_TxAGC_NSS1_11_NSS1_8",
        REG_TX_AGC_A_NSS1_11_NSS1_8_JAGUAR,
    ),
    ("rA_TxAGC_NSS1_3_NSS1_0", REG_TX_AGC_A_NSS1_3_NSS1_0_JAGUAR),
    ("rA_TxAGC_NSS2_3_NSS2_0", REG_TX_AGC_A_NSS2_3_NSS2_0_JAGUAR),
    ("rA_TxAGC_NSS2_7_NSS2_4", REG_TX_AGC_A_NSS2_7_NSS2_4_JAGUAR),
    (
        "rA_TxAGC_NSS2_11_NSS2_8",
        REG_TX_AGC_A_NSS2_11_NSS2_8_JAGUAR,
    ),
    ("rA_TxAGC_NSS3_3_NSS3_0", REG_TX_AGC_A_NSS3_3_NSS3_0_JAGUAR),
];

const TX_POWER_AGC_PATH_B_REGISTERS: &[Rtl8812auTxPowerAgcRegister] = &[
    ("rB_TxAGC_CCK", REG_TX_AGC_B_CCK_JAGUAR),
    ("rB_TxAGC_OFDM18_OFDM6", REG_TX_AGC_B_OFDM18_OFDM6_JAGUAR),
    ("rB_TxAGC_OFDM54_OFDM24", REG_TX_AGC_B_OFDM54_OFDM24_JAGUAR),
    ("rB_TxAGC_MCS3_MCS0", REG_TX_AGC_B_MCS3_MCS0_JAGUAR),
    ("rB_TxAGC_MCS7_MCS4", REG_TX_AGC_B_MCS7_MCS4_JAGUAR),
    ("rB_TxAGC_NSS1_7_NSS1_4", REG_TX_AGC_B_NSS1_7_NSS1_4_JAGUAR),
    (
        "rB_TxAGC_NSS1_11_NSS1_8",
        REG_TX_AGC_B_NSS1_11_NSS1_8_JAGUAR,
    ),
    ("rB_TxAGC_NSS1_3_NSS1_0", REG_TX_AGC_B_NSS1_3_NSS1_0_JAGUAR),
    ("rB_TxAGC_NSS2_3_NSS2_0", REG_TX_AGC_B_NSS2_3_NSS2_0_JAGUAR),
    ("rB_TxAGC_NSS2_7_NSS2_4", REG_TX_AGC_B_NSS2_7_NSS2_4_JAGUAR),
    (
        "rB_TxAGC_NSS2_11_NSS2_8",
        REG_TX_AGC_B_NSS2_11_NSS2_8_JAGUAR,
    ),
    ("rB_TxAGC_NSS3_3_NSS3_0", REG_TX_AGC_B_NSS3_3_NSS3_0_JAGUAR),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rtl8812auTxPowerSafetyProfile {
    MaxIndex,
    LinuxCh36Ht20,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rtl8812auTxPowerControlMode {
    ManualIndex,
    EfuseDerived,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auTxPowerControlReport {
    pub semantics: &'static str,
    pub mode: Rtl8812auTxPowerControlMode,
    pub manual_index: Option<u8>,
    pub manual_index_hex: Option<String>,
    pub path: Rtl8812auRfPath,
    pub register_count: usize,
    pub repeated_value: Option<u32>,
    pub repeated_value_hex: Option<String>,
    pub efuse_source: Option<Rtl8812auTxPowerEfuseSourceReport>,
    pub efuse_plan: Option<Rtl8812auTxPowerEfusePlanReport>,
    pub writes: Vec<Rtl8812auRegisterWriteReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rtl8812auTxPowerEfuseSourceReport {
    pub source_kind: &'static str,
    pub source_path: Option<PathBuf>,
    pub tx_power_start_offset: usize,
    pub tx_power_length: usize,
    pub tx_power_data_hex: String,
    pub non_ff_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auTxPowerEfusePlanReport {
    pub algorithm: &'static str,
    pub upstream_basis: &'static str,
    pub channel: u8,
    pub bandwidth_mhz: u16,
    pub channel_group: Rtl8812auTxPowerChannelGroupReport,
    pub selected_path: Rtl8812auRfPath,
    pub programmed_paths: Vec<Rtl8812auRfPath>,
    pub safety_profile: Rtl8812auTxPowerSafetyProfile,
    pub max_index: u8,
    pub max_index_hex: String,
    pub writes: Vec<Rtl8812auTxPowerDerivedWriteReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auTxPowerChannelGroupReport {
    pub band: &'static str,
    pub group: u8,
    pub group_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auTxPowerDerivedWriteReport {
    pub name: &'static str,
    pub address: u16,
    pub address_hex: String,
    pub path: Rtl8812auRfPath,
    pub value: u32,
    pub value_hex: String,
    pub lanes: Vec<Rtl8812auTxPowerDerivedLaneReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rtl8812auTxPowerDerivedLaneReport {
    pub lane: u8,
    pub rate: &'static str,
    pub rate_section: &'static str,
    pub tx_streams: u8,
    pub efuse_base_offset: usize,
    pub efuse_base_value: u8,
    pub efuse_base_value_hex: String,
    pub efuse_diff_kind: &'static str,
    pub efuse_diff_offset: Option<usize>,
    pub efuse_diff_source_hex: Option<String>,
    pub efuse_diff_value: i8,
    pub by_rate_offset: i8,
    pub tracking_offset: i8,
    pub unclamped_index: i16,
    pub clamp_profile: Rtl8812auTxPowerSafetyProfile,
    pub clamp_max_index: u8,
    pub clamp_max_index_hex: String,
    pub final_index: u8,
    pub final_index_hex: String,
    pub clamped: bool,
}

#[derive(Debug, Clone, Copy)]
enum TxPowerRateFamily {
    Ofdm,
    Ht,
    Vht,
}

#[derive(Debug, Clone, Copy)]
struct TxPowerRateLaneSpec {
    lane: u8,
    rate: &'static str,
    rate_section: &'static str,
    family: TxPowerRateFamily,
    tx_streams: u8,
    by_rate_offset: i8,
}

#[derive(Debug)]
struct TxPowerEfuseRegisterSpec {
    name: &'static str,
    address: u16,
    path: Rtl8812auRfPath,
    lanes: Vec<TxPowerRateLaneSpec>,
}

#[derive(Debug, Clone, Copy)]
struct TxPowerEfuseDiff {
    kind: &'static str,
    offset: Option<usize>,
    source_byte: Option<u8>,
    value: i8,
}

pub fn rtl8812au_tx_power_agc_value(index: u8) -> u32 {
    u32::from(index) * 0x0101_0101
}

pub fn rtl8812au_tx_power_agc_registers(path: Rtl8812auRfPath) -> Vec<Rtl8812auTxPowerAgcRegister> {
    match path {
        Rtl8812auRfPath::A => TX_POWER_AGC_PATH_A_REGISTERS.to_vec(),
        Rtl8812auRfPath::B => TX_POWER_AGC_PATH_B_REGISTERS.to_vec(),
        Rtl8812auRfPath::Both => TX_POWER_AGC_PATH_A_REGISTERS
            .iter()
            .chain(TX_POWER_AGC_PATH_B_REGISTERS.iter())
            .copied()
            .collect(),
    }
}

fn tx_power_lanes(
    entries: &[(u8, &'static str, &'static str, TxPowerRateFamily, u8, i8)],
) -> Vec<TxPowerRateLaneSpec> {
    entries
        .iter()
        .map(
            |&(lane, rate, rate_section, family, tx_streams, by_rate_offset)| TxPowerRateLaneSpec {
                lane,
                rate,
                rate_section,
                family,
                tx_streams,
                by_rate_offset,
            },
        )
        .collect()
}

fn tx_power_efuse_register_specs(path: Rtl8812auRfPath) -> Vec<TxPowerEfuseRegisterSpec> {
    let mut specs = Vec::new();
    if matches!(path, Rtl8812auRfPath::A | Rtl8812auRfPath::Both) {
        specs.extend(tx_power_efuse_path_register_specs(Rtl8812auRfPath::A));
    }
    if matches!(path, Rtl8812auRfPath::B | Rtl8812auRfPath::Both) {
        specs.extend(tx_power_efuse_path_register_specs(Rtl8812auRfPath::B));
    }
    specs
}

fn tx_power_efuse_path_register_specs(path: Rtl8812auRfPath) -> Vec<TxPowerEfuseRegisterSpec> {
    let (
        ofdm_low,
        ofdm_high,
        ht1_low,
        ht1_high,
        ht2_low,
        ht2_high,
        vht1_low,
        vht1_high,
        vht_mixed,
        vht2_mid,
        vht2_high,
    ) = match path {
        Rtl8812auRfPath::A => (
            REG_TX_AGC_A_OFDM18_OFDM6_JAGUAR,
            REG_TX_AGC_A_OFDM54_OFDM24_JAGUAR,
            REG_TX_AGC_A_MCS3_MCS0_JAGUAR,
            REG_TX_AGC_A_MCS7_MCS4_JAGUAR,
            REG_TX_AGC_A_NSS1_7_NSS1_4_JAGUAR,
            REG_TX_AGC_A_NSS1_11_NSS1_8_JAGUAR,
            REG_TX_AGC_A_NSS1_3_NSS1_0_JAGUAR,
            REG_TX_AGC_A_NSS2_3_NSS2_0_JAGUAR,
            REG_TX_AGC_A_NSS2_7_NSS2_4_JAGUAR,
            REG_TX_AGC_A_NSS2_11_NSS2_8_JAGUAR,
            REG_TX_AGC_A_NSS3_3_NSS3_0_JAGUAR,
        ),
        Rtl8812auRfPath::B => (
            REG_TX_AGC_B_OFDM18_OFDM6_JAGUAR,
            REG_TX_AGC_B_OFDM54_OFDM24_JAGUAR,
            REG_TX_AGC_B_MCS3_MCS0_JAGUAR,
            REG_TX_AGC_B_MCS7_MCS4_JAGUAR,
            REG_TX_AGC_B_NSS1_7_NSS1_4_JAGUAR,
            REG_TX_AGC_B_NSS1_11_NSS1_8_JAGUAR,
            REG_TX_AGC_B_NSS1_3_NSS1_0_JAGUAR,
            REG_TX_AGC_B_NSS2_3_NSS2_0_JAGUAR,
            REG_TX_AGC_B_NSS2_7_NSS2_4_JAGUAR,
            REG_TX_AGC_B_NSS2_11_NSS2_8_JAGUAR,
            REG_TX_AGC_B_NSS3_3_NSS3_0_JAGUAR,
        ),
        Rtl8812auRfPath::Both => unreachable!("path-specific TX power specs require A or B"),
    };
    vec![
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_OFDM18_OFDM6",
                Rtl8812auRfPath::B => "rB_TxAGC_OFDM18_OFDM6",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: ofdm_low,
            path,
            lanes: tx_power_lanes(&[
                (0, "ofdm_6m", "ofdm", TxPowerRateFamily::Ofdm, 1, 14),
                (1, "ofdm_9m", "ofdm", TxPowerRateFamily::Ofdm, 1, 14),
                (2, "ofdm_12m", "ofdm", TxPowerRateFamily::Ofdm, 1, 12),
                (3, "ofdm_18m", "ofdm", TxPowerRateFamily::Ofdm, 1, 12),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_OFDM54_OFDM24",
                Rtl8812auRfPath::B => "rB_TxAGC_OFDM54_OFDM24",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: ofdm_high,
            path,
            lanes: tx_power_lanes(&[
                (0, "ofdm_24m", "ofdm", TxPowerRateFamily::Ofdm, 1, 10),
                (1, "ofdm_36m", "ofdm", TxPowerRateFamily::Ofdm, 1, 6),
                (2, "ofdm_48m", "ofdm", TxPowerRateFamily::Ofdm, 1, 2),
                (3, "ofdm_54m", "ofdm", TxPowerRateFamily::Ofdm, 1, 0),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_MCS3_MCS0",
                Rtl8812auRfPath::B => "rB_TxAGC_MCS3_MCS0",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: ht1_low,
            path,
            lanes: tx_power_lanes(&[
                (0, "mcs0", "ht_1ss", TxPowerRateFamily::Ht, 1, 16),
                (1, "mcs1", "ht_1ss", TxPowerRateFamily::Ht, 1, 16),
                (2, "mcs2", "ht_1ss", TxPowerRateFamily::Ht, 1, 14),
                (3, "mcs3", "ht_1ss", TxPowerRateFamily::Ht, 1, 12),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_MCS7_MCS4",
                Rtl8812auRfPath::B => "rB_TxAGC_MCS7_MCS4",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: ht1_high,
            path,
            lanes: tx_power_lanes(&[
                (0, "mcs4", "ht_1ss", TxPowerRateFamily::Ht, 1, 8),
                (1, "mcs5", "ht_1ss", TxPowerRateFamily::Ht, 1, 4),
                (2, "mcs6", "ht_1ss", TxPowerRateFamily::Ht, 1, 2),
                (3, "mcs7", "ht_1ss", TxPowerRateFamily::Ht, 1, 0),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_NSS1_7_NSS1_4",
                Rtl8812auRfPath::B => "rB_TxAGC_NSS1_7_NSS1_4",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: ht2_low,
            path,
            lanes: tx_power_lanes(&[
                (0, "mcs8", "ht_2ss", TxPowerRateFamily::Ht, 2, 16),
                (1, "mcs9", "ht_2ss", TxPowerRateFamily::Ht, 2, 16),
                (2, "mcs10", "ht_2ss", TxPowerRateFamily::Ht, 2, 14),
                (3, "mcs11", "ht_2ss", TxPowerRateFamily::Ht, 2, 12),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_NSS1_11_NSS1_8",
                Rtl8812auRfPath::B => "rB_TxAGC_NSS1_11_NSS1_8",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: ht2_high,
            path,
            lanes: tx_power_lanes(&[
                (0, "mcs12", "ht_2ss", TxPowerRateFamily::Ht, 2, 8),
                (1, "mcs13", "ht_2ss", TxPowerRateFamily::Ht, 2, 4),
                (2, "mcs14", "ht_2ss", TxPowerRateFamily::Ht, 2, 2),
                (3, "mcs15", "ht_2ss", TxPowerRateFamily::Ht, 2, 0),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_NSS1_3_NSS1_0",
                Rtl8812auRfPath::B => "rB_TxAGC_NSS1_3_NSS1_0",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: vht1_low,
            path,
            lanes: tx_power_lanes(&[
                (0, "vht1ss_mcs0", "vht_1ss", TxPowerRateFamily::Vht, 1, 16),
                (1, "vht1ss_mcs1", "vht_1ss", TxPowerRateFamily::Vht, 1, 16),
                (2, "vht1ss_mcs2", "vht_1ss", TxPowerRateFamily::Vht, 1, 14),
                (3, "vht1ss_mcs3", "vht_1ss", TxPowerRateFamily::Vht, 1, 12),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_NSS2_3_NSS2_0",
                Rtl8812auRfPath::B => "rB_TxAGC_NSS2_3_NSS2_0",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: vht1_high,
            path,
            lanes: tx_power_lanes(&[
                (0, "vht1ss_mcs4", "vht_1ss", TxPowerRateFamily::Vht, 1, 8),
                (1, "vht1ss_mcs5", "vht_1ss", TxPowerRateFamily::Vht, 1, 4),
                (2, "vht1ss_mcs6", "vht_1ss", TxPowerRateFamily::Vht, 1, 2),
                (3, "vht1ss_mcs7", "vht_1ss", TxPowerRateFamily::Vht, 1, 0),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_NSS2_7_NSS2_4",
                Rtl8812auRfPath::B => "rB_TxAGC_NSS2_7_NSS2_4",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: vht_mixed,
            path,
            lanes: tx_power_lanes(&[
                (0, "vht1ss_mcs8", "vht_1ss", TxPowerRateFamily::Vht, 1, -2),
                (1, "vht1ss_mcs9", "vht_1ss", TxPowerRateFamily::Vht, 1, -4),
                (2, "vht2ss_mcs0", "vht_2ss", TxPowerRateFamily::Vht, 2, 16),
                (3, "vht2ss_mcs1", "vht_2ss", TxPowerRateFamily::Vht, 2, 16),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_NSS2_11_NSS2_8",
                Rtl8812auRfPath::B => "rB_TxAGC_NSS2_11_NSS2_8",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: vht2_mid,
            path,
            lanes: tx_power_lanes(&[
                (0, "vht2ss_mcs2", "vht_2ss", TxPowerRateFamily::Vht, 2, 14),
                (1, "vht2ss_mcs3", "vht_2ss", TxPowerRateFamily::Vht, 2, 12),
                (2, "vht2ss_mcs4", "vht_2ss", TxPowerRateFamily::Vht, 2, 8),
                (3, "vht2ss_mcs5", "vht_2ss", TxPowerRateFamily::Vht, 2, 4),
            ]),
        },
        TxPowerEfuseRegisterSpec {
            name: match path {
                Rtl8812auRfPath::A => "rA_TxAGC_NSS3_3_NSS3_0",
                Rtl8812auRfPath::B => "rB_TxAGC_NSS3_3_NSS3_0",
                Rtl8812auRfPath::Both => unreachable!(),
            },
            address: vht2_high,
            path,
            lanes: tx_power_lanes(&[
                (0, "vht2ss_mcs6", "vht_2ss", TxPowerRateFamily::Vht, 2, 2),
                (1, "vht2ss_mcs7", "vht_2ss", TxPowerRateFamily::Vht, 2, 0),
                (2, "vht2ss_mcs8", "vht_2ss", TxPowerRateFamily::Vht, 2, -2),
                (3, "vht2ss_mcs9", "vht_2ss", TxPowerRateFamily::Vht, 2, -4),
            ]),
        },
    ]
}

fn tx_power_5g_channel_group(channel: u8) -> Option<u8> {
    match channel {
        15..=42 => Some(0),
        44..=48 => Some(1),
        50..=58 => Some(2),
        60..=80 => Some(3),
        82..=106 => Some(4),
        108..=114 => Some(5),
        116..=122 => Some(6),
        124..=130 => Some(7),
        132..=138 => Some(8),
        140..=144 => Some(9),
        149..=155 => Some(10),
        157..=161 => Some(11),
        165..=171 => Some(12),
        173..=177 => Some(13),
        _ => None,
    }
}

fn tx_power_5g_path_offset(path: Rtl8812auRfPath) -> usize {
    match path {
        Rtl8812auRfPath::A => 18,
        Rtl8812auRfPath::B => 60,
        Rtl8812auRfPath::Both => unreachable!("path offset requires A or B"),
    }
}

fn tx_power_sign_extend_4bit(value: u8) -> i8 {
    let nibble = value & 0x0f;
    if nibble & 0x08 != 0 {
        (nibble as i8) - 16
    } else {
        nibble as i8
    }
}

fn tx_power_diff_from_byte(
    data: &[u8],
    offset: usize,
    high_nibble: bool,
    kind: &'static str,
) -> Result<TxPowerEfuseDiff, RuntimeRadioError> {
    let Some(byte) = data.get(offset).copied() else {
        return Err(RuntimeRadioError::new(
            "tx_power_efuse_diff_out_of_range",
            format!("EFUSE TX-power diff offset {offset} is outside the TX-power region"),
        ));
    };
    let nibble = if high_nibble { byte >> 4 } else { byte & 0x0f };
    Ok(TxPowerEfuseDiff {
        kind,
        offset: Some(RTL8812AU_EFUSE_TX_POWER_START + offset),
        source_byte: Some(byte),
        value: tx_power_sign_extend_4bit(nibble),
    })
}

fn tx_power_efuse_5g_diff(
    data: &[u8],
    path: Rtl8812auRfPath,
    lane: TxPowerRateLaneSpec,
    bandwidth: Bandwidth,
) -> Result<TxPowerEfuseDiff, RuntimeRadioError> {
    let tx_index = lane.tx_streams.saturating_sub(1);
    let path_offset = tx_power_5g_path_offset(path);
    match lane.family {
        TxPowerRateFamily::Ofdm => match tx_index {
            0 => tx_power_diff_from_byte(data, path_offset + 14, false, "ofdm_5g_diff"),
            1 => tx_power_diff_from_byte(data, path_offset + 18, true, "ofdm_5g_diff"),
            2 => tx_power_diff_from_byte(data, path_offset + 18, false, "ofdm_5g_diff"),
            3 => tx_power_diff_from_byte(data, path_offset + 19, false, "ofdm_5g_diff"),
            _ => Err(RuntimeRadioError::new(
                "tx_power_stream_count_unsupported",
                format!("unsupported OFDM TX stream count {}", lane.tx_streams),
            )),
        },
        TxPowerRateFamily::Ht | TxPowerRateFamily::Vht => match bandwidth {
            Bandwidth::Mhz20 => match tx_index {
                0 => tx_power_diff_from_byte(data, path_offset + 14, true, "bw20_5g_diff"),
                1 => tx_power_diff_from_byte(data, path_offset + 15, false, "bw20_5g_diff"),
                2 => tx_power_diff_from_byte(data, path_offset + 16, false, "bw20_5g_diff"),
                3 => tx_power_diff_from_byte(data, path_offset + 17, false, "bw20_5g_diff"),
                _ => Err(RuntimeRadioError::new(
                    "tx_power_stream_count_unsupported",
                    format!("unsupported HT/VHT TX stream count {}", lane.tx_streams),
                )),
            },
            Bandwidth::Mhz40 => match tx_index {
                0 => Ok(TxPowerEfuseDiff {
                    kind: "bw40_5g_diff_default_1ss",
                    offset: None,
                    source_byte: None,
                    value: 0,
                }),
                1 => tx_power_diff_from_byte(data, path_offset + 15, true, "bw40_5g_diff"),
                2 => tx_power_diff_from_byte(data, path_offset + 16, true, "bw40_5g_diff"),
                3 => tx_power_diff_from_byte(data, path_offset + 17, true, "bw40_5g_diff"),
                _ => Err(RuntimeRadioError::new(
                    "tx_power_stream_count_unsupported",
                    format!("unsupported HT/VHT TX stream count {}", lane.tx_streams),
                )),
            },
            Bandwidth::Mhz80 => match tx_index {
                0..=3 => tx_power_diff_from_byte(
                    data,
                    path_offset + 20 + usize::from(tx_index),
                    true,
                    "bw80_5g_diff",
                ),
                _ => Err(RuntimeRadioError::new(
                    "tx_power_stream_count_unsupported",
                    format!("unsupported HT/VHT TX stream count {}", lane.tx_streams),
                )),
            },
        },
    }
}

fn tx_power_safety_clamp(
    profile: Rtl8812auTxPowerSafetyProfile,
    max_index: u8,
    channel: Channel,
    bandwidth: Bandwidth,
    path: Rtl8812auRfPath,
    lane: TxPowerRateLaneSpec,
) -> u8 {
    let profile_max = match profile {
        Rtl8812auTxPowerSafetyProfile::MaxIndex => max_index,
        Rtl8812auTxPowerSafetyProfile::LinuxCh36Ht20
            if channel.number == 36 && bandwidth == Bandwidth::Mhz20 =>
        {
            match (path, lane.family, lane.tx_streams) {
                (Rtl8812auRfPath::A, TxPowerRateFamily::Ofdm, _) => 0x1b,
                (Rtl8812auRfPath::B, TxPowerRateFamily::Ofdm, _) => 0x1d,
                (Rtl8812auRfPath::A, _, 1) => 0x17,
                (Rtl8812auRfPath::B, _, 1) => 0x1c,
                (Rtl8812auRfPath::A, _, 2) => 0x15,
                (Rtl8812auRfPath::B, _, 2) => 0x1a,
                _ => max_index,
            }
        }
        Rtl8812auTxPowerSafetyProfile::LinuxCh36Ht20 => max_index,
    };
    profile_max.min(max_index)
}

pub fn plan_rtl8812au_efuse_tx_power(
    tx_power_data: &[u8],
    channel: Channel,
    bandwidth: Bandwidth,
    selected_path: Rtl8812auRfPath,
    safety_profile: Rtl8812auTxPowerSafetyProfile,
    max_index: u8,
) -> Result<Rtl8812auTxPowerEfusePlanReport, RuntimeRadioError> {
    if channel.band != Band::Ghz5 {
        return Err(RuntimeRadioError::new(
            "tx_power_efuse_band_unsupported",
            "EFUSE-derived TX power currently supports RTL8812AU 5 GHz channel groups only",
        ));
    }
    let Some(group) = tx_power_5g_channel_group(channel.number) else {
        return Err(RuntimeRadioError::new(
            "tx_power_efuse_channel_group_unknown",
            format!(
                "no RTL8812AU 5 GHz TX-power group for channel {}",
                channel.number
            ),
        ));
    };
    let specs = tx_power_efuse_register_specs(selected_path);
    let mut writes = Vec::new();
    let mut programmed_paths = Vec::new();
    for spec in specs {
        if !programmed_paths.contains(&spec.path) {
            programmed_paths.push(spec.path);
        }
        let path_offset = tx_power_5g_path_offset(spec.path);
        let base_offset = path_offset + usize::from(group);
        let Some(base_value) = tx_power_data.get(base_offset).copied() else {
            return Err(RuntimeRadioError::new(
                "tx_power_efuse_base_out_of_range",
                format!("EFUSE TX-power base offset {base_offset} is outside the TX-power region"),
            ));
        };
        if base_value > RTL8812AU_TX_POWER_INDEX_MAX {
            return Err(RuntimeRadioError::new(
                "tx_power_efuse_base_invalid",
                format!(
                    "EFUSE TX-power base {} at offset {} exceeds RTL8812AU max index {}",
                    format_register_value(base_value, 2),
                    RTL8812AU_EFUSE_TX_POWER_START + base_offset,
                    format_register_value(RTL8812AU_TX_POWER_INDEX_MAX, 2)
                ),
            ));
        }
        let mut value = 0u32;
        let mut lanes = Vec::new();
        for lane in spec.lanes {
            let diff = tx_power_efuse_5g_diff(tx_power_data, spec.path, lane, bandwidth)?;
            let unclamped =
                i16::from(base_value) + i16::from(diff.value) + i16::from(lane.by_rate_offset);
            let clamp_max = tx_power_safety_clamp(
                safety_profile,
                max_index,
                channel,
                bandwidth,
                spec.path,
                lane,
            );
            let clamp_min: u8 = if clamp_max == 0 { 0 } else { 1 };
            let final_index = unclamped.clamp(i16::from(clamp_min), i16::from(clamp_max)) as u8;
            value |= u32::from(final_index) << (u32::from(lane.lane) * 8);
            lanes.push(Rtl8812auTxPowerDerivedLaneReport {
                lane: lane.lane,
                rate: lane.rate,
                rate_section: lane.rate_section,
                tx_streams: lane.tx_streams,
                efuse_base_offset: RTL8812AU_EFUSE_TX_POWER_START + base_offset,
                efuse_base_value: base_value,
                efuse_base_value_hex: format_register_value(base_value, 2),
                efuse_diff_kind: diff.kind,
                efuse_diff_offset: diff.offset,
                efuse_diff_source_hex: diff.source_byte.map(|byte| format_register_value(byte, 2)),
                efuse_diff_value: diff.value,
                by_rate_offset: lane.by_rate_offset,
                tracking_offset: 0,
                unclamped_index: unclamped,
                clamp_profile: safety_profile,
                clamp_max_index: clamp_max,
                clamp_max_index_hex: format_register_value(clamp_max, 2),
                final_index,
                final_index_hex: format_register_value(final_index, 2),
                clamped: unclamped != i16::from(final_index),
            });
        }
        writes.push(Rtl8812auTxPowerDerivedWriteReport {
            name: spec.name,
            address: spec.address,
            address_hex: format_register_address(spec.address),
            path: spec.path,
            value,
            value_hex: format_register_value(value, 8),
            lanes,
        });
    }
    Ok(Rtl8812auTxPowerEfusePlanReport {
        algorithm: "rtl8812au_efuse_base_plus_diff_plus_phy_reg_pg_by_rate_with_explicit_safety_clamp",
        upstream_basis: "hal_load_txpwr_info + PHY_GetTxPowerIndexBase + PHY_GetTxPowerByRate + PHY_SetTxPowerIndex_8812A",
        channel: channel.number,
        bandwidth_mhz: bandwidth.mhz(),
        channel_group: Rtl8812auTxPowerChannelGroupReport {
            band: "5ghz",
            group,
            group_name: format!("5g_group_{group:02}"),
        },
        selected_path,
        programmed_paths,
        safety_profile,
        max_index,
        max_index_hex: format_register_value(max_index, 2),
        writes,
    })
}

pub fn run_rtl8812au_manual_tx_power<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    path: Rtl8812auRfPath,
    index: u8,
) -> Result<Vec<Rtl8812auRegisterWriteReport>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    if index > RTL8812AU_TX_POWER_INDEX_MAX {
        return Err(RuntimeRadioError::new(
            "tx_power_index_out_of_range",
            format!(
                "TX power index {} exceeds RTL8812AU max index {}",
                format_register_value(index, 2),
                format_register_value(RTL8812AU_TX_POWER_INDEX_MAX, 2)
            ),
        ));
    }
    let value = rtl8812au_tx_power_agc_value(index);
    let mut reports = Vec::new();
    for (name, address) in rtl8812au_tx_power_agc_registers(path) {
        reports.push(write32_register_report(
            registers, name, address, value, counters,
        )?);
    }
    Ok(reports)
}

pub fn run_rtl8812au_efuse_tx_power<T>(
    registers: &Rtl8812auRegisterAccess<T>,
    counters: &mut RuntimeRadioCounters,
    plan: &Rtl8812auTxPowerEfusePlanReport,
) -> Result<Vec<Rtl8812auRegisterWriteReport>, RuntimeRadioError>
where
    T: Rtl8812auUsbTransport,
{
    let mut reports = Vec::new();
    for write in &plan.writes {
        reports.push(write32_register_report(
            registers,
            write.name,
            write.address,
            write.value,
            counters,
        )?);
    }
    Ok(reports)
}
