use crate::exposure::sanitize_quantity_delta;
use crate::{
    ClassifiedRemoteOpenOrders, LatestLanePosition, POSITION_QUANTITY_TOLERANCE, PortfolioLaneView,
    account_symbol_exposure,
};
use openticker_connectors::ConnectorAccountSnapshot;
use openticker_ledger::{LedgerException, LedgerExceptionKind};

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
