use std::net::SocketAddr;

use radio_core::{Bandwidth, Channel};
use serde::Serialize;

use crate::frame::WfbChannelId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BridgeConfig {
    pub channel: Channel,
    pub bandwidth: Bandwidth,
    pub channel_id: WfbChannelId,
    pub tx_input: SocketConfig,
    pub rx_aggregator: SocketConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SocketConfig {
    Udp(SocketAddr),
    Unix(String),
}
