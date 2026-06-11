use crate::{
    EventWrite, FillRecord, FillWrite, IntentRecord, IntentWrite, OrderRecord, OrderWrite,
    PositionRecord, PositionWrite, ReconciliationRecord, ReconciliationWrite, RiskDecisionRecord,
    RiskDecisionWrite, RuntimeEvent, RuntimeJournal, SignalRecord, SignalWrite, StorageError,
    now_timestamp_ms,
};
use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;

const GLOBAL_FEED_CAPACITY: usize = 2_048;
const BOT_FEED_CAPACITY: usize = 512;
const ENTITY_TIMELINE_CAPACITY: usize = 512;

#[derive(Debug)]
pub struct OperatorReadModels {
    state: RwLock<OperatorReadModelsState>,
}

#[derive(Debug)]
struct OperatorReadModelsState {
    events: VecDeque<RuntimeEvent>,
    events_by_scope: HashMap<String, VecDeque<RuntimeEvent>>,
    events_by_entity: HashMap<String, VecDeque<RuntimeEvent>>,
    signals: VecDeque<SignalRecord>,
    intents: VecDeque<IntentRecord>,
    risk_decisions: VecDeque<RiskDecisionRecord>,
    orders: VecDeque<OrderRecord>,
    orders_by_bot: HashMap<String, VecDeque<OrderRecord>>,
    fills: VecDeque<FillRecord>,
    fills_by_bot: HashMap<String, VecDeque<FillRecord>>,
    positions: VecDeque<PositionRecord>,
    positions_by_bot: HashMap<String, VecDeque<PositionRecord>>,
    latest_position_by_bot: HashMap<String, PositionRecord>,
    latest_position_by_lane: HashMap<(String, String), PositionRecord>,
    reconciliations: VecDeque<ReconciliationRecord>,
    reconciliations_by_bot: HashMap<String, VecDeque<ReconciliationRecord>>,
    latest_reconciliation_by_bot: HashMap<String, ReconciliationRecord>,
    latest_reconciliation_by_lane: HashMap<(String, String), ReconciliationRecord>,
    next_event_id: i64,
    next_signal_id: i64,
    next_intent_id: i64,
    next_risk_decision_id: i64,
    next_order_id: i64,
    next_fill_id: i64,
    next_position_id: i64,
    next_reconciliation_id: i64,
}

impl OperatorReadModels {
    /// Bootstraps the projected operator read models from a journal snapshot.
    ///
    /// # Errors
    ///
    /// Returns a storage error if any journal read fails while loading the
    /// recent event, signal, intent, order, fill, position, or reconciliation
    /// history.
    pub fn bootstrap(journal: &dyn RuntimeJournal) -> Result<Self, StorageError> {
        let events = journal.recent_events(GLOBAL_FEED_CAPACITY)?;
        let signals = journal.recent_signals(GLOBAL_FEED_CAPACITY)?;
        let intents = journal.recent_intents(GLOBAL_FEED_CAPACITY)?;
        let risk_decisions = journal.recent_risk_decisions(GLOBAL_FEED_CAPACITY)?;
        let orders = journal.recent_orders(GLOBAL_FEED_CAPACITY)?;
        let fills = journal.recent_fills(GLOBAL_FEED_CAPACITY)?;
        let positions = journal.recent_positions(GLOBAL_FEED_CAPACITY)?;
        let reconciliations = journal.recent_reconciliations(GLOBAL_FEED_CAPACITY)?;

        let mut state = OperatorReadModelsState::default();
        for record in events {
            state.ingest_runtime_event(record);
        }
        for record in signals {
            state.ingest_signal(record);
        }
        for record in intents {
            state.ingest_intent(record);
        }
        for record in risk_decisions {
            state.ingest_risk_decision(record);
        }
        for record in orders {
            state.ingest_order(record);
        }
        for record in fills {
            state.ingest_fill(record);
        }
        for record in positions {
            state.ingest_position(record);
        }
        for record in reconciliations {
            state.ingest_reconciliation(record);
        }

        Ok(Self {
            state: RwLock::new(state),
        })
    }

