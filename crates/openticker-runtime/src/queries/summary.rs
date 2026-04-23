use crate::{
    ConnectorRuntimeStatus, InstanceSummary, LaneRuntimeState, LaneSummary, Runtime, ServiceError,
    ServiceStatus, connector_resilience_window_active, mode_banner_text,
};
use tracing::error;

impl Runtime {
    #[must_use]
    pub fn kill_switch_active(&self) -> bool {
        self.state.kill_switch_active
    }

    /// Returns the per-symbol lane summaries for the requested bot instance.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is missing.
    pub fn lane_summaries_for_bot(&self, bot_id: &str) -> Result<Vec<LaneSummary>, ServiceError> {
        self.repo().lane_summaries_for_bot(bot_id)
    }

    #[must_use]
    pub fn list_instances(&self) -> Vec<InstanceSummary> {
        self.repo().list_instances()
    }

    /// Fetches a single instance summary by id.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is not found.
    pub fn get_instance(&self, instance_id: &str) -> Result<InstanceSummary, ServiceError> {
        self.repo().get_instance(instance_id)
    }

    #[must_use]
    pub fn status(&self) -> ServiceStatus {
        let total_instances = self.catalog.lanes_by_bot.len();
        let connector_statuses = self.connector_statuses();
        let connector_resilience_windows_active = connector_statuses
            .iter()
            .filter(|status| connector_resilience_window_active(status))
            .count();
        let live_mode_active = connector_statuses
            .iter()
            .any(|status| status.live_mode_active);
        let summaries = self.list_instances();
        let mut running_instances = 0usize;
        let mut paused_instances = 0usize;
        let mut stopped_instances = 0usize;
        let mut reconciling_instances = 0usize;
        let mut reconciliation_blocked_instances = 0usize;
        let mut warmup_ready_instances = 0usize;
        let mut warmup_pending_instances = 0usize;
        for summary in &summaries {
            match summary.state {
                LaneRuntimeState::Running => running_instances += 1,
                LaneRuntimeState::Paused => paused_instances += 1,
                LaneRuntimeState::Stopped => stopped_instances += 1,
                LaneRuntimeState::Reconciling => reconciling_instances += 1,
            }
            if summary.reconciliation_blocked {
                reconciliation_blocked_instances += 1;
            }
            if summary.warmup_ready_symbols == summary.symbols.len() {
                warmup_ready_instances += 1;
            } else {
                warmup_pending_instances += 1;
            }
        }

        let mut any_lane_unready = false;
        let mut warmup_failed_bots = std::collections::HashSet::<&str>::new();
        for instance in self.state.lanes.values() {
            if instance.warmup.last_error.is_some() {
                warmup_failed_bots.insert(instance.config.id.as_str());
            }
            if matches!(instance.state, LaneRuntimeState::Reconciling)
                || instance.reconciliation_blocked
                || instance.recovery_state != crate::LaneRecoveryState::Healthy
            {
                any_lane_unready = true;
            }
        }
        let connectors_ready = self.connector_gateway().connectors_ready().unwrap_or(false);

        ServiceStatus {
            total_instances,
            running_instances,
            paused_instances,
            stopped_instances,
            reconciling_instances,
            reconciliation_blocked_instances,
            warmup_ready_instances,
            warmup_pending_instances,
            warmup_failed_instances: warmup_failed_bots.len(),
            kill_switch_active: self.state.kill_switch_active,
            ready: !any_lane_unready && connectors_ready,
            live_mode_active,
            mode_banner: mode_banner_text(live_mode_active).to_owned(),
            connector_resilience_windows_active,
            observability: self.state.observability.status(),
            connector_statuses,
        }
    }

    #[must_use]
    pub fn connector_statuses(&self) -> Vec<ConnectorRuntimeStatus> {
        match self.connector_gateway().runtime_statuses() {
            Ok(statuses) => statuses,
            Err(error) => {
                error!(error = %error, "failed to read connector statuses");
                Vec::new()
            }
        }
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        let all_instances_ready = !self.state.lanes.values().any(|instance| {
            matches!(instance.state, LaneRuntimeState::Reconciling)
                || instance.reconciliation_blocked
                || instance.recovery_state != crate::LaneRecoveryState::Healthy
        });
        let connectors_ready = self.connector_gateway().connectors_ready().unwrap_or(false);

        all_instances_ready && connectors_ready
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{fixture_bundle, replay_closes, test_bar};
    use crate::{POSITION_QUANTITY_TOLERANCE, Runtime};
    use openticker_core::{IndicatorSignal, SignalPhase};

    #[test]
    fn status_reports_latency_markers_after_processing_bars() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").expect("aapl should start");

        for close in replay_closes() {
            let bar = test_bar(close);
            let _ = runtime
                .process_bar("aapl", &bar, SignalPhase::Confirmed)
                .expect("process_bar should succeed");
        }

        let status = runtime.status();
        assert_eq!(status.connector_resilience_windows_active, 0);
        assert!(status.observability.process_bar_latency_samples > 0);
        assert!(status.observability.process_bar_latency_ms_last.is_some());
        assert!(status.observability.execution_submit_latency_samples > 0);
        assert!(
            status
                .observability
                .execution_submit_latency_ms_last
                .is_some()
        );
    }

