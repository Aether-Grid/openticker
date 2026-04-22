use chrono::{DateTime, Utc};
use openticker_core::OhlcvBar;

/// Builds a simple deterministic OHLCV bar for tests.
///
/// # Panics
///
/// Panics when `timestamp` is not valid RFC3339 input.
#[must_use]
pub fn close_only_bar(timestamp: &str, close: f64) -> OhlcvBar {
    OhlcvBar {
        timestamp: DateTime::parse_from_rfc3339(timestamp)
            .expect("testkit timestamps must be valid RFC3339")
            .with_timezone(&Utc),
        open: close,
        high: close + 0.9,
        low: close - 0.9,
        close,
        volume: 1_000.0,
    }
}

/// Builds a deterministic `(symbol, bar)` pair for tests that track symbol context.
///
/// # Panics
///
/// Panics when `timestamp` is not valid RFC3339 input.
#[must_use]
pub fn close_only_symbol_bar(symbol: &str, timestamp: &str, close: f64) -> (String, OhlcvBar) {
    (symbol.to_owned(), close_only_bar(timestamp, close))
}
