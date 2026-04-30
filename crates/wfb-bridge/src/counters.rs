use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BridgeCounters {
    pub rx: RxCounters,
    pub tx: TxCounters,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RxCounters {
    pub received: u64,
    pub matched: u64,
    pub forwarded: u64,
    pub filtered: u64,
    pub malformed: u64,
    pub send_failed: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TxCounters {
    pub incoming: u64,
    pub injected: u64,
    pub dropped: u64,
    pub malformed: u64,
    pub unsupported_radiotap: u64,
}
