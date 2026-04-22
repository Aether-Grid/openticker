use chrono::{DateTime, Duration, Utc};
use openticker_core::{OhlcvBar, SignalPhase, Timeframe};

use crate::error::DataError;
use crate::normalized::{NormalizedBarUpdate, NormalizedTrade};

#[derive(Debug, Clone)]
pub struct BarBuilder {
    symbol: String,
    timeframe: Timeframe,
    current_start: Option<DateTime<Utc>>,
    current_bar: Option<OhlcvBar>,
}

impl BarBuilder {
    #[must_use]
    pub fn new(symbol: impl Into<String>, timeframe: Timeframe) -> Self {
        Self {
            symbol: symbol.into(),
            timeframe,
            current_start: None,
            current_bar: None,
        }
    }

    /// Ingests a normalized trade and returns zero, one, or two bar updates.
    ///
    /// # Errors
    ///
    /// Returns [`DataError::InvalidPrice`] when `trade.price <= 0.0`.
    /// Returns [`DataError::InvalidQuantity`] when `trade.quantity < 0.0`.
    pub fn push_trade(
        &mut self,
        trade: &NormalizedTrade,
    ) -> Result<Vec<NormalizedBarUpdate>, DataError> {
        if trade.price <= 0.0 {
            return Err(DataError::InvalidPrice);
        }
        if trade.quantity < 0.0 {
            return Err(DataError::InvalidQuantity);
        }

        let bucket_start = floor_to_timeframe(trade.timestamp, self.timeframe);
        let mut updates = Vec::new();

        if self.current_start == Some(bucket_start) {
            if let Some(bar) = self.current_bar.as_mut() {
                apply_trade_to_bar(bar, trade);
                updates.push(NormalizedBarUpdate {
                    symbol: self.symbol.clone(),
                    phase: SignalPhase::Preview,
                    bar: bar.clone(),
                });
            } else {
                let bar = new_bar_from_trade(bucket_start, trade);
                updates.push(NormalizedBarUpdate {
                    symbol: self.symbol.clone(),
                    phase: SignalPhase::Preview,
                    bar: bar.clone(),
                });
                self.current_bar = Some(bar);
            }
            return Ok(updates);
        }

        if self.current_start.is_some()
            && let Some(finalized) = self.current_bar.take()
        {
            updates.push(NormalizedBarUpdate {
                symbol: self.symbol.clone(),
                phase: SignalPhase::Confirmed,
                bar: finalized,
            });
        }

        let bar = new_bar_from_trade(bucket_start, trade);
        updates.push(NormalizedBarUpdate {
            symbol: self.symbol.clone(),
            phase: SignalPhase::Preview,
            bar: bar.clone(),
        });
        self.current_start = Some(bucket_start);
        self.current_bar = Some(bar);

        Ok(updates)
    }

    pub fn flush_confirmed(&mut self) -> Option<NormalizedBarUpdate> {
        let bar = self.current_bar.take()?;
        self.current_start = None;
        Some(NormalizedBarUpdate {
            symbol: self.symbol.clone(),
            phase: SignalPhase::Confirmed,
            bar,
        })
    }
}

fn new_bar_from_trade(bucket_start: DateTime<Utc>, trade: &NormalizedTrade) -> OhlcvBar {
    OhlcvBar {
        timestamp: bucket_start,
        open: trade.price,
        high: trade.price,
        low: trade.price,
        close: trade.price,
        volume: trade.quantity,
    }
}

fn apply_trade_to_bar(bar: &mut OhlcvBar, trade: &NormalizedTrade) {
    bar.high = bar.high.max(trade.price);
    bar.low = bar.low.min(trade.price);
    bar.close = trade.price;
    bar.volume += trade.quantity;
}

