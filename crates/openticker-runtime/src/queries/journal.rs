use crate::{
    FillRecord, IntentRecord, OrderRecord, PositionRecord, ReconciliationRecord,
    ReconciliationReport, RiskDecisionRecord, Runtime, RuntimeEvent, ServiceError, SignalRecord,
};

impl Runtime {
    /// Returns recent runtime events across all scopes.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_events(&self, limit: usize) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.repo().recent_events(limit)
    }

    /// Returns recent runtime events filtered by scope.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_events_by_scope(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.repo().recent_events_by_scope(scope, limit)
    }

    /// Returns recent runtime events filtered by entity id.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_events_for_entity(
        &self,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.repo().recent_events_for_entity(entity_id, limit)
    }

    /// Returns recent runtime events filtered by both scope and entity id.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_events_by_scope_and_entity(
        &self,
        scope: &str,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.repo()
            .recent_events_by_scope_and_entity(scope, entity_id, limit)
    }

    /// Returns recent emitted signal records.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_signals(&self, limit: usize) -> Result<Vec<SignalRecord>, ServiceError> {
        self.repo().recent_signals(limit)
    }

    /// Returns recent generated strategy intents.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_intents(&self, limit: usize) -> Result<Vec<IntentRecord>, ServiceError> {
        self.repo().recent_intents(limit)
    }

    /// Returns recent recorded risk decisions.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_risk_decisions(
        &self,
        limit: usize,
    ) -> Result<Vec<RiskDecisionRecord>, ServiceError> {
        self.repo().recent_risk_decisions(limit)
    }

    /// Returns recent order records.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_orders(&self, limit: usize) -> Result<Vec<OrderRecord>, ServiceError> {
        self.repo().recent_orders(limit)
    }

    /// Returns recent order records for a single bot.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_orders_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<OrderRecord>, ServiceError> {
        self.repo().recent_orders_for_bot(bot_id, limit)
    }

    /// Returns recent fill records.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_fills(&self, limit: usize) -> Result<Vec<FillRecord>, ServiceError> {
        self.repo().recent_fills(limit)
    }

    /// Returns recent fill records for a single bot.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_fills_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<FillRecord>, ServiceError> {
        self.repo().recent_fills_for_bot(bot_id, limit)
    }

    /// Returns recent position transition records.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_positions(&self, limit: usize) -> Result<Vec<PositionRecord>, ServiceError> {
        self.repo().recent_positions(limit)
    }

    /// Returns recent position transition records for a single bot.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_positions_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<PositionRecord>, ServiceError> {
        self.repo().recent_positions_for_bot(bot_id, limit)
    }

    /// Returns recent reconciliation records.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval fails.
    pub fn recent_reconciliations(
        &self,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, ServiceError> {
        self.repo().recent_reconciliations(limit)
    }

    /// Returns recent reconciliation records for a single bot.
    ///
    /// # Errors
    ///
    /// Returns an error when the bot is missing or storage retrieval fails.
    pub fn recent_reconciliations_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, ServiceError> {
        self.repo().recent_reconciliations_for_bot(bot_id, limit)
    }

    /// Returns the latest reconciliation report for an instance.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is missing or storage retrieval fails.
    pub fn reconciliation_report(
        &self,
        instance_id: &str,
    ) -> Result<ReconciliationReport, ServiceError> {
        let repo = self.repo();
        let summary = repo.get_instance(instance_id)?;
        let mut lanes = Vec::new();
        for lane in repo.lanes_for_bot(instance_id)? {
            if let Some(record) = repo
                .latest_reconciliation_for_lane(instance_id, &lane.lane_symbol)?
                .map(crate::repo::RuntimeRepoRead::reconciliation_check_from_record)
            {
                lanes.push(record);
            }
        }
        lanes.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        let latest = lanes.iter().max_by_key(|check| check.id).cloned();
        Ok(ReconciliationReport {
            instance: summary,
            latest,
            lanes,
        })
    }
}
