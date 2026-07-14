use openticker_ledger::{
    AccountPortfolioSnapshot, BotPortfolioSnapshot, LanePortfolioSnapshot, LedgerException,
    LedgerOwnerPath,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceLedgerOwner {
    pub account_id: String,
    pub bot_id: String,
    pub symbol: String,
}

impl From<&LedgerOwnerPath> for TraceLedgerOwner {
    fn from(owner: &LedgerOwnerPath) -> Self {
        Self {
            account_id: owner.account_id.clone(),
            bot_id: owner.bot_id.clone(),
            symbol: owner.symbol.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceLedgerException {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<TraceLedgerOwner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub detail: String,
    pub blocks_new_opens: bool,
}

impl From<&LedgerException> for TraceLedgerException {
    fn from(exception: &LedgerException) -> Self {
        Self {
            kind: exception.kind.as_str().to_owned(),
            owner: exception.owner.as_ref().map(Into::into),
            symbol: exception.symbol.clone(),
            detail: exception.detail.clone(),
            blocks_new_opens: exception.blocks_new_opens,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountCapitalSnapshot {
    pub id: String,
    pub declared_total_usd: f64,
    pub live_balance_usd: Option<f64>,
    pub effective_cap_usd: f64,
    pub attributed_open_notional_usd: f64,
    pub unattributed_open_notional_usd: f64,
    pub reserved_open_notional_usd: f64,
    pub total_committed_notional_usd: f64,
    pub blocked_open_room_usd: f64,
    pub tradeable_open_room_usd: f64,
    pub exceptions: Vec<TraceLedgerException>,
}

impl From<&AccountPortfolioSnapshot> for AccountCapitalSnapshot {
    fn from(snapshot: &AccountPortfolioSnapshot) -> Self {
        Self {
            id: snapshot.id.clone(),
            declared_total_usd: snapshot.declared_total_usd,
            live_balance_usd: snapshot.live_balance_usd,
            effective_cap_usd: snapshot.effective_cap_usd,
            attributed_open_notional_usd: snapshot.attributed_open_notional_usd,
            unattributed_open_notional_usd: snapshot.unattributed_open_notional_usd,
            reserved_open_notional_usd: snapshot.reserved_open_notional_usd,
            total_committed_notional_usd: snapshot.total_committed_notional_usd,
            blocked_open_room_usd: snapshot.blocked_open_room_usd,
            tradeable_open_room_usd: snapshot.tradeable_open_room_usd,
            exceptions: snapshot
                .exceptions
                .iter()
                .map(TraceLedgerException::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BotCapitalSnapshot {
    pub id: String,
    pub account_id: String,
    pub pct: f64,
    pub allocated_usd: f64,
    pub attributed_open_notional_usd: f64,
    pub reserved_open_notional_usd: f64,
    pub total_committed_notional_usd: f64,
    pub blocked_open_room_usd: f64,
    pub tradeable_open_room_usd: f64,
}

impl From<&BotPortfolioSnapshot> for BotCapitalSnapshot {
    fn from(snapshot: &BotPortfolioSnapshot) -> Self {
        Self {
            id: snapshot.id.clone(),
            account_id: snapshot.account_id.clone(),
            pct: snapshot.pct,
            allocated_usd: snapshot.allocated_usd,
            attributed_open_notional_usd: snapshot.attributed_open_notional_usd,
            reserved_open_notional_usd: snapshot.reserved_open_notional_usd,
            total_committed_notional_usd: snapshot.total_committed_notional_usd,
            blocked_open_room_usd: snapshot.blocked_open_room_usd,
            tradeable_open_room_usd: snapshot.tradeable_open_room_usd,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaneCapitalSnapshot {
    pub owner: TraceLedgerOwner,
    pub attributed_open_notional_usd: f64,
    pub reserved_open_notional_usd: f64,
    pub total_committed_notional_usd: f64,
}

impl From<&LanePortfolioSnapshot> for LaneCapitalSnapshot {
    fn from(snapshot: &LanePortfolioSnapshot) -> Self {
        Self {
            owner: (&snapshot.owner).into(),
            attributed_open_notional_usd: snapshot.attributed_open_notional_usd,
            reserved_open_notional_usd: snapshot.reserved_open_notional_usd,
            total_committed_notional_usd: snapshot.total_committed_notional_usd,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CapitalState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountCapitalSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<BotCapitalSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<LaneCapitalSnapshot>,
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
