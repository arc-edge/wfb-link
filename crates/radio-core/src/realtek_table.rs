use serde::Serialize;
use thiserror::Error;

const BIT31: u32 = 1 << 31;
const BIT30: u32 = 1 << 30;
const BIT29: u32 = 1 << 29;
const BIT28: u32 = 1 << 28;
const MASKDWORD: u32 = 0xffff_ffff;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RealtekTableError {
    #[error("array {array_name:?} was not found")]
    ArrayNotFound { array_name: String },
    #[error("array {array_name:?} does not contain a braced initializer")]
    ArrayBodyNotFound { array_name: String },
    #[error("array {array_name:?} contains an odd number of u32 values: {value_count}")]
    OddValueCount {
        array_name: String,
        value_count: usize,
    },
    #[error("invalid numeric literal {literal:?}: {message}")]
    InvalidNumber { literal: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtekTableKind {
    BbPhy,
    BbAgc,
    RfRadioA,
    RfRadioB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RealtekConditionEnv {
    pub cut_version: u8,
    pub package_type: u8,
    pub support_interface: u8,
    pub support_platform: u8,
    pub board_type: u8,
    pub type_glna: u16,
    pub type_gpa: u16,
    pub type_alna: u16,
    pub type_apa: u16,
}

impl RealtekConditionEnv {
    pub fn rtl8812au_awus036ach_default() -> Self {
        Self {
            cut_version: 0,
            package_type: 0,
            support_interface: 0x02,
            support_platform: 0x00,
            board_type: 0xd8,
            type_glna: 0,
            type_gpa: 0,
            type_alna: 0,
            type_apa: 0,
        }
    }
}

impl Default for RealtekConditionEnv {
    fn default() -> Self {
        Self::rtl8812au_awus036ach_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RealtekTablePlan {
    pub array_name: String,
    pub kind: RealtekTableKind,
    pub raw_value_count: usize,
    pub raw_pair_count: usize,
    pub condition_marker_pairs: usize,
    pub skipped_write_pairs: usize,
    pub actions: Vec<RealtekTableAction>,
}

impl RealtekTablePlan {
    pub fn write_count(&self) -> usize {
        self.actions
            .iter()
            .filter(|action| action.kind == RealtekTableActionKind::Write)
            .count()
    }

    pub fn delay_count(&self) -> usize {
        self.actions
            .iter()
            .filter(|action| action.kind == RealtekTableActionKind::Delay)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RealtekTableAction {
    pub pair_index: usize,
    pub kind: RealtekTableActionKind,
    pub address: u32,
    pub address_hex: String,
    pub bitmask: Option<u32>,
    pub bitmask_hex: Option<String>,
    pub data: Option<u32>,
    pub data_hex: Option<String>,
    pub delay_us: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtekTableActionKind {
    Write,
    Delay,
}

pub fn parse_realtek_u32_array(
    source: &str,
    array_name: &str,
) -> Result<Vec<u32>, RealtekTableError> {
    let body = find_array_body(source, array_name)?;
    let body = strip_c_comments(body);
    let values = scan_u32_literals(&body)?;
    if values.len() % 2 != 0 {
        return Err(RealtekTableError::OddValueCount {
            array_name: array_name.to_string(),
            value_count: values.len(),
        });
    }
    Ok(values)
}

pub fn plan_realtek_table(
    array_name: &str,
    kind: RealtekTableKind,
    values: &[u32],
    env: RealtekConditionEnv,
) -> Result<RealtekTablePlan, RealtekTableError> {
    if values.len() % 2 != 0 {
        return Err(RealtekTableError::OddValueCount {
            array_name: array_name.to_string(),
            value_count: values.len(),
        });
    }

    let mut actions = Vec::new();
    let mut condition_marker_pairs = 0usize;
    let mut skipped_write_pairs = 0usize;
    let mut is_matched = true;
    let mut is_skipped = false;
    let mut pre_v1 = 0u32;
    let mut pre_v2 = 0u32;

    for (pair_index, pair) in values.chunks_exact(2).enumerate() {
        let v1 = pair[0];
        let v2 = pair[1];

        if (v1 & (BIT31 | BIT30)) != 0 {
            condition_marker_pairs += 1;
            if (v1 & BIT31) != 0 {
                let c_cond = ((v1 & (BIT29 | BIT28)) >> 28) as u8;
                match c_cond {
                    3 => {
                        is_matched = true;
                        is_skipped = false;
                    }
                    2 => {
                        is_matched = !is_skipped;
                    }
                    _ => {
                        pre_v1 = v1;
                        pre_v2 = v2;
                    }
                }
            } else if (v1 & BIT30) != 0 {
                if !is_skipped {
                    if check_positive(env, pre_v1, pre_v2, v1, v2) {
                        is_matched = true;
                        is_skipped = true;
                    } else {
                        is_matched = false;
                        is_skipped = false;
                    }
                } else {
                    is_matched = false;
                }
            }
            continue;
        }

        if !is_matched {
            skipped_write_pairs += 1;
            continue;
        }

        if let Some(delay_us) = table_delay_us(kind, v1) {
            actions.push(RealtekTableAction {
                pair_index,
                kind: RealtekTableActionKind::Delay,
                address: v1,
                address_hex: format_u32(v1),
                bitmask: None,
                bitmask_hex: None,
                data: None,
                data_hex: None,
                delay_us: Some(delay_us),
            });
        } else {
            actions.push(RealtekTableAction {
                pair_index,
                kind: RealtekTableActionKind::Write,
                address: v1,
                address_hex: format_u32(v1),
                bitmask: Some(table_bitmask(kind)),
                bitmask_hex: Some(format_u32(table_bitmask(kind))),
                data: Some(v2),
                data_hex: Some(format_u32(v2)),
                delay_us: None,
            });
        }
    }

    Ok(RealtekTablePlan {
        array_name: array_name.to_string(),
        kind,
        raw_value_count: values.len(),
        raw_pair_count: values.len() / 2,
        condition_marker_pairs,
        skipped_write_pairs,
        actions,
    })
}

fn find_array_body<'a>(source: &'a str, array_name: &str) -> Result<&'a str, RealtekTableError> {
    let array_offset = source
        .find(array_name)
        .ok_or_else(|| RealtekTableError::ArrayNotFound {
            array_name: array_name.to_string(),
        })?;
    let after_name = &source[array_offset..];
    let brace_rel = after_name
        .find('{')
        .ok_or_else(|| RealtekTableError::ArrayBodyNotFound {
            array_name: array_name.to_string(),
        })?;
    let body_start = array_offset + brace_rel + 1;
    let mut depth = 1usize;

    for (offset, ch) in source[body_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&source[body_start..body_start + offset]);
                }
            }
            _ => {}
        }
    }

    Err(RealtekTableError::ArrayBodyNotFound {
        array_name: array_name.to_string(),
    })
}

fn strip_c_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '/' {
            output.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('/') => {
                chars.next();
                for line_ch in chars.by_ref() {
                    if line_ch == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            Some('*') => {
                chars.next();
                let mut prev = '\0';
                for block_ch in chars.by_ref() {
                    if prev == '*' && block_ch == '/' {
                        break;
                    }
                    prev = block_ch;
                }
            }
            _ => output.push(ch),
        }
    }

    output
}

fn scan_u32_literals(source: &str) -> Result<Vec<u32>, RealtekTableError> {
    let bytes = source.as_bytes();
    let mut values = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'0'
            && index + 2 < bytes.len()
            && matches!(bytes[index + 1], b'x' | b'X')
            && bytes[index + 2].is_ascii_hexdigit()
        {
            let start = index;
            index += 2;
            while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
                index += 1;
            }
            let literal = &source[start..index];
            let value = u32::from_str_radix(&literal[2..], 16).map_err(|error| {
                RealtekTableError::InvalidNumber {
                    literal: literal.to_string(),
                    message: error.to_string(),
                }
            })?;
            values.push(value);
            continue;
        }

        if bytes[index].is_ascii_digit() {
            let start = index;
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            let literal = &source[start..index];
            let value =
                literal
                    .parse::<u32>()
                    .map_err(|error| RealtekTableError::InvalidNumber {
                        literal: literal.to_string(),
                        message: error.to_string(),
                    })?;
            values.push(value);
            continue;
        }

        index += 1;
    }

    Ok(values)
}

