#[allow(clippy::wildcard_imports)]
use super::*;
use rusqlite::types::Value;

const SQLITE_DEFAULT_READ_POOL_SIZE: usize = 4;
const SQLITE_MIN_READ_POOL_SIZE: usize = 2;
const SQLITE_MAX_READ_POOL_SIZE: usize = 8;

impl SqliteRuntimeJournal {
    /// Opens a `SQLite` runtime journal and initializes required schema objects.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the parent directory cannot be created,
    /// the database cannot be opened, or schema initialization fails.
    pub fn open(path: impl AsRef<Path>, busy_timeout_ms: u64) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| StorageError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let busy_timeout = Duration::from_millis(busy_timeout_ms);
        let write_connection = Self::open_write_connection(&path, busy_timeout)?;
        let read_pool_size = Self::read_pool_size();
        let mut read_connections = Vec::with_capacity(read_pool_size);
        for _ in 0..read_pool_size {
            read_connections.push(Mutex::new(Self::open_read_connection(&path, busy_timeout)?));
        }

        Ok(Self {
            path,
            write_connection: Mutex::new(write_connection),
            read_connections,
            next_read_connection: AtomicUsize::new(0),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn open_write_connection(
        path: &Path,
        busy_timeout: Duration,
    ) -> Result<Connection, StorageError> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(busy_timeout)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        sqlite_migrations::run(&mut connection)?;
        Ok(connection)
    }

    fn open_read_connection(
        path: &Path,
        busy_timeout: Duration,
    ) -> Result<Connection, StorageError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(busy_timeout)?;
        Ok(connection)
    }

    fn read_pool_size() -> usize {
        std::thread::available_parallelism().map_or(SQLITE_DEFAULT_READ_POOL_SIZE, |parallelism| {
            parallelism
                .get()
                .clamp(SQLITE_MIN_READ_POOL_SIZE, SQLITE_MAX_READ_POOL_SIZE)
        })
    }

    fn with_write_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, StorageError> {
        let connection = lock_mutex(&self.write_connection, "sqlite_write_connection")?;
        operation(&connection).map_err(StorageError::from)
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, StorageError> {
        let read_pool_len = self.read_connections.len();
        if read_pool_len == 0 {
            return self.with_write_connection(operation);
        }

        let start = self
            .next_read_connection
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        for offset in 0..read_pool_len {
            let index = (start + offset) % read_pool_len;
            if let Ok(connection) = self.read_connections[index].try_lock() {
                return operation(&connection).map_err(StorageError::from);
            }
        }

        let fallback_index = start % read_pool_len;
        let connection = lock_mutex(
            &self.read_connections[fallback_index],
            "sqlite_read_connection",
        )?;
        operation(&connection).map_err(StorageError::from)
    }
}

impl RuntimeJournal for SqliteRuntimeJournal {
    fn append_event(&self, event: EventWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO runtime_events(scope, entity_id, trace_id, kind, payload, created_at_ms)
                VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    event.scope,
                    event.entity_id,
                    event.trace_id,
                    event.kind,
                    event.payload,
                    created_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn recent_events(&self, limit: usize) -> Result<Vec<RuntimeEvent>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, scope, entity_id, trace_id, kind, payload, created_at_ms
                FROM runtime_events
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(RuntimeEvent {
                    id: row.get(0)?,
                    scope: row.get(1)?,
                    entity_id: row.get(2)?,
                    trace_id: row.get(3)?,
                    kind: row.get(4)?,
                    payload: row.get(5)?,
                    created_at_ms: row.get(6)?,
                })
            })?;

