use crate::positions::{
    authoritative_position_quantity, latest_position_for_lane,
    position_reason_is_reconciliation_sync, position_record_indicates_open,
};
use crate::symbols::connector_position_matches_symbol;
use crate::{LatestLanePosition, POSITION_QUANTITY_TOLERANCE, PortfolioLaneView};
use openticker_connectors::ConnectorAccountSnapshot;

#[derive(Debug, Clone)]
pub enum ConnectorPositionOwner {
    None,
    Unique(String),
    Ambiguous(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountSymbolExposure {
    pub symbol: String,
    pub remote_net_qty: f64,
    pub aggregate_managed_qty: f64,
    pub external_delta_qty: f64,
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

fn remote_net_qty_for_symbol(snapshot: &ConnectorAccountSnapshot, symbol: &str) -> f64 {
    position_quantity_for_symbol(snapshot, symbol)
}

pub(crate) fn sanitize_quantity_delta(value: f64) -> f64 {
    if value.abs() <= POSITION_QUANTITY_TOLERANCE {
        0.0
    } else {
        value
    }
}
