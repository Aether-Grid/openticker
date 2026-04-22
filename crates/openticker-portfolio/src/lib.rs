use openticker_connectors::{ConnectorAccountSnapshot, ConnectorOpenOrder};
use openticker_ledger::{
    AccountLedger, AccountPortfolioSnapshot, BotPortfolioSnapshot, LanePortfolioSnapshot,
    LedgerException, LedgerExceptionKind, LedgerOwnerPath, LedgerSnapshot, sanitize_ledger_value,
};
use openticker_storage::{OrderRecord, PositionRecord};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

const POSITION_QUANTITY_TOLERANCE: f64 = 1e-9;
const DEFAULT_BINANCE_CASH_BALANCE_ASSETS: [&str; 5] = ["USD", "USDT", "USDC", "BUSD", "FDUSD"];

#[derive(Debug, Clone)]
pub struct PortfolioLaneView {
    pub lane_id: String,
    pub bot_id: String,
    pub account_id: String,
    pub symbol: String,
    pub budget_pct: f64,
    pub effective_position_quantity: f64,
    pub position_notional_usd: f64,
    pub daily_loss_pct_accumulated: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct AccountRiskSnapshot {
    pub open_positions: u32,
    pub daily_loss_pct: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct LedgerRooms {
    pub remaining_bot_usd: f64,
    pub remaining_account_usd: f64,
}

#[derive(Debug, Clone)]
pub enum ConnectorPositionOwner {
    None,
    Unique(String),
    Ambiguous(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct AccountLedgerRefreshState {
    pub lane_open_notionals: Vec<(LedgerOwnerPath, f64)>,
    pub live_balance_usd: Option<f64>,
    pub exceptions: Vec<LedgerException>,
}

#[derive(Debug, Clone)]
pub struct LatestLanePosition {
    pub lane_id: String,
    pub position: Option<PositionRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountSymbolExposure {
    pub symbol: String,
    pub remote_net_qty: f64,
    pub aggregate_managed_qty: f64,
    pub external_delta_qty: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalOpenOrderIdentity {
    pub bot_id: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRemoteOpenOrder {
    pub bot_id: String,
    pub order: ConnectorOpenOrder,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClassifiedRemoteOpenOrders {
    pub managed_orders: Vec<ManagedRemoteOpenOrder>,
    pub external_orders: Vec<ConnectorOpenOrder>,
    pub unsafe_reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReconciliationPositions {
    pub local_has_position: bool,
    pub connector_has_position: bool,
    pub resolved_has_position: bool,
    pub resolved_position_quantity: f64,
    pub resolved_entry_price: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReconciliationAssessmentSummary {
    pub local_open_orders: usize,
    pub local_open_order_ids: Vec<String>,
    pub connector_open_orders: usize,
    pub connector_open_orders_detail: Vec<ConnectorOpenOrder>,
    pub managed_remote_open_orders: usize,
    pub external_remote_open_orders: usize,
    pub positions: ReconciliationPositions,
    pub remote_net_qty: Option<f64>,
    pub aggregate_managed_qty: f64,
    pub external_delta_qty: Option<f64>,
    pub safe_to_trade: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReconciliationAssessment {
    pub symbol: String,
    pub local_open_orders: usize,
    pub local_open_order_ids: Vec<String>,
    pub connector_open_orders: usize,
    pub connector_open_orders_detail: Vec<ConnectorOpenOrder>,
    pub managed_remote_open_orders: usize,
    pub external_remote_open_orders: usize,
    pub connector_snapshot_available: bool,
    pub connector_snapshot: Option<ConnectorAccountSnapshot>,
    pub positions: ReconciliationPositions,
    pub remote_net_qty: Option<f64>,
    pub aggregate_managed_qty: f64,
    pub external_delta_qty: Option<f64>,
    pub safe_to_trade: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LedgerRejectionPayload {
    pub intent: String,
    pub decision: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committed_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocated_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_cap_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradeable_room_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_room_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exceptions: Option<Vec<LedgerException>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LedgerRejectionEventPayload {
    pub intent: String,
    pub decision: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committed_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocated_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_cap_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradeable_room_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_room_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exceptions: Option<Vec<LedgerException>>,
    pub symbol: String,
    pub bar_timestamp: String,
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
pub fn latest_authoritative_position(
    symbol: &str,
    positions: &[PositionRecord],
) -> Option<PositionRecord> {
    positions.iter().rev().find_map(|position| {
        (position_record_is_authoritative(position) && position.symbol.as_deref() == Some(symbol))
            .then_some(position.clone())
    })
}

#[must_use]
pub fn open_orders_for_symbol(
    snapshot: &ConnectorAccountSnapshot,
    symbol: &str,
) -> Vec<ConnectorOpenOrder> {
    snapshot
        .open_orders
        .iter()
        .filter(|order| order.symbol == symbol && !order_status_terminal(order.status.as_str()))
        .cloned()
        .collect()
}

#[must_use]
pub fn position_quantity_for_symbol(snapshot: &ConnectorAccountSnapshot, symbol: &str) -> f64 {
    snapshot
        .positions
        .iter()
        .filter_map(|position| {
            if position.quantity.abs() <= POSITION_QUANTITY_TOLERANCE {
                return None;
            }
            if connector_position_matches_symbol(position.symbol.as_str(), symbol) {
                Some(position.quantity.abs())
            } else {
                None
            }
        })
        .sum::<f64>()
}

#[must_use]
pub fn local_open_order_ids<S: BuildHasher>(
    orders: &[OrderRecord],
    filled_client_order_ids: &HashSet<String, S>,
) -> Vec<String> {
    orders
        .iter()
        .filter(|order| {
            !order_status_terminal(order.status.as_str())
                && !filled_client_order_ids.contains(order.client_order_id.as_str())
        })
        .map(|order| order.client_order_id.clone())
        .collect()
}

#[must_use]
pub fn classify_remote_open_orders<S: BuildHasher, T: BuildHasher>(
    account_id: &str,
    symbol: &str,
    orders: &[ConnectorOpenOrder],
    matches_by_client_order_id: &HashMap<String, Vec<LocalOpenOrderIdentity>, S>,
    eligible_bot_ids: &HashSet<String, T>,
) -> ClassifiedRemoteOpenOrders {
    let mut classified = ClassifiedRemoteOpenOrders::default();

    for order in orders {
        let Some(matches) = matches_by_client_order_id.get(order.client_order_id.as_str()) else {
            classified.external_orders.push(order.clone());
            continue;
        };
        if matches.is_empty() {
            classified.external_orders.push(order.clone());
            continue;
        }

        let mut identities = matches.clone();
        identities.sort();
        identities.dedup();

        if identities.len() != 1 || identities[0].symbol.is_none() {
            classified.unsafe_reasons.push(format!(
                "client_order_id={} matched multiple local orders ({})",
                order.client_order_id,
                describe_local_open_order_identities(matches)
            ));
            continue;
        }

        let Some(identity) = identities.pop() else {
            continue;
        };
        let Some(local_symbol) = identity.symbol else {
            continue;
        };
        if local_symbol != symbol {
            classified.unsafe_reasons.push(format!(
                "client_order_id={} matched local symbol {} but remote symbol is {}",
                order.client_order_id, local_symbol, symbol,
            ));
            continue;
        }

        if !eligible_bot_ids.contains(identity.bot_id.as_str()) {
            classified.unsafe_reasons.push(format!(
                "client_order_id={} matched bot {} but not account {} symbol {}",
                order.client_order_id, identity.bot_id, account_id, symbol,
            ));
            continue;
        }

        classified.managed_orders.push(ManagedRemoteOpenOrder {
            bot_id: identity.bot_id,
            order: order.clone(),
        });
    }

    classified
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn reconciliation_assessment_summary(
    bot_id: &str,
    instance_position_quantity: f64,
    latest_local_position: Option<&PositionRecord>,
    local_open_order_ids: Vec<String>,
    classified_orders: &ClassifiedRemoteOpenOrders,
    connector_unavailable_reason: Option<&str>,
    exposure: Option<&AccountSymbolExposure>,
    aggregate_managed_qty: f64,
) -> ReconciliationAssessmentSummary {
    let local_has_position = latest_local_position.as_ref().is_some_and(|position| {
        position.has_position || position.quantity > POSITION_QUANTITY_TOLERANCE
    });
    let persisted_position_quantity = latest_local_position
        .as_ref()
        .map_or(0.0, |position| position.quantity.max(0.0));
    let resolved_position_quantity = if instance_position_quantity > POSITION_QUANTITY_TOLERANCE {
        instance_position_quantity
    } else if persisted_position_quantity > POSITION_QUANTITY_TOLERANCE {
        persisted_position_quantity
    } else if local_has_position {
        1.0
    } else {
        0.0
    };
    let resolved_entry_price = if resolved_position_quantity > POSITION_QUANTITY_TOLERANCE {
        latest_local_position
            .as_ref()
            .and_then(|position| position.has_position.then_some(position.entry_price))
            .flatten()
            .filter(|price| price.is_finite() && *price > 0.0)
    } else {
        None
    };
    let connector_open_orders_detail = classified_orders
        .managed_orders
        .iter()
        .filter(|managed| managed.bot_id == bot_id)
        .map(|managed| managed.order.clone())
        .collect::<Vec<_>>();

    let mut blocking_differences = connector_unavailable_reason
        .map(str::to_owned)
        .into_iter()
        .collect::<Vec<_>>();
    blocking_differences.extend(classified_orders.unsafe_reasons.iter().cloned());

    let mut warnings = Vec::new();
    if let Some(exposure) = exposure {
        let managed_deficit_qty = exposure.aggregate_managed_qty - exposure.remote_net_qty;
        if managed_deficit_qty > POSITION_QUANTITY_TOLERANCE {
            warnings.push(format!(
                "managed_position_deficit(remote_net_qty={},aggregate_managed_qty={},deficit_qty={})",
                exposure.remote_net_qty,
                exposure.aggregate_managed_qty,
                managed_deficit_qty,
            ));
        } else if exposure.external_delta_qty > POSITION_QUANTITY_TOLERANCE {
            warnings.push(format!(
                "external_position_surplus(remote_net_qty={},aggregate_managed_qty={},surplus_qty={})",
                exposure.remote_net_qty,
                exposure.aggregate_managed_qty,
                exposure.external_delta_qty,
            ));
        }
    }

    let safe_to_trade = blocking_differences.is_empty();
    let reason = if blocking_differences.is_empty() && warnings.is_empty() {
        "state_aligned".to_owned()
    } else {
        blocking_differences
            .into_iter()
            .chain(warnings)
            .collect::<Vec<_>>()
            .join(";")
    };

    ReconciliationAssessmentSummary {
        local_open_orders: local_open_order_ids.len(),
        local_open_order_ids,
        connector_open_orders: connector_open_orders_detail.len(),
        connector_open_orders_detail,
        managed_remote_open_orders: classified_orders.managed_orders.len(),
        external_remote_open_orders: classified_orders.external_orders.len(),
        positions: ReconciliationPositions {
            local_has_position,
            connector_has_position: exposure
                .is_some_and(|value| value.remote_net_qty > POSITION_QUANTITY_TOLERANCE)
                || (exposure.is_none() && local_has_position),
            resolved_has_position: resolved_position_quantity > POSITION_QUANTITY_TOLERANCE,
            resolved_position_quantity,
            resolved_entry_price,
        },
        remote_net_qty: exposure.map(|value| value.remote_net_qty),
        aggregate_managed_qty: exposure
            .map_or(aggregate_managed_qty, |value| value.aggregate_managed_qty),
        external_delta_qty: exposure.map(|value| value.external_delta_qty),
        safe_to_trade,
        reason,
    }
}

#[must_use]
pub fn build_reconciliation_assessment(
    symbol: String,
    connector_snapshot_available: bool,
    connector_snapshot: Option<ConnectorAccountSnapshot>,
    summary: ReconciliationAssessmentSummary,
) -> ReconciliationAssessment {
    ReconciliationAssessment {
        symbol,
        local_open_orders: summary.local_open_orders,
        local_open_order_ids: summary.local_open_order_ids,
        connector_open_orders: summary.connector_open_orders,
        connector_open_orders_detail: summary.connector_open_orders_detail,
        managed_remote_open_orders: summary.managed_remote_open_orders,
        external_remote_open_orders: summary.external_remote_open_orders,
        connector_snapshot_available,
        connector_snapshot,
        positions: summary.positions,
        remote_net_qty: summary.remote_net_qty,
        aggregate_managed_qty: summary.aggregate_managed_qty,
        external_delta_qty: summary.external_delta_qty,
        safe_to_trade: summary.safe_to_trade,
        reason: summary.reason,
    }
}

#[must_use]
pub fn reconciliation_differences(reason: &str) -> Vec<String> {
    if reason == "state_aligned" {
        Vec::new()
    } else {
        reason
            .split(';')
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

#[must_use]
pub fn account_symbol_exposure(
    account_id: &str,
    symbol: &str,
    snapshot: &ConnectorAccountSnapshot,
    lanes: &[PortfolioLaneView],
    latest_positions: &[LatestLanePosition],
) -> AccountSymbolExposure {
    let remote_net_qty = remote_net_qty_for_symbol(snapshot, symbol);
    let aggregate_managed_qty = lanes
        .iter()
        .filter(|lane| {
            lane.account_id == account_id
                && connector_position_matches_symbol(symbol, lane.symbol.as_str())
        })
        .filter_map(|lane| latest_position_for_lane(latest_positions, lane.lane_id.as_str()))
        .map(authoritative_position_quantity)
        .sum::<f64>();
    let external_delta_qty = sanitize_quantity_delta(remote_net_qty - aggregate_managed_qty);

    AccountSymbolExposure {
        symbol: symbol.to_owned(),
        remote_net_qty,
        aggregate_managed_qty,
        external_delta_qty,
    }
}

#[must_use]
pub fn connector_position_owner(
    account_id: &str,
    connector_symbol: &str,
    lanes: &[PortfolioLaneView],
    latest_positions: &[LatestLanePosition],
) -> ConnectorPositionOwner {
    let mut matching_lane_ids = lanes
        .iter()
        .filter(|lane| {
            lane.account_id == account_id
                && connector_position_matches_symbol(connector_symbol, lane.symbol.as_str())
        })
        .map(|lane| lane.lane_id.clone())
        .collect::<Vec<_>>();
    matching_lane_ids.sort();

    if matching_lane_ids.is_empty() {
        return ConnectorPositionOwner::None;
    }
    if matching_lane_ids.len() == 1 {
        return ConnectorPositionOwner::Unique(matching_lane_ids[0].clone());
    }

    let mut strong_local_holders = Vec::new();
    let mut any_local_holders = Vec::new();
    for lane_id in &matching_lane_ids {
        let Some(position) = latest_position_for_lane(latest_positions, lane_id) else {
            continue;
        };
        if !position_record_indicates_open(position) {
            continue;
        }

        any_local_holders.push(lane_id.clone());
        if !position_reason_is_reconciliation_sync(position.reason.as_str()) {
            strong_local_holders.push(lane_id.clone());
        }
    }

    if strong_local_holders.len() == 1 {
        ConnectorPositionOwner::Unique(strong_local_holders[0].clone())
    } else if strong_local_holders.len() > 1 {
        ConnectorPositionOwner::Ambiguous(strong_local_holders)
    } else if any_local_holders.len() == 1 {
        ConnectorPositionOwner::Unique(any_local_holders[0].clone())
    } else {
        ConnectorPositionOwner::Ambiguous(matching_lane_ids)
    }
}

#[must_use]
pub fn account_risk_snapshot(lanes: &[PortfolioLaneView]) -> AccountRiskSnapshot {
    let mut open_positions = 0_u32;
    let mut daily_loss_pct = 0.0;

    for lane in lanes {
        if lane.effective_position_quantity > POSITION_QUANTITY_TOLERANCE {
            open_positions = open_positions.saturating_add(1);
        }
        daily_loss_pct += lane.daily_loss_pct_accumulated;
    }

    AccountRiskSnapshot {
        open_positions,
        daily_loss_pct,
    }
}

#[must_use]
pub fn ledger_rooms(ledger: &AccountLedger, bot_id: &str, bot_pct: f64) -> LedgerRooms {
    LedgerRooms {
        remaining_bot_usd: ledger.bot_tradeable_open_room_usd(bot_id, bot_pct),
        remaining_account_usd: ledger.account_tradeable_open_room_usd(),
    }
}

#[must_use]
pub fn bot_ledger_rejection_payload(
    ledger: &AccountLedger,
    account_id: &str,
    bot_id: &str,
    bot_pct: f64,
    intent_label: &str,
    reason_code: &str,
) -> LedgerRejectionPayload {
    let bot_snapshot = ledger.bot_snapshot(account_id, bot_id, bot_pct);
    LedgerRejectionPayload {
        intent: intent_label.to_owned(),
        decision: "rejected".to_owned(),
        reason: reason_code.to_owned(),
        committed_usd: Some(bot_snapshot.total_committed_notional_usd),
        allocated_usd: Some(bot_snapshot.allocated_usd),
        effective_cap_usd: None,
        tradeable_room_usd: Some(bot_snapshot.tradeable_open_room_usd),
        blocked_room_usd: Some(bot_snapshot.blocked_open_room_usd),
        exceptions: None,
    }
}

#[must_use]
pub fn account_ledger_rejection_payload(
    ledger: &AccountLedger,
    account_id: &str,
    intent_label: &str,
    reason_code: &str,
) -> LedgerRejectionPayload {
    let account_snapshot = ledger.account_snapshot(account_id);
    LedgerRejectionPayload {
        intent: intent_label.to_owned(),
        decision: "rejected".to_owned(),
        reason: reason_code.to_owned(),
        committed_usd: Some(account_snapshot.total_committed_notional_usd),
        allocated_usd: None,
        effective_cap_usd: Some(account_snapshot.effective_cap_usd),
        tradeable_room_usd: Some(account_snapshot.tradeable_open_room_usd),
        blocked_room_usd: Some(account_snapshot.blocked_open_room_usd),
        exceptions: Some(account_snapshot.exceptions),
    }
}

#[must_use]
pub fn dust_ledger_rejection_payload(intent_label: &str) -> LedgerRejectionPayload {
    LedgerRejectionPayload {
        intent: intent_label.to_owned(),
        decision: "skipped".to_owned(),
        reason: "ledger_dust".to_owned(),
        committed_usd: None,
        allocated_usd: None,
        effective_cap_usd: None,
        tradeable_room_usd: None,
        blocked_room_usd: None,
        exceptions: None,
    }
}

#[must_use]
pub fn ledger_rejection_event_payload(
    payload: LedgerRejectionPayload,
    symbol: &str,
    bar_timestamp: &str,
) -> LedgerRejectionEventPayload {
    LedgerRejectionEventPayload {
        intent: payload.intent,
        decision: payload.decision,
        reason: payload.reason,
        committed_usd: payload.committed_usd,
        allocated_usd: payload.allocated_usd,
        effective_cap_usd: payload.effective_cap_usd,
        tradeable_room_usd: payload.tradeable_room_usd,
        blocked_room_usd: payload.blocked_room_usd,
        exceptions: payload.exceptions,
        symbol: symbol.to_owned(),
        bar_timestamp: bar_timestamp.to_owned(),
    }
}

#[must_use]
pub fn managed_position_deficit_exceptions(
    account_id: &str,
    snapshot: &ConnectorAccountSnapshot,
    lanes: &[PortfolioLaneView],
    latest_positions: &[LatestLanePosition],
) -> Vec<LedgerException> {
    let mut symbols = lanes
        .iter()
        .filter(|lane| lane.account_id == account_id)
        .map(|lane| lane.symbol.clone())
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();

    let mut exceptions = Vec::new();
    for symbol in symbols {
        let exposure =
            account_symbol_exposure(account_id, &symbol, snapshot, lanes, latest_positions);
        let deficit_qty =
            sanitize_quantity_delta(exposure.aggregate_managed_qty - exposure.remote_net_qty);
        if deficit_qty <= POSITION_QUANTITY_TOLERANCE {
            continue;
        }

        exceptions.push(LedgerException {
            kind: LedgerExceptionKind::ManagedPositionDeficit,
            owner: None,
            symbol: Some(symbol),
            detail: format!(
                "remote_net_qty={},aggregate_managed_qty={},deficit_qty={}",
                exposure.remote_net_qty, exposure.aggregate_managed_qty, deficit_qty,
            ),
            blocks_new_opens: true,
        });
    }

    exceptions.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    exceptions
}

#[must_use]
pub fn unmapped_managed_open_order_exceptions(
    symbol: &str,
    classified_orders: &ClassifiedRemoteOpenOrders,
) -> Vec<LedgerException> {
    classified_orders
        .unsafe_reasons
        .iter()
        .cloned()
        .map(|detail| LedgerException {
            kind: LedgerExceptionKind::UnmappedManagedOpenOrder,
            owner: None,
            symbol: Some(symbol.to_owned()),
            detail,
            blocks_new_opens: true,
        })
        .collect()
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

#[must_use]
pub fn live_balance_from_snapshot(
    account_kind: &str,
    snapshot: &ConnectorAccountSnapshot,
    known_open_notional_usd: f64,
    cash_balance_assets: &[String],
) -> Option<f64> {
    match account_kind {
        "alpaca" => snapshot
            .balances
            .iter()
            .find(|balance| balance.asset.eq_ignore_ascii_case("EQUITY"))
            .map(|balance| sanitize_ledger_value(balance.free + balance.locked)),
        "binance" => {
            let quote_cash_usd = snapshot
                .balances
                .iter()
                .filter(|balance| {
                    is_binance_cash_balance_asset(balance.asset.as_str(), cash_balance_assets)
                })
                .map(|balance| sanitize_ledger_value(balance.free + balance.locked))
                .sum::<f64>();
            let computed = quote_cash_usd + known_open_notional_usd;
            if computed > POSITION_QUANTITY_TOLERANCE {
                Some(computed)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn latest_position_for_lane<'a>(
    latest_positions: &'a [LatestLanePosition],
    lane_id: &str,
) -> Option<&'a PositionRecord> {
    latest_positions
        .iter()
        .find(|latest| latest.lane_id == lane_id)
        .and_then(|latest| latest.position.as_ref())
}

fn remote_net_qty_for_symbol(snapshot: &ConnectorAccountSnapshot, symbol: &str) -> f64 {
    position_quantity_for_symbol(snapshot, symbol)
}

fn authoritative_position_quantity(position: &PositionRecord) -> f64 {
    if !position_record_is_authoritative(position) {
        return 0.0;
    }

    if position.quantity > POSITION_QUANTITY_TOLERANCE {
        position.quantity.max(0.0)
    } else if position.has_position {
        1.0
    } else {
        0.0
    }
}

fn sanitize_quantity_delta(value: f64) -> f64 {
    if value.abs() <= POSITION_QUANTITY_TOLERANCE {
        0.0
    } else {
        value
    }
}

fn position_record_indicates_open(position: &PositionRecord) -> bool {
    authoritative_position_quantity(position) > POSITION_QUANTITY_TOLERANCE
        || (position_record_is_authoritative(position) && position.has_position)
}

fn position_record_is_authoritative(position: &PositionRecord) -> bool {
    !position_reason_is_close_requested(position.reason.as_str())
        && !position_reason_is_reconciliation_sync(position.reason.as_str())
}

fn position_reason_is_close_requested(reason: &str) -> bool {
    reason == "close_requested"
}

fn position_reason_is_reconciliation_sync(reason: &str) -> bool {
    reason.ends_with("_reconciliation_sync")
}

fn connector_position_matches_symbol(connector_symbol: &str, symbol: &str) -> bool {
    connector_symbol == symbol
        || symbol_base_asset(symbol)
            .is_some_and(|asset| connector_symbol.eq_ignore_ascii_case(asset))
}

fn symbol_base_asset(symbol: &str) -> Option<&str> {
    const QUOTE_SUFFIXES: [&str; 6] = ["FDUSD", "USDT", "USDC", "BUSD", "USD", "BTC"];
    QUOTE_SUFFIXES.iter().find_map(|suffix| {
        if symbol.len() > suffix.len() && symbol.ends_with(suffix) {
            Some(&symbol[..symbol.len() - suffix.len()])
        } else {
            None
        }
    })
}

fn order_status_terminal(status: &str) -> bool {
    matches!(
        status,
        "filled" | "cancelled" | "rejected" | "expired" | "done" | "cancel_requested"
    )
}

fn is_binance_cash_balance_asset(asset: &str, cash_balance_assets: &[String]) -> bool {
    if cash_balance_assets.is_empty() {
        return DEFAULT_BINANCE_CASH_BALANCE_ASSETS
            .iter()
            .any(|configured_asset| asset.eq_ignore_ascii_case(configured_asset));
    }

    cash_balance_assets.iter().any(|configured_asset| {
        let configured_asset = configured_asset.trim();
        !configured_asset.is_empty() && asset.eq_ignore_ascii_case(configured_asset)
    })
}

fn describe_local_open_order_identities(identities: &[LocalOpenOrderIdentity]) -> String {
    let mut described = identities
        .iter()
        .map(|identity| {
            format!(
                "{}:{}",
                identity.bot_id,
                identity.symbol.as_deref().unwrap_or("<unknown-symbol>")
            )
        })
        .collect::<Vec<_>>();
    described.sort();
    described.dedup();
    described.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use openticker_connectors::{
        ConnectorAccountSnapshot, ConnectorOpenOrder, ConnectorPosition, ConnectorPrivateBalance,
    };
    use openticker_ledger::AccountLedger;
    use openticker_storage::OrderRecord;
    use std::collections::{HashMap, HashSet};

    fn lane(id: &str, bot: &str, account: &str, symbol: &str) -> PortfolioLaneView {
        PortfolioLaneView {
            lane_id: id.to_owned(),
            bot_id: bot.to_owned(),
            account_id: account.to_owned(),
            symbol: symbol.to_owned(),
            budget_pct: 25.0,
            effective_position_quantity: 1.0,
            position_notional_usd: 100.0,
            daily_loss_pct_accumulated: 1.5,
        }
    }

    fn open_position(bot: &str, symbol: &str, reason: &str) -> PositionRecord {
        PositionRecord {
            id: 1,
            bot_id: bot.to_owned(),
            symbol: Some(symbol.to_owned()),
            trace_id: None,
            bar_timestamp: None,
            has_position: true,
            quantity: 1.0,
            entry_price: Some(100.0),
            realized_pnl_usd: 0.0,
            reason: reason.to_owned(),
            created_at_ms: 1,
        }
    }

    fn position_with_state(
        lane_id: i64,
        bot: &str,
        symbol: &str,
        has_position: bool,
        quantity: f64,
        reason: &str,
    ) -> PositionRecord {
        PositionRecord {
            id: lane_id,
            bot_id: bot.to_owned(),
            symbol: Some(symbol.to_owned()),
            trace_id: None,
            bar_timestamp: None,
            has_position,
            quantity,
            entry_price: Some(100.0),
            realized_pnl_usd: 0.0,
            reason: reason.to_owned(),
            created_at_ms: lane_id,
        }
    }

    fn connector_open_order(client_order_id: &str, symbol: &str) -> ConnectorOpenOrder {
        ConnectorOpenOrder {
            client_order_id: client_order_id.to_owned(),
            symbol: symbol.to_owned(),
            status: "open".to_owned(),
            quantity: 1.0,
        }
    }

    fn order_record(client_order_id: &str, status: &str, symbol: &str) -> OrderRecord {
        OrderRecord {
            id: 1,
            bot_id: "bot-a".to_owned(),
            symbol: Some(symbol.to_owned()),
            trace_id: None,
            bar_timestamp: None,
            client_order_id: client_order_id.to_owned(),
            intent: "buy".to_owned(),
            status: status.to_owned(),
            price: 100.0,
            quantity: 1.0,
            created_at_ms: 1,
        }
    }

    fn latest_position(
        bot: &str,
        symbol: &str,
        quantity: f64,
        entry_price: Option<f64>,
    ) -> PositionRecord {
        PositionRecord {
            id: 1,
            bot_id: bot.to_owned(),
            symbol: Some(symbol.to_owned()),
            trace_id: None,
            bar_timestamp: None,
            has_position: quantity > 0.0,
            quantity,
            entry_price,
            realized_pnl_usd: 0.0,
            reason: "entry_fill".to_owned(),
            created_at_ms: 1,
        }
    }

    const FLOAT_ASSERT_EPSILON: f64 = 1e-9;

    fn assert_f64_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < FLOAT_ASSERT_EPSILON,
            "expected {actual} to be within {FLOAT_ASSERT_EPSILON} of {expected}"
        );
    }

    fn assert_opt_f64_close(actual: Option<f64>, expected: f64) {
        assert!(
            actual.is_some_and(|value| (value - expected).abs() < FLOAT_ASSERT_EPSILON),
            "expected {actual:?} to contain a value within {FLOAT_ASSERT_EPSILON} of {expected}"
        );
    }

    #[test]
    fn classify_remote_open_orders_returns_external_orders_without_local_match() {
        let order = connector_open_order("external-1", "BTCUSDT");
        let classified = classify_remote_open_orders(
            "acct",
            "BTCUSDT",
            std::slice::from_ref(&order),
            &HashMap::new(),
            &HashSet::new(),
        );

        assert_eq!(classified.external_orders, vec![order]);
        assert!(classified.managed_orders.is_empty());
        assert!(classified.unsafe_reasons.is_empty());
    }

    #[test]
    fn open_orders_for_symbol_excludes_terminal_orders() {
        let snapshot = ConnectorAccountSnapshot {
            open_orders: vec![
                connector_open_order("open-1", "BTCUSDT"),
                ConnectorOpenOrder {
                    client_order_id: "filled-1".to_owned(),
                    symbol: "BTCUSDT".to_owned(),
                    status: "filled".to_owned(),
                    quantity: 1.0,
                },
                connector_open_order("other-1", "ETHUSDT"),
            ],
            positions: Vec::new(),
            balances: Vec::new(),
        };

        assert_eq!(
            open_orders_for_symbol(&snapshot, "BTCUSDT"),
            vec![connector_open_order("open-1", "BTCUSDT")]
        );
    }

    #[test]
    fn local_open_order_ids_ignore_terminal_and_filled_orders() {
        let orders = vec![
            order_record("open-1", "open", "BTCUSDT"),
            order_record("filled-1", "filled", "BTCUSDT"),
            order_record("filled-by-fill-1", "open", "BTCUSDT"),
        ];
        let filled_client_order_ids = HashSet::from(["filled-by-fill-1".to_owned()]);

        assert_eq!(
            local_open_order_ids(&orders, &filled_client_order_ids),
            vec!["open-1".to_owned()]
        );
    }

    #[test]
    fn classify_remote_open_orders_accepts_exact_managed_match() {
        let order = connector_open_order("managed-1", "BTCUSDT");
        let matches = HashMap::from([(
            "managed-1".to_owned(),
            vec![LocalOpenOrderIdentity {
                bot_id: "bot-a".to_owned(),
                symbol: Some("BTCUSDT".to_owned()),
            }],
        )]);
        let eligible_bot_ids = HashSet::from(["bot-a".to_owned()]);

        let classified = classify_remote_open_orders(
            "acct",
            "BTCUSDT",
            std::slice::from_ref(&order),
            &matches,
            &eligible_bot_ids,
        );

        assert_eq!(
            classified.managed_orders,
            vec![ManagedRemoteOpenOrder {
                bot_id: "bot-a".to_owned(),
                order,
            }]
        );
        assert!(classified.external_orders.is_empty());
        assert!(classified.unsafe_reasons.is_empty());
    }

    #[test]
    fn classify_remote_open_orders_flags_ambiguous_local_matches() {
        let order = connector_open_order("ambiguous-1", "BTCUSDT");
        let matches = HashMap::from([(
            "ambiguous-1".to_owned(),
            vec![
                LocalOpenOrderIdentity {
                    bot_id: "bot-a".to_owned(),
                    symbol: Some("BTCUSDT".to_owned()),
                },
                LocalOpenOrderIdentity {
                    bot_id: "bot-b".to_owned(),
                    symbol: Some("BTCUSDT".to_owned()),
                },
            ],
        )]);
        let eligible_bot_ids = HashSet::from(["bot-a".to_owned(), "bot-b".to_owned()]);

        let classified = classify_remote_open_orders(
            "acct",
            "BTCUSDT",
            std::slice::from_ref(&order),
            &matches,
            &eligible_bot_ids,
        );

        assert!(classified.managed_orders.is_empty());
        assert!(classified.external_orders.is_empty());
        assert_eq!(
            classified.unsafe_reasons,
            vec![
                "client_order_id=ambiguous-1 matched multiple local orders (bot-a:BTCUSDT,bot-b:BTCUSDT)"
                    .to_owned(),
            ]
        );
    }

    #[test]
    fn classify_remote_open_orders_flags_ineligible_bot_matches() {
        let order = connector_open_order("managed-2", "BTCUSDT");
        let matches = HashMap::from([(
            "managed-2".to_owned(),
            vec![LocalOpenOrderIdentity {
                bot_id: "bot-a".to_owned(),
                symbol: Some("BTCUSDT".to_owned()),
            }],
        )]);
        let eligible_bot_ids = HashSet::from(["bot-b".to_owned()]);

        let classified = classify_remote_open_orders(
            "acct",
            "BTCUSDT",
            std::slice::from_ref(&order),
            &matches,
            &eligible_bot_ids,
        );

        assert!(classified.managed_orders.is_empty());
        assert!(classified.external_orders.is_empty());
        assert_eq!(
            classified.unsafe_reasons,
            vec![
                "client_order_id=managed-2 matched bot bot-a but not account acct symbol BTCUSDT"
                    .to_owned(),
            ]
        );
    }

    #[test]
    fn reconciliation_assessment_summary_prefers_runtime_quantity_and_scopes_bot_orders() {
        let classified_orders = ClassifiedRemoteOpenOrders {
            managed_orders: vec![
                ManagedRemoteOpenOrder {
                    bot_id: "bot-a".to_owned(),
                    order: connector_open_order("managed-a", "BTCUSDT"),
                },
                ManagedRemoteOpenOrder {
                    bot_id: "bot-b".to_owned(),
                    order: connector_open_order("managed-b", "BTCUSDT"),
                },
            ],
            external_orders: vec![connector_open_order("external-1", "BTCUSDT")],
            unsafe_reasons: Vec::new(),
        };

        let summary = reconciliation_assessment_summary(
            "bot-a",
            2.0,
            Some(&latest_position("bot-a", "BTCUSDT", 1.0, Some(100.0))),
            vec!["local-1".to_owned(), "local-2".to_owned()],
            &classified_orders,
            None,
            None,
            3.0,
        );

        assert_eq!(summary.local_open_orders, 2);
        assert_eq!(
            summary.local_open_order_ids,
            vec!["local-1".to_owned(), "local-2".to_owned()]
        );
        assert_eq!(summary.connector_open_orders, 1);
        assert_eq!(
            summary.connector_open_orders_detail,
            vec![connector_open_order("managed-a", "BTCUSDT")]
        );
        assert_eq!(summary.managed_remote_open_orders, 2);
        assert_eq!(summary.external_remote_open_orders, 1);
        assert!(summary.positions.local_has_position);
        assert!(summary.positions.connector_has_position);
        assert!(summary.positions.resolved_has_position);
        assert!((summary.positions.resolved_position_quantity - 2.0).abs() < 1e-6);
        assert_opt_f64_close(summary.positions.resolved_entry_price, 100.0);
        assert_f64_close(summary.aggregate_managed_qty, 3.0);
        assert_eq!(summary.remote_net_qty, None);
        assert_eq!(summary.external_delta_qty, None);
        assert!(summary.safe_to_trade);
        assert_eq!(summary.reason, "state_aligned");
    }

    #[test]
    fn reconciliation_assessment_summary_keeps_blockers_and_warnings() {
        let classified_orders = ClassifiedRemoteOpenOrders {
            managed_orders: vec![ManagedRemoteOpenOrder {
                bot_id: "bot-a".to_owned(),
                order: connector_open_order("managed-a", "BTCUSDT"),
            }],
            external_orders: Vec::new(),
            unsafe_reasons: vec!["ambiguous_remote_order".to_owned()],
        };
        let exposure = AccountSymbolExposure {
            symbol: "BTCUSDT".to_owned(),
            remote_net_qty: 0.5,
            aggregate_managed_qty: 1.0,
            external_delta_qty: 0.5,
        };

        let summary = reconciliation_assessment_summary(
            "bot-a",
            0.0,
            None,
            Vec::new(),
            &classified_orders,
            Some("snapshot_unavailable"),
            Some(&exposure),
            1.0,
        );

        assert_eq!(summary.connector_open_orders, 1);
        assert!(summary.positions.connector_has_position);
        assert!(!summary.positions.local_has_position);
        assert!(!summary.positions.resolved_has_position);
        assert!(!summary.safe_to_trade);
        assert_eq!(
            summary.reason,
            "snapshot_unavailable;ambiguous_remote_order;managed_position_deficit(remote_net_qty=0.5,aggregate_managed_qty=1,deficit_qty=0.5)"
        );
        assert_opt_f64_close(summary.remote_net_qty, 0.5);
        assert_f64_close(summary.aggregate_managed_qty, 1.0);
        assert_opt_f64_close(summary.external_delta_qty, 0.5);
    }

    #[test]
    fn build_reconciliation_assessment_wraps_summary_with_snapshot_metadata() {
        let summary = ReconciliationAssessmentSummary {
            local_open_orders: 2,
            local_open_order_ids: vec!["local-1".to_owned(), "local-2".to_owned()],
            connector_open_orders: 1,
            connector_open_orders_detail: vec![connector_open_order("managed-a", "BTCUSDT")],
            managed_remote_open_orders: 3,
            external_remote_open_orders: 1,
            positions: ReconciliationPositions {
                local_has_position: true,
                connector_has_position: true,
                resolved_has_position: true,
                resolved_position_quantity: 2.0,
                resolved_entry_price: Some(100.0),
            },
            remote_net_qty: Some(1.5),
            aggregate_managed_qty: 2.0,
            external_delta_qty: Some(0.5),
            safe_to_trade: false,
            reason: "snapshot_unavailable".to_owned(),
        };
        let snapshot = ConnectorAccountSnapshot {
            open_orders: vec![connector_open_order("managed-a", "BTCUSDT")],
            positions: vec![ConnectorPosition {
                symbol: "BTC".to_owned(),
                quantity: 1.5,
            }],
            balances: Vec::new(),
        };

        let assessment = build_reconciliation_assessment(
            "BTCUSDT".to_owned(),
            true,
            Some(snapshot.clone()),
            summary,
        );

        assert_eq!(assessment.symbol, "BTCUSDT");
        assert_eq!(assessment.local_open_orders, 2);
        assert_eq!(assessment.connector_open_orders, 1);
        assert_eq!(assessment.managed_remote_open_orders, 3);
        assert_eq!(assessment.external_remote_open_orders, 1);
        assert!(assessment.connector_snapshot_available);
        assert_eq!(assessment.connector_snapshot, Some(snapshot));
        assert!(assessment.positions.local_has_position);
        assert_opt_f64_close(assessment.remote_net_qty, 1.5);
        assert_f64_close(assessment.aggregate_managed_qty, 2.0);
        assert_opt_f64_close(assessment.external_delta_qty, 0.5);
        assert!(!assessment.safe_to_trade);
        assert_eq!(assessment.reason, "snapshot_unavailable");
    }

    #[test]
    fn reconciliation_differences_splits_reason_payload() {
        assert_eq!(
            reconciliation_differences("blocked_a;warning_b"),
            vec!["blocked_a".to_owned(), "warning_b".to_owned()]
        );
        assert!(reconciliation_differences("state_aligned").is_empty());
    }

    #[test]
    fn unmapped_managed_open_order_exceptions_wrap_unsafe_reasons() {
        let classified_orders = ClassifiedRemoteOpenOrders {
            managed_orders: Vec::new(),
            external_orders: Vec::new(),
            unsafe_reasons: vec![
                "client_order_id=order-1 matched multiple local orders".to_owned(),
                "client_order_id=order-2 matched wrong symbol".to_owned(),
            ],
        };

        let exceptions = unmapped_managed_open_order_exceptions("BTCUSDT", &classified_orders);

        assert_eq!(exceptions.len(), 2);
        assert_eq!(
            exceptions[0].kind,
            LedgerExceptionKind::UnmappedManagedOpenOrder
        );
        assert_eq!(exceptions[0].symbol.as_deref(), Some("BTCUSDT"));
        assert!(exceptions[0].blocks_new_opens);
        assert_eq!(
            exceptions[0].detail,
            "client_order_id=order-1 matched multiple local orders"
        );
        assert_eq!(
            exceptions[1].kind,
            LedgerExceptionKind::UnmappedManagedOpenOrder
        );
        assert_eq!(
            exceptions[1].detail,
            "client_order_id=order-2 matched wrong symbol"
        );
    }

    #[test]
    fn apply_account_ledger_refresh_state_updates_ledger_snapshot_fields() {
        let mut ledger = AccountLedger::new(1_000.0);
        ledger.set_unattributed_open_notional_usd(125.0);

        let owner = LedgerOwnerPath::new("acct", "bot-a", "BTCUSDT");
        let refresh_state = AccountLedgerRefreshState {
            lane_open_notionals: vec![(owner.clone(), 250.0)],
            live_balance_usd: Some(750.0),
            exceptions: vec![LedgerException {
                kind: LedgerExceptionKind::ManagedPositionDeficit,
                owner: None,
                symbol: Some("BTCUSDT".to_owned()),
                detail: "deficit_qty=0.5".to_owned(),
                blocks_new_opens: true,
            }],
        };
        let exceptions = vec![LedgerException {
            kind: LedgerExceptionKind::UnmappedManagedOpenOrder,
            owner: None,
            symbol: Some("BTCUSDT".to_owned()),
            detail: "client_order_id=order-1 matched multiple local orders".to_owned(),
            blocks_new_opens: true,
        }];

        apply_account_ledger_refresh_state(&mut ledger, refresh_state, exceptions);

        let snapshot = ledger.account_snapshot("acct");
        assert_opt_f64_close(snapshot.live_balance_usd, 750.0);
        assert!(snapshot.unattributed_open_notional_usd.abs() < 1e-6);
        assert_eq!(snapshot.exceptions.len(), 1);
        assert_eq!(
            snapshot.exceptions[0].kind,
            LedgerExceptionKind::UnmappedManagedOpenOrder
        );
        let lane_snapshot = ledger.lane_snapshots();
        assert_eq!(lane_snapshot.len(), 1);
        assert_eq!(lane_snapshot[0].owner, owner);
        assert!((lane_snapshot[0].attributed_open_notional_usd - 250.0).abs() < 1e-6);
    }

    #[test]
    fn sync_account_ledger_from_lanes_replaces_lane_open_notionals() {
        let mut ledger = AccountLedger::new(1_000.0);
        let existing_owner = LedgerOwnerPath::new("acct", "bot-a", "BTCUSDT");
        ledger.replace_lane_open_notional(vec![(existing_owner, 50.0)]);

        let lanes = vec![PortfolioLaneView {
            lane_id: "lane-eth".to_owned(),
            bot_id: "bot-b".to_owned(),
            account_id: "acct".to_owned(),
            symbol: "ETHUSDT".to_owned(),
            budget_pct: 25.0,
            effective_position_quantity: 1.0,
            position_notional_usd: 300.0,
            daily_loss_pct_accumulated: 0.0,
        }];

        sync_account_ledger_from_lanes(&mut ledger, &lanes);

        let lane_snapshots = ledger.lane_snapshots();
        assert_eq!(lane_snapshots.len(), 1);
        assert_eq!(
            lane_snapshots[0].owner,
            LedgerOwnerPath::new("acct", "bot-b", "ETHUSDT")
        );
        assert!((lane_snapshots[0].attributed_open_notional_usd - 300.0).abs() < 1e-6);
    }

    #[test]
    fn connector_position_owner_prefers_strong_local_holder() {
        let lanes = vec![
            lane("lane-a", "bot-a", "acct", "BTCUSDT"),
            lane("lane-b", "bot-b", "acct", "BTCUSDT"),
        ];
        let latest_positions = vec![
            LatestLanePosition {
                lane_id: "lane-a".to_owned(),
                position: Some(open_position("bot-a", "BTCUSDT", "entry_fill")),
            },
            LatestLanePosition {
                lane_id: "lane-b".to_owned(),
                position: Some(open_position(
                    "bot-b",
                    "BTCUSDT",
                    "position_reconciliation_sync",
                )),
            },
        ];

        match connector_position_owner("acct", "BTC", &lanes, &latest_positions) {
            ConnectorPositionOwner::Unique(owner) => assert_eq!(owner, "lane-a"),
            other => panic!("unexpected owner resolution: {other:?}"),
        }
    }

    #[test]
    fn latest_authoritative_position_skips_close_requested_rows() {
        let positions = vec![
            PositionRecord {
                id: 1,
                bot_id: "bot".to_owned(),
                symbol: Some("BTCUSDT".to_owned()),
                trace_id: None,
                bar_timestamp: None,
                has_position: false,
                quantity: 0.0,
                entry_price: None,
                realized_pnl_usd: 0.0,
                reason: "close_requested".to_owned(),
                created_at_ms: 1,
            },
            open_position("bot", "BTCUSDT", "entry_fill"),
        ];

        let latest = latest_authoritative_position("BTCUSDT", &positions).unwrap();
        assert_eq!(latest.reason, "entry_fill");
    }

    #[test]
    fn latest_authoritative_position_skips_reconciliation_sync_rows() {
        let positions = vec![
            PositionRecord {
                id: 1,
                bot_id: "bot".to_owned(),
                symbol: Some("BTCUSDT".to_owned()),
                trace_id: None,
                bar_timestamp: None,
                has_position: true,
                quantity: 1.0,
                entry_price: Some(100.0),
                realized_pnl_usd: 0.0,
                reason: "startup_reconciliation_sync".to_owned(),
                created_at_ms: 1,
            },
            open_position("bot", "BTCUSDT", "entry_fill"),
        ];

        let latest = latest_authoritative_position("BTCUSDT", &positions).unwrap();
        assert_eq!(latest.reason, "entry_fill");
    }

    #[test]
    fn managed_position_deficit_exceptions_only_block_true_deficits() {
        let snapshot = ConnectorAccountSnapshot {
            open_orders: Vec::new(),
            positions: vec![ConnectorPosition {
                symbol: "BTC".to_owned(),
                quantity: 0.5,
            }],
            balances: Vec::new(),
        };
        let lanes = vec![lane("lane-a", "bot-a", "acct", "BTCUSDT")];
        let latest_positions = vec![LatestLanePosition {
            lane_id: "lane-a".to_owned(),
            position: Some(position_with_state(
                1,
                "bot-a",
                "BTCUSDT",
                true,
                1.0,
                "entry_fill",
            )),
        }];

        let exceptions =
            managed_position_deficit_exceptions("acct", &snapshot, &lanes, &latest_positions);

        assert_eq!(exceptions.len(), 1);
        assert_eq!(
            exceptions[0].kind,
            LedgerExceptionKind::ManagedPositionDeficit
        );
        assert_eq!(exceptions[0].symbol.as_deref(), Some("BTCUSDT"));
        assert!(exceptions[0].detail.contains("deficit_qty=0.5"));
        assert!(exceptions[0].blocks_new_opens);
    }

    #[test]
    fn managed_position_deficit_exceptions_ignore_external_surplus() {
        let snapshot = ConnectorAccountSnapshot {
            open_orders: Vec::new(),
            positions: vec![ConnectorPosition {
                symbol: "BTC".to_owned(),
                quantity: 1.5,
            }],
            balances: Vec::new(),
        };
        let lanes = vec![lane("lane-a", "bot-a", "acct", "BTCUSDT")];
        let latest_positions = vec![LatestLanePosition {
            lane_id: "lane-a".to_owned(),
            position: Some(position_with_state(
                1,
                "bot-a",
                "BTCUSDT",
                true,
                1.0,
                "entry_fill",
            )),
        }];

        let exceptions =
            managed_position_deficit_exceptions("acct", &snapshot, &lanes, &latest_positions);

        assert!(exceptions.is_empty());
    }

    #[test]
    fn close_requested_and_reconciliation_sync_rows_are_not_authoritative_owners() {
        let lanes = vec![
            lane("lane-a", "bot-a", "acct", "BTCUSDT"),
            lane("lane-b", "bot-b", "acct", "BTCUSDT"),
        ];
        let latest_positions = vec![
            LatestLanePosition {
                lane_id: "lane-a".to_owned(),
                position: Some(position_with_state(
                    1,
                    "bot-a",
                    "BTCUSDT",
                    true,
                    1.0,
                    "close_requested",
                )),
            },
            LatestLanePosition {
                lane_id: "lane-b".to_owned(),
                position: Some(position_with_state(
                    2,
                    "bot-b",
                    "BTCUSDT",
                    true,
                    1.0,
                    "position_reconciliation_sync",
                )),
            },
        ];

        match connector_position_owner("acct", "BTC", &lanes, &latest_positions) {
            ConnectorPositionOwner::Ambiguous(owners) => {
                assert_eq!(owners, vec!["lane-a".to_owned(), "lane-b".to_owned()]);
            }
            other => panic!("unexpected owner resolution: {other:?}"),
        }
    }

    #[test]
    fn symbol_suffix_matching_does_not_alias_unrelated_assets() {
        match connector_position_owner(
            "acct",
            "ETH",
            &[lane("lane-a", "bot-a", "acct", "BETHUSDT")],
            &[],
        ) {
            ConnectorPositionOwner::None => {}
            other => panic!("unexpected owner resolution: {other:?}"),
        }
    }

    #[test]
    fn position_quantity_for_symbol_treats_base_asset_positions_as_symbol_positions() {
        let snapshot = ConnectorAccountSnapshot {
            open_orders: Vec::new(),
            positions: vec![ConnectorPosition {
                symbol: "BTC".to_owned(),
                quantity: 0.42,
            }],
            balances: Vec::new(),
        };

        assert!(position_quantity_for_symbol(&snapshot, "BTCUSDT") > POSITION_QUANTITY_TOLERANCE);
        assert!(position_quantity_for_symbol(&snapshot, "ETHUSDT") <= POSITION_QUANTITY_TOLERANCE);
    }

    #[test]
    fn account_risk_snapshot_sums_open_positions_and_daily_loss() {
        let mut lanes = vec![lane("lane-a", "bot-a", "acct", "BTCUSDT")];
        let mut flat = lane("lane-b", "bot-b", "acct", "ETHUSDT");
        flat.effective_position_quantity = 0.0;
        flat.daily_loss_pct_accumulated = 2.0;
        lanes.push(flat);

        let snapshot = account_risk_snapshot(&lanes);
        assert_eq!(snapshot.open_positions, 1);
        assert!((snapshot.daily_loss_pct - 3.5).abs() < 1e-6);
    }

    #[test]
    fn live_balance_from_binance_snapshot_includes_known_open_notional() {
        let snapshot = ConnectorAccountSnapshot {
            open_orders: Vec::new(),
            positions: vec![ConnectorPosition {
                symbol: "BTC".to_owned(),
                quantity: 0.5,
            }],
            balances: vec![ConnectorPrivateBalance {
                asset: "USDT".to_owned(),
                free: 400.0,
                locked: 100.0,
            }],
        };

        assert_eq!(
            live_balance_from_snapshot("binance", &snapshot, 250.0, &[]),
            Some(750.0)
        );
    }

    #[test]
    fn live_balance_from_binance_snapshot_respects_configured_cash_assets() {
        let snapshot = ConnectorAccountSnapshot {
            open_orders: Vec::new(),
            positions: vec![ConnectorPosition {
                symbol: "BTC".to_owned(),
                quantity: 0.5,
            }],
            balances: vec![
                ConnectorPrivateBalance {
                    asset: "USDT".to_owned(),
                    free: 400.0,
                    locked: 100.0,
                },
                ConnectorPrivateBalance {
                    asset: "USDC".to_owned(),
                    free: 80.0,
                    locked: 20.0,
                },
            ],
        };
        let cash_balance_assets = vec!["USDC".to_owned()];

        assert_eq!(
            live_balance_from_snapshot("binance", &snapshot, 250.0, &cash_balance_assets),
            Some(350.0)
        );
    }

    #[test]
    fn live_balance_from_snapshot_returns_none_when_binance_cash_is_effectively_zero() {
        let snapshot = ConnectorAccountSnapshot {
            open_orders: Vec::new(),
            positions: Vec::new(),
            balances: vec![ConnectorPrivateBalance {
                asset: "USDT".to_owned(),
                free: 5e-10,
                locked: 0.0,
            }],
        };

        assert_eq!(
            live_balance_from_snapshot("binance", &snapshot, 0.0, &[]),
            None
        );
    }

    #[test]
    fn symbol_base_asset_handles_fdusd_pairs() {
        assert_eq!(symbol_base_asset("BTCFDUSD"), Some("BTC"));
    }

    #[test]
    fn ledger_rooms_use_bot_and_account_tradeable_room() {
        let mut ledger = AccountLedger::new(1_000.0);
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        ledger.try_reserve_open(&owner, 250.0, 40.0).unwrap();

        let rooms = ledger_rooms(&ledger, "bot-a", 40.0);
        assert!((rooms.remaining_bot_usd - 150.0).abs() < 1e-6);
        assert!((rooms.remaining_account_usd - 750.0).abs() < 1e-6);
    }

    #[test]
    fn bot_ledger_rejection_payload_exposes_bot_snapshot_fields() {
        let mut ledger = AccountLedger::new(1_000.0);
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        ledger.try_reserve_open(&owner, 250.0, 40.0).unwrap();

        let payload = bot_ledger_rejection_payload(
            &ledger,
            "acct",
            "bot-a",
            40.0,
            "open_long",
            "bot_ledger_exhausted",
        );

        assert_eq!(payload.intent, "open_long");
        assert_eq!(payload.decision, "rejected");
        assert_eq!(payload.reason, "bot_ledger_exhausted");
        assert_opt_f64_close(payload.committed_usd, 250.0);
        assert_opt_f64_close(payload.allocated_usd, 400.0);
        assert_opt_f64_close(payload.tradeable_room_usd, 150.0);
    }
}
