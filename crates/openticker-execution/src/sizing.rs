use crate::{OrderLedgerOutcome, OrderQuantityResolution};
use openticker_config::ExecutionConstraintsConfig;
use openticker_core::{MarketType, TradeIntent};
use openticker_risk::RiskLimits;

const QUANTITY_TOLERANCE: f64 = 1e-9;

#[allow(clippy::too_many_arguments)]
pub fn resolve_order_quantity_with_constraints(
    intent: TradeIntent,
    market: MarketType,
    price: f64,
    current_position_quantity: f64,
    remaining_bot_budget_usd: f64,
    remaining_account_budget_usd: f64,
    limits: RiskLimits,
    target_order_notional_usd: f64,
    execution_constraints: &ExecutionConstraintsConfig,
    fractional_equity_entry_supported: bool,
) -> OrderQuantityResolution {
    if !price.is_finite() || price <= 0.0 {
        return OrderQuantityResolution {
            quantity: 0.0,
            adjustment_reason: Some("invalid_price".to_owned()),
            ledger_outcome: None,
        };
    }

    let mut ledger_outcome = None;

    let raw_quantity = match intent {
        TradeIntent::NoOp => 0.0,
        TradeIntent::CloseLong | TradeIntent::ReduceLong => current_position_quantity.max(0.0),
        TradeIntent::OpenLong | TradeIntent::AddLong => {
            let capped_target_order_notional_usd =
                target_order_notional_usd.min(limits.max_order_notional_usd);
            if !capped_target_order_notional_usd.is_finite()
                || capped_target_order_notional_usd <= 0.0
            {
                return OrderQuantityResolution {
                    quantity: 0.0,
                    adjustment_reason: Some("invalid_target_notional".to_owned()),
                    ledger_outcome: None,
                };
            }

            let desired_notional_usd = capped_target_order_notional_usd
                .min(remaining_bot_budget_usd.max(0.0))
                .min(remaining_account_budget_usd.max(0.0));

            if !desired_notional_usd.is_finite() || desired_notional_usd <= 0.0 {
                ledger_outcome = if remaining_bot_budget_usd <= f64::EPSILON {
                    Some(OrderLedgerOutcome::BotExhausted)
                } else if remaining_account_budget_usd <= f64::EPSILON {
                    Some(OrderLedgerOutcome::AccountExhausted)
                } else {
                    None
                };
                return OrderQuantityResolution {
                    quantity: 0.0,
                    adjustment_reason: Some("no_remaining_notional_room".to_owned()),
                    ledger_outcome,
                };
            }

            desired_notional_usd / price
        }
    };

    let market_adjusted_quantity = apply_market_order_quantity_policy(
        market,
        intent,
        raw_quantity,
        fractional_equity_entry_supported,
    );
    let (constrained_quantity, constraint_reason) = apply_execution_constraints(
        intent,
        price,
        market_adjusted_quantity,
        execution_constraints,
    );

    let mut reasons = Vec::new();
    if (market_adjusted_quantity - raw_quantity).abs() > QUANTITY_TOLERANCE {
        reasons.push(format!(
            "market_policy_adjustment(from={raw_quantity},to={market_adjusted_quantity})"
        ));
    }
    if let Some(constraint_reason) = constraint_reason {
        reasons.push(constraint_reason);
    }
    if matches!(intent, TradeIntent::OpenLong | TradeIntent::AddLong)
        && constrained_quantity <= QUANTITY_TOLERANCE
        && raw_quantity > QUANTITY_TOLERANCE
    {
        ledger_outcome = Some(OrderLedgerOutcome::Dust);
    }

    OrderQuantityResolution {
        quantity: constrained_quantity,
        adjustment_reason: if reasons.is_empty() {
            None
        } else {
            Some(reasons.join(","))
        },
        ledger_outcome,
    }
}

