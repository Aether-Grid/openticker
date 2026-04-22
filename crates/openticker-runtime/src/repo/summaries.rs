use super::RuntimeRepoRead;
use crate::{
    InstancePnlStatus, InstancePositionStatus, InstanceSummary, InstanceWarmupStatus,
    LaneRecoveryState, LaneRecoveryStatus, LaneRuntime, LaneSummary, POSITION_QUANTITY_TOLERANCE,
    ReconciliationCheck, ReconciliationRecord, ServiceError, SymbolReconciliationSummary,
    aggregate_bot_state, current_instance_open_notional_usd, effective_position_quantity,
    execution_mode_is_live, mode_banner_text,
};
use openticker_portfolio::reconciliation_differences;

impl RuntimeRepoRead<'_> {
    /// Returns the per-symbol lane summaries for the requested bot instance.
    ///
    /// # Errors
    ///
    /// Returns an error when the bot is unknown or its lane set cannot be resolved.
    pub(crate) fn lane_summaries_for_bot(
        &self,
        bot_id: &str,
    ) -> Result<Vec<LaneSummary>, ServiceError> {
        let mut lanes = self
            .lanes_for_bot(bot_id)?
            .into_iter()
            .map(Self::lane_summary_from_runtime)
            .collect::<Vec<_>>();
        lanes.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        Ok(lanes)
    }

    #[must_use]
    pub(crate) fn list_instances(&self) -> Vec<InstanceSummary> {
        let mut values = self
            .catalog
            .lanes_by_bot
            .keys()
            .filter_map(|bot_id| {
                self.lanes_for_bot(bot_id)
                    .ok()
                    .map(|lanes| Self::summary_from_bot(bot_id, &lanes))
            })
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.id.cmp(&right.id));
        values
    }

    pub(crate) fn get_instance(&self, instance_id: &str) -> Result<InstanceSummary, ServiceError> {
        let lanes = self.lanes_for_bot(instance_id)?;
        Ok(Self::summary_from_bot(instance_id, &lanes))
    }

    pub(crate) fn lane_summary_from_runtime(runtime: &LaneRuntime) -> LaneSummary {
        LaneSummary {
            bot_id: runtime.config.id.clone(),
            symbol: runtime.lane_symbol.clone(),
            state: runtime.state,
            has_position: runtime.has_position,
            quantity: effective_position_quantity(runtime),
            entry_price: runtime.inventory.average_cost_usd().or(runtime.entry_price),
            realized_pnl_usd: runtime.inventory.realized_pnl.net_usd,
            warmup: warmup_status_from_state(&runtime.warmup),
            recovery: lane_recovery_status_from_runtime(runtime),
            reconciliation_blocked: runtime.reconciliation_blocked,
            remote_net_qty: runtime.remote_net_qty,
            aggregate_managed_qty: runtime.aggregate_managed_qty,
            external_delta_qty: runtime.external_delta_qty,
            managed_remote_open_orders: runtime.managed_remote_open_orders,
            external_remote_open_orders: runtime.external_remote_open_orders,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn summary_from_bot(bot_id: &str, lanes: &[&LaneRuntime]) -> InstanceSummary {
        let runtime = lanes
            .first()
            .expect("summary_from_bot requires at least one lane runtime");
        let live_mode_active = execution_mode_is_live(runtime.execution_mode);
        let open_lanes = lanes
            .iter()
            .filter(|lane| effective_position_quantity(lane) > POSITION_QUANTITY_TOLERANCE)
            .collect::<Vec<_>>();
        let warmup_last_error = lanes.iter().find_map(|lane| lane.warmup.last_error.clone());
        let last_warmup_timestamp = lanes
            .iter()
            .filter_map(|lane| lane.warmup.last_warmup_timestamp)
            .max();
        let warmup = if lanes.len() == 1 {
            warmup_status_from_state(&runtime.warmup)
        } else {
            InstanceWarmupStatus {
                required_bars: lanes.len(),
                loaded_bars: lanes.iter().filter(|lane| lane.warmup.ready).count(),
                ready: lanes.iter().all(|lane| lane.warmup.ready),
                last_error: warmup_last_error,
                last_warmup_timestamp: last_warmup_timestamp
                    .map(|timestamp| timestamp.to_rfc3339()),
            }
        };
        let recovery = if lanes.len() == 1 {
            lane_recovery_status_from_runtime(runtime)
        } else {
            LaneRecoveryStatus {
                state: aggregate_recovery_state(lanes),
                trading_blocked_by_recovery: lanes
                    .iter()
                    .any(|lane| lane.recovery_state != LaneRecoveryState::Healthy),
                started_at_ms: lanes
                    .iter()
                    .filter_map(|lane| lane.recovery_started_at_ms)
                    .min(),
                target_timestamp: lanes
                    .iter()
                    .filter_map(|lane| lane.recovery_target_timestamp)
                    .max()
                    .map(|timestamp| timestamp.to_rfc3339()),
                last_progress_timestamp: lanes
                    .iter()
                    .filter_map(|lane| lane.recovery_last_progress_timestamp)
                    .max()
                    .map(|timestamp| timestamp.to_rfc3339()),
                last_error: lanes
                    .iter()
                    .find_map(|lane| lane.recovery_last_error.clone()),
                consecutive_no_progress_cycles: lanes
                    .iter()
                    .map(|lane| lane.recovery_consecutive_no_progress_cycles)
                    .max()
                    .unwrap_or(0),
                last_recovered_at_timestamp: lanes
                    .iter()
                    .filter_map(|lane| lane.last_recovered_at_timestamp)
                    .max()
                    .map(|timestamp| timestamp.to_rfc3339()),
            }
        };
        let mut reconciliation_by_symbol = lanes
            .iter()
            .map(|lane| SymbolReconciliationSummary {
                symbol: lane.lane_symbol.clone(),
                reconciliation_blocked: lane.reconciliation_blocked,
                remote_net_qty: lane.remote_net_qty,
                aggregate_managed_qty: lane.aggregate_managed_qty,
                external_delta_qty: lane.external_delta_qty,
                managed_remote_open_orders: lane.managed_remote_open_orders,
                external_remote_open_orders: lane.external_remote_open_orders,
            })
            .collect::<Vec<_>>();
        reconciliation_by_symbol.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        InstanceSummary {
            id: bot_id.to_owned(),
            enabled: runtime.config.enabled,
            market: runtime.config.market,
            symbols: runtime.config.symbols.clone(),
            symbol: runtime.config.symbols.first().cloned().unwrap_or_default(),
            timeframe: runtime.config.timeframe,
            account: runtime.config.account.clone(),
            execution_mode: runtime.execution_mode,
            live_mode_active,
            mode_banner: mode_banner_text(live_mode_active).to_owned(),
            state: aggregate_bot_state(lanes),
            position: InstancePositionStatus {
                has_position: !open_lanes.is_empty(),
                quantity: open_lanes
                    .iter()
                    .map(|lane| effective_position_quantity(lane))
                    .sum(),
                entry_price: if open_lanes.len() == 1 {
                    open_lanes[0]
                        .inventory
                        .average_cost_usd()
                        .or(open_lanes[0].entry_price)
                } else {
                    None
                },
            },
            pnl: InstancePnlStatus {
                realized_usd: lanes
                    .iter()
                    .map(|lane| lane.inventory.realized_pnl.net_usd)
                    .sum(),
                realized_gross_usd: lanes
                    .iter()
                    .map(|lane| lane.inventory.realized_pnl.gross_usd)
                    .sum(),
                realized_fees_usd: lanes
                    .iter()
                    .map(|lane| lane.inventory.realized_pnl.fees_usd)
                    .sum(),
            },
            open_symbol_count: open_lanes.len(),
            aggregate_position_notional_usd: lanes
                .iter()
                .map(|lane| current_instance_open_notional_usd(lane))
                .sum(),
            aggregate_realized_pnl_usd: lanes
                .iter()
                .map(|lane| lane.inventory.realized_pnl.net_usd)
                .sum(),
            reconciliation_blocked: lanes.iter().any(|lane| lane.reconciliation_blocked),
            reconciliation_by_symbol,
            warmup,
            recovery,
            warmup_ready_symbols: lanes.iter().filter(|lane| lane.warmup.ready).count(),
            polling_enabled: runtime.config.polling_enabled,
            polling_interval_ms: runtime.config.polling_interval_ms,
        }
    }

    pub(crate) fn reconciliation_check_from_record(
        record: ReconciliationRecord,
    ) -> ReconciliationCheck {
        let differences = reconciliation_differences(&record.reason);
        ReconciliationCheck {
            id: record.id,
            source: record.source,
            symbol: record.symbol,
            safe_to_trade: record.safe_to_trade,
            local_open_orders: record.local_open_orders,
            connector_open_orders: record.connector_open_orders,
            local_has_position: record.local_has_position,
            connector_has_position: record.connector_has_position,
            reason: record.reason,
            differences,
            created_at_ms: record.created_at_ms,
        }
    }
}

fn lane_recovery_status_from_runtime(runtime: &LaneRuntime) -> LaneRecoveryStatus {
    LaneRecoveryStatus {
        state: runtime.recovery_state,
        trading_blocked_by_recovery: runtime.recovery_state != LaneRecoveryState::Healthy,
        started_at_ms: runtime.recovery_started_at_ms,
        target_timestamp: runtime
            .recovery_target_timestamp
            .map(|timestamp| timestamp.to_rfc3339()),
        last_progress_timestamp: runtime
            .recovery_last_progress_timestamp
            .map(|timestamp| timestamp.to_rfc3339()),
        last_error: runtime.recovery_last_error.clone(),
        consecutive_no_progress_cycles: runtime.recovery_consecutive_no_progress_cycles,
        last_recovered_at_timestamp: runtime
            .last_recovered_at_timestamp
            .map(|timestamp| timestamp.to_rfc3339()),
    }
}

fn warmup_status_from_state(warmup: &openticker_lane::InstanceWarmupState) -> InstanceWarmupStatus {
    InstanceWarmupStatus {
        required_bars: warmup.required_bars,
        loaded_bars: warmup.loaded_bars,
        ready: warmup.ready,
        last_error: warmup.last_error.clone(),
        last_warmup_timestamp: warmup
            .last_warmup_timestamp
            .map(|timestamp| timestamp.to_rfc3339()),
    }
}

fn aggregate_recovery_state(lanes: &[&LaneRuntime]) -> LaneRecoveryState {
    if lanes
        .iter()
        .any(|lane| lane.recovery_state == LaneRecoveryState::OutOfSync)
    {
        LaneRecoveryState::OutOfSync
    } else if lanes
        .iter()
        .any(|lane| lane.recovery_state == LaneRecoveryState::CatchingUp)
    {
        LaneRecoveryState::CatchingUp
    } else {
        LaneRecoveryState::Healthy
    }
}
