use crate::{
    LatestLanePosition, PortfolioLaneView, live_balance_from_snapshot,
    managed_position_deficit_exceptions,
};
use openticker_connectors::ConnectorAccountSnapshot;
use openticker_ledger::{
    AccountLedger, AccountPortfolioSnapshot, BotPortfolioSnapshot, LanePortfolioSnapshot,
    LedgerException, LedgerOwnerPath, LedgerSnapshot,
};

#[derive(Debug, Clone, Copy)]
pub struct LedgerRooms {
    pub remaining_bot_usd: f64,
    pub remaining_account_usd: f64,
}

#[derive(Debug, Clone)]
pub struct AccountLedgerRefreshState {
    pub lane_open_notionals: Vec<(LedgerOwnerPath, f64)>,
    pub live_balance_usd: Option<f64>,
    pub exceptions: Vec<LedgerException>,
}

#[must_use]
pub fn ledger_snapshot(
    mut accounts: Vec<AccountPortfolioSnapshot>,
    mut bots: Vec<BotPortfolioSnapshot>,
    mut lanes: Vec<LanePortfolioSnapshot>,
) -> LedgerSnapshot {
    accounts.sort_by(|left, right| left.id.cmp(&right.id));
    bots.sort_by(|left, right| left.id.cmp(&right.id));
    lanes.sort_by(|left, right| {
        left.owner
            .account_id
            .cmp(&right.owner.account_id)
            .then_with(|| left.owner.bot_id.cmp(&right.owner.bot_id))
            .then_with(|| left.owner.symbol.cmp(&right.owner.symbol))
    });

    LedgerSnapshot {
        accounts,
        bots,
        lanes,
    }
}

#[must_use]
pub fn lane_open_notionals(lanes: &[PortfolioLaneView]) -> Vec<(LedgerOwnerPath, f64)> {
    lanes
        .iter()
        .map(|lane| {
            (
                LedgerOwnerPath::new(
                    lane.account_id.clone(),
                    lane.bot_id.clone(),
                    lane.symbol.clone(),
                ),
                lane.position_notional_usd,
            )
        })
        .collect()
}

#[must_use]
pub fn ledger_rooms(ledger: &AccountLedger, bot_id: &str, bot_pct: f64) -> LedgerRooms {
    LedgerRooms {
        remaining_bot_usd: ledger.bot_tradeable_open_room_usd(bot_id, bot_pct),
        remaining_account_usd: ledger.account_tradeable_open_room_usd(),
    }
}

#[must_use]
pub fn account_ledger_refresh_state(
    account_id: &str,
    account_kind: &str,
    snapshot: &ConnectorAccountSnapshot,
    lanes: &[PortfolioLaneView],
    latest_positions: &[LatestLanePosition],
    known_open_notional_usd: f64,
    cash_balance_assets: &[String],
) -> AccountLedgerRefreshState {
    AccountLedgerRefreshState {
        lane_open_notionals: lane_open_notionals(lanes),
        live_balance_usd: live_balance_from_snapshot(
            account_kind,
            snapshot,
            known_open_notional_usd,
            cash_balance_assets,
        ),
        exceptions: managed_position_deficit_exceptions(
            account_id,
            snapshot,
            lanes,
            latest_positions,
        ),
    }
}

pub fn apply_account_ledger_refresh_state(
    ledger: &mut AccountLedger,
    refresh_state: AccountLedgerRefreshState,
    exceptions: Vec<LedgerException>,
) {
    ledger.replace_lane_open_notional(refresh_state.lane_open_notionals);
    ledger.set_unattributed_open_notional_usd(0.0);
    ledger.replace_exceptions(exceptions);
    ledger.set_live_balance_usd(refresh_state.live_balance_usd);
}

pub fn sync_account_ledger_from_lanes(ledger: &mut AccountLedger, lanes: &[PortfolioLaneView]) {
    ledger.replace_lane_open_notional(lane_open_notionals(lanes));
}