fn check_positive(
    env: RealtekConditionEnv,
    condition1: u32,
    condition2: u32,
    condition3: u32,
    condition4: u32,
) -> bool {
    let board_type = ((env.board_type & (1 << 4)) >> 4)
        | (((env.board_type & (1 << 3)) >> 3) << 1)
        | (((env.board_type & (1 << 7)) >> 7) << 2)
        | (((env.board_type & (1 << 6)) >> 6) << 3)
        | (((env.board_type & (1 << 2)) >> 2) << 4)
        | (((env.board_type & (1 << 1)) >> 1) << 5)
        | (((env.board_type & (1 << 5)) >> 5) << 6);

    let mut cond1 = condition1;
    let cond2 = condition2;
    let _cond3 = condition3;
    let cond4 = condition4;

    let cut_version_for_para = if env.cut_version == 0 {
        15
    } else {
        env.cut_version
    };
    let package_type_for_para = if env.package_type == 0 {
        15
    } else {
        env.package_type
    };

    let mut driver1 = (u32::from(cut_version_for_para) << 24)
        | (u32::from(env.support_interface & 0xf0) << 16)
        | (u32::from(env.support_platform) << 16)
        | (u32::from(package_type_for_para) << 12)
        | (u32::from(env.support_interface & 0x0f) << 8)
        | u32::from(board_type);

    let driver2 = u32::from(env.type_glna & 0x00ff)
        | (u32::from(env.type_gpa & 0x00ff) << 8)
        | (u32::from(env.type_alna & 0x00ff) << 16)
        | (u32::from(env.type_apa & 0x00ff) << 24);

    let driver4 = u32::from((env.type_glna & 0xff00) >> 8)
        | u32::from(env.type_gpa & 0xff00)
        | (u32::from(env.type_alna & 0xff00) << 8)
        | (u32::from(env.type_apa & 0xff00) << 16);

    if ((cond1 & 0x0000_f000) != 0) && ((cond1 & 0x0000_f000) != (driver1 & 0x0000_f000)) {
        return false;
    }
    if ((cond1 & 0x0f00_0000) != 0) && ((cond1 & 0x0f00_0000) != (driver1 & 0x0f00_0000)) {
        return false;
    }

    cond1 &= 0x00ff_0fff;
    driver1 &= 0x00ff_0fff;

    if (cond1 & driver1) != cond1 {
        return false;
    }

    if (cond1 & 0x0f) == 0 {
        return true;
    }

    let mut bit_mask = 0u32;
    if (cond1 & (1 << 0)) != 0 {
        bit_mask |= 0x0000_00ff;
    }
    if (cond1 & (1 << 1)) != 0 {
        bit_mask |= 0x0000_ff00;
    }
    if (cond1 & (1 << 2)) != 0 {
        bit_mask |= 0x00ff_0000;
    }
    if (cond1 & (1 << 3)) != 0 {
        bit_mask |= 0xff00_0000;
    }

    ((cond2 & bit_mask) == (driver2 & bit_mask)) && ((cond4 & bit_mask) == (driver4 & bit_mask))
}

