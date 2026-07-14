use crate::{OrderQuantityResolution, resolve_order_quantity_with_constraints};
use openticker_config::ExecutionConstraintsConfig;
use openticker_core::{MarketType, TradeIntent};
use openticker_risk::RiskLimits;

fn limits() -> RiskLimits {
    RiskLimits {
        max_daily_loss_pct: 5.0,
        max_open_positions: 5,
        max_order_notional_usd: 10_000.0,
        max_spread_bps: 20,
        max_slippage_bps: 30,
        stale_data_ms: 3_000,
        cooldown_after_reject_ms: 1_000,
    }
}

fn any_intent(selector: u8) -> TradeIntent {
    match selector % 5 {
        0 => TradeIntent::NoOp,
        1 => TradeIntent::OpenLong,
        2 => TradeIntent::AddLong,
        3 => TradeIntent::ReduceLong,
        _ => TradeIntent::CloseLong,
    }
}

fn positive_value(selector: u8) -> f64 {
    f64::from((selector % 20) + 1)
}

fn price_case(selector: u8) -> f64 {
    match selector % 4 {
        0 => 0.0,
        1 => -1.0,
        2 => f64::INFINITY,
        _ => positive_value(selector),
    }
}

fn position_case(selector: u8) -> f64 {
    match selector % 4 {
        0 => 0.0,
        1 => -positive_value(selector),
        2 => f64::NEG_INFINITY,
        _ => positive_value(selector),
    }
}

fn constraints_case(selector: u8) -> ExecutionConstraintsConfig {
    match selector % 4 {
        0 => ExecutionConstraintsConfig::default(),
        1 => ExecutionConstraintsConfig {
            quantity_step: Some(0.1),
            min_quantity: None,
            min_notional_usd: None,
        },
        2 => ExecutionConstraintsConfig {
            quantity_step: None,
            min_quantity: Some(2.0),
            min_notional_usd: None,
        },
        _ => ExecutionConstraintsConfig {
            quantity_step: Some(0.1),
            min_quantity: Some(1.0),
            min_notional_usd: Some(100.0),
        },
    }
}

#[kani::proof]
fn proof_resolve_order_quantity_never_returns_non_negative_finite_quantity() {
    let resolution: OrderQuantityResolution = resolve_order_quantity_with_constraints(
        any_intent(kani::any()),
        if kani::any() {
            MarketType::Equities
        } else {
            MarketType::Crypto
        },
        price_case(kani::any()),
        position_case(kani::any()),
        positive_value(kani::any()),
        positive_value(kani::any()),
        limits(),
        positive_value(kani::any()) * 10.0,
        &constraints_case(kani::any()),
        kani::any(),
    );

    assert!(resolution.quantity >= 0.0);
    assert!(resolution.quantity.is_finite());
}

#[kani::proof]
fn proof_entry_constraints_zero_out_invalid_entries() {
    let resolution = resolve_order_quantity_with_constraints(
        TradeIntent::OpenLong,
        MarketType::Crypto,
        100.0,
        0.0,
        95.0,
        95.0,
        limits(),
        95.0,
        &ExecutionConstraintsConfig {
            quantity_step: Some(0.1),
            min_quantity: Some(2.0),
            min_notional_usd: Some(200.0),
        },
        false,
    );

    assert_eq!(resolution.quantity, 0.0);
}

#[kani::proof]
fn proof_close_constraints_bypass_min_notional_only() {
    let min_notional_bypass = resolve_order_quantity_with_constraints(
        TradeIntent::CloseLong,
        MarketType::Crypto,
        100.0,
        0.5,
        1_000.0,
        1_000.0,
        limits(),
        100.0,
        &ExecutionConstraintsConfig {
            quantity_step: None,
            min_quantity: Some(0.1),
            min_notional_usd: Some(75.0),
        },
        false,
    );
    assert!(min_notional_bypass.quantity > 0.0);

    let min_quantity_violation = resolve_order_quantity_with_constraints(
        TradeIntent::CloseLong,
        MarketType::Crypto,
        100.0,
        0.05,
        1_000.0,
        1_000.0,
        limits(),
        100.0,
        &ExecutionConstraintsConfig {
            quantity_step: None,
            min_quantity: Some(0.1),
            min_notional_usd: None,
        },
        false,
    );
    assert_eq!(min_quantity_violation.quantity, 0.0);
}

#[kani::proof]
fn proof_close_with_negative_position_quantity_yields_zero() {
    let resolution = resolve_order_quantity_with_constraints(
        if kani::any() {
            TradeIntent::CloseLong
        } else {
            TradeIntent::ReduceLong
        },
        if kani::any() {
            MarketType::Equities
        } else {
            MarketType::Crypto
        },
        positive_value(kani::any()),
        -positive_value(kani::any()),
        positive_value(kani::any()),
        positive_value(kani::any()),
        limits(),
        positive_value(kani::any()) * 10.0,
        &constraints_case(kani::any()),
        kani::any(),
    );

    assert_eq!(resolution.quantity, 0.0);
}
