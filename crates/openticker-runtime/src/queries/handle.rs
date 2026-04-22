use crate::{
    CycleTrace, CycleTraceSummary, FillRecord, InstanceSummary, IntentRecord, LaneSummary,
    OperatorReadModels, OrderRecord, PositionRecord, ReconciliationRecord, ReconciliationReport,
    RiskDecisionRecord, Runtime, RuntimeEvent, RuntimeJournal, ServiceError, SignalRecord,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RuntimeQueryHandle {
    journal: Arc<dyn RuntimeJournal>,
    read_models: Arc<OperatorReadModels>,
}

impl Runtime {
    #[must_use]
    pub fn query_handle(&self) -> RuntimeQueryHandle {
        RuntimeQueryHandle {
            journal: Arc::clone(&self.journal),
            read_models: Arc::clone(&self.read_models),
        }
    }
}

impl RuntimeQueryHandle {
    #[must_use]
    pub fn snapshot_recent_events(&self, limit: usize) -> Vec<RuntimeEvent> {
        self.read_models.recent_events(limit)
    }

    #[must_use]
    pub fn snapshot_recent_events_by_scope(&self, scope: &str, limit: usize) -> Vec<RuntimeEvent> {
        self.read_models.recent_events_by_scope(scope, limit)
    }

    #[must_use]
    pub fn snapshot_recent_events_for_entity(
        &self,
        entity_id: &str,
        limit: usize,
    ) -> Vec<RuntimeEvent> {
        self.read_models.recent_events_for_entity(entity_id, limit)
    }

    #[must_use]
    pub fn snapshot_recent_signals(&self, limit: usize) -> Vec<SignalRecord> {
        self.read_models.recent_signals(limit)
    }

    #[must_use]
    pub fn snapshot_recent_intents(&self, limit: usize) -> Vec<IntentRecord> {
        self.read_models.recent_intents(limit)
    }

    #[must_use]
    pub fn snapshot_recent_risk_decisions(&self, limit: usize) -> Vec<RiskDecisionRecord> {
        self.read_models.recent_risk_decisions(limit)
    }

    #[must_use]
    pub fn snapshot_recent_orders(&self, limit: usize) -> Vec<OrderRecord> {
        self.read_models.recent_orders(limit)
    }

    #[must_use]
    pub fn snapshot_recent_orders_for_bot(&self, bot_id: &str, limit: usize) -> Vec<OrderRecord> {
        self.read_models.recent_orders_for_bot(bot_id, limit)
    }

    #[must_use]
    pub fn snapshot_recent_fills(&self, limit: usize) -> Vec<FillRecord> {
        self.read_models.recent_fills(limit)
    }

    #[must_use]
    pub fn snapshot_recent_fills_for_bot(&self, bot_id: &str, limit: usize) -> Vec<FillRecord> {
        self.read_models.recent_fills_for_bot(bot_id, limit)
    }

    #[must_use]
    pub fn snapshot_recent_positions(&self, limit: usize) -> Vec<PositionRecord> {
        self.read_models.recent_positions(limit)
    }

    #[must_use]
    pub fn snapshot_recent_positions_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Vec<PositionRecord> {
        self.read_models.recent_positions_for_bot(bot_id, limit)
    }

    #[must_use]
    pub fn snapshot_latest_position_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Option<PositionRecord> {
        self.read_models.latest_position_for_lane(bot_id, symbol)
    }

    #[must_use]
    pub fn snapshot_latest_position_for_bot(&self, bot_id: &str) -> Option<PositionRecord> {
        self.read_models.latest_position_for_bot(bot_id)
    }

    #[must_use]
    pub fn snapshot_recent_reconciliations(&self, limit: usize) -> Vec<ReconciliationRecord> {
        self.read_models.recent_reconciliations(limit)
    }

    #[must_use]
    pub fn snapshot_recent_reconciliations_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Vec<ReconciliationRecord> {
        self.read_models
            .recent_reconciliations_for_bot(bot_id, limit)
    }

    #[must_use]
    pub fn snapshot_latest_reconciliation_for_bot(
        &self,
        bot_id: &str,
    ) -> Option<ReconciliationRecord> {
        self.read_models.latest_reconciliation_for_bot(bot_id)
    }

    /// Builds a reconciliation report using projected latest-state reads when available.
    ///
    /// # Errors
    ///
    /// Returns an error when fallback reconciliation reads from the journal fail.
    pub fn snapshot_reconciliation_report(
        &self,
        instance: InstanceSummary,
        lane_summaries: &[LaneSummary],
    ) -> Result<ReconciliationReport, ServiceError> {
        let mut lanes = Vec::new();
        for lane in lane_summaries {
            let record = self
                .read_models
                .latest_reconciliation_for_lane(&instance.id, &lane.symbol);
            let record = match record {
                Some(record) => Some(record),
                None => self
                    .journal
                    .latest_reconciliation_for_lane(&instance.id, &lane.symbol)
                    .map_err(ServiceError::from)?,
            };
            if let Some(record) = record {
                self.read_models
                    .ingest_reconciliations(std::slice::from_ref(&record));
                lanes.push(crate::repo::RuntimeRepoRead::reconciliation_check_from_record(record));
            }
        }
        lanes.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        let latest = lanes.iter().max_by_key(|check| check.id).cloned();

        Ok(ReconciliationReport {
            instance,
            latest,
            lanes,
        })
    }

    /// Returns the most recent runtime events from the journal.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_events(&self, limit: usize) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.journal
            .recent_events(limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent runtime events for a specific scope.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_events_by_scope(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.journal
            .recent_events_by_scope(scope, limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent runtime events for a specific entity.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_events_for_entity(
        &self,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.journal
            .recent_events_for_entity(entity_id, limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent runtime events matching both scope and entity.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_events_by_scope_and_entity(
        &self,
        scope: &str,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.journal
            .recent_events_by_scope_and_entity(scope, entity_id, limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted signal records.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_signals(&self, limit: usize) -> Result<Vec<SignalRecord>, ServiceError> {
        self.journal
            .recent_signals(limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted intent records.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_intents(&self, limit: usize) -> Result<Vec<IntentRecord>, ServiceError> {
        self.journal
            .recent_intents(limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted risk-decision records.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_risk_decisions(
        &self,
        limit: usize,
    ) -> Result<Vec<RiskDecisionRecord>, ServiceError> {
        self.journal
            .recent_risk_decisions(limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted order records.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_orders(&self, limit: usize) -> Result<Vec<OrderRecord>, ServiceError> {
        self.journal
            .recent_orders(limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted order records for one bot.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_orders_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<OrderRecord>, ServiceError> {
        self.journal
            .recent_orders_for_bot(bot_id, limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted fill records.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_fills(&self, limit: usize) -> Result<Vec<FillRecord>, ServiceError> {
        self.journal.recent_fills(limit).map_err(ServiceError::from)
    }

    /// Returns the most recent persisted fill records for one bot.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_fills_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<FillRecord>, ServiceError> {
        self.journal
            .recent_fills_for_bot(bot_id, limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted position records.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_positions(&self, limit: usize) -> Result<Vec<PositionRecord>, ServiceError> {
        self.journal
            .recent_positions(limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted position records for one bot.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_positions_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<PositionRecord>, ServiceError> {
        self.journal
            .recent_positions_for_bot(bot_id, limit)
            .map_err(ServiceError::from)
    }

    /// Returns recent cycle-trace summaries for one bot with optional filters.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails or a stored trace cannot be summarized.
    pub fn recent_cycle_traces_for_bot(
        &self,
        bot_id: &str,
        symbol: Option<&str>,
        phase: Option<&str>,
        outcome: Option<&str>,
        bar_timestamp: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CycleTraceSummary>, ServiceError> {
        let records = self
            .journal
            .recent_cycle_traces_for_bot(bot_id, symbol, phase, outcome, bar_timestamp, limit)
            .map_err(ServiceError::from)?;
        records
            .into_iter()
            .map(|record| super::cycles::cycle_trace_summary_from_record(&record))
            .collect()
    }

    /// Returns one cycle trace when it belongs to the requested bot.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails or the stored payload cannot be decoded.
    pub fn cycle_trace_for_bot(
        &self,
        bot_id: &str,
        trace_id: &str,
    ) -> Result<Option<CycleTrace>, ServiceError> {
        let Some(record) = self
            .journal
            .cycle_trace_by_id(trace_id)
            .map_err(ServiceError::from)?
        else {
            return Ok(None);
        };
        if record.bot_id != bot_id {
            return Ok(None);
        }

        let mut detail = serde_json::from_str::<CycleTrace>(&record.payload_json)?;
        detail.summary = super::cycles::cycle_trace_summary_from_record(&record)?;
        Ok(Some(detail))
    }

    /// Returns the most recent persisted reconciliation records.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_reconciliations(
        &self,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, ServiceError> {
        self.journal
            .recent_reconciliations(limit)
            .map_err(ServiceError::from)
    }

    /// Returns the most recent persisted reconciliation records for one bot.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal read fails.
    pub fn recent_reconciliations_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, ServiceError> {
        self.journal
            .recent_reconciliations_for_bot(bot_id, limit)
            .map_err(ServiceError::from)
    }

    /// Builds a reconciliation report for an instance.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying reconciliation snapshot lookup fails.
    pub fn reconciliation_report(
        &self,
        instance: InstanceSummary,
        lane_summaries: &[LaneSummary],
    ) -> Result<ReconciliationReport, ServiceError> {
        self.snapshot_reconciliation_report(instance, lane_summaries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndicatorSignal;
    use crate::test_support::{
        create_temp_db_path, fixture_bundle, fixture_bundle_with_db_path, test_bar_at,
    };

    #[test]
    fn snapshot_queries_reflect_recent_runtime_appends() {
        let mut runtime = Runtime::from_config(&fixture_bundle());
        runtime.start_instance("aapl").expect("bot should start");

        let outcome = runtime
            .process_manual_signal(
                "aapl",
                IndicatorSignal::BuyConfirmed,
                123.45,
                test_bar_at("2030-01-01T00:00:00Z", 123.45).timestamp,
            )
            .expect("manual signal should execute successfully");
        assert!(matches!(outcome.risk, crate::ProcessBarRisk::Allowed));

        let query = runtime.query_handle();
        assert!(!query.snapshot_recent_signals(20).is_empty());
        assert!(!query.snapshot_recent_intents(20).is_empty());
        assert!(!query.snapshot_recent_risk_decisions(20).is_empty());
        assert!(!query.snapshot_recent_orders(20).is_empty());
        assert!(!query.snapshot_recent_fills(20).is_empty());
        assert!(!query.snapshot_recent_positions(20).is_empty());
        assert!(!query.snapshot_recent_events(50).is_empty());
        assert!(
            !query
                .snapshot_recent_events_for_entity("aapl", 50)
                .is_empty()
        );
    }

    #[test]
    fn snapshot_reconciliation_report_matches_runtime_report_shape() {
        let mut runtime = Runtime::from_config(&fixture_bundle());
        runtime
            .reconcile_instance("aapl")
            .expect("manual reconciliation should succeed");

        let summary = runtime
            .get_instance("aapl")
            .expect("instance summary should be available");
        let lanes = runtime
            .lane_summaries_for_bot("aapl")
            .expect("lane summaries should be available");

        let snapshot_report = runtime
            .query_handle()
            .snapshot_reconciliation_report(summary, lanes.as_slice())
            .expect("snapshot report should be available");
        let runtime_report = runtime
            .reconciliation_report("aapl")
            .expect("runtime report should be available");

        assert_eq!(snapshot_report.instance.id, "aapl");
        assert_eq!(snapshot_report.lanes.len(), lanes.len());
        assert_eq!(
            snapshot_report.latest.as_ref().map(|check| check.id),
            runtime_report.latest.as_ref().map(|check| check.id)
        );
    }

    #[test]
    fn snapshot_read_models_bootstrap_from_persisted_journal_tails() {
        let db_path = create_temp_db_path("snapshot-query-bootstrap");
        let config = fixture_bundle_with_db_path(db_path.clone());

        {
            let mut runtime = Runtime::from_config_with_storage(&config)
                .expect("runtime should initialize with sqlite backend");
            runtime.start_instance("aapl").expect("bot should start");
            runtime
                .process_manual_signal(
                    "aapl",
                    IndicatorSignal::BuyConfirmed,
                    123.45,
                    test_bar_at("2030-01-01T00:00:00Z", 123.45).timestamp,
                )
                .expect("manual signal should persist journal records");
        }

        let runtime = Runtime::from_config_with_storage(&config)
            .expect("runtime should bootstrap persisted journal state");
        let query = runtime.query_handle();

        assert!(!query.snapshot_recent_orders_for_bot("aapl", 20).is_empty());
        assert!(
            !query
                .snapshot_recent_events_for_entity("aapl", 50)
                .is_empty()
        );
    }
}
