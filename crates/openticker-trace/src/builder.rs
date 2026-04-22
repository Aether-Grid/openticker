use crate::{CapitalState, CycleOutcome, CycleRiskDecisionLabel, CycleTraceSummary, TraceIdentity};
use openticker_core::{IndicatorSignal, TradeIntent};
use openticker_ledger::{AccountPortfolioSnapshot, BotPortfolioSnapshot, LanePortfolioSnapshot};

#[must_use]
pub fn build_cycle_summary(
    identity: &TraceIdentity,
    signal: IndicatorSignal,
    intent: TradeIntent,
    risk_decision: CycleRiskDecisionLabel,
    outcome: CycleOutcome,
    created_at_ms: i64,
) -> CycleTraceSummary {
    CycleTraceSummary::from_identity(
        identity,
        signal,
        intent,
        risk_decision,
        outcome,
        created_at_ms,
    )
}

#[must_use]
pub fn build_capital_state(
    account: Option<&AccountPortfolioSnapshot>,
    bot: Option<&BotPortfolioSnapshot>,
    lane: Option<&LanePortfolioSnapshot>,
) -> CapitalState {
    CapitalState {
        account: account.map(Into::into),
        bot: bot.map(Into::into),
        lane: lane.map(Into::into),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openticker_ledger::{AccountPortfolioSnapshot, LanePortfolioSnapshot, LedgerOwnerPath};

    #[test]
    fn capital_state_includes_requested_rows_only() {
        let lane = LanePortfolioSnapshot {
            owner: LedgerOwnerPath::new("alpaca-paper", "aapl", "AAPL"),
            attributed_open_notional_usd: 100.0,
            reserved_open_notional_usd: 25.0,
            total_committed_notional_usd: 125.0,
        };
        let account = AccountPortfolioSnapshot {
            id: "alpaca-paper".to_owned(),
            declared_total_usd: 20_000.0,
            live_balance_usd: Some(19_500.0),
            effective_cap_usd: 19_500.0,
            attributed_open_notional_usd: 100.0,
            unattributed_open_notional_usd: 0.0,
            reserved_open_notional_usd: 25.0,
            total_committed_notional_usd: 125.0,
            blocked_open_room_usd: 0.0,
            tradeable_open_room_usd: 19_375.0,
            exceptions: Vec::new(),
        };

        let state = build_capital_state(Some(&account), None, Some(&lane));

        assert!(state.account.is_some());
        assert!(state.bot.is_none());
        assert!(state.lane.is_some());
    }
}
