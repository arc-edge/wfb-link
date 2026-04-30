use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Chipset {
    Rtl8812au,
}

impl fmt::Display for Chipset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Chipset::Rtl8812au => f.write_str("rtl8812au"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct KnownAdapter {
    pub vid: u16,
    pub pid: u16,
    pub chipset: Chipset,
    pub name: &'static str,
}

impl KnownAdapter {
    pub const fn rtl8812au(vid: u16, pid: u16, name: &'static str) -> Self {
        Self {
            vid,
            pid,
            chipset: Chipset::Rtl8812au,
            name,
        }
    }
}

pub const KNOWN_ADAPTERS: &[KnownAdapter] = &[
    KnownAdapter::rtl8812au(0x0bda, 0x8812, "Realtek RTL8812AU / ALFA AWUS036ACH class"),
    KnownAdapter::rtl8812au(0x0bda, 0x881a, "Realtek RTL8812AU"),
    KnownAdapter::rtl8812au(0x0bda, 0x881b, "Realtek RTL8812AU"),
    KnownAdapter::rtl8812au(0x0bda, 0x881c, "Realtek RTL8812AU"),
    KnownAdapter::rtl8812au(0x050d, 0x1106, "Belkin F9L1109v1"),
    KnownAdapter::rtl8812au(0x050d, 0x1109, "Belkin RTL8812AU"),
    KnownAdapter::rtl8812au(0x0846, 0x9051, "Netgear A6200 v2"),
    KnownAdapter::rtl8812au(0x0411, 0x025d, "Buffalo RTL8812AU"),
    KnownAdapter::rtl8812au(0x04bb, 0x0952, "I-O DATA RTL8812AU"),
    KnownAdapter::rtl8812au(0x2357, 0x0101, "TP-Link Archer T4U v1"),
    KnownAdapter::rtl8812au(0x2357, 0x0103, "TP-Link Archer T4UH"),
    KnownAdapter::rtl8812au(0x2357, 0x010d, "TP-Link Archer T4U v2"),
    KnownAdapter::rtl8812au(0x2357, 0x010e, "TP-Link Archer T4UH v2"),
    KnownAdapter::rtl8812au(0x2357, 0x010f, "TP-Link RTL8812AU"),
    KnownAdapter::rtl8812au(0x2357, 0x0122, "TP-Link RTL8812AU"),
    KnownAdapter::rtl8812au(0x2604, 0x0012, "Tenda U12"),
    KnownAdapter::rtl8812au(0x7392, 0xa822, "Edimax EW-7822UAC"),
    KnownAdapter::rtl8812au(0x0409, 0x0408, "NEC RTL8812AU"),
];

pub fn known_adapters() -> &'static [KnownAdapter] {
    KNOWN_ADAPTERS
}

pub fn lookup_known_adapter(vid: u16, pid: u16) -> Option<KnownAdapter> {
    KNOWN_ADAPTERS
        .iter()
        .copied()
        .find(|adapter| adapter.vid == vid && adapter.pid == pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_generic_rtl8812au_awus036ach_class_id() {
        let adapter = lookup_known_adapter(0x0bda, 0x8812).expect("known adapter");
        assert_eq!(adapter.chipset, Chipset::Rtl8812au);
        assert!(adapter.name.contains("AWUS036ACH"));
    }

    #[test]
    fn unknown_id_is_not_supported() {
        assert!(lookup_known_adapter(0xffff, 0xffff).is_none());
    }
}