    pub fn record_runtime_event_write(&self, write: &EventWrite) {
        let mut state = self.write_state();
        let record = RuntimeEvent {
            id: next_id(&mut state.next_event_id),
            scope: write.scope.clone(),
            entity_id: write.entity_id.clone(),
            trace_id: write.trace_id.clone(),
            kind: write.kind.clone(),
            payload: write.payload.clone(),
            created_at_ms: now_timestamp_ms(),
        };
        state.push_runtime_event(record);
    }

    pub fn record_signal_write(&self, write: &SignalWrite) {
        let mut state = self.write_state();
        let record = SignalRecord {
            id: next_id(&mut state.next_signal_id),
            bot_id: write.bot_id.clone(),
            symbol: Some(write.symbol.clone()),
            trace_id: write.trace_id.clone(),
            bar_timestamp: write.bar_timestamp.clone(),
            phase: write.phase.clone(),
            signal: write.signal.clone(),
            close: write.close,
            metadata_json: write.metadata_json.clone(),
            created_at_ms: now_timestamp_ms(),
        };
        state.push_signal(record);
    }

    pub fn record_intent_write(&self, write: &IntentWrite) {
        let mut state = self.write_state();
        let record = IntentRecord {
            id: next_id(&mut state.next_intent_id),
            bot_id: write.bot_id.clone(),
            symbol: Some(write.symbol.clone()),
            trace_id: write.trace_id.clone(),
            bar_timestamp: write.bar_timestamp.clone(),
            signal: write.signal.clone(),
            intent: write.intent.clone(),
            metadata_json: write.metadata_json.clone(),
            strategy_rationale: write.strategy_rationale.clone(),
            has_position_before: write.has_position_before,
            created_at_ms: now_timestamp_ms(),
        };
        state.push_intent(record);
    }

    pub fn record_risk_decision_write(&self, write: &RiskDecisionWrite) {
        let mut state = self.write_state();
        let record = RiskDecisionRecord {
            id: next_id(&mut state.next_risk_decision_id),
            bot_id: write.bot_id.clone(),
            symbol: Some(write.symbol.clone()),
            trace_id: write.trace_id.clone(),
            bar_timestamp: write.bar_timestamp.clone(),
            intent: write.intent.clone(),
            decision: write.decision.clone(),
            reason: write.reason.clone(),
            created_at_ms: now_timestamp_ms(),
        };
        state.push_risk_decision(record);
    }

    pub fn record_order_write(&self, write: &OrderWrite) {
        let mut state = self.write_state();
        let record = OrderRecord {
            id: next_id(&mut state.next_order_id),
            bot_id: write.bot_id.clone(),
            symbol: Some(write.symbol.clone()),
            trace_id: write.trace_id.clone(),
            bar_timestamp: write.bar_timestamp.clone(),
            client_order_id: write.client_order_id.clone(),
            intent: write.intent.clone(),
            status: write.status.clone(),
            price: write.price,
            quantity: write.quantity,
            created_at_ms: now_timestamp_ms(),
        };
        state.push_order(record);
    }

    pub fn record_fill_write(&self, write: &FillWrite) {
        let mut state = self.write_state();
        let record = FillRecord {
            id: next_id(&mut state.next_fill_id),
            bot_id: write.bot_id.clone(),
            symbol: Some(write.symbol.clone()),
            trace_id: write.trace_id.clone(),
            bar_timestamp: write.bar_timestamp.clone(),
            client_order_id: write.client_order_id.clone(),
            price: write.price,
            quantity: write.quantity,
            fee_asset: write.fee_asset.clone(),
            fee_amount: write.fee_amount,
            fee_normalized_usd: write.fee_normalized_usd,
            created_at_ms: now_timestamp_ms(),
        };
        state.push_fill(record);
    }

    pub fn record_position_write(&self, write: &PositionWrite) {
        let mut state = self.write_state();
        let record = PositionRecord {
            id: next_id(&mut state.next_position_id),
            bot_id: write.bot_id.clone(),
            symbol: Some(write.symbol.clone()),
            trace_id: write.trace_id.clone(),
            bar_timestamp: write.bar_timestamp.clone(),
            has_position: write.has_position,
            quantity: write.quantity,
            entry_price: write.entry_price,
            realized_pnl_usd: write.realized_pnl_usd,
            reason: write.reason.clone(),
            created_at_ms: now_timestamp_ms(),
        };
        state.push_position(record);
    }