fn apply_market_order_quantity_policy(
    market: MarketType,
    intent: TradeIntent,
    quantity: f64,
    fractional_equity_entry_supported: bool,
) -> f64 {
    if !quantity.is_finite() || quantity <= QUANTITY_TOLERANCE {
        return 0.0;
    }

    let normalized = match market {
        MarketType::Crypto => quantity,
        MarketType::Equities => match intent {
            TradeIntent::OpenLong | TradeIntent::AddLong => {
                if fractional_equity_entry_supported {
                    quantity
                } else {
                    quantity.floor()
                }
            }
            TradeIntent::CloseLong | TradeIntent::ReduceLong => {
                if quantity >= 1.0 {
                    quantity.floor()
                } else {
                    quantity
                }
            }
            TradeIntent::NoOp => 0.0,
        },
    };

    if normalized <= QUANTITY_TOLERANCE {
        0.0
    } else {
        normalized
    }
}

fn floor_to_step(value: f64, step: f64) -> f64 {
    if !value.is_finite() || !step.is_finite() || step <= 0.0 {
        return 0.0;
    }

    let step_units = ((value / step) + QUANTITY_TOLERANCE).floor();
    if !step_units.is_finite() || step_units <= 0.0 {
        return 0.0;
    }

    let floored = step_units * step;
    if !floored.is_finite() {
        return 0.0;
    }

    floored.min(value)
}

fn apply_execution_constraints(
    intent: TradeIntent,
    price: f64,
    quantity: f64,
    execution_constraints: &ExecutionConstraintsConfig,
) -> (f64, Option<String>) {
    if quantity <= QUANTITY_TOLERANCE {
        return (0.0, None);
    }

    let mut adjusted_quantity = quantity;
    let mut reasons = Vec::new();

    if let Some(quantity_step) = execution_constraints.quantity_step {
        let stepped_quantity = floor_to_step(adjusted_quantity, quantity_step);
        if (stepped_quantity - adjusted_quantity).abs() > QUANTITY_TOLERANCE {
            reasons.push(format!(
                "step_floor(step={quantity_step},from={adjusted_quantity},to={stepped_quantity})"
            ));
        }
        adjusted_quantity = stepped_quantity;
    }

    if adjusted_quantity <= QUANTITY_TOLERANCE {
        if matches!(intent, TradeIntent::CloseLong | TradeIntent::ReduceLong) {
            reasons.push("residual_close_untradable".to_owned());
            reasons.push("quantity_zeroed_by_constraints".to_owned());
            return (0.0, Some(reasons.join(",")));
        }

        reasons.push("quantity_zeroed_by_constraints".to_owned());
        return (0.0, Some(reasons.join(",")));
    }

    let min_quantity_violated = execution_constraints
        .min_quantity
        .is_some_and(|min| adjusted_quantity < min);
    let notional = position_notional_usd(adjusted_quantity, price);
    let min_notional_violated = execution_constraints
        .min_notional_usd
        .is_some_and(|min| notional < min);

    if !min_quantity_violated && !min_notional_violated {
        return if reasons.is_empty() {
            (adjusted_quantity, None)
        } else {
            (adjusted_quantity, Some(reasons.join(",")))
        };
    }

    if matches!(intent, TradeIntent::CloseLong | TradeIntent::ReduceLong) {
        if min_quantity_violated {
            reasons.push("below_min_quantity".to_owned());
            reasons.push("residual_close_untradable".to_owned());
            reasons.push("quantity_zeroed_by_constraints".to_owned());
            return (0.0, Some(reasons.join(",")));
        }
        if min_notional_violated {
            reasons.push("below_min_notional_usd".to_owned());
            reasons.push("close_min_notional_bypass".to_owned());
            return (adjusted_quantity, Some(reasons.join(",")));
        }
    }

    if min_quantity_violated {
        reasons.push("below_min_quantity".to_owned());
    }
    if min_notional_violated {
        reasons.push("below_min_notional_usd".to_owned());
    }
    reasons.push("quantity_zeroed_by_constraints".to_owned());
    (0.0, Some(reasons.join(",")))
}