            let mut events = rows.collect::<Result<Vec<_>, _>>()?;
            events.reverse();
            Ok(events)
        })
    }

    fn recent_events_by_scope(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, scope, entity_id, trace_id, kind, payload, created_at_ms
                FROM runtime_events
                WHERE scope = ?1
                ORDER BY id DESC
                LIMIT ?2
                ",
            )?;

            let rows = statement.query_map(params![scope, limit], |row| {
                Ok(RuntimeEvent {
                    id: row.get(0)?,
                    scope: row.get(1)?,
                    entity_id: row.get(2)?,
                    trace_id: row.get(3)?,
                    kind: row.get(4)?,
                    payload: row.get(5)?,
                    created_at_ms: row.get(6)?,
                })
            })?;

            let mut events = rows.collect::<Result<Vec<_>, _>>()?;
            events.reverse();
            Ok(events)
        })
    }

    fn recent_events_for_entity(
        &self,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, scope, entity_id, trace_id, kind, payload, created_at_ms
                FROM runtime_events
                WHERE entity_id = ?1
                ORDER BY id DESC
                LIMIT ?2
                ",
            )?;

            let rows = statement.query_map(params![entity_id, limit], |row| {
                Ok(RuntimeEvent {
                    id: row.get(0)?,
                    scope: row.get(1)?,
                    entity_id: row.get(2)?,
                    trace_id: row.get(3)?,
                    kind: row.get(4)?,
                    payload: row.get(5)?,
                    created_at_ms: row.get(6)?,
                })
            })?;

            let mut events = rows.collect::<Result<Vec<_>, _>>()?;
            events.reverse();
            Ok(events)
        })
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

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, scope, entity_id, trace_id, kind, payload, created_at_ms
                FROM runtime_events
                WHERE scope = ?1 AND entity_id = ?2
                ORDER BY id DESC
                LIMIT ?3
                ",
            )?;

            let rows = statement.query_map(params![scope, entity_id, limit], |row| {
                Ok(RuntimeEvent {
                    id: row.get(0)?,
                    scope: row.get(1)?,
                    entity_id: row.get(2)?,
                    trace_id: row.get(3)?,
                    kind: row.get(4)?,
                    payload: row.get(5)?,
                    created_at_ms: row.get(6)?,
                })
            })?;

            let mut events = rows.collect::<Result<Vec<_>, _>>()?;
            events.reverse();
            Ok(events)
        })
    }

    fn append_signal(&self, signal: SignalWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO signals(bot_id, symbol, trace_id, bar_timestamp, phase, signal, close, metadata_json, created_at_ms)
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ",
                params![
                    signal.bot_id,
                    signal.symbol,
                    signal.trace_id,
                    signal.bar_timestamp,
                    signal.phase,
                    signal.signal,
                    signal.close,
                    signal.metadata_json,
                    created_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn recent_signals(&self, limit: usize) -> Result<Vec<SignalRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, phase, signal, close, metadata_json, created_at_ms
                FROM signals
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(SignalRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    phase: row.get(5)?,
                    signal: row.get(6)?,
                    close: row.get(7)?,
                    metadata_json: row.get(8)?,
                    created_at_ms: row.get(9)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn append_intent(&self, intent: IntentWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO trade_intents(
                    bot_id,
                    symbol,
                    trace_id,
                    bar_timestamp,
                    signal,
                    intent,
                    metadata_json,
                    strategy_rationale,
                    has_position_before,
                    created_at_ms
                )
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    intent.bot_id,
                    intent.symbol,
                    intent.trace_id,
                    intent.bar_timestamp,
                    intent.signal,
                    intent.intent,
                    intent.metadata_json,
                    intent.strategy_rationale,
                    i64::from(intent.has_position_before),
                    created_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn recent_intents(&self, limit: usize) -> Result<Vec<IntentRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, signal, intent, metadata_json, strategy_rationale, has_position_before, created_at_ms
                FROM trade_intents
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(IntentRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    signal: row.get(5)?,
                    intent: row.get(6)?,
                    metadata_json: row.get(7)?,
                    strategy_rationale: row.get(8)?,
                    has_position_before: row.get::<_, i64>(9)? != 0,
                    created_at_ms: row.get(10)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn append_risk_decision(&self, decision: RiskDecisionWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO risk_decisions(
                    bot_id,
                    symbol,
                    trace_id,
                    bar_timestamp,
                    intent,
                    decision,
                    reason,
                    created_at_ms
                )
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
                params![
                    decision.bot_id,
                    decision.symbol,
                    decision.trace_id,
                    decision.bar_timestamp,
                    decision.intent,
                    decision.decision,
                    decision.reason,
                    created_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn recent_risk_decisions(&self, limit: usize) -> Result<Vec<RiskDecisionRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, intent, decision, reason, created_at_ms
                FROM risk_decisions
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(RiskDecisionRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    intent: row.get(5)?,
                    decision: row.get(6)?,
                    reason: row.get(7)?,
                    created_at_ms: row.get(8)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn append_order(&self, order: OrderWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT OR IGNORE INTO orders(
                    bot_id,
                    symbol,
                    trace_id,
                    bar_timestamp,
                    client_order_id,
                    intent,
                    status,
                    price,
                    quantity,
                    created_at_ms
                )
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    order.bot_id,
                    order.symbol,
                    order.trace_id,
                    order.bar_timestamp,
                    order.client_order_id,
                    order.intent,
                    order.status,
                    order.price,
                    order.quantity,
                    created_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn recent_orders(&self, limit: usize) -> Result<Vec<OrderRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, client_order_id, intent, status, price, quantity, created_at_ms
                FROM orders
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(OrderRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    client_order_id: row.get(5)?,
                    intent: row.get(6)?,
                    status: row.get(7)?,
                    price: row.get(8)?,
                    quantity: row.get(9)?,
                    created_at_ms: row.get(10)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn recent_orders_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<OrderRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, client_order_id, intent, status, price, quantity, created_at_ms
                FROM orders
                WHERE bot_id = ?1
                ORDER BY id DESC
                LIMIT ?2
                ",
            )?;

            let rows = statement.query_map(params![bot_id, limit], |row| {
                Ok(OrderRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    client_order_id: row.get(5)?,
                    intent: row.get(6)?,
                    status: row.get(7)?,
                    price: row.get(8)?,
                    quantity: row.get(9)?,
                    created_at_ms: row.get(10)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn orders_by_client_order_id(
        &self,
        client_order_id: &str,
    ) -> Result<Vec<OrderRecord>, StorageError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, client_order_id, intent, status, price, quantity, created_at_ms
                FROM orders
                WHERE client_order_id = ?1
                ORDER BY id ASC
                ",
            )?;

            let rows = statement.query_map([client_order_id], |row| {
                Ok(OrderRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    client_order_id: row.get(5)?,
                    intent: row.get(6)?,
                    status: row.get(7)?,
                    price: row.get(8)?,
                    quantity: row.get(9)?,
                    created_at_ms: row.get(10)?,
                })
            })?;

            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    fn append_fill(&self, fill: FillWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT OR IGNORE INTO fills(
                    bot_id,
                    symbol,
                    trace_id,
                    bar_timestamp,
                    client_order_id,
                    price,
                    quantity,
                    fee_asset,
                    fee_amount,
                    fee_normalized_usd,
                    created_at_ms
                )
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ",
                params![
                    fill.bot_id,
                    fill.symbol,
                    fill.trace_id,
                    fill.bar_timestamp,
                    fill.client_order_id,
                    fill.price,
                    fill.quantity,
                    fill.fee_asset,
                    fill.fee_amount,
                    fill.fee_normalized_usd,
                    created_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn recent_fills(&self, limit: usize) -> Result<Vec<FillRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, client_order_id, price, quantity, fee_asset, fee_amount, fee_normalized_usd, created_at_ms
                FROM fills
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(FillRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    client_order_id: row.get(5)?,
                    price: row.get(6)?,
                    quantity: row.get(7)?,
                    fee_asset: row.get(8)?,
                    fee_amount: row.get(9)?,
                    fee_normalized_usd: row.get(10)?,
                    created_at_ms: row.get(11)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn recent_fills_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<FillRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, client_order_id, price, quantity, fee_asset, fee_amount, fee_normalized_usd, created_at_ms
                FROM fills
                WHERE bot_id = ?1
                ORDER BY id DESC
                LIMIT ?2
                ",
            )?;

            let rows = statement.query_map(params![bot_id, limit], |row| {
                Ok(FillRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    client_order_id: row.get(5)?,
                    price: row.get(6)?,
                    quantity: row.get(7)?,
                    fee_asset: row.get(8)?,
                    fee_amount: row.get(9)?,
                    fee_normalized_usd: row.get(10)?,
                    created_at_ms: row.get(11)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn append_position(&self, position: PositionWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        let quantity = if position.quantity.is_finite() && position.quantity > 0.0 {
            position.quantity
        } else {
            0.0
        };
        let entry_price = position
            .entry_price
            .filter(|price| price.is_finite() && *price > 0.0);
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO positions(bot_id, symbol, trace_id, bar_timestamp, has_position, quantity, entry_price, realized_pnl_usd, reason, created_at_ms)
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    position.bot_id,
                    position.symbol,
                    position.trace_id,
                    position.bar_timestamp,
                    i64::from(position.has_position),
                    quantity,
                    entry_price,
                    if position.realized_pnl_usd.is_finite() {
                        position.realized_pnl_usd
                    } else {
                        0.0
                    },
                    position.reason,
                    created_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn recent_positions(&self, limit: usize) -> Result<Vec<PositionRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, has_position, quantity, entry_price, realized_pnl_usd, reason, created_at_ms
                FROM positions
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(PositionRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    has_position: row.get::<_, i64>(5)? != 0,
                    quantity: row.get(6)?,
                    entry_price: row.get(7)?,
                    realized_pnl_usd: row.get(8)?,
                    reason: row.get(9)?,
                    created_at_ms: row.get(10)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn recent_positions_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<PositionRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, has_position, quantity, entry_price, realized_pnl_usd, reason, created_at_ms
                FROM positions
                WHERE bot_id = ?1
                ORDER BY id DESC
                LIMIT ?2
                ",
            )?;

            let rows = statement.query_map(params![bot_id, limit], |row| {
                Ok(PositionRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    has_position: row.get::<_, i64>(5)? != 0,
                    quantity: row.get(6)?,
                    entry_price: row.get(7)?,
                    realized_pnl_usd: row.get(8)?,
                    reason: row.get(9)?,
                    created_at_ms: row.get(10)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn latest_position_for_bot(
        &self,
        bot_id: &str,
    ) -> Result<Option<PositionRecord>, StorageError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, has_position, quantity, entry_price, realized_pnl_usd, reason, created_at_ms
                FROM positions
                WHERE bot_id = ?1
                ORDER BY id DESC
                LIMIT 1
                ",
            )?;

            let mut rows = statement.query(params![bot_id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(PositionRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    has_position: row.get::<_, i64>(5)? != 0,
                    quantity: row.get(6)?,
                    entry_price: row.get(7)?,
                    realized_pnl_usd: row.get(8)?,
                    reason: row.get(9)?,
                    created_at_ms: row.get(10)?,
                }))
            } else {
                Ok(None)
            }
        })
    }

    fn latest_position_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<Option<PositionRecord>, StorageError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT id, bot_id, symbol, trace_id, bar_timestamp, has_position, quantity, entry_price, realized_pnl_usd, reason, created_at_ms
                FROM positions
                WHERE bot_id = ?1 AND symbol = ?2
                ORDER BY id DESC
                LIMIT 1
                ",
            )?;

            let mut rows = statement.query(params![bot_id, symbol])?;
            if let Some(row) = rows.next()? {
                Ok(Some(PositionRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    symbol: row.get(2)?,
                    trace_id: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    has_position: row.get::<_, i64>(5)? != 0,
                    quantity: row.get(6)?,
                    entry_price: row.get(7)?,
                    realized_pnl_usd: row.get(8)?,
                    reason: row.get(9)?,
                    created_at_ms: row.get(10)?,
                }))
            } else {
                Ok(None)
            }
        })
    }

    fn append_cycle_trace(&self, trace: CycleTraceWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT OR IGNORE INTO cycle_traces(
                    trace_id,
                    bot_id,
                    symbol,
                    bar_timestamp,
                    phase,
                    trigger_kind,
                    signal,
                    intent,
                    risk_decision,
                    outcome,
                    created_at_ms,
                    payload_json
                )
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ",
                params![
                    trace.trace_id,
                    trace.bot_id,
                    trace.symbol,
                    trace.bar_timestamp,
                    trace.phase,
                    trace.trigger_kind,
                    trace.signal,
                    trace.intent,
                    trace.risk_decision,
                    trace.outcome,
                    created_at_ms,
                    trace.payload_json,
                ],
            )?;
            Ok(())
        })
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

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut query = String::from(
                "
                SELECT id, trace_id, bot_id, symbol, bar_timestamp, phase, trigger_kind, signal, intent, risk_decision, outcome, created_at_ms, payload_json
                FROM cycle_traces
                WHERE bot_id = ?
                ",
            );
            let mut query_params = vec![Value::from(bot_id.to_owned())];

            if let Some(symbol) = symbol {
                query.push_str(" AND symbol = ?");
                query_params.push(Value::from(symbol.to_owned()));
            }
            if let Some(phase) = phase {
                query.push_str(" AND phase = ?");
                query_params.push(Value::from(phase.to_owned()));
            }
            if let Some(outcome) = outcome {
                query.push_str(" AND outcome = ?");
                query_params.push(Value::from(outcome.to_owned()));
            }
            if let Some(bar_timestamp) = bar_timestamp {
                query.push_str(" AND bar_timestamp = ?");
                query_params.push(Value::from(bar_timestamp.to_owned()));
            }

            query.push_str(" ORDER BY id DESC LIMIT ?");
            query_params.push(Value::from(limit));

            let mut statement = connection.prepare_cached(&query)?;
            let rows = statement.query_map(params_from_iter(query_params), |row| {
                Ok(CycleTraceRecord {
                    id: row.get(0)?,
                    trace_id: row.get(1)?,
                    bot_id: row.get(2)?,
                    symbol: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    phase: row.get(5)?,
                    trigger_kind: row.get(6)?,
                    signal: row.get(7)?,
                    intent: row.get(8)?,
                    risk_decision: row.get(9)?,
                    outcome: row.get(10)?,
                    created_at_ms: row.get(11)?,
                    payload_json: row.get(12)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn cycle_trace_by_id(&self, trace_id: &str) -> Result<Option<CycleTraceRecord>, StorageError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, trace_id, bot_id, symbol, bar_timestamp, phase, trigger_kind, signal, intent, risk_decision, outcome, created_at_ms, payload_json
                FROM cycle_traces
                WHERE trace_id = ?1
                LIMIT 1
                ",
            )?;

            let mut rows = statement.query(params![trace_id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(CycleTraceRecord {
                    id: row.get(0)?,
                    trace_id: row.get(1)?,
                    bot_id: row.get(2)?,
                    symbol: row.get(3)?,
                    bar_timestamp: row.get(4)?,
                    phase: row.get(5)?,
                    trigger_kind: row.get(6)?,
                    signal: row.get(7)?,
                    intent: row.get(8)?,
                    risk_decision: row.get(9)?,
                    outcome: row.get(10)?,
                    created_at_ms: row.get(11)?,
                    payload_json: row.get(12)?,
                }))
            } else {
                Ok(None)
            }
        })
    }

    fn append_reconciliation(&self, record: ReconciliationWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO reconciliations(
                    bot_id,
                    source,
                    symbol,
                    safe_to_trade,
                    local_open_orders,
                    connector_open_orders,
                    local_has_position,
                    connector_has_position,
                    reason,
                    created_at_ms
                )
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    record.bot_id,
                    record.source,
                    record.symbol,
                    i64::from(record.safe_to_trade),
                    record.local_open_orders,
                    record.connector_open_orders,
                    i64::from(record.local_has_position),
                    i64::from(record.connector_has_position),
                    record.reason,
                    created_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn recent_reconciliations(
        &self,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT
                    id,
                    bot_id,
                    source,
                    symbol,
                    safe_to_trade,
                    local_open_orders,
                    connector_open_orders,
                    local_has_position,
                    connector_has_position,
                    reason,
                    created_at_ms
                FROM reconciliations
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(ReconciliationRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    source: row.get(2)?,
                    symbol: row.get(3)?,
                    safe_to_trade: row.get::<_, i64>(4)? != 0,
                    local_open_orders: row.get(5)?,
                    connector_open_orders: row.get(6)?,
                    local_has_position: row.get::<_, i64>(7)? != 0,
                    connector_has_position: row.get::<_, i64>(8)? != 0,
                    reason: row.get(9)?,
                    created_at_ms: row.get(10)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn recent_reconciliations_for_bot(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<ReconciliationRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT
                    id,
                    bot_id,
                    source,
                    symbol,
                    safe_to_trade,
                    local_open_orders,
                    connector_open_orders,
                    local_has_position,
                    connector_has_position,
                    reason,
                    created_at_ms
                FROM reconciliations
                WHERE bot_id = ?1
                ORDER BY id DESC
                LIMIT ?2
                ",
            )?;

            let rows = statement.query_map(params![bot_id, limit], |row| {
                Ok(ReconciliationRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    source: row.get(2)?,
                    symbol: row.get(3)?,
                    safe_to_trade: row.get::<_, i64>(4)? != 0,
                    local_open_orders: row.get(5)?,
                    connector_open_orders: row.get(6)?,
                    local_has_position: row.get::<_, i64>(7)? != 0,
                    connector_has_position: row.get::<_, i64>(8)? != 0,
                    reason: row.get(9)?,
                    created_at_ms: row.get(10)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn latest_reconciliation_for_bot(
        &self,
        bot_id: &str,
    ) -> Result<Option<ReconciliationRecord>, StorageError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT
                    id,
                    bot_id,
                    source,
                    symbol,
                    safe_to_trade,
                    local_open_orders,
                    connector_open_orders,
                    local_has_position,
                    connector_has_position,
                    reason,
                    created_at_ms
                FROM reconciliations
                WHERE bot_id = ?1
                ORDER BY id DESC
                LIMIT 1
                ",
            )?;

            let mut rows = statement.query(params![bot_id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(ReconciliationRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    source: row.get(2)?,
                    symbol: row.get(3)?,
                    safe_to_trade: row.get::<_, i64>(4)? != 0,
                    local_open_orders: row.get(5)?,
                    connector_open_orders: row.get(6)?,
                    local_has_position: row.get::<_, i64>(7)? != 0,
                    connector_has_position: row.get::<_, i64>(8)? != 0,
                    reason: row.get(9)?,
                    created_at_ms: row.get(10)?,
                }))
            } else {
                Ok(None)
            }
        })
    }

    fn latest_reconciliation_for_lane(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<Option<ReconciliationRecord>, StorageError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "
                SELECT
                    id,
                    bot_id,
                    source,
                    symbol,
                    safe_to_trade,
                    local_open_orders,
                    connector_open_orders,
                    local_has_position,
                    connector_has_position,
                    reason,
                    created_at_ms
                FROM reconciliations
                WHERE bot_id = ?1 AND symbol = ?2
                ORDER BY id DESC
                LIMIT 1
                ",
            )?;

            let mut rows = statement.query(params![bot_id, symbol])?;
            if let Some(row) = rows.next()? {
                Ok(Some(ReconciliationRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    source: row.get(2)?,
                    symbol: row.get(3)?,
                    safe_to_trade: row.get::<_, i64>(4)? != 0,
                    local_open_orders: row.get(5)?,
                    connector_open_orders: row.get(6)?,
                    local_has_position: row.get::<_, i64>(7)? != 0,
                    connector_has_position: row.get::<_, i64>(8)? != 0,
                    reason: row.get(9)?,
                    created_at_ms: row.get(10)?,
                }))
            } else {
                Ok(None)
            }
        })
    }

    fn append_bot_event(&self, event: BotEventWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO bot_events(bot_id, kind, payload, created_at_ms)
                VALUES(?1, ?2, ?3, ?4)
                ",
                params![event.bot_id, event.kind, event.payload, created_at_ms],
            )?;
            Ok(())
        })
    }

    fn recent_bot_events(&self, limit: usize) -> Result<Vec<BotEventRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, bot_id, kind, payload, created_at_ms
                FROM bot_events
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(BotEventRecord {
                    id: row.get(0)?,
                    bot_id: row.get(1)?,
                    kind: row.get(2)?,
                    payload: row.get(3)?,
                    created_at_ms: row.get(4)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn append_service_event(&self, event: ServiceEventWrite) -> Result<(), StorageError> {
        let created_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO service_events(kind, payload, created_at_ms)
                VALUES(?1, ?2, ?3)
                ",
                params![event.kind, event.payload, created_at_ms],
            )?;
            Ok(())
        })
    }

    fn recent_service_events(&self, limit: usize) -> Result<Vec<ServiceEventRecord>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT id, kind, payload, created_at_ms
                FROM service_events
                ORDER BY id DESC
                LIMIT ?1
                ",
            )?;

            let rows = statement.query_map([limit], |row| {
                Ok(ServiceEventRecord {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    payload: row.get(2)?,
                    created_at_ms: row.get(3)?,
                })
            })?;

            let mut records = rows.collect::<Result<Vec<_>, _>>()?;
            records.reverse();
            Ok(records)
        })
    }

    fn upsert_bot_snapshot(&self, snapshot: BotSnapshotWrite) -> Result<(), StorageError> {
        let updated_at_ms = now_timestamp_ms();
        self.with_write_connection(|connection| {
            connection.execute(
                "
                INSERT INTO bot_snapshots(bot_id, state, execution_mode, enabled, updated_at_ms)
                VALUES(?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(bot_id) DO UPDATE SET
                    state = excluded.state,
                    execution_mode = excluded.execution_mode,
                    enabled = excluded.enabled,
                    updated_at_ms = excluded.updated_at_ms
                ",
                params![
                    snapshot.bot_id,
                    snapshot.state,
                    snapshot.execution_mode,
                    i64::from(snapshot.enabled),
                    updated_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    fn load_bot_snapshots(&self) -> Result<Vec<BotSnapshot>, StorageError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                SELECT bot_id, state, execution_mode, enabled, updated_at_ms
                FROM bot_snapshots
                ORDER BY bot_id ASC
                ",
            )?;

            statement
                .query_map([], |row| {
                    Ok(BotSnapshot {
                        bot_id: row.get(0)?,
                        state: row.get(1)?,
                        execution_mode: row.get(2)?,
                        enabled: row.get::<_, i64>(3)? != 0,
                        updated_at_ms: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
        })
    }

    fn prune_bots_except(&self, active_bot_ids: &HashSet<String>) -> Result<(), StorageError> {
        let mut connection = lock_mutex(&self.write_connection, "sqlite_write_connection")?;
        let transaction = connection.transaction()?;

        if active_bot_ids.is_empty() {
            for table in [
                "signals",
                "trade_intents",
                "risk_decisions",
                "orders",
                "fills",
                "positions",
                "cycle_traces",
                "reconciliations",
                "bot_events",
                "bot_snapshots",
            ] {
                transaction.execute(&format!("DELETE FROM {table}"), [])?;
            }
            transaction.execute("DELETE FROM runtime_events WHERE entity_id IS NOT NULL", [])?;
        } else {
            for table in [
                "signals",
                "trade_intents",
                "risk_decisions",
                "orders",
                "fills",
                "positions",
                "cycle_traces",
                "reconciliations",
                "bot_events",
                "bot_snapshots",
            ] {
                delete_sqlite_rows_for_removed_bots(&transaction, table, "bot_id", active_bot_ids)?;
            }

            let placeholders = std::iter::repeat_n("?", active_bot_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "DELETE FROM runtime_events WHERE entity_id IS NOT NULL AND entity_id NOT IN ({placeholders})"
            );
            transaction.execute(&sql, params_from_iter(active_bot_ids.iter()))?;
        }

        transaction.commit()?;
        Ok(())
    }
}

fn delete_sqlite_rows_for_removed_bots(
    connection: &Connection,
    table: &str,
    column: &str,
    active_bot_ids: &HashSet<String>,
) -> Result<(), rusqlite::Error> {
    let placeholders = std::iter::repeat_n("?", active_bot_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("DELETE FROM {table} WHERE {column} NOT IN ({placeholders})");
    connection.execute(&sql, params_from_iter(active_bot_ids.iter()))?;
    Ok(())
}
