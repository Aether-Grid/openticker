use chrono::{DateTime, Utc};
use openticker_core::TradeIntent;
use openticker_execution::{
    ExecutionError, ExecutionRequest, ExecutionRouter, PaperExecutionRouter, stable_client_order_id,
};

fn fixed_timestamp() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn request(intent: TradeIntent) -> ExecutionRequest {
    ExecutionRequest {
        instance_id: "bot-a".to_owned(),
        symbol: "AAPL".to_owned(),
        timestamp: fixed_timestamp(),
        intent,
        price: 123.45,
        quantity: 1.0,
    }
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

#[test]
fn paper_router_rejects_non_executable_intent() {
    let router = PaperExecutionRouter;
    let request = request(TradeIntent::NoOp);

    assert!(matches!(
        router.submit(&request),
        Err(ExecutionError::NonExecutableIntent)
    ));
}

#[test]
fn paper_router_rejects_non_positive_price() {
    let router = PaperExecutionRouter;
    let mut request = request(TradeIntent::OpenLong);
    request.price = 0.0;

    assert!(matches!(
        router.submit(&request),
        Err(ExecutionError::InvalidPrice)
    ));
}
