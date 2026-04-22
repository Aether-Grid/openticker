use super::orchestrator::ReconciliationEngine;
use crate::{
    InstanceSummary, LaneRuntimeState, POSITION_QUANTITY_TOLERANCE, ReconciliationAssessment,
    ReconciliationSyncOutcome, ServiceError, TradeIntent, calculate_position_notional_usd,
    sync_inventory_from_runtime_fields,
};
use std::collections::HashSet;

impl ReconciliationEngine<'_> {
    pub(super) fn mark_startup_reconciliation_failed(
        &mut self,
        instance_id: &str,
        error: &ServiceError,
    ) -> Result<(), ServiceError> {
        {
            let instance = self.repo.instance_mut(instance_id)?;
            instance.state = LaneRuntimeState::Paused;
            instance.reconciliation_blocked = true;
            instance.resume_after_startup_reconcile = false;
        }

        let bot_id = self.repo.as_read().instance(instance_id)?.config.id.clone();
        let summary = self.repo.as_read().get_instance(&bot_id)?;
        self.repo.as_read().persist_instance_state(&summary)?;

        let detail = format!("source=startup,error={error}");
        self.repo.as_read().append_instance_event(
            instance_id,
            "instance.reconcile_failed",
            summary.state,
            &detail,
        )?;
        self.repo.as_read().append_runtime_event(
            "reconcile",
            Some(instance_id),
            "reconcile.failed",
            detail,
        )?;
        Ok(())
    }

    pub(super) fn sync_reconciliation_state(
        &mut self,
        instance_id: &str,
        source: &str,
        assessment: &ReconciliationAssessment,
    ) -> Result<ReconciliationSyncOutcome, ServiceError> {
        if !matches!(source, "manual" | "startup") || !assessment.connector_snapshot_available {
            return Ok(ReconciliationSyncOutcome::default());
        }

        let mut outcome = ReconciliationSyncOutcome::default();
        let current_realized_pnl_usd = self
            .repo
            .as_read()
            .latest_authoritative_position_for_instance(instance_id)?
            .map_or(
                self.repo.as_read().instance(instance_id)?.realized_pnl_usd,
                |position| position.realized_pnl_usd,
            );
        {
            let instance = self.repo.instance_mut(instance_id)?;
            instance.realized_pnl_usd = current_realized_pnl_usd;
            sync_inventory_from_runtime_fields(instance);
        }
        let (bot_id, symbol) = {
            let repo = self.repo.as_read();
            let instance = repo.instance(instance_id)?;
            (instance.config.id.clone(), instance.lane_symbol.clone())
        };
        let latest_raw_position = self
            .repo
            .journal
            .latest_position_for_lane(&bot_id, &symbol)
            .map_err(ServiceError::from)?;
        let should_refresh_position = latest_raw_position.as_ref().is_some_and(|position| {
            position.reason.ends_with("_reconciliation_sync")
                && (position.has_position != assessment.positions.resolved_has_position
                    || (position.quantity - assessment.positions.resolved_position_quantity).abs()
                        > POSITION_QUANTITY_TOLERANCE
                    || position.entry_price != assessment.positions.resolved_entry_price)
        });
        if should_refresh_position {
            let reason = format!("{source}_managed_state_refresh");
            self.repo.as_read().append_position_record(
                instance_id,
                assessment.positions.resolved_has_position,
                assessment.positions.resolved_position_quantity,
                assessment.positions.resolved_entry_price,
                current_realized_pnl_usd,
                &reason,
            )?;
            outcome.position_synced = true;
        }

        let connector_open_order_ids = assessment
            .connector_open_orders_detail
            .iter()
            .map(|order| order.client_order_id.as_str())
            .collect::<HashSet<_>>();
        for client_order_id in &assessment.local_open_order_ids {
            if !connector_open_order_ids.contains(client_order_id.as_str()) {
                self.repo.as_read().append_fill_record(
                    instance_id,
                    client_order_id,
                    0.0,
                    0.0,
                    None,
                    None,
                    None,
                )?;
                outcome.stale_local_open_orders_closed_count = outcome
                    .stale_local_open_orders_closed_count
                    .saturating_add(1);
            }
        }

        let local_open_order_ids = assessment
            .local_open_order_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        for order in &assessment.connector_open_orders_detail {
            if !local_open_order_ids.contains(order.client_order_id.as_str()) {
                let has_existing_order = self
                    .repo
                    .as_read()
                    .orders_by_client_order_id(order.client_order_id.as_str())?
                    .into_iter()
                    .any(|record| {
                        record.bot_id == bot_id && record.symbol.as_deref() == Some(symbol.as_str())
                    });
                if has_existing_order {
                    continue;
                }
                self.repo.as_read().append_order_record(
                    instance_id,
                    order.client_order_id.as_str(),
                    TradeIntent::NoOp,
                    order.status.as_str(),
                    0.0,
                    order.quantity,
                )?;
                outcome.remote_open_orders_backfilled_count = outcome
                    .remote_open_orders_backfilled_count
                    .saturating_add(1);
            }
        }

        Ok(outcome)
    }

    pub(super) fn apply_reconciliation_assessment_state(
        &mut self,
        instance_id: &str,
        assessment: &ReconciliationAssessment,
    ) -> Result<(), ServiceError> {
        let instance = self.repo.instance_mut(instance_id)?;
        instance.has_position = assessment.positions.resolved_has_position;
        instance.position_quantity = assessment.positions.resolved_position_quantity;
        if instance.has_position {
            instance.entry_price = assessment.positions.resolved_entry_price;
            if instance.position_notional_usd <= 0.0 {
                instance.position_notional_usd = if let Some(entry_price) = instance.entry_price {
                    calculate_position_notional_usd(instance.position_quantity, entry_price)
                } else {
                    instance.position_quantity
                };
            }
        } else {
            instance.position_notional_usd = 0.0;
            instance.entry_price = None;
        }
        sync_inventory_from_runtime_fields(instance);
        instance.reconciliation_blocked = !assessment.safe_to_trade;
        instance.remote_net_qty = assessment.remote_net_qty;
        instance.aggregate_managed_qty = assessment.aggregate_managed_qty;
        instance.external_delta_qty = assessment.external_delta_qty;
        instance.managed_remote_open_orders = assessment.managed_remote_open_orders;
        instance.external_remote_open_orders = assessment.external_remote_open_orders;
        Ok(())
    }

    pub(super) fn refresh_account_ledger_for_assessment(
        &self,
        account_id: &str,
        assessment: &ReconciliationAssessment,
    ) -> Result<(), ServiceError> {
        if let Some(snapshot) = assessment.connector_snapshot.as_ref() {
            self.repo
                .as_read()
                .refresh_account_ledger_from_snapshot(account_id, snapshot)
        } else {
            self.repo
                .as_read()
                .sync_account_ledger_from_instances(account_id)
        }
    }

    pub(super) fn finalize_reconciliation_result(
        &mut self,
        instance_id: &str,
        source: &str,
        assessment: &ReconciliationAssessment,
        sync: &ReconciliationSyncOutcome,
    ) -> Result<InstanceSummary, ServiceError> {
        let event_kind = if assessment.safe_to_trade {
            "instance.reconciled"
        } else {
            "instance.reconcile_unresolved"
        };
        let target_state = if matches!(source, "startup")
            && assessment.safe_to_trade
            && self
                .repo
                .as_read()
                .instance(instance_id)?
                .resume_after_startup_reconcile
        {
            LaneRuntimeState::Running
        } else {
            LaneRuntimeState::Paused
        };
        let summary =
            self.repo
                .reborrow()
                .transition_instance(instance_id, target_state, event_kind)?;

        if matches!(source, "startup") {
            self.repo
                .instance_mut(instance_id)?
                .resume_after_startup_reconcile = false;
        }

        self.repo
            .as_read()
            .append_reconciliation_record(instance_id, source, assessment)?;

        let detail = format!(
            "source={source},symbol={},safe_to_trade={},local_open_orders={},connector_open_orders={},managed_remote_open_orders={},external_remote_open_orders={},remote_net_qty={:?},aggregate_managed_qty={},external_delta_qty={:?},local_has_position={},connector_has_position={},position_synced={},stale_local_open_orders_closed_count={},remote_open_orders_backfilled_count={},reason={}",
            assessment.symbol,
            assessment.safe_to_trade,
            assessment.local_open_orders,
            assessment.connector_open_orders,
            assessment.managed_remote_open_orders,
            assessment.external_remote_open_orders,
            assessment.remote_net_qty,
            assessment.aggregate_managed_qty,
            assessment.external_delta_qty,
            assessment.positions.local_has_position,
            assessment.positions.connector_has_position,
            sync.position_synced,
            sync.stale_local_open_orders_closed_count,
            sync.remote_open_orders_backfilled_count,
            assessment.reason,
        );
        self.repo.as_read().append_instance_event(
            instance_id,
            "instance.reconcile_result",
            summary.state,
            &detail,
        )?;
        self.repo.as_read().append_runtime_event(
            "reconcile",
            Some(instance_id),
            if assessment.safe_to_trade {
                "reconcile.ok"
            } else {
                "reconcile.blocked"
            },
            detail,
        )?;

        Ok(summary)
    }
}
