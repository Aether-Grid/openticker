use crate::{AccountSymbolExposure, ClassifiedRemoteOpenOrders, POSITION_QUANTITY_TOLERANCE};
use openticker_connectors::{ConnectorAccountSnapshot, ConnectorOpenOrder};
use openticker_storage::PositionRecord;

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
