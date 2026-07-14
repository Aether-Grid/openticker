use openticker_core::TradeIntent;

use crate::types::{RiskContext, RiskDecision, RiskLimits};

pub trait RiskPolicy {
    fn evaluate(&self, context: RiskContext) -> RiskDecision;
}

#[derive(Debug, Clone, Copy)]
pub struct BasicRiskPolicy {
    pub limits: RiskLimits,
    pub kill_switch_active: bool,
}

impl RiskPolicy for BasicRiskPolicy {
    fn evaluate(&self, context: RiskContext) -> RiskDecision {
        if self.kill_switch_active {
            return RiskDecision::Reject {
                reason: "kill switch enabled",
            };
        }

        if context.intent != TradeIntent::NoOp
            && (!context.price.is_finite()
                || context.price <= 0.0
                || !context.quantity.is_finite()
                || context.quantity <= 0.0)
        {
            return RiskDecision::Reject {
                reason: "order quantity must be positive",
            };
        }

        if !matches!(context.intent, TradeIntent::OpenLong | TradeIntent::AddLong) {
            return RiskDecision::Allow(context.intent);
        }

        if context.cooldown_active {
            return RiskDecision::Reject {
                reason: "cooldown active",
            };
        }

        if context.stale_data {
            return RiskDecision::Reject {
                reason: "stale market data",
            };
        }

        // Boundary value allowed (`>`): a spread *equal* to the configured maximum is still
        // tradeable. The limit names the worst spread we are willing to accept,
        // so hitting it exactly is allowed; only strictly worse is rejected.
        if context.observed_spread_bps > self.limits.max_spread_bps {
            return RiskDecision::Reject {
                reason: "spread exceeds max",
            };
        }

        // Boundary value allowed (`>`): same rationale as the spread limit. Slippage equal to
        // the configured maximum is the worst we will tolerate and is allowed;
        // only strictly greater slippage is rejected.
        if context.estimated_slippage_bps > self.limits.max_slippage_bps {
            return RiskDecision::Reject {
                reason: "slippage exceeds max",
            };
        }

        // Boundary value rejected (`>=`): reaching the daily-loss limit *exactly* rejects new
        // opens. The limit is a protective floor on capital we refuse to risk
        // past, so once losses hit it we stop adding exposure rather than
        // allowing one more open at the boundary.
        if context.account_daily_loss_pct >= self.limits.max_daily_loss_pct {
            return RiskDecision::Reject {
                reason: "daily loss exceeds max",
            };
        }

        // Boundary value allowed (`>`): an order whose notional equals the per-order maximum
        // is allowed; the limit is the largest single order we permit.
        let notional = context.price * context.quantity;
        if notional > self.limits.max_order_notional_usd {
            return RiskDecision::Reject {
                reason: "order notional exceeds max",
            };
        }

        // Boundary value rejected (`>=`): when the account already holds the maximum number of
        // open positions, a new open is rejected. The limit caps concurrent
        // positions, so opening another would exceed the cap rather than meet
        // it; equal count means no room remains.
        if context.account_open_positions >= self.limits.max_open_positions {
            return RiskDecision::Reject {
                reason: "open positions exceeds max",
            };
        }

        RiskDecision::Allow(context.intent)
    }
}
