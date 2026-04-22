use openticker_core::TradeIntent;

use super::*;

fn baseline_policy() -> BasicRiskPolicy {
    BasicRiskPolicy {
        limits: RiskLimits {
            max_daily_loss_pct: 3.0,
            max_open_positions: 2,
            max_order_notional_usd: 750.0,
            max_spread_bps: 20,
            max_slippage_bps: 25,
            stale_data_ms: 3_000,
            cooldown_after_reject_ms: 1_000,
        },
        kill_switch_active: false,
    }
}

fn baseline_context(intent: TradeIntent) -> RiskContext {
    RiskContext {
        intent,
        price: 100.0,
        quantity: 1.0,
        stale_data: false,
        account_open_positions: 1,
        account_daily_loss_pct: 0.5,
        observed_spread_bps: 5,
        estimated_slippage_bps: 8,
        cooldown_active: false,
    }
}

#[test]
fn rejects_over_notional_limit() {
    let mut policy = baseline_policy();
    policy.limits.max_order_notional_usd = 100.0;

    let mut context = baseline_context(TradeIntent::OpenLong);
    context.quantity = 2.0;

    let decision = policy.evaluate(context);
    assert!(matches!(
        decision,
        RiskDecision::Reject {
            reason: "order notional exceeds max"
        }
    ));
}

#[test]
fn rejects_when_open_positions_limit_is_reached() {
    let policy = baseline_policy();
    let mut context = baseline_context(TradeIntent::OpenLong);
    context.account_open_positions = 2;

    let decision = policy.evaluate(context);
    assert!(matches!(
        decision,
        RiskDecision::Reject {
            reason: "open positions exceeds max"
        }
    ));
}

#[test]
fn rejects_when_daily_loss_limit_is_reached() {
    let policy = baseline_policy();
    let mut context = baseline_context(TradeIntent::OpenLong);
    context.account_daily_loss_pct = 3.0;

    let decision = policy.evaluate(context);
    assert!(matches!(
        decision,
        RiskDecision::Reject {
            reason: "daily loss exceeds max"
        }
    ));
}

#[test]
fn spread_limit_allows_equal_value_and_rejects_above_limit() {
    let policy = baseline_policy();

    let mut at_limit = baseline_context(TradeIntent::OpenLong);
    at_limit.observed_spread_bps = policy.limits.max_spread_bps;
    assert!(matches!(
        policy.evaluate(at_limit),
        RiskDecision::Allow(TradeIntent::OpenLong)
    ));

    let mut above_limit = baseline_context(TradeIntent::OpenLong);
    above_limit.observed_spread_bps = policy.limits.max_spread_bps + 1;
    assert!(matches!(
        policy.evaluate(above_limit),
        RiskDecision::Reject {
            reason: "spread exceeds max"
        }
    ));
}

#[test]
fn rejects_when_spread_or_slippage_limits_are_exceeded() {
    let policy = baseline_policy();
    let mut spread_context = baseline_context(TradeIntent::OpenLong);
    spread_context.observed_spread_bps = 30;
    let spread_decision = policy.evaluate(spread_context);
    assert!(matches!(
        spread_decision,
        RiskDecision::Reject {
            reason: "spread exceeds max"
        }
    ));

    let mut slippage_context = baseline_context(TradeIntent::OpenLong);
    slippage_context.estimated_slippage_bps = 30;
    let slippage_decision = policy.evaluate(slippage_context);
    assert!(matches!(
        slippage_decision,
        RiskDecision::Reject {
            reason: "slippage exceeds max"
        }
    ));
}

#[test]
fn rejects_when_cooldown_is_active_or_data_is_stale() {
    let policy = baseline_policy();
    let mut cooldown_context = baseline_context(TradeIntent::OpenLong);
    cooldown_context.cooldown_active = true;
    let cooldown_decision = policy.evaluate(cooldown_context);
    assert!(matches!(
        cooldown_decision,
        RiskDecision::Reject {
            reason: "cooldown active"
        }
    ));

    let mut stale_context = baseline_context(TradeIntent::OpenLong);
    stale_context.stale_data = true;
    let stale_decision = policy.evaluate(stale_context);
    assert!(matches!(
        stale_decision,
        RiskDecision::Reject {
            reason: "stale market data"
        }
    ));
}

#[test]
fn allows_non_open_intents_when_kill_switch_is_off() {
    let policy = baseline_policy();

    let close = policy.evaluate(baseline_context(TradeIntent::CloseLong));
    assert!(matches!(close, RiskDecision::Allow(TradeIntent::CloseLong)));

    let noop = policy.evaluate(baseline_context(TradeIntent::NoOp));
    assert!(matches!(noop, RiskDecision::Allow(TradeIntent::NoOp)));
}

#[test]
fn applies_open_long_guards_to_add_long() {
    let mut policy = baseline_policy();
    policy.limits.max_order_notional_usd = 100.0;

    let mut context = baseline_context(TradeIntent::AddLong);
    context.quantity = 2.0;

    let decision = policy.evaluate(context);
    assert!(matches!(
        decision,
        RiskDecision::Reject {
            reason: "order notional exceeds max"
        }
    ));
}

#[test]
fn rejects_non_noop_intents_with_non_positive_quantity() {
    let policy = baseline_policy();
    let mut context = baseline_context(TradeIntent::CloseLong);
    context.quantity = 0.0;

    let decision = policy.evaluate(context);
    assert!(matches!(
        decision,
        RiskDecision::Reject {
            reason: "order quantity must be positive"
        }
    ));
}

#[test]
fn close_long_bypasses_open_only_gates_but_still_requires_valid_quantity() {
    let policy = baseline_policy();
    let mut context = baseline_context(TradeIntent::CloseLong);
    context.cooldown_active = true;
    context.stale_data = true;
    context.account_daily_loss_pct = 10.0;
    context.observed_spread_bps = 100;
    context.estimated_slippage_bps = 100;

    assert!(matches!(
        policy.evaluate(context),
        RiskDecision::Allow(TradeIntent::CloseLong)
    ));

    let mut invalid = baseline_context(TradeIntent::CloseLong);
    invalid.quantity = 0.0;
    assert!(matches!(
        policy.evaluate(invalid),
        RiskDecision::Reject {
            reason: "order quantity must be positive"
        }
    ));
}

#[test]
fn kill_switch_rejects_before_other_checks() {
    let mut policy = baseline_policy();
    policy.kill_switch_active = true;

    let mut context = baseline_context(TradeIntent::CloseLong);
    context.quantity = 0.0;

    let decision = policy.evaluate(context);
    assert!(matches!(
        decision,
        RiskDecision::Reject {
            reason: "kill switch enabled"
        }
    ));
}
