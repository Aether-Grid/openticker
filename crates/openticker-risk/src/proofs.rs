use crate::policy::{BasicRiskPolicy, RiskPolicy};
use crate::types::{RiskContext, RiskDecision, RiskLimits};
use openticker_core::TradeIntent;

fn sample_limits() -> RiskLimits {
    RiskLimits {
        max_daily_loss_pct: 3.0,
        max_open_positions: 2,
        max_order_notional_usd: 750.0,
        max_spread_bps: 20,
        max_slippage_bps: 25,
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

fn opening_intent(selector: u8) -> TradeIntent {
    if selector % 2 == 0 {
        TradeIntent::OpenLong
    } else {
        TradeIntent::AddLong
    }
}

fn non_opening_intent(selector: u8) -> TradeIntent {
    match selector % 3 {
        0 => TradeIntent::NoOp,
        1 => TradeIntent::ReduceLong,
        _ => TradeIntent::CloseLong,
    }
}

fn positive_price(selector: u8) -> f64 {
    f64::from((selector % 20) + 1)
}

fn positive_quantity(selector: u8) -> f64 {
    f64::from((selector % 16) + 1)
}

fn invalid_number(selector: u8) -> f64 {
    match selector % 4 {
        0 => 0.0,
        1 => -1.0,
        2 => f64::INFINITY,
        _ => f64::NAN,
    }
}

#[kani::proof]
fn proof_kill_switch_always_rejects() {
    let policy = BasicRiskPolicy {
        limits: sample_limits(),
        kill_switch_active: true,
    };
    let context = RiskContext {
        intent: any_intent(kani::any()),
        price: invalid_number(kani::any()),
        quantity: invalid_number(kani::any()),
        stale_data: kani::any(),
        account_open_positions: u32::from(kani::any::<u8>() % 5),
        account_daily_loss_pct: f64::from(kani::any::<u8>() % 10),
        observed_spread_bps: u32::from(kani::any::<u8>()),
        estimated_slippage_bps: u32::from(kani::any::<u8>()),
        cooldown_active: kani::any(),
    };

    assert!(matches!(
        policy.evaluate(context),
        RiskDecision::Reject { .. }
    ));
}

#[kani::proof]
fn proof_executable_intents_require_positive_finite_price() {
    let policy = BasicRiskPolicy {
        limits: sample_limits(),
        kill_switch_active: false,
    };
    let context = RiskContext {
        intent: opening_intent(kani::any()),
        price: invalid_number(kani::any()),
        quantity: positive_quantity(kani::any()),
        stale_data: false,
        account_open_positions: 0,
        account_daily_loss_pct: 0.0,
        observed_spread_bps: 0,
        estimated_slippage_bps: 0,
        cooldown_active: false,
    };

    assert!(matches!(
        policy.evaluate(context),
        RiskDecision::Reject { .. }
    ));
}

#[kani::proof]
fn proof_executable_intents_require_positive_finite_quantity() {
    let policy = BasicRiskPolicy {
        limits: sample_limits(),
        kill_switch_active: false,
    };
    let context = RiskContext {
        intent: any_intent(kani::any::<u8>() % 4 + 1),
        price: positive_price(kani::any()),
        quantity: invalid_number(kani::any()),
        stale_data: false,
        account_open_positions: 0,
        account_daily_loss_pct: 0.0,
        observed_spread_bps: 0,
        estimated_slippage_bps: 0,
        cooldown_active: false,
    };

    assert!(matches!(
        policy.evaluate(context),
        RiskDecision::Reject { .. }
    ));
}

#[kani::proof]
fn proof_non_opening_intents_bypass_open_only_guards() {
    let policy = BasicRiskPolicy {
        limits: sample_limits(),
        kill_switch_active: false,
    };
    let intent = non_opening_intent(kani::any());
    let decision = policy.evaluate(RiskContext {
        intent,
        price: positive_price(kani::any()),
        quantity: positive_quantity(kani::any()),
        stale_data: true,
        account_open_positions: sample_limits().max_open_positions,
        account_daily_loss_pct: sample_limits().max_daily_loss_pct,
        observed_spread_bps: sample_limits().max_spread_bps + 1,
        estimated_slippage_bps: sample_limits().max_slippage_bps + 1,
        cooldown_active: true,
    });

    assert!(matches!(decision, RiskDecision::Allow(allowed) if allowed == intent));
}