fn table_bitmask(kind: RealtekTableKind) -> u32 {
    match kind {
        RealtekTableKind::BbPhy | RealtekTableKind::BbAgc => MASKDWORD,
        RealtekTableKind::RfRadioA | RealtekTableKind::RfRadioB => 0x000f_ffff,
    }
}

fn table_delay_us(kind: RealtekTableKind, address: u32) -> Option<u64> {
    match kind {
        RealtekTableKind::BbPhy => match address {
            0xfe => Some(50_000),
            0xfd => Some(5_000),
            0xfc => Some(1_000),
            0xfb => Some(50),
            0xfa => Some(5),
            0xf9 => Some(1),
            _ => None,
        },
        RealtekTableKind::RfRadioA | RealtekTableKind::RfRadioB => match address {
            0xfe | 0xffe => Some(50_000),
            _ => None,
        },
        RealtekTableKind::BbAgc => None,
    }
}

fn format_u32(value: u32) -> String {
    format!("0x{value:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_array_and_ignores_comments() {
        let source = r#"
            u32 ignored[] = { 0x1, 0x2 };
            u32 array_mp_8812a_phy_reg[] = {
                0x800, 0x12345678, /* comment 0xff */
                // line comment 0xee
                0xfe, 0x00000000,
            };
        "#;

        let values = parse_realtek_u32_array(source, "array_mp_8812a_phy_reg").expect("array");

        assert_eq!(values, vec![0x800, 0x1234_5678, 0xfe, 0]);
    }

    #[test]
    fn plans_usb_positive_condition_branch() {
        let source = r#"
            u32 array_mp_8812a_mac_reg[] = {
                0x80000200, 0x00000000, 0x40000000, 0x00000000,
                0x011, 0x00000066,
                0xA0000000, 0x00000000,
                0x011, 0x0000005A,
                0xB0000000, 0x00000000,
            };
        "#;
        let values = parse_realtek_u32_array(source, "array_mp_8812a_mac_reg").expect("array");

        let plan = plan_realtek_table(
            "array_mp_8812a_mac_reg",
            RealtekTableKind::BbPhy,
            &values,
            RealtekConditionEnv::default(),
        )
        .expect("plan");

        assert_eq!(plan.condition_marker_pairs, 4);
        assert_eq!(plan.skipped_write_pairs, 1);
        assert_eq!(plan.write_count(), 1);
        assert_eq!(plan.actions[0].address, 0x011);
        assert_eq!(plan.actions[0].data, Some(0x66));
    }

    #[test]
    fn plans_awus036ach_board_branch_and_delay() {
        let values = vec![
            0x8000_0008,
            0,
            0x4000_0000,
            0,
            0x0c68,
            0x5979_1979,
            0xa000_0000,
            0,
            0x0c68,
            0x5979_9979,
            0xb000_0000,
            0,
            0xfe,
            0,
        ];

        let plan = plan_realtek_table(
            "array",
            RealtekTableKind::BbPhy,
            &values,
            RealtekConditionEnv::default(),
        )
        .expect("plan");

        assert_eq!(plan.write_count(), 1);
        assert_eq!(plan.delay_count(), 1);
        assert_eq!(plan.actions[0].data, Some(0x5979_1979));
        assert_eq!(plan.actions[1].delay_us, Some(50_000));
    }
}
