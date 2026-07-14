use crate::{
    BotEventRecord, BotEventWrite, BotSnapshot, BotSnapshotWrite, CycleTraceRecord,
    CycleTraceWrite, EventWrite, FillRecord, FillWrite, IntentRecord, IntentWrite, OrderRecord,
    OrderWrite, PositionRecord, PositionWrite, ReconciliationRecord, ReconciliationWrite,
    RiskDecisionRecord, RiskDecisionWrite, RuntimeEvent, ServiceEventRecord, ServiceEventWrite,
    SignalRecord, SignalWrite, StorageError,
};
use std::collections::HashSet;

pub trait RuntimeJournal: std::fmt::Debug + Send + Sync {
    /// Persists a runtime event.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the event.
    fn append_event(&self, event: EventWrite) -> Result<(), StorageError>;

    /// Returns the most recent runtime events up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_events(&self, limit: usize) -> Result<Vec<RuntimeEvent>, StorageError>;

    /// Returns the most recent runtime events for a specific `scope` up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_events_by_scope(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, StorageError>;

    /// Returns the most recent runtime events for a specific `entity_id` up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_events_for_entity(
        &self,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, StorageError>;

    /// Returns the most recent runtime events for a specific `scope` and `entity_id` up to
    /// `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_events_by_scope_and_entity(
        &self,
        scope: &str,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, StorageError>;

    /// Persists a strategy signal.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the signal.
    fn append_signal(&self, signal: SignalWrite) -> Result<(), StorageError>;

    /// Returns the most recent signals up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_signals(&self, limit: usize) -> Result<Vec<SignalRecord>, StorageError>;

    /// Persists a trade intent.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the intent.
    fn append_intent(&self, intent: IntentWrite) -> Result<(), StorageError>;

    /// Returns the most recent intents up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_intents(&self, limit: usize) -> Result<Vec<IntentRecord>, StorageError>;

    /// Persists a risk decision.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the risk decision.
    fn append_risk_decision(&self, decision: RiskDecisionWrite) -> Result<(), StorageError>;

    /// Returns the most recent risk decisions up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_risk_decisions(&self, limit: usize) -> Result<Vec<RiskDecisionRecord>, StorageError>;

    /// Persists an order lifecycle record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the order.
    fn append_order(&self, order: OrderWrite) -> Result<(), StorageError>;

    /// Returns the most recent orders up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_orders(&self, limit: usize) -> Result<Vec<OrderRecord>, StorageError>;

    /// Returns recent orders for `bot_id` up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_orders_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<OrderRecord>, StorageError>;

    /// Returns all persisted order records matching one `client_order_id`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn orders_by_client_order_id(
        &self,
        client_order_id: &str,
    ) -> Result<Vec<OrderRecord>, StorageError>;

    /// Persists a fill record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the fill.
    fn append_fill(&self, fill: FillWrite) -> Result<(), StorageError>;

    /// Returns the most recent fills up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_fills(&self, limit: usize) -> Result<Vec<FillRecord>, StorageError>;

    /// Returns recent fills for `bot_id` up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_fills_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<FillRecord>, StorageError>;

    /// Persists a position snapshot entry.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the position.
    fn append_position(&self, position: PositionWrite) -> Result<(), StorageError>;

    /// Returns recent position entries up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_positions(&self, limit: usize) -> Result<Vec<PositionRecord>, StorageError>;

    /// Returns recent position entries for `bot_id` up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_positions_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<PositionRecord>, StorageError>;

    /// Persists an authoritative cycle trace record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the cycle trace.
    fn append_cycle_trace(&self, trace: CycleTraceWrite) -> Result<(), StorageError>;

    /// Returns recent cycle traces for a bot with optional server-side filters.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_cycle_traces_for_bot(
        &self,
        bot_id: &str,
        symbol: Option<&str>,
        phase: Option<&str>,
        outcome: Option<&str>,
        bar_timestamp: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CycleTraceRecord>, StorageError>;

    /// Returns a persisted cycle trace by its opaque `trace_id`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn cycle_trace_by_id(&self, trace_id: &str) -> Result<Option<CycleTraceRecord>, StorageError>;

    /// Returns the latest position for `bot_id`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn latest_position_for_bot(&self, bot_id: &str)
    -> Result<Option<PositionRecord>, StorageError>;

    /// Returns the latest position for a specific `(bot_id, symbol)` lane.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn latest_position_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<Option<PositionRecord>, StorageError>;

    /// Persists a reconciliation record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the record.
    fn append_reconciliation(&self, record: ReconciliationWrite) -> Result<(), StorageError>;

    /// Returns recent reconciliation records up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_reconciliations(
        &self,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, StorageError>;

    /// Returns recent reconciliation records for `bot_id` up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_reconciliations_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, StorageError>;

    /// Returns the latest reconciliation for `bot_id`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn latest_reconciliation_for_bot(
        &self,
        bot_id: &str,
    ) -> Result<Option<ReconciliationRecord>, StorageError>;

    /// Returns the latest reconciliation record for a specific `(bot_id, symbol)` lane.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn latest_reconciliation_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<Option<ReconciliationRecord>, StorageError>;

    /// Persists a bot lifecycle event.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the event.
    fn append_bot_event(&self, event: BotEventWrite) -> Result<(), StorageError>;

    /// Returns recent bot events up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_bot_events(&self, limit: usize) -> Result<Vec<BotEventRecord>, StorageError>;

    /// Persists a service-level event.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the event.
    fn append_service_event(&self, event: ServiceEventWrite) -> Result<(), StorageError>;

    /// Returns recent service events up to `limit`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn recent_service_events(&self, limit: usize) -> Result<Vec<ServiceEventRecord>, StorageError>;

    /// Inserts or updates the latest bot snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cannot persist the snapshot.
    fn upsert_bot_snapshot(&self, snapshot: BotSnapshotWrite) -> Result<(), StorageError>;

    /// Loads all current bot snapshots.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend query fails.
    fn load_bot_snapshots(&self) -> Result<Vec<BotSnapshot>, StorageError>;

    /// Removes persisted rows for bot ids that are no longer configured.
    ///
    /// All bot-scoped records (signals, intents, risk decisions, orders,
    /// fills, positions, cycle traces, reconciliations, bot events, and bot
    /// snapshots) whose bot id is not in `active_bot_ids` are deleted, as are
    /// runtime events tied to a pruned entity. Runtime events without an
    /// entity id (global/service events) are always retained.
    ///
    /// An **empty** `active_bot_ids` set means no bots are configured and
    /// therefore prunes ALL bot-scoped data. Both backends implement these
    /// semantics; `tests/backend_parity.rs` pins them.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the backend cleanup fails.
    fn prune_bots_except(&self, active_bot_ids: &HashSet<String>) -> Result<(), StorageError>;
}