    pub fn record_reconciliation_write(&self, write: &ReconciliationWrite) {
        let mut state = self.write_state();
        let record = ReconciliationRecord {
            id: next_id(&mut state.next_reconciliation_id),
            bot_id: write.bot_id.clone(),
            source: write.source.clone(),
            symbol: write.symbol.clone(),
            safe_to_trade: write.safe_to_trade,
            local_open_orders: write.local_open_orders,
            connector_open_orders: write.connector_open_orders,
            local_has_position: write.local_has_position,
            connector_has_position: write.connector_has_position,
            reason: write.reason.clone(),
            created_at_ms: now_timestamp_ms(),
        };
        state.push_reconciliation(record);
    }

    pub fn ingest_reconciliations(&self, records: &[ReconciliationRecord]) {
        let mut state = self.write_state();
        for record in records {
            state.ingest_reconciliation(record.clone());
        }
    }

    pub fn recent_events(&self, limit: usize) -> Vec<RuntimeEvent> {
        recent_from_deque(&self.read_state().events, limit)
    }

    pub fn recent_events_by_scope(&self, scope: &str, limit: usize) -> Vec<RuntimeEvent> {
        self.read_state()
            .events_by_scope
            .get(scope)
            .map_or_else(Vec::new, |records| recent_from_deque(records, limit))
    }

    pub fn recent_events_for_entity(&self, entity_id: &str, limit: usize) -> Vec<RuntimeEvent> {
        self.read_state()
            .events_by_entity
            .get(entity_id)
            .map_or_else(Vec::new, |records| recent_from_deque(records, limit))
    }

    pub fn recent_signals(&self, limit: usize) -> Vec<SignalRecord> {
        recent_from_deque(&self.read_state().signals, limit)
    }

    pub fn recent_intents(&self, limit: usize) -> Vec<IntentRecord> {
        recent_from_deque(&self.read_state().intents, limit)
    }

    pub fn recent_risk_decisions(&self, limit: usize) -> Vec<RiskDecisionRecord> {
        recent_from_deque(&self.read_state().risk_decisions, limit)
    }

    pub fn recent_orders(&self, limit: usize) -> Vec<OrderRecord> {
        recent_from_deque(&self.read_state().orders, limit)
    }

    pub fn recent_orders_for_bot(&self, bot_id: &str, limit: usize) -> Vec<OrderRecord> {
        self.read_state()
            .orders_by_bot
            .get(bot_id)
            .map_or_else(Vec::new, |records| recent_from_deque(records, limit))
    }

    pub fn recent_fills(&self, limit: usize) -> Vec<FillRecord> {
        recent_from_deque(&self.read_state().fills, limit)
    }

    pub fn recent_fills_for_bot(&self, bot_id: &str, limit: usize) -> Vec<FillRecord> {
        self.read_state()
            .fills_by_bot
            .get(bot_id)
            .map_or_else(Vec::new, |records| recent_from_deque(records, limit))
    }

    pub fn recent_positions(&self, limit: usize) -> Vec<PositionRecord> {
        recent_from_deque(&self.read_state().positions, limit)
    }

    pub fn recent_positions_for_bot(&self, bot_id: &str, limit: usize) -> Vec<PositionRecord> {
        self.read_state()
            .positions_by_bot
            .get(bot_id)
            .map_or_else(Vec::new, |records| recent_from_deque(records, limit))
    }

    pub fn recent_reconciliations(&self, limit: usize) -> Vec<ReconciliationRecord> {
        recent_from_deque(&self.read_state().reconciliations, limit)
    }

    pub fn recent_reconciliations_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Vec<ReconciliationRecord> {
        self.read_state()
            .reconciliations_by_bot
            .get(bot_id)
            .map_or_else(Vec::new, |records| recent_from_deque(records, limit))
    }

    pub fn latest_reconciliation_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Option<ReconciliationRecord> {
        self.read_state()
            .latest_reconciliation_by_lane
            .get(&(bot_id.to_owned(), symbol.to_owned()))
            .cloned()
    }

    pub fn latest_position_for_lane(&self, bot_id: &str, symbol: &str) -> Option<PositionRecord> {
        self.read_state()
            .latest_position_by_lane
            .get(&(bot_id.to_owned(), symbol.to_owned()))
            .cloned()
    }

