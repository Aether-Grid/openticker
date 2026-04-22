use crate::{
    IndicatorSignal, InstanceSummary, POSITION_QUANTITY_TOLERANCE, ProcessBarRisk, Runtime,
    ServiceError, TradeIntent, effective_position_quantity, sync_remote_position_quantity,
    trade_intent_label,
};
use openticker_core::OhlcvBar;
use openticker_lane::{
    LaneManualOpsEngine, ManualCloseContext, ManualCloseOutcome, ManualCloseSignalOutcome,
    ManualCloseSignalRisk, close_lane_position as close_lane_position_workflow,
};
use openticker_portfolio::position_quantity_for_symbol;

struct RuntimeManualOpsEngine<'a> {
    runtime: &'a mut Runtime,
}

impl LaneManualOpsEngine for RuntimeManualOpsEngine<'_> {
    type Error = ServiceError;

    fn manual_close_context(&self, instance_id: &str) -> Result<ManualCloseContext, Self::Error> {
        let instance = self.runtime.instance(instance_id)?;
        Ok(ManualCloseContext {
            bot_id: instance.config.id.clone(),
            account_id: instance.config.account.clone(),
            reconciliation_remote_snapshot: self
                .runtime
                .catalog
                .accounts
                .get(&instance.config.account)
                .is_some_and(|account| account.reconciliation_remote_snapshot),
            has_local_position: effective_position_quantity(instance) > POSITION_QUANTITY_TOLERANCE,
        })
    }

    fn sync_remote_position_for_manual_close(
        &mut self,
        instance_id: &str,
        account_id: &str,
    ) -> Result<bool, Self::Error> {
        let (bot_id, symbol) = {
            let instance = self.runtime.instance(instance_id)?;
            (instance.config.id.clone(), instance.lane_symbol.clone())
        };

        self.runtime
            .connector_gateway()
            .ensure_account_connector_ready(instance_id, account_id)?;
        let snapshot_outcome = self
            .runtime
            .connector_gateway()
            .fetch_account_snapshot_outcome(instance_id, account_id)?;
        let snapshot = snapshot_outcome.snapshot.ok_or_else(|| {
            ServiceError::ExecutionConnectorUnavailable {
                instance_id: bot_id,
                account_id: account_id.to_owned(),
                reason: snapshot_outcome
                    .unavailable_reason
                    .unwrap_or_else(|| "connector snapshot unavailable".to_owned()),
            }
        })?;
        let remote_quantity = position_quantity_for_symbol(&snapshot, &symbol);
        let has_position = remote_quantity > POSITION_QUANTITY_TOLERANCE;

        let local_position_changed = {
            let instance = self.runtime.instance_mut(instance_id)?;
            sync_remote_position_quantity(instance, remote_quantity)
        };

        if local_position_changed {
            let (entry_price, realized_pnl_usd) = {
                let instance = self.runtime.instance(instance_id)?;
                (instance.entry_price, instance.realized_pnl_usd)
            };
            self.runtime.append_position_record(
                instance_id,
                has_position,
                remote_quantity,
                entry_price,
                realized_pnl_usd,
                "close_positions_sync",
            )?;
        }

        self.runtime
            .refresh_account_ledger_from_snapshot(account_id, &snapshot)?;
        Ok(has_position)
    }

    fn fetch_latest_bar_for_manual_close(
        &mut self,
        instance_id: &str,
    ) -> Result<OhlcvBar, Self::Error> {
        let (_, latest_bar) = self.runtime.fetch_latest_bar_for_lane(instance_id)?;
        Ok(latest_bar)
    }

    fn process_manual_close_signal(
        &mut self,
        instance_id: &str,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<ManualCloseSignalOutcome, Self::Error> {
        let outcome = self.runtime.process_manual_signal_for_lane(
            instance_id,
            IndicatorSignal::SellConfirmed,
            price,
            timestamp,
        )?;
        let risk = match outcome.risk {
            ProcessBarRisk::Allowed => ManualCloseSignalRisk::Allowed,
            ProcessBarRisk::Rejected { reason } => ManualCloseSignalRisk::Rejected { reason },
        };

        Ok(ManualCloseSignalOutcome {
            intent: outcome.intent,
            risk,
        })
    }
}

impl Runtime {
    /// Records a manual request to cancel all open orders for an instance.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is missing or event persistence fails.
    pub fn cancel_open_orders(
        &mut self,
        instance_id: &str,
    ) -> Result<InstanceSummary, ServiceError> {
        let summary = self.get_instance(instance_id)?;
        for lane_id in self.lane_ids_for_bot_cloned(instance_id)? {
            self.append_order_record(
                &lane_id,
                &format!("{lane_id}-manual-cancel"),
                TradeIntent::NoOp,
                "cancel_requested",
                0.0,
                0.0,
            )?;
            self.append_instance_event(
                &lane_id,
                "instance.cancel_open_orders",
                summary.state,
                "manual request",
            )?;
        }
        self.append_runtime_event(
            "order",
            Some(instance_id),
            "order.cancel_all_requested",
            "source=manual".to_owned(),
        )?;
        self.get_instance(instance_id)
    }

