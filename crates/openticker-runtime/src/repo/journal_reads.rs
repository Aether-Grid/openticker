use super::RuntimeRepoRead;
use crate::{
    CycleTraceRecord, FillRecord, IntentRecord, OrderRecord, PositionRecord, ReconciliationRecord,
    RiskDecisionRecord, RuntimeEvent, ServiceError, SignalRecord,
};

impl RuntimeRepoRead<'_> {
    pub(crate) fn recent_events(&self, limit: usize) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.journal
            .recent_events(limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_events_by_scope(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.journal
            .recent_events_by_scope(scope, limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_events_for_entity(
        &self,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.journal
            .recent_events_for_entity(entity_id, limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_events_by_scope_and_entity(
        &self,
        scope: &str,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, ServiceError> {
        self.journal
            .recent_events_by_scope_and_entity(scope, entity_id, limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_signals(&self, limit: usize) -> Result<Vec<SignalRecord>, ServiceError> {
        self.journal
            .recent_signals(limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_intents(&self, limit: usize) -> Result<Vec<IntentRecord>, ServiceError> {
        self.journal
            .recent_intents(limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_risk_decisions(
        &self,
        limit: usize,
    ) -> Result<Vec<RiskDecisionRecord>, ServiceError> {
        self.journal
            .recent_risk_decisions(limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_orders(&self, limit: usize) -> Result<Vec<OrderRecord>, ServiceError> {
        self.journal
            .recent_orders(limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_orders_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<OrderRecord>, ServiceError> {
        self.journal
            .recent_orders_for_bot(bot_id, limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn orders_by_client_order_id(
        &self,
        client_order_id: &str,
    ) -> Result<Vec<OrderRecord>, ServiceError> {
        self.journal
            .orders_by_client_order_id(client_order_id)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_fills(&self, limit: usize) -> Result<Vec<FillRecord>, ServiceError> {
        self.journal.recent_fills(limit).map_err(ServiceError::from)
    }

    pub(crate) fn recent_fills_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<FillRecord>, ServiceError> {
        self.journal
            .recent_fills_for_bot(bot_id, limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_positions(
        &self,
        limit: usize,
    ) -> Result<Vec<PositionRecord>, ServiceError> {
        self.journal
            .recent_positions(limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_positions_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<PositionRecord>, ServiceError> {
        self.journal
            .recent_positions_for_bot(bot_id, limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_cycle_traces_for_bot(
        &self,
        bot_id: &str,
        symbol: Option<&str>,
        phase: Option<&str>,
        outcome: Option<&str>,
        bar_timestamp: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CycleTraceRecord>, ServiceError> {
        self.journal
            .recent_cycle_traces_for_bot(bot_id, symbol, phase, outcome, bar_timestamp, limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn cycle_trace_by_id(
        &self,
        trace_id: &str,
    ) -> Result<Option<CycleTraceRecord>, ServiceError> {
        self.journal
            .cycle_trace_by_id(trace_id)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_reconciliations(
        &self,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, ServiceError> {
        self.journal
            .recent_reconciliations(limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn recent_reconciliations_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, ServiceError> {
        self.lane_ids_for_bot(bot_id)?;
        self.journal
            .recent_reconciliations_for_bot(bot_id, limit)
            .map_err(ServiceError::from)
    }

    pub(crate) fn latest_reconciliation_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<Option<ReconciliationRecord>, ServiceError> {
        self.journal
            .latest_reconciliation_for_lane(bot_id, symbol)
            .map_err(ServiceError::from)
    }

    pub(crate) fn latest_reconciliation_for_bot(
        &self,
        bot_id: &str,
    ) -> Result<Option<ReconciliationRecord>, ServiceError> {
        self.journal
            .latest_reconciliation_for_bot(bot_id)
            .map_err(ServiceError::from)
    }
}
