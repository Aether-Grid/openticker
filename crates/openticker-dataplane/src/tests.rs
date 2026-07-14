mod buffer;
mod dataplane;
mod registry;

use crate::StreamKey;
use chrono::{TimeZone, Utc};
use openticker_core::{OhlcvBar, Timeframe};

fn key(account_id: &str, symbol: &str, timeframe: Timeframe) -> StreamKey {
    StreamKey {
        account_id: account_id.to_owned(),
        symbol: symbol.to_owned(),
        timeframe,
    }
}

fn bar(minute: i64, close: f64) -> OhlcvBar {
    OhlcvBar {
        timestamp: Utc.timestamp_opt(minute * 60, 0).single().unwrap(),
        open: close,
        high: close,
        low: close,
        close,
        volume: 1.0,
    }
}
