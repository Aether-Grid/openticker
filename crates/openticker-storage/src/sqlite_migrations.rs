use crate::StorageError;
use rusqlite::Connection;

const SQLITE_SCHEMA_SQL: &str = r"
CREATE TABLE IF NOT EXISTS runtime_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scope TEXT NOT NULL,
    entity_id TEXT NULL,
    trace_id TEXT NULL,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_events_created_at_ms
    ON runtime_events(created_at_ms);

CREATE INDEX IF NOT EXISTS idx_runtime_events_scope_entity_id
    ON runtime_events(scope, entity_id);

CREATE INDEX IF NOT EXISTS idx_runtime_events_entity_id_created_at_ms
    ON runtime_events(entity_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_runtime_events_scope_id_desc
    ON runtime_events(scope, id DESC);

CREATE INDEX IF NOT EXISTS idx_runtime_events_entity_id_id_desc
    ON runtime_events(entity_id, id DESC);

CREATE INDEX IF NOT EXISTS idx_runtime_events_scope_entity_id_id_desc
    ON runtime_events(scope, entity_id, id DESC);

CREATE INDEX IF NOT EXISTS idx_runtime_events_trace_id_id_desc
    ON runtime_events(trace_id, id DESC);

CREATE TABLE IF NOT EXISTS signals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id TEXT NOT NULL,
    symbol TEXT NULL,
    trace_id TEXT NULL,
    bar_timestamp TEXT NOT NULL,
    phase TEXT NOT NULL,
    signal TEXT NOT NULL,
    close REAL NOT NULL,
    metadata_json TEXT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_signals_bot_id_created_at_ms
    ON signals(bot_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_signals_trace_id
    ON signals(trace_id);

CREATE TABLE IF NOT EXISTS trade_intents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id TEXT NOT NULL,
    symbol TEXT NULL,
    trace_id TEXT NULL,
    bar_timestamp TEXT NOT NULL,
    signal TEXT NOT NULL,
    intent TEXT NOT NULL,
    metadata_json TEXT NULL,
    strategy_rationale TEXT NULL,
    has_position_before INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_trade_intents_bot_id_created_at_ms
    ON trade_intents(bot_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_trade_intents_trace_id
    ON trade_intents(trace_id);

CREATE TABLE IF NOT EXISTS risk_decisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id TEXT NOT NULL,
    symbol TEXT NULL,
    trace_id TEXT NULL,
    bar_timestamp TEXT NOT NULL,
    intent TEXT NOT NULL,
    decision TEXT NOT NULL,
    reason TEXT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_risk_decisions_bot_id_created_at_ms
    ON risk_decisions(bot_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_risk_decisions_trace_id
    ON risk_decisions(trace_id);

CREATE TABLE IF NOT EXISTS orders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id TEXT NOT NULL,
    symbol TEXT NULL,
    trace_id TEXT NULL,
    bar_timestamp TEXT NULL,
    client_order_id TEXT NOT NULL,
    intent TEXT NOT NULL,
    status TEXT NOT NULL,
    price REAL NOT NULL,
    quantity REAL NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_orders_bot_id_created_at_ms
    ON orders(bot_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_orders_bot_id_id_desc
    ON orders(bot_id, id DESC);

CREATE INDEX IF NOT EXISTS idx_orders_client_order_id
    ON orders(client_order_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_bot_client_order_id
    ON orders(bot_id, COALESCE(symbol, ''), client_order_id);

CREATE INDEX IF NOT EXISTS idx_orders_bot_id_symbol_created_at_ms
    ON orders(bot_id, symbol, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_orders_trace_id
    ON orders(trace_id);

CREATE TABLE IF NOT EXISTS fills (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id TEXT NOT NULL,
    symbol TEXT NULL,
    trace_id TEXT NULL,
    bar_timestamp TEXT NULL,
    client_order_id TEXT NOT NULL,
    price REAL NOT NULL,
    quantity REAL NOT NULL,
    fee_asset TEXT NULL,
    fee_amount REAL NULL,
    fee_normalized_usd REAL NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_fills_bot_id_created_at_ms
    ON fills(bot_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_fills_bot_id_id_desc
    ON fills(bot_id, id DESC);

CREATE INDEX IF NOT EXISTS idx_fills_client_order_id
    ON fills(client_order_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_fills_bot_client_order_id
    ON fills(bot_id, COALESCE(symbol, ''), client_order_id);

CREATE INDEX IF NOT EXISTS idx_fills_bot_id_symbol_created_at_ms
    ON fills(bot_id, symbol, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_fills_trace_id
    ON fills(trace_id);

CREATE TABLE IF NOT EXISTS positions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id TEXT NOT NULL,
    symbol TEXT NULL,
    trace_id TEXT NULL,
    bar_timestamp TEXT NULL,
    has_position INTEGER NOT NULL,
    quantity REAL NOT NULL,
    entry_price REAL NULL,
    realized_pnl_usd REAL NOT NULL DEFAULT 0.0,
    reason TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_positions_bot_id_created_at_ms
    ON positions(bot_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_positions_bot_id_id_desc
    ON positions(bot_id, id DESC);

CREATE INDEX IF NOT EXISTS idx_positions_bot_id_symbol_created_at_ms
    ON positions(bot_id, symbol, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_positions_bot_id_symbol_id_desc
    ON positions(bot_id, symbol, id DESC);

CREATE INDEX IF NOT EXISTS idx_positions_trace_id
    ON positions(trace_id);

CREATE TABLE IF NOT EXISTS cycle_traces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trace_id TEXT NOT NULL,
    bot_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    bar_timestamp TEXT NOT NULL,
    phase TEXT NOT NULL,
    trigger_kind TEXT NOT NULL,
    signal TEXT NOT NULL,
    intent TEXT NOT NULL,
    risk_decision TEXT NOT NULL,
    outcome TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_cycle_traces_trace_id
    ON cycle_traces(trace_id);

CREATE INDEX IF NOT EXISTS idx_cycle_traces_bot_id_id_desc
    ON cycle_traces(bot_id, id DESC);

CREATE INDEX IF NOT EXISTS idx_cycle_traces_bot_id_symbol_id_desc
    ON cycle_traces(bot_id, symbol, id DESC);

CREATE INDEX IF NOT EXISTS idx_cycle_traces_bot_id_symbol_bar_timestamp_desc
    ON cycle_traces(bot_id, symbol, bar_timestamp DESC);

CREATE TABLE IF NOT EXISTS reconciliations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id TEXT NOT NULL,
    source TEXT NOT NULL,
    symbol TEXT NOT NULL,
    safe_to_trade INTEGER NOT NULL,
    local_open_orders INTEGER NOT NULL,
    connector_open_orders INTEGER NOT NULL,
    local_has_position INTEGER NOT NULL,
    connector_has_position INTEGER NOT NULL,
    reason TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reconciliations_bot_id_created_at_ms
    ON reconciliations(bot_id, created_at_ms);

CREATE INDEX IF NOT EXISTS idx_reconciliations_bot_id_id_desc
    ON reconciliations(bot_id, id DESC);

CREATE INDEX IF NOT EXISTS idx_reconciliations_bot_id_symbol_id_desc
    ON reconciliations(bot_id, symbol, id DESC);

CREATE TABLE IF NOT EXISTS bot_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_bot_events_bot_id_created_at_ms
    ON bot_events(bot_id, created_at_ms);

CREATE TABLE IF NOT EXISTS service_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_service_events_created_at_ms
    ON service_events(created_at_ms);

CREATE TABLE IF NOT EXISTS bot_snapshots (
    bot_id TEXT PRIMARY KEY,
    state TEXT NOT NULL,
    execution_mode TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
";

pub(crate) const CURRENT_SCHEMA_VERSION: i64 = 9;

pub(crate) fn run(connection: &mut Connection) -> Result<(), StorageError> {
    let current_version = user_version(connection)?;
    if current_version == 0 {
        connection.execute_batch(SQLITE_SCHEMA_SQL)?;
        set_user_version(connection, CURRENT_SCHEMA_VERSION)?;
        return Ok(());
    }

    if current_version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::IncompatibleSchemaVersion {
            actual: current_version,
            expected: CURRENT_SCHEMA_VERSION,
        });
    }

    connection.execute_batch(SQLITE_SCHEMA_SQL)?;
    Ok(())
}

pub(crate) fn user_version(connection: &Connection) -> Result<i64, rusqlite::Error> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

pub(crate) fn set_user_version(
    connection: &Connection,
    version: i64,
) -> Result<(), rusqlite::Error> {
    connection.execute_batch(&format!("PRAGMA user_version = {version};"))
}
