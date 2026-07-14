use crate::support::{lock_mutex, now_timestamp_ms};
use crate::{
    BotEventRecord, BotEventWrite, BotSnapshot, BotSnapshotWrite, CycleTraceRecord,
    CycleTraceWrite, EventWrite, FillRecord, FillWrite, IntentRecord, IntentWrite, OrderRecord,
    OrderWrite, PositionRecord, PositionWrite, ReconciliationRecord, ReconciliationWrite,
    RiskDecisionRecord, RiskDecisionWrite, RuntimeEvent, RuntimeJournal, ServiceEventRecord,
    ServiceEventWrite, SignalRecord, SignalWrite, StorageError,
};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct InMemoryRuntimeJournal {
    events: Mutex<Vec<RuntimeEvent>>,
    signals: Mutex<Vec<SignalRecord>>,
    intents: Mutex<Vec<IntentRecord>>,
    risk_decisions: Mutex<Vec<RiskDecisionRecord>>,
    orders: Mutex<Vec<OrderRecord>>,
    fills: Mutex<Vec<FillRecord>>,
    positions: Mutex<Vec<PositionRecord>>,
    cycle_traces: Mutex<Vec<CycleTraceRecord>>,
    reconciliations: Mutex<Vec<ReconciliationRecord>>,
    bot_events: Mutex<Vec<BotEventRecord>>,
    service_events: Mutex<Vec<ServiceEventRecord>>,
    bot_snapshots: Mutex<HashMap<String, BotSnapshot>>,
}

fn recent_records<T: Clone>(mutex: &Mutex<Vec<T>>, label: &'static str, limit: usize) -> Vec<T> {
    if limit == 0 {
        return Vec::new();
    }

    let records = lock_mutex(mutex, label);
    let start = records.len().saturating_sub(limit);
    records[start..].to_vec()
}

impl RuntimeJournal for InMemoryRuntimeJournal {
    fn append_event(&self, event: EventWrite) -> Result<(), StorageError> {
        let mut events = lock_mutex(&self.events, "events");
        let id = events.last().map_or(1, |entry| entry.id.saturating_add(1));
        events.push(RuntimeEvent {
            id,
            scope: event.scope,
            entity_id: event.entity_id,
            trace_id: event.trace_id,
            kind: event.kind,
            payload: event.payload,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_events(&self, limit: usize) -> Result<Vec<RuntimeEvent>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let events = lock_mutex(&self.events, "events");
        let start = events.len().saturating_sub(limit);
        Ok(events[start..].to_vec())
    }

    fn recent_events_by_scope(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let events = lock_mutex(&self.events, "events");
        let mut filtered = events
            .iter()
            .filter(|event| event.scope == scope)
            .cloned()
            .collect::<Vec<_>>();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered.split_off(start))
    }

    fn recent_events_for_entity(
        &self,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let events = lock_mutex(&self.events, "events");
        let mut filtered = events
            .iter()
            .filter(|event| event.entity_id.as_deref() == Some(entity_id))
            .cloned()
            .collect::<Vec<_>>();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered.split_off(start))
    }

