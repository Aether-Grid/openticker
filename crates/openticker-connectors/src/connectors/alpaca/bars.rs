use openticker_core::{OhlcvBar, Timeframe};
use serde::Deserialize;
use std::collections::HashMap;

pub(super) const ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS: i64 = 30;
const ALPACA_RECENT_BARS_LOOKBACK_MAX_DAYS: i64 = 730;
const ALPACA_RECENT_BARS_LOOKBACK_SLACK: i64 = 6;

#[derive(Debug, Deserialize)]
pub(super) struct AlpacaBarsPayload {
    #[serde(default)]
    pub(super) bars: Option<Vec<AlpacaBarPayload>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AlpacaHistoricalBarsPayload {
    #[serde(default)]
    pub(super) bars: HashMap<String, Vec<AlpacaBarPayload>>,
    #[serde(default)]
    pub(super) next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AlpacaBarPayload {
    #[serde(rename = "t")]
    pub(super) timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "o")]
    pub(super) open: f64,
    #[serde(rename = "h")]
    pub(super) high: f64,
    #[serde(rename = "l")]
    pub(super) low: f64,
    #[serde(rename = "c")]
    pub(super) close: f64,
    #[serde(rename = "v")]
    pub(super) volume: f64,
}

pub(super) fn alpaca_timeframe_label(timeframe: Timeframe) -> &'static str {
    match timeframe {
        Timeframe::M1 => "1Min",
        Timeframe::M5 => "5Min",
        Timeframe::M15 => "15Min",
        Timeframe::M30 => "30Min",
        Timeframe::H1 => "1Hour",
        Timeframe::H4 => "4Hour",
        Timeframe::D1 => "1Day",
    }
}

pub(super) fn alpaca_recent_bars_lookback_start(
    now: chrono::DateTime<chrono::Utc>,
    timeframe: Timeframe,
    limit: usize,
) -> String {
    let requested_bars = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    // Guard against a zero or sub-second timeframe duration: without a floor a
    // tiny duration collapses `lookback_seconds` toward zero and the lookback
    // window would degenerate. A minimum of one second per bar keeps the
    // computed window monotonic in `limit` before the day clamp applies.
    let seconds_per_bar = i64::try_from(timeframe.duration().as_secs())
        .unwrap_or(i64::MAX)
        .max(1);
    let lookback_seconds = seconds_per_bar
        .saturating_mul(requested_bars)
        .saturating_mul(ALPACA_RECENT_BARS_LOOKBACK_SLACK);
    let lookback_days = (lookback_seconds / 86_400).clamp(
        ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS,
        ALPACA_RECENT_BARS_LOOKBACK_MAX_DAYS,
    );

    (now - chrono::Duration::days(lookback_days)).to_rfc3339()
}

pub(super) fn historical_alpaca_bars_for_symbol(
    mut bars: HashMap<String, Vec<AlpacaBarPayload>>,
    symbol: &str,
) -> Vec<AlpacaBarPayload> {
    bars.remove(symbol).unwrap_or_default()
}

fn alpaca_bar_is_confirmed(
    timestamp: chrono::DateTime<chrono::Utc>,
    timeframe: Timeframe,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    match chrono::Duration::from_std(timeframe.duration()) {
        Ok(duration) => timestamp + duration <= now,
        Err(_) => false,
    }
}

pub(super) fn normalize_recent_alpaca_bars(
    bars: Vec<AlpacaBarPayload>,
    timeframe: Timeframe,
    now: chrono::DateTime<chrono::Utc>,
    limit: usize,
) -> Vec<OhlcvBar> {
    let mut normalized = bars
        .into_iter()
        .map(|bar| OhlcvBar {
            timestamp: bar.timestamp,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .filter(|bar| alpaca_bar_is_confirmed(bar.timestamp, timeframe, now))
        .take(limit)
        .collect::<Vec<_>>();
    normalized.reverse();
    normalized
}

pub(super) fn normalize_confirmed_alpaca_range_bars(
    bars: Vec<AlpacaBarPayload>,
    timeframe: Timeframe,
    end_at: chrono::DateTime<chrono::Utc>,
    start_after: Option<chrono::DateTime<chrono::Utc>>,
    limit: usize,
) -> Vec<OhlcvBar> {
    let now = chrono::Utc::now();
    bars.into_iter()
        .map(|bar| OhlcvBar {
            timestamp: bar.timestamp,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .filter(|bar| {
            alpaca_bar_is_confirmed(bar.timestamp, timeframe, now)
                && bar.timestamp <= end_at
                && start_after.is_none_or(|start| bar.timestamp > start)
        })
        .take(limit)
        .collect()
}

pub(super) fn latest_confirmed_alpaca_bar(
    bars: Vec<AlpacaBarPayload>,
    timeframe: Timeframe,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<OhlcvBar> {
    normalize_recent_alpaca_bars(bars, timeframe, now, 1)
        .into_iter()
        .next()
}
