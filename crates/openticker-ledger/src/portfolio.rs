use serde::Serialize;

use crate::{LedgerException, LedgerOwnerPath};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LanePortfolioSnapshot {
    pub owner: LedgerOwnerPath,
    pub attributed_open_notional_usd: f64,
    pub reserved_open_notional_usd: f64,
    pub total_committed_notional_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BotPortfolioSnapshot {
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountPortfolioSnapshot {
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
    pub exceptions: Vec<LedgerException>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LedgerSnapshot {
    pub accounts: Vec<AccountPortfolioSnapshot>,
    pub bots: Vec<BotPortfolioSnapshot>,
    pub lanes: Vec<LanePortfolioSnapshot>,
}