    /// Submits a manual close for the instance's current position using the
    /// normal execution path.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is missing, cannot fetch a usable
    /// price, or the downstream manual close path fails.
    #[allow(clippy::too_many_lines)]
    pub fn close_positions(&mut self, instance_id: &str) -> Result<InstanceSummary, ServiceError> {
        for lane_id in self.lane_ids_for_bot_cloned(instance_id)? {
            self.close_lane_position(&lane_id)?;
        }
        self.get_instance(instance_id)
    }

    #[allow(clippy::too_many_lines)]
    fn close_lane_position(&mut self, lane_id: &str) -> Result<InstanceSummary, ServiceError> {
        let bot_id = self.instance(lane_id)?.config.id.clone();
        let summary = self.get_instance(&bot_id)?;
        let workflow_outcome = {
            let mut engine = RuntimeManualOpsEngine { runtime: self };
            close_lane_position_workflow(&mut engine, lane_id)?
        };

        if matches!(workflow_outcome, ManualCloseOutcome::AlreadyFlat) {
            self.append_instance_event(
                lane_id,
                "instance.close_positions",
                summary.state,
                "already flat (connector)",
            )?;
            self.append_runtime_event(
                "position",
                Some(&bot_id),
                "position.close_all_skipped",
                "source=manual,reason=already_flat_connector".to_owned(),
            )?;
            return self.get_instance(&bot_id);
        }

        let ManualCloseOutcome::Processed {
            intent,
            risk,
            price,
            timestamp,
        } = workflow_outcome
        else {
            unreachable!("already-flat manual close handled above");
        };

        let detail = match risk {
            ManualCloseSignalRisk::Allowed => {
                if matches!(intent, TradeIntent::CloseLong | TradeIntent::ReduceLong) {
                    format!(
                        "manual close submitted intent={} price={} timestamp={}",
                        trade_intent_label(intent),
                        price,
                        timestamp.to_rfc3339(),
                    )
                } else {
                    "already flat".to_owned()
                }
            }
            ManualCloseSignalRisk::Rejected { reason } => {
                return Err(ServiceError::InvalidConfiguration(format!(
                    "close positions rejected for instance `{bot_id}`: {reason}"
                )));
            }
        };

        self.append_instance_event(lane_id, "instance.close_positions", summary.state, &detail)?;
        self.append_runtime_event(
            "position",
            Some(&bot_id),
            if matches!(intent, TradeIntent::CloseLong | TradeIntent::ReduceLong) {
                "position.close_all_submitted"
            } else {
                "position.close_all_skipped"
            },
            format!(
                "source=manual,intent={},price={},timestamp={}",
                trade_intent_label(intent),
                price,
                timestamp.to_rfc3339(),
            ),
        )?;

        self.get_instance(&bot_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_bundle;

    #[test]
    fn records_order_and_position_scope_events() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").unwrap();
        runtime
            .process_manual_signal(
                "aapl",
                IndicatorSignal::BuyConfirmed,
                123.45,
                chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            )
            .unwrap();

        runtime.cancel_open_orders("aapl").unwrap();
        runtime.close_positions("aapl").unwrap();

        let orders = runtime.recent_orders(10).unwrap();
        assert!(
            orders
                .iter()
                .any(|order| order.status == "cancel_requested")
        );

        let positions = runtime.recent_positions(10).unwrap();
        assert!(
            positions
                .iter()
                .any(|position| position.reason == "order_filled" && !position.has_position)
        );
    }

    #[test]
    fn close_positions_submits_a_sell_for_open_position() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").unwrap();

        runtime
            .process_manual_signal(
                "aapl",
                IndicatorSignal::BuyConfirmed,
                123.45,
                chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            )
            .unwrap();

        runtime.close_positions("aapl").unwrap();

        let orders = runtime.recent_orders(20).unwrap();
        assert!(orders.iter().any(|order| order.intent == "close_long"));
        assert!(
            !runtime
                .recent_positions(20)
                .unwrap()
                .iter()
                .any(|position| position.reason == "close_requested")
        );

        let instance = runtime.instance("aapl").unwrap();
        assert!(!instance.has_position);
        assert!(instance.position_quantity <= POSITION_QUANTITY_TOLERANCE);
    }

    #[test]
    fn close_positions_clears_stale_local_position_when_connector_is_flat() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").unwrap();
        runtime
            .catalog
            .accounts
            .get_mut("alpaca-paper")
            .unwrap()
            .reconciliation_remote_snapshot = true;

        {
            let instance = runtime.instance_mut("aapl").unwrap();
            instance.has_position = true;
            instance.position_quantity = 8.0;
            instance.position_notional_usd = 987.6;
            instance.entry_price = Some(123.45);
        }

        runtime.close_positions("aapl").unwrap();

        let orders = runtime.recent_orders(20).unwrap();
        assert!(!orders.iter().any(|order| order.intent == "close_long"));

        let positions = runtime.recent_positions(20).unwrap();
        assert!(
            positions
                .iter()
                .any(|position| position.reason == "close_positions_sync")
        );
        assert!(positions.iter().any(|position| !position.has_position));

        let instance = runtime.instance("aapl").unwrap();
        assert!(!instance.has_position);
        assert!(instance.position_quantity <= POSITION_QUANTITY_TOLERANCE);
        assert!(instance.position_notional_usd <= POSITION_QUANTITY_TOLERANCE);
        assert!(instance.entry_price.is_none());
    }
}