    fn recent_events_by_scope_and_entity(
        &self,
        scope: &str,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let events = lock_mutex(&self.events, "events");
        let mut filtered = events
            .iter()
            .filter(|event| event.scope == scope && event.entity_id.as_deref() == Some(entity_id))
            .cloned()
            .collect::<Vec<_>>();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered.split_off(start))
    }

    fn append_signal(&self, signal: SignalWrite) -> Result<(), StorageError> {
        let mut signals = lock_mutex(&self.signals, "signals");
        let id = signals.last().map_or(1, |entry| entry.id.saturating_add(1));
        signals.push(SignalRecord {
            id,
            bot_id: signal.bot_id,
            symbol: Some(signal.symbol),
            trace_id: signal.trace_id,
            bar_timestamp: signal.bar_timestamp,
            phase: signal.phase,
            signal: signal.signal,
            close: signal.close,
            metadata_json: signal.metadata_json,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_signals(&self, limit: usize) -> Result<Vec<SignalRecord>, StorageError> {
        Ok(recent_records(&self.signals, "signals", limit))
    }

    fn append_intent(&self, intent: IntentWrite) -> Result<(), StorageError> {
        let mut intents = lock_mutex(&self.intents, "intents");
        let id = intents.last().map_or(1, |entry| entry.id.saturating_add(1));
        intents.push(IntentRecord {
            id,
            bot_id: intent.bot_id,
            symbol: Some(intent.symbol),
            trace_id: intent.trace_id,
            bar_timestamp: intent.bar_timestamp,
            signal: intent.signal,
            intent: intent.intent,
            metadata_json: intent.metadata_json,
            strategy_rationale: intent.strategy_rationale,
            has_position_before: intent.has_position_before,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_intents(&self, limit: usize) -> Result<Vec<IntentRecord>, StorageError> {
        Ok(recent_records(&self.intents, "intents", limit))
    }

    fn append_risk_decision(&self, decision: RiskDecisionWrite) -> Result<(), StorageError> {
        let mut risk_decisions = lock_mutex(&self.risk_decisions, "risk decisions");
        let id = risk_decisions
            .last()
            .map_or(1, |entry| entry.id.saturating_add(1));
        risk_decisions.push(RiskDecisionRecord {
            id,
            bot_id: decision.bot_id,
            symbol: Some(decision.symbol),
            trace_id: decision.trace_id,
            bar_timestamp: decision.bar_timestamp,
            intent: decision.intent,
            decision: decision.decision,
            reason: decision.reason,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_risk_decisions(&self, limit: usize) -> Result<Vec<RiskDecisionRecord>, StorageError> {
        Ok(recent_records(
            &self.risk_decisions,
            "risk decisions",
            limit,
        ))
    }

    fn append_order(&self, order: OrderWrite) -> Result<(), StorageError> {
        let mut orders = lock_mutex(&self.orders, "orders");
        if orders.iter().any(|existing| {
            existing.bot_id == order.bot_id
                && existing.symbol.as_deref() == Some(order.symbol.as_str())
                && existing.client_order_id == order.client_order_id
        }) {
            return Ok(());
        }
        let id = orders.last().map_or(1, |entry| entry.id.saturating_add(1));
        orders.push(OrderRecord {
            id,
            bot_id: order.bot_id,
            symbol: Some(order.symbol),
            trace_id: order.trace_id,
            bar_timestamp: order.bar_timestamp,
            client_order_id: order.client_order_id,
            intent: order.intent,
            status: order.status,
            price: order.price,
            quantity: order.quantity,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_orders(&self, limit: usize) -> Result<Vec<OrderRecord>, StorageError> {
        Ok(recent_records(&self.orders, "orders", limit))
    }

    fn recent_orders_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<OrderRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let orders = lock_mutex(&self.orders, "orders");
        let mut filtered = orders
            .iter()
            .filter(|order| order.bot_id == bot_id)
            .cloned()
            .collect::<Vec<_>>();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered.split_off(start))
    }

    fn orders_by_client_order_id(
        &self,
        client_order_id: &str,
    ) -> Result<Vec<OrderRecord>, StorageError> {
        let orders = lock_mutex(&self.orders, "orders");
        Ok(orders
            .iter()
            .filter(|order| order.client_order_id == client_order_id)
            .cloned()
            .collect())
    }

    fn append_fill(&self, fill: FillWrite) -> Result<(), StorageError> {
        let mut fills = lock_mutex(&self.fills, "fills");
        if fills.iter().any(|existing| {
            existing.bot_id == fill.bot_id
                && existing.symbol.as_deref() == Some(fill.symbol.as_str())
                && existing.client_order_id == fill.client_order_id
        }) {
            return Ok(());
        }
        let id = fills.last().map_or(1, |entry| entry.id.saturating_add(1));
        fills.push(FillRecord {
            id,
            bot_id: fill.bot_id,
            symbol: Some(fill.symbol),
            trace_id: fill.trace_id,
            bar_timestamp: fill.bar_timestamp,
            client_order_id: fill.client_order_id,
            price: fill.price,
            quantity: fill.quantity,
            fee_asset: fill.fee_asset,
            fee_amount: fill
                .fee_amount
                .filter(|value| value.is_finite() && *value > 0.0),
            fee_normalized_usd: fill
                .fee_normalized_usd
                .filter(|value| value.is_finite() && *value > 0.0),
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_fills(&self, limit: usize) -> Result<Vec<FillRecord>, StorageError> {
        Ok(recent_records(&self.fills, "fills", limit))
    }

    fn recent_fills_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<FillRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let fills = lock_mutex(&self.fills, "fills");
        let mut filtered = fills
            .iter()
            .filter(|fill| fill.bot_id == bot_id)
            .cloned()
            .collect::<Vec<_>>();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered.split_off(start))
    }

    fn append_position(&self, position: PositionWrite) -> Result<(), StorageError> {
        let mut positions = lock_mutex(&self.positions, "positions");
        let id = positions
            .last()
            .map_or(1, |entry| entry.id.saturating_add(1));
        let quantity = if position.quantity.is_finite() && position.quantity > 0.0 {
            position.quantity
        } else {
            0.0
        };
        let entry_price = position
            .entry_price
            .filter(|price| price.is_finite() && *price > 0.0);
        let realized_pnl_usd = if position.realized_pnl_usd.is_finite() {
            position.realized_pnl_usd
        } else {
            0.0
        };
        positions.push(PositionRecord {
            id,
            bot_id: position.bot_id,
            symbol: Some(position.symbol),
            trace_id: position.trace_id,
            bar_timestamp: position.bar_timestamp,
            has_position: position.has_position,
            quantity,
            entry_price,
            realized_pnl_usd,
            reason: position.reason,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_positions(&self, limit: usize) -> Result<Vec<PositionRecord>, StorageError> {
        Ok(recent_records(&self.positions, "positions", limit))
    }

    fn recent_positions_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<PositionRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let positions = lock_mutex(&self.positions, "positions");
        let mut filtered = positions
            .iter()
            .filter(|position| position.bot_id == bot_id)
            .cloned()
            .collect::<Vec<_>>();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered.split_off(start))
    }

    fn append_cycle_trace(&self, trace: CycleTraceWrite) -> Result<(), StorageError> {
        let mut traces = lock_mutex(&self.cycle_traces, "cycle traces");
        if traces
            .iter()
            .any(|existing| existing.trace_id == trace.trace_id)
        {
            return Ok(());
        }
        let id = traces.last().map_or(1, |entry| entry.id.saturating_add(1));
        traces.push(CycleTraceRecord {
            id,
            trace_id: trace.trace_id,
            bot_id: trace.bot_id,
            symbol: trace.symbol,
            bar_timestamp: trace.bar_timestamp,
            phase: trace.phase,
            trigger_kind: trace.trigger_kind,
            signal: trace.signal,
            intent: trace.intent,
            risk_decision: trace.risk_decision,
            outcome: trace.outcome,
            created_at_ms: now_timestamp_ms(),
            payload_json: trace.payload_json,
        });
        Ok(())
    }

    fn recent_cycle_traces_for_bot(
        &self,
        bot_id: &str,
        symbol: Option<&str>,
        phase: Option<&str>,
        outcome: Option<&str>,
        bar_timestamp: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CycleTraceRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let traces = lock_mutex(&self.cycle_traces, "cycle traces");
        let mut filtered = traces
            .iter()
            .filter(|trace| trace.bot_id == bot_id)
            .filter(|trace| symbol.is_none_or(|value| trace.symbol == value))
            .filter(|trace| phase.is_none_or(|value| trace.phase == value))
            .filter(|trace| outcome.is_none_or(|value| trace.outcome == value))
            .filter(|trace| bar_timestamp.is_none_or(|value| trace.bar_timestamp == value))
            .cloned()
            .collect::<Vec<_>>();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered.split_off(start))
    }

    fn cycle_trace_by_id(&self, trace_id: &str) -> Result<Option<CycleTraceRecord>, StorageError> {
        let traces = lock_mutex(&self.cycle_traces, "cycle traces");
        Ok(traces
            .iter()
            .find(|trace| trace.trace_id == trace_id)
            .cloned())
    }

    fn latest_position_for_bot(
        &self,
        bot_id: &str,
    ) -> Result<Option<PositionRecord>, StorageError> {
        let positions = lock_mutex(&self.positions, "positions");
        Ok(positions
            .iter()
            .rev()
            .find(|position| position.bot_id == bot_id)
            .cloned())
    }

    fn latest_position_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<Option<PositionRecord>, StorageError> {
        let positions = lock_mutex(&self.positions, "positions");
        Ok(positions
            .iter()
            .rev()
            .find(|position| {
                position.bot_id == bot_id && position.symbol.as_deref() == Some(symbol)
            })
            .cloned())
    }

    fn append_reconciliation(&self, record: ReconciliationWrite) -> Result<(), StorageError> {
        let mut reconciliations = lock_mutex(&self.reconciliations, "reconciliations");
        let id = reconciliations
            .last()
            .map_or(1, |entry| entry.id.saturating_add(1));
        reconciliations.push(ReconciliationRecord {
            id,
            bot_id: record.bot_id,
            source: record.source,
            symbol: record.symbol,
            safe_to_trade: record.safe_to_trade,
            local_open_orders: record.local_open_orders,
            connector_open_orders: record.connector_open_orders,
            local_has_position: record.local_has_position,
            connector_has_position: record.connector_has_position,
            reason: record.reason,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_reconciliations(
        &self,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, StorageError> {
        Ok(recent_records(
            &self.reconciliations,
            "reconciliations",
            limit,
        ))
    }

    fn recent_reconciliations_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let reconciliations = lock_mutex(&self.reconciliations, "reconciliations");
        let mut filtered = reconciliations
            .iter()
            .filter(|record| record.bot_id == bot_id)
            .cloned()
            .collect::<Vec<_>>();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered.split_off(start))
    }

    fn latest_reconciliation_for_bot(
        &self,
        bot_id: &str,
    ) -> Result<Option<ReconciliationRecord>, StorageError> {
        let reconciliations = lock_mutex(&self.reconciliations, "reconciliations");
        Ok(reconciliations
            .iter()
            .rev()
            .find(|record| record.bot_id == bot_id)
            .cloned())
    }

    fn latest_reconciliation_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<Option<ReconciliationRecord>, StorageError> {
        let reconciliations = lock_mutex(&self.reconciliations, "reconciliations");
        Ok(reconciliations
            .iter()
            .rev()
            .find(|record| record.bot_id == bot_id && record.symbol == symbol)
            .cloned())
    }

    fn append_bot_event(&self, event: BotEventWrite) -> Result<(), StorageError> {
        let mut events = lock_mutex(&self.bot_events, "bot events");
        let id = events.last().map_or(1, |entry| entry.id.saturating_add(1));
        events.push(BotEventRecord {
            id,
            bot_id: event.bot_id,
            kind: event.kind,
            payload: event.payload,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_bot_events(&self, limit: usize) -> Result<Vec<BotEventRecord>, StorageError> {
        Ok(recent_records(&self.bot_events, "bot events", limit))
    }

    fn append_service_event(&self, event: ServiceEventWrite) -> Result<(), StorageError> {
        let mut events = lock_mutex(&self.service_events, "service events");
        let id = events.last().map_or(1, |entry| entry.id.saturating_add(1));
        events.push(ServiceEventRecord {
            id,
            kind: event.kind,
            payload: event.payload,
            created_at_ms: now_timestamp_ms(),
        });
        Ok(())
    }

    fn recent_service_events(&self, limit: usize) -> Result<Vec<ServiceEventRecord>, StorageError> {
        Ok(recent_records(
            &self.service_events,
            "service events",
            limit,
        ))
    }

    fn upsert_bot_snapshot(&self, snapshot: BotSnapshotWrite) -> Result<(), StorageError> {
        let mut snapshots = lock_mutex(&self.bot_snapshots, "bot snapshots");
        snapshots.insert(
            snapshot.bot_id.clone(),
            BotSnapshot {
                bot_id: snapshot.bot_id,
                state: snapshot.state,
                execution_mode: snapshot.execution_mode,
                enabled: snapshot.enabled,
                updated_at_ms: now_timestamp_ms(),
            },
        );
        Ok(())
    }

    fn load_bot_snapshots(&self) -> Result<Vec<BotSnapshot>, StorageError> {
        let snapshots = lock_mutex(&self.bot_snapshots, "bot snapshots");
        let mut values = snapshots.values().cloned().collect::<Vec<_>>();
        values.sort_by(|left, right| left.bot_id.cmp(&right.bot_id));
        Ok(values)
    }

    fn prune_bots_except(&self, active_bot_ids: &HashSet<String>) -> Result<(), StorageError> {
        let mut events = lock_mutex(&self.events, "events");
        events.retain(|event| match event.entity_id.as_deref() {
            Some(bot_id) => active_bot_ids.contains(bot_id),
            None => true,
        });

        lock_mutex(&self.signals, "signals")
            .retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.intents, "intents")
            .retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.risk_decisions, "risk decisions")
            .retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.orders, "orders").retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.fills, "fills").retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.positions, "positions")
            .retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.cycle_traces, "cycle traces")
            .retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.reconciliations, "reconciliations")
            .retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.bot_events, "bot events")
            .retain(|record| active_bot_ids.contains(&record.bot_id));
        lock_mutex(&self.bot_snapshots, "bot snapshots")
            .retain(|bot_id, _| active_bot_ids.contains(bot_id));

        Ok(())
    }
}
