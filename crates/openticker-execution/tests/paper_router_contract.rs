use chrono::{DateTime, Utc};
use openticker_core::TradeIntent;
use openticker_execution::{
    ExecutionError, ExecutionRequest, ExecutionRouter, PaperExecutionRouter,
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
