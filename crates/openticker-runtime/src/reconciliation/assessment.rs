use super::orchestrator::ReconciliationEngineRead;
use crate::{RECONCILIATION_JOURNAL_SCAN_LIMIT, ReconciliationAssessment, ServiceError};
use openticker_portfolio::{
    local_open_order_ids as collect_local_open_order_ids, open_orders_for_symbol,
};
use std::collections::HashSet;

impl ReconciliationEngineRead<'_> {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn assess_reconciliation(
        &self,
        instance_id: &str,
    ) -> Result<ReconciliationAssessment, ServiceError> {
        let (account_id, bot_id, symbol, instance_position_quantity) = {
            let instance = self.repo.instance(instance_id)?;
            (
                instance.config.account.clone(),
                instance.config.id.clone(),
                instance.lane_symbol.clone(),
                instance.position_quantity,
            )
        };
        let use_remote_snapshot = self
            .repo
            .catalog
            .accounts
            .get(account_id.as_str())
            .is_some_and(|account| account.reconciliation_remote_snapshot);

        let latest_local_position = self
            .repo
            .latest_authoritative_position_for_instance(instance_id)?;

        let local_orders = self
            .repo
            .recent_orders_for_bot(&bot_id, RECONCILIATION_JOURNAL_SCAN_LIMIT)?;
        let local_orders = local_orders
            .into_iter()
            .filter(|order| order.symbol.as_deref() == Some(symbol.as_str()))
            .collect::<Vec<_>>();
        let local_fill_ids = self
            .repo
            .recent_fills_for_bot(&bot_id, RECONCILIATION_JOURNAL_SCAN_LIMIT)?
            .into_iter()
            .filter(|fill| fill.symbol.as_deref() == Some(symbol.as_str()))
            .map(|fill| fill.client_order_id)
            .collect::<HashSet<_>>();
        let local_open_order_ids = collect_local_open_order_ids(&local_orders, &local_fill_ids);

        let account_lanes = self.repo.accounting_lanes_for_account(&account_id);
        let latest_positions = self.repo.latest_accounting_positions_for_account(
            &account_id,
            RECONCILIATION_JOURNAL_SCAN_LIMIT,
        )?;
        let aggregate_managed_qty = openticker_portfolio::account_symbol_exposure(
            &account_id,
            &symbol,
            &crate::ConnectorAccountSnapshot::default(),
            &account_lanes,
            &latest_positions,
        )
        .aggregate_managed_qty;
        let connector = self
            .connector_gateway()
            .fetch_account_snapshot_outcome(instance_id, &account_id)?;
        let connector_snapshot_available = use_remote_snapshot && connector.snapshot.is_some();
        let remote_open_orders = if use_remote_snapshot {
            connector
                .snapshot
                .as_ref()
                .map_or_else(Vec::new, |snapshot| {
                    open_orders_for_symbol(snapshot, &symbol)
                })
        } else {
            Vec::new()
        };
        let classified_orders =
            self.repo
                .classify_remote_open_orders(&account_id, &symbol, &remote_open_orders)?;
        let exposure = if use_remote_snapshot {
            connector.snapshot.as_ref().map(|snapshot| {
                openticker_portfolio::account_symbol_exposure(
                    &account_id,
                    &symbol,
                    snapshot,
                    &account_lanes,
                    &latest_positions,
                )
            })
        } else {
            None
        };
        let summary = openticker_portfolio::reconciliation_assessment_summary(
            &bot_id,
            instance_position_quantity,
            latest_local_position.as_ref(),
            local_open_order_ids,
            &classified_orders,
            if use_remote_snapshot {
                connector.unavailable_reason.as_deref()
            } else {
                None
            },
            exposure.as_ref(),
            aggregate_managed_qty,
        );

        Ok(openticker_portfolio::build_reconciliation_assessment(
            symbol,
            connector_snapshot_available,
            connector.snapshot,
            summary,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::POSITION_QUANTITY_TOLERANCE;
    use crate::Runtime;
    use crate::test_support::fixture_bundle;
    use openticker_storage::PositionWrite;

    #[test]
    fn startup_reconciliation_ignores_close_requested_position_rows() {
        let config = fixture_bundle();
        let runtime = Runtime::from_config(&config);

        runtime
            .journal
            .append_position(PositionWrite {
                bot_id: "aapl".to_owned(),
                symbol: "AAPL".to_owned(),
                trace_id: None,
                bar_timestamp: None,
                has_position: true,
                quantity: 1.0,
                entry_price: Some(123.45),
                realized_pnl_usd: 5.5,
                reason: "order_filled".to_owned(),
            })
            .expect("position should persist");
        runtime
            .journal
            .append_position(PositionWrite {
                bot_id: "aapl".to_owned(),
                symbol: "AAPL".to_owned(),
                trace_id: None,
                bar_timestamp: None,
                has_position: false,
                quantity: 0.0,
                entry_price: None,
                realized_pnl_usd: 5.5,
                reason: "close_requested".to_owned(),
            })
            .expect("position should persist");

        let latest_authoritative = runtime
            .repo()
            .latest_authoritative_position_for_instance("aapl")
            .expect("position lookup should succeed")
            .expect("position should exist");
        assert_eq!(latest_authoritative.reason, "order_filled");
        assert!(latest_authoritative.has_position);

        let assessment = runtime
            .reconciliation_engine()
            .assess_reconciliation("aapl")
            .expect("assessment should succeed");
        assert!(assessment.positions.local_has_position);
        assert!(
            (assessment.positions.resolved_position_quantity - 1.0).abs()
                < POSITION_QUANTITY_TOLERANCE
        );
    }
}