fn position_notional_usd(quantity: f64, price_usd: f64) -> f64 {
    if quantity <= QUANTITY_TOLERANCE || !quantity.is_finite() || !price_usd.is_finite() {
        0.0
    } else {
        quantity * price_usd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn resolve_order_quantity_applies_recommended_market_rounding_policy() {
        let constraints = ExecutionConstraintsConfig::default();

        let equities_entry = resolve_order_quantity_with_constraints(
            TradeIntent::OpenLong,
            MarketType::Equities,
            100.0,
            0.0,
            10_000.0,
            10_000.0,
            limits(),
            250.0,
            &constraints,
            false,
        );
        assert!((equities_entry.quantity - 2.0).abs() < QUANTITY_TOLERANCE);

        let fractionable_equities_entry = resolve_order_quantity_with_constraints(
            TradeIntent::OpenLong,
            MarketType::Equities,
            100.0,
            0.0,
            10_000.0,
            10_000.0,
            limits(),
            250.0,
            &constraints,
            true,
        );
        assert!((fractionable_equities_entry.quantity - 2.5).abs() < QUANTITY_TOLERANCE);

        let equities_close = resolve_order_quantity_with_constraints(
            TradeIntent::CloseLong,
            MarketType::Equities,
            100.0,
            3.75,
            375.0,
            375.0,
            limits(),
            250.0,
            &constraints,
            false,
        );
        assert!((equities_close.quantity - 3.0).abs() < QUANTITY_TOLERANCE);

        let equities_legacy_fractional_close = resolve_order_quantity_with_constraints(
            TradeIntent::CloseLong,
            MarketType::Equities,
            100.0,
            0.75,
            75.0,
            75.0,
            limits(),
            250.0,
            &constraints,
            false,
        );
        assert!((equities_legacy_fractional_close.quantity - 0.75).abs() < QUANTITY_TOLERANCE);

        let crypto_entry = resolve_order_quantity_with_constraints(
            TradeIntent::OpenLong,
            MarketType::Crypto,
            100.0,
            0.0,
            10_000.0,
            10_000.0,
            limits(),
            250.0,
            &constraints,
            false,
        );
        assert!((crypto_entry.quantity - 2.5).abs() < QUANTITY_TOLERANCE);
    }

    #[test]
    fn resolve_order_quantity_applies_execution_constraints() {
        let constraints = ExecutionConstraintsConfig {
            quantity_step: Some(0.1),
            min_quantity: Some(1.0),
            min_notional_usd: Some(100.0),
        };

        let blocked_entry = resolve_order_quantity_with_constraints(
            TradeIntent::OpenLong,
            MarketType::Crypto,
            100.0,
            0.0,
            95.0,
            95.0,
            limits(),
            95.0,
            &constraints,
            false,
        );
        assert!((blocked_entry.quantity - 0.0).abs() < f64::EPSILON);
        assert!(
            blocked_entry
                .adjustment_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("quantity_zeroed_by_constraints"))
        );

        let residual_close = resolve_order_quantity_with_constraints(
            TradeIntent::CloseLong,
            MarketType::Crypto,
            100.0,
            0.95,
            95.0,
            95.0,
            limits(),
            95.0,
            &constraints,
            false,
        );
        assert!((residual_close.quantity - 0.0).abs() < QUANTITY_TOLERANCE);
        assert!(
            residual_close
                .adjustment_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("residual_close_untradable"))
        );
    }

    #[test]
    fn close_quantity_uses_stepped_amount_when_only_min_notional_is_violated() {
        let constraints = ExecutionConstraintsConfig {
            quantity_step: Some(0.00001),
            min_quantity: Some(0.00001),
            min_notional_usd: Some(5.0),
        };

        let residual_close = resolve_order_quantity_with_constraints(
            TradeIntent::CloseLong,
            MarketType::Crypto,
            0.133_409_05,
            0.000_149_85,
            1_000.0,
            1_000.0,
            limits(),
            100.0,
            &constraints,
            false,
        );
        assert!((residual_close.quantity - 0.00014).abs() < QUANTITY_TOLERANCE);
        assert!(
            residual_close
                .adjustment_reason
                .as_deref()
                .is_some_and(|reason| {
                    reason.contains("step_floor(step=0.00001")
                        && reason.contains("below_min_notional_usd")
                        && reason.contains("close_min_notional_bypass")
                })
        );
    }
}

#[cfg(kani)]
mod proofs {
    use super::resolve_order_quantity_with_constraints;
    use crate::OrderQuantityResolution;
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
            positive_value(kani::any()),
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
}
