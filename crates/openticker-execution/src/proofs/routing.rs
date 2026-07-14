use crate::{
    ExecutionError, ExecutionRequest, ExecutionRouter, OrderSide, PaperExecutionRouter,
    order_side_for_intent,
};
use chrono::{LocalResult, TimeZone, Utc};
use openticker_core::TradeIntent;

fn fixed_timestamp() -> chrono::DateTime<Utc> {
    match Utc.timestamp_opt(0, 0) {
        LocalResult::Single(timestamp) => timestamp,
        _ => unreachable!(),
    }
}

fn executable_intent(selector: u8) -> TradeIntent {
    match selector % 4 {
        0 => TradeIntent::OpenLong,
        1 => TradeIntent::AddLong,
        2 => TradeIntent::ReduceLong,
        _ => TradeIntent::CloseLong,
    }
}

fn positive_value(selector: u8) -> f64 {
    f64::from((selector % 20) + 1)
}

fn invalid_value(selector: u8) -> f64 {
    match selector % 4 {
        0 => 0.0,
        1 => -1.0,
        2 => f64::INFINITY,
        _ => f64::NAN,
    }
}

#[kani::proof]
fn proof_noop_is_never_executable() {
    assert!(matches!(
        order_side_for_intent(TradeIntent::NoOp),
        Err(ExecutionError::NonExecutableIntent)
    ));
}

#[kani::proof]
fn proof_order_side_mapping_matches_long_only_semantics() {
    let intent = executable_intent(kani::any());
    let side = order_side_for_intent(intent);

    match intent {
        TradeIntent::OpenLong | TradeIntent::AddLong => {
            assert!(matches!(side, Ok(OrderSide::Buy)));
        }
        TradeIntent::ReduceLong | TradeIntent::CloseLong => {
            assert!(matches!(side, Ok(OrderSide::Sell)));
        }
        TradeIntent::NoOp => unreachable!(),
    }
}

#[kani::proof]
fn proof_paper_router_never_accepts_invalid_inputs() {
    let router = PaperExecutionRouter;
    let request = ExecutionRequest {
        instance_id: "bot-1".to_owned(),
        symbol: "AAPL".to_owned(),
        timestamp: fixed_timestamp(),
        intent: executable_intent(kani::any()),
        price: invalid_value(kani::any()),
        quantity: positive_value(kani::any()),
    };
    assert!(router.submit(&request).is_err());

    let request = ExecutionRequest {
        quantity: invalid_value(kani::any()),
        price: positive_value(kani::any()),
        ..request
    };
    assert!(router.submit(&request).is_err());
}

#[kani::proof]
fn proof_paper_router_accepted_orders_match_request_and_intent() {
    let router = PaperExecutionRouter;
    let request = ExecutionRequest {
        instance_id: "bot-1".to_owned(),
        symbol: "AAPL".to_owned(),
        timestamp: fixed_timestamp(),
        intent: executable_intent(kani::any()),
        price: positive_value(kani::any()),
        quantity: positive_value(kani::any()),
    };

    let accepted = router.submit(&request);
    assert!(accepted.is_ok());
    let accepted = match accepted {
        Ok(accepted) => accepted,
        Err(_) => unreachable!(),
    };

    assert_eq!(accepted.price, request.price);
    assert_eq!(accepted.quantity, request.quantity);
    assert_eq!(
        accepted.side,
        order_side_for_intent(request.intent).unwrap_or(OrderSide::Buy)
    );
}
