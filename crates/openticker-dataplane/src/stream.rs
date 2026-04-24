use openticker_core::{OhlcvBar, Timeframe};
use serde::Serialize;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StreamKey {
    pub account_id: String,
    pub symbol: String,
    pub timeframe: Timeframe,
}

impl Hash for StreamKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.account_id.hash(state);
        self.symbol.hash(state);
        self.timeframe.to_string().hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamSource {
    Watchlist,
    Instance(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamPreviewConnectionState {
    Connecting,
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamUpdateSource {
    Poll,
    PreviewStream,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamSpec {
    pub key: StreamKey,
    pub retention: usize,
    pub polling_interval_ms: u64,
    pub close_poll_retry_ms: Option<u64>,
    pub close_poll_grace_ms: Option<u64>,
    pub preview_enabled: bool,
    pub sources: Vec<StreamSource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamStatus {
    pub key: StreamKey,
    pub retention: usize,
    pub polling_interval_ms: u64,
    pub close_poll_retry_ms: Option<u64>,
    pub close_poll_grace_ms: Option<u64>,
    pub last_attempt_ms: Option<i64>,
    pub last_success_ms: Option<i64>,
    pub last_error: Option<String>,
    pub latest_bar: Option<OhlcvBar>,
    pub fetch_count: u64,
    pub error_count: u64,
    pub transport_staleness_ms: Option<u64>,
    pub staleness_ms: Option<u64>,
    pub confirmed_bar_close_ms: Option<i64>,
    pub confirmed_bar_staleness_ms: Option<u64>,
    pub confirmed_bar_stale_deadline_ms: Option<i64>,
    pub latest_preview_bar: Option<OhlcvBar>,
    pub preview_enabled: bool,
    pub preview_connection_state: Option<StreamPreviewConnectionState>,
    pub last_preview_update_ms: Option<i64>,
    pub last_preview_error: Option<String>,
    pub last_confirmed_update_source: Option<StreamUpdateSource>,
    pub attached_instances: Vec<String>,
    pub sparkline: Vec<f64>,
}

pub(crate) fn compare_stream_key(left: &StreamKey, right: &StreamKey) -> Ordering {
    left.account_id
        .cmp(&right.account_id)
        .then_with(|| left.symbol.cmp(&right.symbol))
        .then_with(|| left.timeframe.to_string().cmp(&right.timeframe.to_string()))
}