    pub fn latest_position_for_bot(&self, bot_id: &str) -> Option<PositionRecord> {
        self.read_state()
            .latest_position_by_bot
            .get(bot_id)
            .cloned()
    }

    pub fn latest_reconciliation_for_bot(&self, bot_id: &str) -> Option<ReconciliationRecord> {
        self.read_state()
            .latest_reconciliation_by_bot
            .get(bot_id)
            .cloned()
    }

    fn read_state(&self) -> std::sync::RwLockReadGuard<'_, OperatorReadModelsState> {
        match self.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Workspace policy: recover from poisoning and log instead of
                // failing forever. The read models hold plain in-memory
                // collections that stay structurally valid after a panic in
                // another thread.
                self.state.clear_poison();
                tracing::warn!(
                    "operator read model lock was poisoned; clearing poison and recovering"
                );
                poisoned.into_inner()
            }
        }
    }

    fn write_state(&self) -> std::sync::RwLockWriteGuard<'_, OperatorReadModelsState> {
        match self.state.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Workspace policy: recover from poisoning and log instead of
                // failing forever. The read models hold plain in-memory
                // collections that stay structurally valid after a panic in
                // another thread.
                self.state.clear_poison();
                tracing::warn!(
                    "operator read model lock was poisoned; clearing poison and recovering"
                );
                poisoned.into_inner()
            }
        }
    }
}

impl Default for OperatorReadModelsState {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            events_by_scope: HashMap::new(),
            events_by_entity: HashMap::new(),
            signals: VecDeque::new(),
            intents: VecDeque::new(),
            risk_decisions: VecDeque::new(),
            orders: VecDeque::new(),
            orders_by_bot: HashMap::new(),
            fills: VecDeque::new(),
            fills_by_bot: HashMap::new(),
            positions: VecDeque::new(),
            positions_by_bot: HashMap::new(),
            latest_position_by_bot: HashMap::new(),
            latest_position_by_lane: HashMap::new(),
            reconciliations: VecDeque::new(),
            reconciliations_by_bot: HashMap::new(),
            latest_reconciliation_by_bot: HashMap::new(),
            latest_reconciliation_by_lane: HashMap::new(),
            next_event_id: 1,
            next_signal_id: 1,
            next_intent_id: 1,
            next_risk_decision_id: 1,
            next_order_id: 1,
            next_fill_id: 1,
            next_position_id: 1,
            next_reconciliation_id: 1,
        }
    }
}

impl OperatorReadModelsState {
    fn ingest_runtime_event(&mut self, record: RuntimeEvent) {
        self.next_event_id = self.next_event_id.max(record.id.saturating_add(1));
        self.push_runtime_event(record);
    }

    fn ingest_signal(&mut self, record: SignalRecord) {
        self.next_signal_id = self.next_signal_id.max(record.id.saturating_add(1));
        self.push_signal(record);
    }

    fn ingest_intent(&mut self, record: IntentRecord) {
        self.next_intent_id = self.next_intent_id.max(record.id.saturating_add(1));
        self.push_intent(record);
    }

    fn ingest_risk_decision(&mut self, record: RiskDecisionRecord) {
        self.next_risk_decision_id = self.next_risk_decision_id.max(record.id.saturating_add(1));
        self.push_risk_decision(record);
    }

    fn ingest_order(&mut self, record: OrderRecord) {
        self.next_order_id = self.next_order_id.max(record.id.saturating_add(1));
        self.push_order(record);
    }

    fn ingest_fill(&mut self, record: FillRecord) {
        self.next_fill_id = self.next_fill_id.max(record.id.saturating_add(1));
        self.push_fill(record);
    }

    fn ingest_position(&mut self, record: PositionRecord) {
        self.next_position_id = self.next_position_id.max(record.id.saturating_add(1));
        self.push_position(record);
    }

    fn ingest_reconciliation(&mut self, record: ReconciliationRecord) {
        self.next_reconciliation_id = self.next_reconciliation_id.max(record.id.saturating_add(1));
        self.push_reconciliation(record);
    }