    #[test]
    fn status_tracks_risk_reject_totals() {
        let mut config = fixture_bundle();
        config.risk_profiles[0].max_daily_loss_pct = 0.0;

        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").expect("aapl should start");

        for close in replay_closes() {
            let bar = test_bar(close);
            let _ = runtime
                .process_bar("aapl", &bar, SignalPhase::Confirmed)
                .expect("process_bar should succeed");
        }

        let risks = runtime
            .recent_risk_decisions(500)
            .expect("risk decisions should load");
        assert!(risks.iter().any(|record| record.decision == "rejected"));

        let status = runtime.status();
        assert!(status.observability.risk_rejects_total > 0);
    }

    #[test]
    fn status_counts_states_in_single_pass() {
        let mut config = fixture_bundle();
        let base = config.instances[0].clone();
        for idx in 1..4 {
            let mut twin = base.clone();
            twin.id = format!("aapl-{idx}");
            config.instances.push(twin);
        }

        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").expect("start aapl");
        runtime.start_instance("aapl-1").expect("start aapl-1");
        runtime.pause_instance("aapl-1").expect("pause aapl-1");

        let status = runtime.status();
        assert_eq!(status.total_instances, 4);
        assert_eq!(status.running_instances, 1);
        assert_eq!(status.paused_instances, 1);
        assert_eq!(status.stopped_instances, 2);
    }

    #[test]
    fn instance_summary_reports_current_position_state() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").expect("aapl should start");

        runtime
            .process_manual_signal(
                "aapl",
                IndicatorSignal::BuyConfirmed,
                123.45,
                chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                    .expect("timestamp should parse")
                    .with_timezone(&chrono::Utc),
            )
            .expect("manual signal should process");

        let opened = runtime.get_instance("aapl").expect("instance should exist");
        assert!(opened.position.has_position);
        assert!(opened.position.quantity > POSITION_QUANTITY_TOLERANCE);
        assert_eq!(opened.position.entry_price, Some(123.45));

        runtime
            .close_positions("aapl")
            .expect("closing positions should succeed");

        let closed = runtime.get_instance("aapl").expect("instance should exist");
        assert!(!closed.position.has_position);
        assert!(closed.position.quantity <= POSITION_QUANTITY_TOLERANCE);
        assert_eq!(closed.position.entry_price, None);
    }

    #[test]
    fn multi_symbol_instance_summary_does_not_alias_one_lane_reconciliation_fields() {
        let mut config = fixture_bundle();
        config.instances[0].symbols = vec!["AAPL".to_owned(), "MSFT".to_owned()];

        let mut runtime = Runtime::from_config(&config);
        for lane in runtime.state.lanes.values_mut() {
            if lane.lane_symbol == "AAPL" {
                lane.remote_net_qty = Some(10.0);
                lane.aggregate_managed_qty = 6.0;
                lane.external_delta_qty = Some(4.0);
                lane.managed_remote_open_orders = 2;
                lane.external_remote_open_orders = 1;
            } else if lane.lane_symbol == "MSFT" {
                lane.remote_net_qty = Some(7.0);
                lane.aggregate_managed_qty = 5.0;
                lane.external_delta_qty = Some(2.0);
                lane.managed_remote_open_orders = 1;
                lane.external_remote_open_orders = 3;
            }
        }

        let summary = runtime.get_instance("aapl").expect("instance should exist");
        assert_eq!(summary.symbols, vec!["AAPL".to_owned(), "MSFT".to_owned()]);
        assert_eq!(summary.reconciliation_by_symbol.len(), 2);
        assert!(summary.reconciliation_by_symbol.iter().any(|item| {
            item.symbol == "AAPL"
                && item.remote_net_qty == Some(10.0)
                && (item.aggregate_managed_qty - 6.0).abs() < POSITION_QUANTITY_TOLERANCE
                && item.external_delta_qty == Some(4.0)
                && item.managed_remote_open_orders == 2
                && item.external_remote_open_orders == 1
        }));
        assert!(summary.reconciliation_by_symbol.iter().any(|item| {
            item.symbol == "MSFT"
                && item.remote_net_qty == Some(7.0)
                && (item.aggregate_managed_qty - 5.0).abs() < POSITION_QUANTITY_TOLERANCE
                && item.external_delta_qty == Some(2.0)
                && item.managed_remote_open_orders == 1
                && item.external_remote_open_orders == 3
        }));

        let lanes = runtime
            .lane_summaries_for_bot("aapl")
            .expect("lane summaries should load");
        assert_eq!(lanes.len(), 2);
        assert!(lanes.iter().any(|lane| {
            lane.symbol == "AAPL"
                && lane.remote_net_qty == Some(10.0)
                && (lane.aggregate_managed_qty - 6.0).abs() < POSITION_QUANTITY_TOLERANCE
                && lane.external_delta_qty == Some(4.0)
        }));
        assert!(lanes.iter().any(|lane| {
            lane.symbol == "MSFT"
                && lane.remote_net_qty == Some(7.0)
                && (lane.aggregate_managed_qty - 5.0).abs() < POSITION_QUANTITY_TOLERANCE
                && lane.external_delta_qty == Some(2.0)
        }));
    }
}
