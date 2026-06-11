use rusqlite::{Connection, OpenFlags, params, params_from_iter};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

mod in_memory_impl;
mod operator_read_models;
mod sqlite_impl;
mod sqlite_migrations;

#[cfg(test)]
mod tests;

pub use operator_read_models::OperatorReadModels;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeEvent {
    pub id: i64,
    pub scope: String,
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub kind: String,
    pub payload: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventWrite {
    pub scope: String,
    pub entity_id: Option<String>,
    pub trace_id: Option<String>,
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BotSnapshot {
    pub bot_id: String,
    pub state: String,
    pub execution_mode: String,
    pub enabled: bool,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotSnapshotWrite {
    pub bot_id: String,
    pub state: String,
    pub execution_mode: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SignalRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub phase: String,
    pub signal: String,
    pub close: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub phase: String,
    pub signal: String,
    pub close: f64,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntentRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub signal: String,
    pub intent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_rationale: Option<String>,
    pub has_position_before: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntentWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub signal: String,
    pub intent: String,
    pub metadata_json: Option<String>,
    pub strategy_rationale: Option<String>,
    pub has_position_before: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RiskDecisionRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub intent: String,
    pub decision: String,
    pub reason: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskDecisionWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub intent: String,
    pub decision: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_timestamp: Option<String>,
    pub client_order_id: String,
    pub intent: String,
    pub status: String,
    pub price: f64,
    pub quantity: f64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: Option<String>,
    pub client_order_id: String,
    pub intent: String,
    pub status: String,
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FillRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_timestamp: Option<String>,
    pub client_order_id: String,
    pub price: f64,
    pub quantity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_normalized_usd: Option<f64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: Option<String>,
    pub client_order_id: String,
    pub price: f64,
    pub quantity: f64,
    pub fee_asset: Option<String>,
    pub fee_amount: Option<f64>,
    pub fee_normalized_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_timestamp: Option<String>,
    pub has_position: bool,
    pub quantity: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: Option<String>,
    pub has_position: bool,
    pub quantity: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BotEventRecord {
    pub id: i64,
    pub bot_id: String,
    pub kind: String,
    pub payload: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotEventWrite {
    pub bot_id: String,
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceEventRecord {
    pub id: i64,
    pub kind: String,
    pub payload: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEventWrite {
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconciliationRecord {
    pub id: i64,
    pub bot_id: String,
    pub source: String,
    pub symbol: String,
    pub safe_to_trade: bool,
    pub local_open_orders: i64,
    pub connector_open_orders: i64,
    pub local_has_position: bool,
    pub connector_has_position: bool,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationWrite {
    pub bot_id: String,
    pub source: String,
    pub symbol: String,
    pub safe_to_trade: bool,
    pub local_open_orders: i64,
    pub connector_open_orders: i64,
    pub local_has_position: bool,
    pub connector_has_position: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CycleTraceRecord {
    pub id: i64,
    pub trace_id: String,
    pub bot_id: String,
    pub symbol: String,
    pub bar_timestamp: String,
    pub phase: String,
    pub trigger_kind: String,
    pub signal: String,
    pub intent: String,
    pub risk_decision: String,
    pub outcome: String,
    pub created_at_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleTraceWrite {
    pub trace_id: String,
    pub bot_id: String,
    pub symbol: String,
    pub bar_timestamp: String,
    pub phase: String,
    pub trigger_kind: String,
    pub signal: String,
    pub intent: String,
    pub risk_decision: String,
    pub outcome: String,
    pub payload_json: String,
}

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

#[derive(Debug)]
pub struct SqliteRuntimeJournal {
    path: PathBuf,
    write_connection: Mutex<Connection>,
    read_connections: Vec<Mutex<Connection>>,
    next_read_connection: AtomicUsize,
}

fn recent_records<T: Clone>(mutex: &Mutex<Vec<T>>, label: &'static str, limit: usize) -> Vec<T> {
    if limit == 0 {
        return Vec::new();
    }

    let records = lock_mutex(mutex, label);
    let start = records.len().saturating_sub(limit);
    records[start..].to_vec()
}

fn lock_mutex<'a, T>(mutex: &'a Mutex<T>, label: &'static str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            // Workspace policy: recover from poisoning and log instead of
            // failing forever. A poisoned mutex only means another thread
            // panicked while holding the guard; the protected state remains
            // usable (a SQLite connection stays consistent after a Rust-side
            // panic — at worst an open transaction was rolled back).
            mutex.clear_poison();
            tracing::warn!("{label} mutex was poisoned; clearing poison and recovering the lock");
            poisoned.into_inner()
        }
    }
}

/// Last timestamp handed out by [`now_timestamp_ms`], used as a monotonic floor.
static LAST_TIMESTAMP_MS: AtomicI64 = AtomicI64::new(0);

/// Applies a monotonic floor to a raw wall-clock reading.
///
/// `raw` is the current wall-clock timestamp in milliseconds, or `None` when
/// the system clock is unavailable (e.g. it reports a time before the UNIX
/// epoch). The returned value never decreases across calls sharing the same
/// `floor` cell: a regressed or unavailable clock yields the last known good
/// timestamp instead of going backwards or collapsing to zero. Note that if
/// the clock has never been readable since process start, the floor is still 0
/// and that initial zero value will be returned.
fn monotonic_timestamp_ms(floor: &AtomicI64, raw: Option<i64>) -> i64 {
    match raw {
        Some(raw) => floor.fetch_max(raw, Ordering::Relaxed).max(raw),
        None => floor.load(Ordering::Relaxed),
    }
}

/// Returns the current wall-clock time in milliseconds since the UNIX epoch,
/// clamped to never go backwards within this process.
///
/// Granularity note: this feeds the `created_at_ms` fields on journal records,
/// which therefore cannot distinguish records created within the same
/// millisecond. Where exact insertion order matters, the autoincrement `id`
/// column is the tiebreaker.
pub(crate) fn now_timestamp_ms() -> i64 {
    let raw = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX));
    monotonic_timestamp_ms(&LAST_TIMESTAMP_MS, raw)
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to create storage directory `{path}`: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("storage backend `{kind}` is not supported")]
    UnsupportedBackend { kind: String },
    #[error(
        "sqlite schema version {actual} is incompatible with expected version {expected}; recreate the database"
    )]
    IncompatibleSchemaVersion { actual: i64, expected: i64 },
    #[error("internal storage error: {0}")]
    Internal(String),
    #[error("sqlite migration {version} `{name}` failed: {source}")]
    Migration {
        version: i64,
        name: &'static str,
        #[source]
        source: rusqlite::Error,
    },
    #[error("sqlite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}
