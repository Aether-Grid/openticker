use crate::{ConnectorError, ConnectorKind};
use openticker_core::{OhlcvBar, Timeframe};

#[cfg(test)]
pub(super) fn parse_kline_row(row: &[serde_json::Value]) -> Result<OhlcvBar, ConnectorError> {
    parse_kline_row_with_close_time(row).map(|(bar, _)| bar)
}

/// Parses a single Binance kline row into an [`OhlcvBar`] plus its close-time
/// in epoch milliseconds.
///
/// Binance kline rows carry open time at index 0 and (for full rows) close
/// time at index 6. A well-formed REST response always includes the close
/// time, but this parser is defensive: rows with fewer than 7 fields fall back
/// to using the open time as the close time. That fallback makes such a bar
/// look like it closed at its open instant, so it is treated as already
/// confirmed by the strict `close_time_ms < now_ms` filters downstream; this
/// is the conservative choice for a truncated row where the real close time is
/// unknown. Rows with fewer than 6 fields, or any field that is not a valid
/// JSON number, are rejected with a [`ConnectorError::RemoteSnapshot`].
pub(super) fn parse_kline_row_with_close_time(
    row: &[serde_json::Value],
) -> Result<(OhlcvBar, i64), ConnectorError> {
    if row.len() < 6 {
        return Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("kline row expected at least 6 fields, got {}", row.len()),
        });
    }

    let open_time_ms = value_to_i64(&row[0], "open_time_ms")?;
    let timestamp = chrono::DateTime::from_timestamp_millis(open_time_ms).ok_or_else(|| {
        ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("kline open time `{open_time_ms}` is not a valid timestamp"),
        }
    })?;

    let close_time_ms = if row.len() >= 7 {
        value_to_i64(&row[6], "close_time_ms")?
    } else {
        open_time_ms
    };

    Ok((
        OhlcvBar {
            timestamp,
            open: value_to_f64(&row[1], "open")?,
            high: value_to_f64(&row[2], "high")?,
            low: value_to_f64(&row[3], "low")?,
            close: value_to_f64(&row[4], "close")?,
            volume: value_to_f64(&row[5], "volume")?,
        },
        close_time_ms,
    ))
}

pub(super) fn normalize_recent_binance_klines(
    rows: Vec<Vec<serde_json::Value>>,
    limit: usize,
    now_ms: i64,
) -> Result<Vec<OhlcvBar>, ConnectorError> {
    let mut bars = rows
        .into_iter()
        .filter_map(|row| match parse_kline_row_with_close_time(&row) {
            Ok((bar, close_time_ms)) if close_time_ms < now_ms => Some(Ok(bar)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if bars.len() > limit {
        bars.drain(0..bars.len() - limit);
    }

    Ok(bars)
}

pub(super) fn latest_confirmed_binance_bar(
    rows: Vec<Vec<serde_json::Value>>,
    now_ms: i64,
) -> Result<Option<OhlcvBar>, ConnectorError> {
    Ok(normalize_recent_binance_klines(rows, 1, now_ms)?
        .into_iter()
        .next())
}

fn value_to_i64(value: &serde_json::Value, field: &str) -> Result<i64, ConnectorError> {
    match value {
        serde_json::Value::Number(number) => {
            number
                .as_i64()
                .ok_or_else(|| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("field `{field}` is not an i64-compatible number"),
                })
        }
        serde_json::Value::String(raw) => {
            raw.parse::<i64>()
                .map_err(|_| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("field `{field}` is not a valid integer: `{raw}`"),
                })
        }
        _ => Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("field `{field}` must be string or number"),
        }),
    }
}

fn value_to_f64(value: &serde_json::Value, field: &str) -> Result<f64, ConnectorError> {
    match value {
        serde_json::Value::Number(number) => {
            number
                .as_f64()
                .ok_or_else(|| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("field `{field}` is not an f64-compatible number"),
                })
        }
        serde_json::Value::String(raw) => {
            raw.parse::<f64>()
                .map_err(|_| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("field `{field}` is not a valid decimal: `{raw}`"),
                })
        }
        _ => Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("field `{field}` must be string or number"),
        }),
    }
}

pub(super) fn binance_interval_label(timeframe: Timeframe) -> &'static str {
    match timeframe {
        Timeframe::M1 => "1m",
        Timeframe::M5 => "5m",
        Timeframe::M15 => "15m",
        Timeframe::M30 => "30m",
        Timeframe::H1 => "1h",
        Timeframe::H4 => "4h",
        Timeframe::D1 => "1d",
    }
}
