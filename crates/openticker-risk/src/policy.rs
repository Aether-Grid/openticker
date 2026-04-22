use openticker_core::TradeIntent;

use crate::{RiskContext, RiskDecision, RiskLimits};

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

        if context.observed_spread_bps > self.limits.max_spread_bps {
            return RiskDecision::Reject {
                reason: "spread exceeds max",
            };
        }

        if context.estimated_slippage_bps > self.limits.max_slippage_bps {
            return RiskDecision::Reject {
                reason: "slippage exceeds max",
            };
        }

        if context.account_daily_loss_pct >= self.limits.max_daily_loss_pct {
            return RiskDecision::Reject {
                reason: "daily loss exceeds max",
            };
        }

        let notional = context.price * context.quantity;
        if notional > self.limits.max_order_notional_usd {
            return RiskDecision::Reject {
                reason: "order notional exceeds max",
            };
        }

        if context.account_open_positions >= self.limits.max_open_positions {
            return RiskDecision::Reject {
                reason: "open positions exceeds max",
            };
        }

        RiskDecision::Allow(context.intent)
    }
}
