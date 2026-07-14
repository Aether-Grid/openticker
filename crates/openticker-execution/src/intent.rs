use chrono::{DateTime, Utc};
use openticker_core::TradeIntent;

use crate::{ExecutionError, OrderSide};

pub fn stable_client_order_id(
    instance_id: &str,
    symbol: &str,
    timestamp: DateTime<Utc>,
    intent: TradeIntent,
) -> String {
    format!(
        "{}-{}-{}-{}",
        instance_id,
        symbol,
        timestamp.timestamp_millis(),
        trade_intent_label(intent)
    )
}

/// Maps a trade intent into an executable order side.
///
/// # Errors
///
/// Returns [`ExecutionError::NonExecutableIntent`] when `intent` is [`TradeIntent::NoOp`].
pub fn order_side_for_intent(intent: TradeIntent) -> Result<OrderSide, ExecutionError> {
    match intent {
        TradeIntent::OpenLong | TradeIntent::AddLong => Ok(OrderSide::Buy),
        TradeIntent::ReduceLong | TradeIntent::CloseLong => Ok(OrderSide::Sell),
        TradeIntent::NoOp => Err(ExecutionError::NonExecutableIntent),
    }
}

fn trade_intent_label(intent: TradeIntent) -> &'static str {
    match intent {
        TradeIntent::NoOp => "no_op",
        TradeIntent::OpenLong => "open_long",
        TradeIntent::AddLong => "add_long",
        TradeIntent::ReduceLong => "reduce_long",
        TradeIntent::CloseLong => "close_long",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_order_id_is_deterministic() {
        let timestamp = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let id_a = stable_client_order_id("aapl", "AAPL", timestamp, TradeIntent::OpenLong);
        let id_b = stable_client_order_id("aapl", "AAPL", timestamp, TradeIntent::OpenLong);
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn maps_intent_to_expected_order_side() {
        assert_eq!(
            order_side_for_intent(TradeIntent::OpenLong).unwrap(),
            OrderSide::Buy
        );
        assert_eq!(
            order_side_for_intent(TradeIntent::CloseLong).unwrap(),
            OrderSide::Sell
        );
    }

    #[test]
    fn rejects_non_executable_noop_intent() {
        assert!(matches!(
            order_side_for_intent(TradeIntent::NoOp),
            Err(ExecutionError::NonExecutableIntent)
        ));
    }
}