fn floor_to_timeframe(timestamp: DateTime<Utc>, timeframe: Timeframe) -> DateTime<Utc> {
    let timeframe_seconds = timeframe.duration().as_secs().cast_signed();
    let unix_seconds = timestamp.timestamp();
    let floored_unix = unix_seconds - (unix_seconds.rem_euclid(timeframe_seconds));
    DateTime::<Utc>::from_timestamp(floored_unix, 0)
        .unwrap_or(timestamp - Duration::seconds(unix_seconds.rem_euclid(timeframe_seconds)))
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use openticker_core::{SignalPhase, Timeframe};

    use crate::bar_builder::BarBuilder;
    use crate::error::DataError;
    use crate::normalized::NormalizedTrade;

    fn assert_float_eq(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "left={left}, right={right}");
    }

    fn trade(symbol: &str, timestamp: &str, price: f64, quantity: f64) -> NormalizedTrade {
        NormalizedTrade {
            symbol: symbol.to_owned(),
            timestamp: DateTime::parse_from_rfc3339(timestamp)
                .unwrap()
                .with_timezone(&Utc),
            price,
            quantity,
        }
    }

    fn timestamp(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn push_trade_rejects_non_positive_price() {
        let mut builder = BarBuilder::new("AAPL", Timeframe::M1);

        let err = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:05Z", 0.0, 1.0))
            .unwrap_err();
        assert!(matches!(err, DataError::InvalidPrice));

        let err = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:06Z", -1.0, 1.0))
            .unwrap_err();
        assert!(matches!(err, DataError::InvalidPrice));

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:07Z", 100.0, 1.0))
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].phase, SignalPhase::Preview);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn push_trade_rejects_negative_quantity() {
        let mut builder = BarBuilder::new("AAPL", Timeframe::M1);

        let err = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:05Z", 100.0, -0.01))
            .unwrap_err();
        assert!(matches!(err, DataError::InvalidQuantity));

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:07Z", 100.0, 1.0))
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].phase, SignalPhase::Preview);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn timeframe_flooring_is_exact_at_minute_boundaries() {
        let mut builder = BarBuilder::new("AAPL", Timeframe::M1);

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:01:00Z", 100.0, 1.0))
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:01:00Z"));

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:01:59Z", 101.0, 1.0))
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].phase, SignalPhase::Preview);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:01:00Z"));

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:02:00Z", 102.0, 1.0))
            .unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].phase, SignalPhase::Confirmed);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:01:00Z"));
        assert_eq!(updates[1].phase, SignalPhase::Preview);
        assert_eq!(updates[1].bar.timestamp, timestamp("2026-01-01T00:02:00Z"));
    }

    #[test]
    fn same_bucket_updates_emit_preview_only() {
        let mut builder = BarBuilder::new("AAPL", Timeframe::M1);

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:05Z", 100.0, 1.0))
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].phase, SignalPhase::Preview);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:00:00Z"));

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:40Z", 101.5, 2.0))
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].phase, SignalPhase::Preview);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:00:00Z"));
        assert_float_eq(updates[0].bar.open, 100.0);
        assert_float_eq(updates[0].bar.high, 101.5);
        assert_float_eq(updates[0].bar.low, 100.0);
        assert_float_eq(updates[0].bar.close, 101.5);
        assert_float_eq(updates[0].bar.volume, 3.0);
    }

    #[test]
    fn bucket_rollover_emits_confirmed_before_next_preview() {
        let mut builder = BarBuilder::new("AAPL", Timeframe::M1);

        builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:05Z", 100.0, 1.0))
            .unwrap();

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:01:00Z", 99.0, 1.0))
            .unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].phase, SignalPhase::Confirmed);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:00:00Z"));
        assert_float_eq(updates[0].bar.close, 100.0);
        assert_eq!(updates[1].phase, SignalPhase::Preview);
        assert_eq!(updates[1].bar.timestamp, timestamp("2026-01-01T00:01:00Z"));
        assert_float_eq(updates[1].bar.open, 99.0);
    }

    #[test]
    fn flush_confirmed_is_one_shot() {
        let mut builder = BarBuilder::new("AAPL", Timeframe::M1);

        builder
            .push_trade(&trade("AAPL", "2026-01-01T00:00:05Z", 100.0, 1.0))
            .unwrap();

        let confirmed = builder.flush_confirmed().expect("flush should emit bar");
        assert_eq!(confirmed.phase, SignalPhase::Confirmed);
        assert_eq!(confirmed.bar.timestamp, timestamp("2026-01-01T00:00:00Z"));
        assert_float_eq(confirmed.bar.close, 100.0);

        assert!(builder.flush_confirmed().is_none());

        let updates = builder
            .push_trade(&trade("AAPL", "2026-01-01T00:01:05Z", 101.0, 1.0))
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].phase, SignalPhase::Preview);
        assert_eq!(updates[0].bar.timestamp, timestamp("2026-01-01T00:01:00Z"));
    }
}