    fn push_runtime_event(&mut self, record: RuntimeEvent) {
        push_bounded(&mut self.events, record.clone(), GLOBAL_FEED_CAPACITY);
        push_to_index(
            &mut self.events_by_scope,
            record.scope.clone(),
            record.clone(),
            GLOBAL_FEED_CAPACITY,
        );
        if let Some(entity_id) = record.entity_id.clone() {
            push_to_index(
                &mut self.events_by_entity,
                entity_id,
                record,
                ENTITY_TIMELINE_CAPACITY,
            );
        }
    }

    fn push_signal(&mut self, record: SignalRecord) {
        push_bounded(&mut self.signals, record, GLOBAL_FEED_CAPACITY);
    }

    fn push_intent(&mut self, record: IntentRecord) {
        push_bounded(&mut self.intents, record, GLOBAL_FEED_CAPACITY);
    }

    fn push_risk_decision(&mut self, record: RiskDecisionRecord) {
        push_bounded(&mut self.risk_decisions, record, GLOBAL_FEED_CAPACITY);
    }

    fn push_order(&mut self, record: OrderRecord) {
        push_bounded(&mut self.orders, record.clone(), GLOBAL_FEED_CAPACITY);
        push_to_index(
            &mut self.orders_by_bot,
            record.bot_id.clone(),
            record,
            BOT_FEED_CAPACITY,
        );
    }

    fn push_fill(&mut self, record: FillRecord) {
        push_bounded(&mut self.fills, record.clone(), GLOBAL_FEED_CAPACITY);
        push_to_index(
            &mut self.fills_by_bot,
            record.bot_id.clone(),
            record,
            BOT_FEED_CAPACITY,
        );
    }

    fn push_position(&mut self, record: PositionRecord) {
        push_bounded(&mut self.positions, record.clone(), GLOBAL_FEED_CAPACITY);
        push_to_index(
            &mut self.positions_by_bot,
            record.bot_id.clone(),
            record.clone(),
            BOT_FEED_CAPACITY,
        );

        upsert_latest_by_id(
            &mut self.latest_position_by_bot,
            record.bot_id.clone(),
            record.clone(),
            |value| value.id,
        );

        if let Some(symbol) = record.symbol.clone() {
            upsert_latest_by_id(
                &mut self.latest_position_by_lane,
                (record.bot_id.clone(), symbol),
                record,
                |value| value.id,
            );
        }
    }

    fn push_reconciliation(&mut self, record: ReconciliationRecord) {
        push_bounded(
            &mut self.reconciliations,
            record.clone(),
            GLOBAL_FEED_CAPACITY,
        );
        push_to_index(
            &mut self.reconciliations_by_bot,
            record.bot_id.clone(),
            record.clone(),
            BOT_FEED_CAPACITY,
        );

        upsert_latest_by_id(
            &mut self.latest_reconciliation_by_bot,
            record.bot_id.clone(),
            record.clone(),
            |value| value.id,
        );
        upsert_latest_by_id(
            &mut self.latest_reconciliation_by_lane,
            (record.bot_id.clone(), record.symbol.clone()),
            record,
            |value| value.id,
        );
    }
}

fn next_id(counter: &mut i64) -> i64 {
    let id = *counter;
    *counter = counter.saturating_add(1);
    id
}

fn push_bounded<T>(records: &mut VecDeque<T>, record: T, capacity: usize) {
    records.push_back(record);
    while records.len() > capacity {
        records.pop_front();
    }
}

fn push_to_index<T: Clone>(
    index: &mut HashMap<String, VecDeque<T>>,
    key: String,
    record: T,
    capacity: usize,
) {
    let records = index.entry(key).or_default();
    push_bounded(records, record, capacity);
}

fn recent_from_deque<T: Clone>(records: &VecDeque<T>, limit: usize) -> Vec<T> {
    if limit == 0 {
        return Vec::new();
    }
    let start = records.len().saturating_sub(limit);
    records.iter().skip(start).cloned().collect()
}

fn upsert_latest_by_id<K, V, F>(map: &mut HashMap<K, V>, key: K, value: V, id_of: F)
where
    K: Eq + std::hash::Hash,
    F: Fn(&V) -> i64,
{
    let should_replace = map
        .get(&key)
        .is_none_or(|current| id_of(current) <= id_of(&value));
    if should_replace {
        map.insert(key, value);
    }
}
