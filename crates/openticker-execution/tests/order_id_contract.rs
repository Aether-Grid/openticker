use chrono::{DateTime, Utc};
use openticker_core::TradeIntent;
use openticker_execution::stable_client_order_id;

fn fixed_timestamp() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn same_timestamp_same_intent_different_instance_produces_distinct_client_order_ids() {
    let timestamp = fixed_timestamp();
    let left = stable_client_order_id("bot-a", "AAPL", timestamp, TradeIntent::OpenLong);
    let right = stable_client_order_id("bot-b", "AAPL", timestamp, TradeIntent::OpenLong);

    assert_ne!(left, right);
}

#[test]
fn same_timestamp_same_intent_different_symbol_produces_distinct_client_order_ids() {
    let timestamp = fixed_timestamp();
    let left = stable_client_order_id("bot-a", "AAPL", timestamp, TradeIntent::OpenLong);
    let right = stable_client_order_id("bot-a", "MSFT", timestamp, TradeIntent::OpenLong);

    assert_ne!(left, right);
}

#[test]
fn same_timestamp_same_instance_different_intent_produces_distinct_client_order_ids() {
    let timestamp = fixed_timestamp();
    let left = stable_client_order_id("bot-a", "AAPL", timestamp, TradeIntent::OpenLong);
    let right = stable_client_order_id("bot-a", "AAPL", timestamp, TradeIntent::CloseLong);

    assert_ne!(left, right);
}
