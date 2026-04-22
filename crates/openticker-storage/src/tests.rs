use super::*;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

#[test]
fn in_memory_journal_returns_recent_events() {
    let journal = InMemoryRuntimeJournal::default();

    journal
        .append_event(EventWrite {
            scope: "instance".to_owned(),
            entity_id: Some("aapl".to_owned()),
            trace_id: None,
            kind: "instance.started".to_owned(),
            payload: "{}".to_owned(),
        })
        .unwrap();
    journal
        .append_event(EventWrite {
            scope: "instance".to_owned(),
            entity_id: Some("aapl".to_owned()),
            trace_id: None,
            kind: "instance.paused".to_owned(),
            payload: "{}".to_owned(),
        })
        .unwrap();

    let events = journal.recent_events(1).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "instance.paused");

    let instance_events = journal.recent_events_by_scope("instance", 10).unwrap();
    assert_eq!(instance_events.len(), 2);
    let missing_scope_events = journal.recent_events_by_scope("signal", 10).unwrap();
    assert!(missing_scope_events.is_empty());

    let entity_events = journal.recent_events_for_entity("aapl", 10).unwrap();
    assert_eq!(entity_events.len(), 2);
    assert!(
        entity_events
            .iter()
            .all(|event| event.entity_id.as_deref() == Some("aapl"))
    );

    let scoped_entity_events = journal
        .recent_events_by_scope_and_entity("instance", "aapl", 10)
        .unwrap();
    assert_eq!(scoped_entity_events.len(), 2);
    assert!(
        journal
            .recent_events_for_entity("msft", 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn in_memory_journal_tracks_reconciliation_records() {
    let journal = InMemoryRuntimeJournal::default();

    journal
        .append_reconciliation(ReconciliationWrite {
            bot_id: "aapl".to_owned(),
            source: "startup".to_owned(),
            symbol: "AAPL".to_owned(),
            safe_to_trade: false,
            local_open_orders: 1,
            connector_open_orders: 0,
            local_has_position: true,
            connector_has_position: false,
            reason: "position_mismatch(local=true,connector=false)".to_owned(),
        })
        .unwrap();

    let records = journal.recent_reconciliations(10).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].bot_id, "aapl");
    assert!(!records[0].safe_to_trade);

    let latest = journal.latest_reconciliation_for_bot("aapl").unwrap();
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().source, "startup");
}

#[test]
fn in_memory_journal_deduplicates_fills_by_instance_and_client_order_id() {
    let journal = InMemoryRuntimeJournal::default();

    journal
        .append_fill(FillWrite {
            bot_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: None,
            client_order_id: "aapl-1-open_long".to_owned(),
            price: 123.45,
            quantity: 1.0,
            fee_asset: None,
            fee_amount: None,
            fee_normalized_usd: None,
        })
        .unwrap();
    journal
        .append_fill(FillWrite {
            bot_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: None,
            client_order_id: "aapl-1-open_long".to_owned(),
            price: 123.45,
            quantity: 1.0,
            fee_asset: None,
            fee_amount: None,
            fee_normalized_usd: None,
        })
        .unwrap();

    let fills = journal.recent_fills(10).unwrap();
    assert_eq!(fills.len(), 1);
}

#[test]
fn in_memory_journal_can_lookup_orders_by_client_order_id() {
    let journal = InMemoryRuntimeJournal::default();

    for (bot_id, symbol) in [("aapl", "AAPL"), ("msft", "MSFT")] {
        journal
            .append_order(OrderWrite {
                bot_id: bot_id.to_owned(),
                symbol: symbol.to_owned(),
                trace_id: None,
                bar_timestamp: None,
                client_order_id: "shared-order-id".to_owned(),
                intent: "open_long".to_owned(),
                status: "submitted".to_owned(),
                price: 100.0,
                quantity: 1.0,
            })
            .unwrap();
    }

    let orders = journal
        .orders_by_client_order_id("shared-order-id")
        .unwrap();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].bot_id, "aapl");
    assert_eq!(orders[1].bot_id, "msft");
}

#[test]
fn sqlite_journal_persists_events_and_snapshots() {
    let path = create_temp_db_path("persists");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    seed_sqlite_journal(&journal).unwrap();

    let reopened = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    assert_sqlite_journal_contents(&reopened).unwrap();
}

#[test]
fn sqlite_journal_can_lookup_orders_by_client_order_id() {
    let path = create_temp_db_path("orders-by-client-order-id");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();

    for (bot_id, symbol) in [("aapl", "AAPL"), ("msft", "MSFT")] {
        journal
            .append_order(OrderWrite {
                bot_id: bot_id.to_owned(),
                symbol: symbol.to_owned(),
                trace_id: None,
                bar_timestamp: None,
                client_order_id: "shared-order-id".to_owned(),
                intent: "open_long".to_owned(),
                status: "submitted".to_owned(),
                price: 100.0,
                quantity: 1.0,
            })
            .unwrap();
    }

    let orders = journal
        .orders_by_client_order_id("shared-order-id")
        .unwrap();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].bot_id, "aapl");
    assert_eq!(orders[1].bot_id, "msft");
}

#[test]
fn sqlite_journal_sets_schema_version_for_fresh_database() {
    let path = create_temp_db_path("schema-version");
    let _journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();

    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        sqlite_migrations::user_version(&connection).unwrap(),
        sqlite_migrations::CURRENT_SCHEMA_VERSION
    );
}

#[test]
fn sqlite_journal_uses_bot_schema_names() {
    let path = create_temp_db_path("bot-schema");
    let _journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();

    let connection = Connection::open(&path).unwrap();
    let tables = connection
        .prepare(
            "
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
              AND name IN ('bot_events', 'bot_snapshots', 'instance_events', 'instance_snapshots')
            ORDER BY name ASC
            ",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        tables,
        vec!["bot_events".to_owned(), "bot_snapshots".to_owned()]
    );

    let mut statement = connection
        .prepare("PRAGMA table_info(bot_snapshots)")
        .unwrap();
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(columns.iter().any(|column| column == "bot_id"));
    assert!(!columns.iter().any(|column| column == "instance_id"));
}

#[test]
fn sqlite_journal_rejects_older_schema_versions() {
    let path = create_temp_db_path("legacy-schema-version");
    let connection = Connection::open(&path).unwrap();
    sqlite_migrations::set_user_version(&connection, sqlite_migrations::CURRENT_SCHEMA_VERSION - 1)
        .unwrap();
    drop(connection);

    let error = SqliteRuntimeJournal::open(&path, 1_000).unwrap_err();
    assert!(matches!(
        error,
        StorageError::IncompatibleSchemaVersion { actual, expected }
            if actual == sqlite_migrations::CURRENT_SCHEMA_VERSION - 1
                && expected == sqlite_migrations::CURRENT_SCHEMA_VERSION
    ));
}

#[test]
fn sqlite_journal_rejects_newer_schema_versions() {
    let path = create_temp_db_path("future-schema-version");
    let connection = Connection::open(&path).unwrap();
    sqlite_migrations::set_user_version(&connection, sqlite_migrations::CURRENT_SCHEMA_VERSION + 1)
        .unwrap();
    drop(connection);

    let error = SqliteRuntimeJournal::open(&path, 1_000).unwrap_err();
    assert!(matches!(
        error,
        StorageError::IncompatibleSchemaVersion {
            actual,
            expected,
        } if actual == sqlite_migrations::CURRENT_SCHEMA_VERSION + 1
            && expected == sqlite_migrations::CURRENT_SCHEMA_VERSION
    ));
}

#[test]
fn in_memory_journal_prunes_removed_instances() {
    let journal = InMemoryRuntimeJournal::default();
    append_signal_flow_records(&journal).unwrap();
    append_order_flow_records(&journal).unwrap();
    append_cycle_trace_records(&journal).unwrap();
    append_reconciliation_records(&journal).unwrap();
    append_removed_instance_records(&journal, "msft").unwrap();

    journal
        .prune_bots_except(&HashSet::from(["aapl".to_owned()]))
        .unwrap();

    assert_pruned_journal_contents(&journal).unwrap();
}

#[test]
fn sqlite_journal_prunes_removed_instances() {
    let path = create_temp_db_path("prune-removed-instances");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    append_signal_flow_records(&journal).unwrap();
    append_order_flow_records(&journal).unwrap();
    append_cycle_trace_records(&journal).unwrap();
    append_reconciliation_records(&journal).unwrap();
    append_removed_instance_records(&journal, "msft").unwrap();

    journal
        .prune_bots_except(&HashSet::from(["aapl".to_owned()]))
        .unwrap();

    let reopened = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    assert_pruned_journal_contents(&reopened).unwrap();
}

fn seed_sqlite_journal(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    append_signal_flow_records(journal)?;
    append_order_flow_records(journal)?;
    append_cycle_trace_records(journal)?;
    append_reconciliation_records(journal)?;
    Ok(())
}

fn append_signal_flow_records(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    journal.append_event(EventWrite {
        scope: "service".to_owned(),
        entity_id: None,
        trace_id: None,
        kind: "service.started".to_owned(),
        payload: "{}".to_owned(),
    })?;
    journal.append_event(EventWrite {
        scope: "order".to_owned(),
        entity_id: Some("aapl".to_owned()),
        trace_id: Some("trace-aapl-1".to_owned()),
        kind: "order.submitted".to_owned(),
        payload: "{}".to_owned(),
    })?;
    journal.upsert_bot_snapshot(BotSnapshotWrite {
        bot_id: "aapl".to_owned(),
        state: "paused".to_owned(),
        execution_mode: "paper".to_owned(),
        enabled: true,
    })?;
    journal.append_signal(SignalWrite {
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        trace_id: Some("trace-aapl-1".to_owned()),
        bar_timestamp: "2026-01-01T00:00:00Z".to_owned(),
        phase: "confirmed".to_owned(),
        signal: "buy_confirmed".to_owned(),
        close: 123.45,
        metadata_json: Some(r#"{"strength":"strong"}"#.to_owned()),
    })?;
    journal.append_intent(IntentWrite {
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        trace_id: Some("trace-aapl-1".to_owned()),
        bar_timestamp: "2026-01-01T00:00:00Z".to_owned(),
        signal: "buy_confirmed".to_owned(),
        intent: "open_long".to_owned(),
        metadata_json: Some(r#"{"strength":"strong"}"#.to_owned()),
        strategy_rationale: Some("signal_metadata_matched".to_owned()),
        has_position_before: false,
    })?;
    journal.append_risk_decision(RiskDecisionWrite {
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        trace_id: Some("trace-aapl-1".to_owned()),
        bar_timestamp: "2026-01-01T00:00:00Z".to_owned(),
        intent: "open_long".to_owned(),
        decision: "allowed".to_owned(),
        reason: None,
    })?;
    Ok(())
}

fn append_order_flow_records(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    let order = OrderWrite {
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        trace_id: Some("trace-aapl-1".to_owned()),
        bar_timestamp: Some("2026-01-01T00:00:00Z".to_owned()),
        client_order_id: "aapl-1-open_long".to_owned(),
        intent: "open_long".to_owned(),
        status: "submitted".to_owned(),
        price: 123.45,
        quantity: 1.0,
    };
    journal.append_order(order.clone())?;
    journal.append_order(order)?;

    let fill = FillWrite {
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        trace_id: Some("trace-aapl-1".to_owned()),
        bar_timestamp: Some("2026-01-01T00:00:00Z".to_owned()),
        client_order_id: "aapl-1-open_long".to_owned(),
        price: 123.45,
        quantity: 1.0,
        fee_asset: Some("USD".to_owned()),
        fee_amount: Some(0.5),
        fee_normalized_usd: Some(0.5),
    };
    journal.append_fill(fill.clone())?;
    journal.append_fill(fill)?;

    journal.append_position(PositionWrite {
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        trace_id: Some("trace-aapl-1".to_owned()),
        bar_timestamp: Some("2026-01-01T00:00:00Z".to_owned()),
        has_position: true,
        quantity: 1.0,
        entry_price: Some(123.45),
        realized_pnl_usd: 12.34,
        reason: "order_filled".to_owned(),
    })?;
    journal.append_bot_event(BotEventWrite {
        bot_id: "aapl".to_owned(),
        kind: "instance.started".to_owned(),
        payload: "state=running".to_owned(),
    })?;
    journal.append_service_event(ServiceEventWrite {
        kind: "service.started".to_owned(),
        payload: "instances=1".to_owned(),
    })?;
    Ok(())
}

fn append_cycle_trace_records(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    journal.append_cycle_trace(CycleTraceWrite {
        trace_id: "trace-aapl-1".to_owned(),
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        bar_timestamp: "2026-01-01T00:00:00Z".to_owned(),
        phase: "confirmed".to_owned(),
        trigger_kind: "market_bar".to_owned(),
        signal: "buy_confirmed".to_owned(),
        intent: "open_long".to_owned(),
        risk_decision: "allowed".to_owned(),
        outcome: "accepted_filled".to_owned(),
        payload_json: r#"{"summary":{"trace_id":"trace-aapl-1"}}"#.to_owned(),
    })
}

fn append_reconciliation_records(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    journal.append_reconciliation(ReconciliationWrite {
        bot_id: "aapl".to_owned(),
        source: "manual".to_owned(),
        symbol: "AAPL".to_owned(),
        safe_to_trade: true,
        local_open_orders: 0,
        connector_open_orders: 0,
        local_has_position: false,
        connector_has_position: false,
        reason: "state_aligned".to_owned(),
    })?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn append_removed_instance_records(
    journal: &dyn RuntimeJournal,
    instance_id: &str,
) -> Result<(), StorageError> {
    journal.append_event(EventWrite {
        scope: "instance".to_owned(),
        entity_id: Some(instance_id.to_owned()),
        trace_id: None,
        kind: "instance.removed".to_owned(),
        payload: "{}".to_owned(),
    })?;
    journal.upsert_bot_snapshot(BotSnapshotWrite {
        bot_id: instance_id.to_owned(),
        state: "stopped".to_owned(),
        execution_mode: "paper".to_owned(),
        enabled: false,
    })?;
    journal.append_signal(SignalWrite {
        bot_id: instance_id.to_owned(),
        symbol: instance_id.to_uppercase(),
        trace_id: Some(format!("trace-{instance_id}-1")),
        bar_timestamp: "2026-01-01T00:05:00Z".to_owned(),
        phase: "confirmed".to_owned(),
        signal: "sell_confirmed".to_owned(),
        close: 98.76,
        metadata_json: None,
    })?;
    journal.append_intent(IntentWrite {
        bot_id: instance_id.to_owned(),
        symbol: instance_id.to_uppercase(),
        trace_id: Some(format!("trace-{instance_id}-1")),
        bar_timestamp: "2026-01-01T00:05:00Z".to_owned(),
        signal: "sell_confirmed".to_owned(),
        intent: "close_long".to_owned(),
        metadata_json: None,
        strategy_rationale: Some("removed_bot_seed".to_owned()),
        has_position_before: true,
    })?;
    journal.append_risk_decision(RiskDecisionWrite {
        bot_id: instance_id.to_owned(),
        symbol: instance_id.to_uppercase(),
        trace_id: Some(format!("trace-{instance_id}-1")),
        bar_timestamp: "2026-01-01T00:05:00Z".to_owned(),
        intent: "close_long".to_owned(),
        decision: "allowed".to_owned(),
        reason: None,
    })?;
    journal.append_order(OrderWrite {
        bot_id: instance_id.to_owned(),
        symbol: instance_id.to_uppercase(),
        trace_id: Some(format!("trace-{instance_id}-1")),
        bar_timestamp: Some("2026-01-01T00:05:00Z".to_owned()),
        client_order_id: format!("{instance_id}-1-close_long"),
        intent: "close_long".to_owned(),
        status: "submitted".to_owned(),
        price: 98.76,
        quantity: 2.0,
    })?;
    journal.append_fill(FillWrite {
        bot_id: instance_id.to_owned(),
        symbol: instance_id.to_uppercase(),
        trace_id: Some(format!("trace-{instance_id}-1")),
        bar_timestamp: Some("2026-01-01T00:05:00Z".to_owned()),
        client_order_id: format!("{instance_id}-1-close_long"),
        price: 98.76,
        quantity: 2.0,
        fee_asset: None,
        fee_amount: None,
        fee_normalized_usd: None,
    })?;
    journal.append_position(PositionWrite {
        bot_id: instance_id.to_owned(),
        symbol: instance_id.to_uppercase(),
        trace_id: Some(format!("trace-{instance_id}-1")),
        bar_timestamp: Some("2026-01-01T00:05:00Z".to_owned()),
        has_position: false,
        quantity: 0.0,
        entry_price: None,
        realized_pnl_usd: -5.0,
        reason: "removed_bot_seed".to_owned(),
    })?;
    journal.append_cycle_trace(CycleTraceWrite {
        trace_id: format!("trace-{instance_id}-1"),
        bot_id: instance_id.to_owned(),
        symbol: instance_id.to_uppercase(),
        bar_timestamp: "2026-01-01T00:05:00Z".to_owned(),
        phase: "confirmed".to_owned(),
        trigger_kind: "market_bar".to_owned(),
        signal: "sell_confirmed".to_owned(),
        intent: "close_long".to_owned(),
        risk_decision: "allowed".to_owned(),
        outcome: "accepted_filled".to_owned(),
        payload_json: format!(r#"{{"summary":{{"trace_id":"trace-{instance_id}-1"}}}}"#),
    })?;
    journal.append_reconciliation(ReconciliationWrite {
        bot_id: instance_id.to_owned(),
        source: "startup".to_owned(),
        symbol: instance_id.to_uppercase(),
        safe_to_trade: false,
        local_open_orders: 1,
        connector_open_orders: 0,
        local_has_position: false,
        connector_has_position: false,
        reason: "removed_bot_seed".to_owned(),
    })?;
    journal.append_bot_event(BotEventWrite {
        bot_id: instance_id.to_owned(),
        kind: "instance.stopped".to_owned(),
        payload: "state=stopped".to_owned(),
    })?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn assert_sqlite_journal_contents(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    let events = journal.recent_events(10)?;
    assert_eq!(events.len(), 2);

    let order_events = journal.recent_events_by_scope("order", 10)?;
    assert_eq!(order_events.len(), 1);
    assert_eq!(order_events[0].kind, "order.submitted");

    let entity_events = journal.recent_events_for_entity("aapl", 10)?;
    assert_eq!(entity_events.len(), 1);
    assert_eq!(entity_events[0].scope, "order");

    let scoped_entity_events = journal.recent_events_by_scope_and_entity("order", "aapl", 10)?;
    assert_eq!(scoped_entity_events.len(), 1);
    assert_eq!(scoped_entity_events[0].kind, "order.submitted");
    assert_eq!(
        scoped_entity_events[0].trace_id.as_deref(),
        Some("trace-aapl-1")
    );
    assert!(
        journal
            .recent_events_by_scope_and_entity("service", "aapl", 10)?
            .is_empty()
    );

    let signals = journal.recent_signals(10)?;
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].signal, "buy_confirmed");
    assert_eq!(signals[0].trace_id.as_deref(), Some("trace-aapl-1"));

    let intents = journal.recent_intents(10)?;
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].intent, "open_long");

    let risk_decisions = journal.recent_risk_decisions(10)?;
    assert_eq!(risk_decisions.len(), 1);
    assert_eq!(risk_decisions[0].decision, "allowed");
    assert_eq!(risk_decisions[0].trace_id.as_deref(), Some("trace-aapl-1"));

    let orders = journal.recent_orders(10)?;
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].status, "submitted");
    assert_eq!(orders[0].trace_id.as_deref(), Some("trace-aapl-1"));
    assert_eq!(
        orders[0].bar_timestamp.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );

    let instance_orders = journal.recent_orders_for_bot("aapl", 10)?;
    assert_eq!(instance_orders.len(), 1);
    assert_eq!(instance_orders[0].bot_id, "aapl");

    let fills = journal.recent_fills(10)?;
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].client_order_id, "aapl-1-open_long");
    assert_eq!(fills[0].trace_id.as_deref(), Some("trace-aapl-1"));
    assert_eq!(
        fills[0].bar_timestamp.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );
    assert_eq!(fills[0].fee_asset.as_deref(), Some("USD"));
    assert_eq!(fills[0].fee_amount, Some(0.5));
    assert_eq!(fills[0].fee_normalized_usd, Some(0.5));

    let instance_fills = journal.recent_fills_for_bot("aapl", 10)?;
    assert_eq!(instance_fills.len(), 1);
    assert_eq!(instance_fills[0].bot_id, "aapl");
    assert!(journal.recent_fills_for_bot("msft", 10)?.is_empty());

    let positions = journal.recent_positions(10)?;
    assert_eq!(positions.len(), 1);
    assert!(positions[0].has_position);
    assert_eq!(positions[0].trace_id.as_deref(), Some("trace-aapl-1"));
    assert_eq!(
        positions[0].bar_timestamp.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );
    assert!((positions[0].quantity - 1.0).abs() < f64::EPSILON);
    assert_eq!(positions[0].entry_price, Some(123.45));
    assert!((positions[0].realized_pnl_usd - 12.34).abs() < f64::EPSILON);

    let instance_positions = journal.recent_positions_for_bot("aapl", 10)?;
    assert_eq!(instance_positions.len(), 1);
    assert_eq!(instance_positions[0].bot_id, "aapl");
    assert_eq!(instance_positions[0].symbol.as_deref(), Some("AAPL"));
    assert!(journal.recent_positions_for_bot("msft", 10)?.is_empty());

    let latest_position = journal.latest_position_for_bot("aapl")?;
    assert!(latest_position.is_some());
    let latest_position = latest_position.unwrap();
    assert!(latest_position.has_position);
    assert!((latest_position.quantity - 1.0).abs() < f64::EPSILON);
    assert_eq!(latest_position.entry_price, Some(123.45));
    assert!((latest_position.realized_pnl_usd - 12.34).abs() < f64::EPSILON);
    assert_eq!(latest_position.symbol.as_deref(), Some("AAPL"));
    assert!(journal.latest_position_for_bot("msft")?.is_none());
    let latest_lane_position = journal.latest_position_for_lane("aapl", "AAPL")?;
    assert!(latest_lane_position.is_some());
    assert!(journal.latest_position_for_lane("aapl", "MSFT")?.is_none());

    let cycle_traces =
        journal.recent_cycle_traces_for_bot("aapl", Some("AAPL"), None, None, None, 10)?;
    assert_eq!(cycle_traces.len(), 1);
    assert_eq!(cycle_traces[0].trace_id, "trace-aapl-1");
    assert_eq!(cycle_traces[0].outcome, "accepted_filled");
    let cycle_trace = journal.cycle_trace_by_id("trace-aapl-1")?;
    assert!(cycle_trace.is_some());

    let reconciliations = journal.recent_reconciliations(10)?;
    assert_eq!(reconciliations.len(), 1);
    assert_eq!(reconciliations[0].source, "manual");

    let instance_reconciliations = journal.recent_reconciliations_for_bot("aapl", 10)?;
    assert_eq!(instance_reconciliations.len(), 1);
    assert!(instance_reconciliations[0].safe_to_trade);

    let latest_reconciliation = journal.latest_reconciliation_for_bot("aapl")?;
    assert!(latest_reconciliation.is_some());
    assert_eq!(latest_reconciliation.unwrap().symbol, "AAPL");
    let latest_lane_reconciliation = journal.latest_reconciliation_for_lane("aapl", "AAPL")?;
    assert!(latest_lane_reconciliation.is_some());

    let bot_events = journal.recent_bot_events(10)?;
    assert_eq!(bot_events.len(), 1);
    assert_eq!(bot_events[0].kind, "instance.started");

    let service_events = journal.recent_service_events(10)?;
    assert_eq!(service_events.len(), 1);
    assert_eq!(service_events[0].kind, "service.started");

    let snapshots = journal.load_bot_snapshots()?;
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].bot_id, "aapl");
    assert_eq!(snapshots[0].state, "paused");
    Ok(())
}

fn assert_pruned_journal_contents(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    assert!(journal.recent_events_for_entity("msft", 10)?.is_empty());
    assert_eq!(journal.recent_events_by_scope("service", 10)?.len(), 1);
    assert_eq!(journal.recent_service_events(10)?.len(), 1);

    assert_eq!(journal.recent_signals(10)?.len(), 1);
    assert_eq!(journal.recent_signals(10)?[0].bot_id, "aapl");
    assert_eq!(journal.recent_intents(10)?.len(), 1);
    assert_eq!(journal.recent_intents(10)?[0].bot_id, "aapl");
    assert_eq!(journal.recent_risk_decisions(10)?.len(), 1);
    assert_eq!(journal.recent_risk_decisions(10)?[0].bot_id, "aapl");
    assert_eq!(
        journal
            .recent_cycle_traces_for_bot("aapl", None, None, None, None, 10)?
            .len(),
        1
    );
    assert!(journal.cycle_trace_by_id("trace-msft-1")?.is_none());

    assert!(journal.recent_orders_for_bot("msft", 10)?.is_empty());
    assert!(journal.recent_fills_for_bot("msft", 10)?.is_empty());
    assert!(journal.recent_positions_for_bot("msft", 10)?.is_empty());
    assert!(journal.latest_position_for_bot("msft")?.is_none());
    assert!(
        journal
            .recent_reconciliations_for_bot("msft", 10)?
            .is_empty()
    );
    assert!(journal.latest_reconciliation_for_bot("msft")?.is_none());

    let snapshots = journal.load_bot_snapshots()?;
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].bot_id, "aapl");
    Ok(())
}

#[test]
fn sqlite_journal_enables_wal_mode() {
    let path = create_temp_db_path("wal");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();

    let connection = Connection::open(journal.path()).unwrap();
    let mode = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
        .unwrap();

    assert_eq!(mode.to_lowercase(), "wal");
}

#[test]
fn sqlite_journal_reads_do_not_block_on_write_connection_mutex() {
    let path = create_temp_db_path("read-write-mutex-separation");
    let journal = Arc::new(SqliteRuntimeJournal::open(&path, 1_000).unwrap());

    assert!(!journal.read_connections.is_empty());
    journal
        .append_event(EventWrite {
            scope: "service".to_owned(),
            entity_id: None,
            trace_id: None,
            kind: "service.started".to_owned(),
            payload: "{}".to_owned(),
        })
        .unwrap();

    let _write_guard = journal.write_connection.lock().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = {
        let journal = Arc::clone(&journal);
        thread::spawn(move || {
            let result = journal.recent_events(10).map(|events| events.len());
            tx.send(result).unwrap();
        })
    };

    let read_count = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("read query blocked behind write-connection mutex");
    assert_eq!(read_count.unwrap(), 1);
    reader.join().unwrap();
}

#[test]
fn sqlite_journal_concurrent_readers_and_writer_succeed() {
    let path = create_temp_db_path("concurrent-readers-writer");
    let journal = Arc::new(SqliteRuntimeJournal::open(&path, 1_000).unwrap());

    journal
        .append_event(EventWrite {
            scope: "service".to_owned(),
            entity_id: None,
            trace_id: None,
            kind: "service.seeded".to_owned(),
            payload: "{}".to_owned(),
        })
        .unwrap();

    let writer = {
        let journal = Arc::clone(&journal);
        thread::spawn(move || -> Result<(), String> {
            for sequence in 0..200 {
                journal
                    .append_event(EventWrite {
                        scope: "service".to_owned(),
                        entity_id: None,
                        trace_id: Some(format!("trace-{sequence}")),
                        kind: "service.tick".to_owned(),
                        payload: format!(r#"{{"sequence":{sequence}}}"#),
                    })
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        })
    };

    let readers = (0..4)
        .map(|_| {
            let journal = Arc::clone(&journal);
            thread::spawn(move || -> Result<(), String> {
                for _ in 0..200 {
                    let _ = journal
                        .recent_events(25)
                        .map_err(|error| error.to_string())?;
                    let _ = journal
                        .recent_events_by_scope("service", 25)
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            })
        })
        .collect::<Vec<_>>();

    writer.join().unwrap().unwrap();
    for reader in readers {
        reader.join().unwrap().unwrap();
    }

    let events = journal.recent_events(500).unwrap();
    assert!(events.len() >= 201);
}

#[test]
fn sqlite_journal_creates_runtime_event_lookup_indexes() {
    let path = create_temp_db_path("runtime-event-indexes");
    let _journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();

    let connection = Connection::open(&path).unwrap();
    let mut statement = connection
        .prepare("PRAGMA index_list(runtime_events)")
        .unwrap();
    let indexes = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert!(indexes.contains(&"idx_runtime_events_scope_id_desc".to_owned()));
    assert!(indexes.contains(&"idx_runtime_events_entity_id_id_desc".to_owned()));
    assert!(indexes.contains(&"idx_runtime_events_scope_entity_id_id_desc".to_owned()));
    assert!(indexes.contains(&"idx_runtime_events_trace_id_id_desc".to_owned()));
}

#[test]
fn sqlite_journal_creates_unique_fill_dedupe_index() {
    let path = create_temp_db_path("fill-indexes");
    let _journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();

    let connection = Connection::open(&path).unwrap();
    let mut statement = connection.prepare("PRAGMA index_list(fills)").unwrap();
    let indexes = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert!(
        indexes
            .iter()
            .any(|(name, unique)| { name == "idx_fills_bot_client_order_id" && *unique == 1 })
    );
}

#[test]
fn sqlite_journal_creates_bot_recent_lookup_indexes() {
    let path = create_temp_db_path("bot-recent-indexes");
    let _journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();

    let connection = Connection::open(&path).unwrap();
    assert_table_has_indexes(
        &connection,
        "orders",
        &[
            "idx_orders_bot_id_created_at_ms",
            "idx_orders_bot_id_id_desc",
        ],
    );
    assert_table_has_indexes(
        &connection,
        "fills",
        &["idx_fills_bot_id_created_at_ms", "idx_fills_bot_id_id_desc"],
    );
    assert_table_has_indexes(
        &connection,
        "positions",
        &[
            "idx_positions_bot_id_created_at_ms",
            "idx_positions_bot_id_id_desc",
            "idx_positions_bot_id_symbol_id_desc",
        ],
    );
    assert_table_has_indexes(
        &connection,
        "reconciliations",
        &[
            "idx_reconciliations_bot_id_created_at_ms",
            "idx_reconciliations_bot_id_id_desc",
            "idx_reconciliations_bot_id_symbol_id_desc",
        ],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn sqlite_journal_bot_scoped_recent_queries_preserve_ordering() {
    let path = create_temp_db_path("bot-recent-ordering");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();

    for sequence in 1..=3 {
        let trace_id = format!("trace-aapl-{sequence}");
        let bar_timestamp = format!("2026-01-01T00:0{sequence}:00Z");
        let client_order_id = format!("aapl-{sequence}-open_long");
        let outcome = if sequence == 3 {
            "blocked_risk"
        } else {
            "accepted_filled"
        };

        journal
            .append_order(OrderWrite {
                bot_id: "aapl".to_owned(),
                symbol: "AAPL".to_owned(),
                trace_id: Some(trace_id.clone()),
                bar_timestamp: Some(bar_timestamp.clone()),
                client_order_id: client_order_id.clone(),
                intent: "open_long".to_owned(),
                status: "submitted".to_owned(),
                price: 100.0 + f64::from(sequence),
                quantity: f64::from(sequence),
            })
            .unwrap();
        journal
            .append_fill(FillWrite {
                bot_id: "aapl".to_owned(),
                symbol: "AAPL".to_owned(),
                trace_id: Some(trace_id.clone()),
                bar_timestamp: Some(bar_timestamp.clone()),
                client_order_id,
                price: 100.0 + f64::from(sequence),
                quantity: f64::from(sequence),
                fee_asset: Some("USD".to_owned()),
                fee_amount: Some(0.1 * f64::from(sequence)),
                fee_normalized_usd: Some(0.1 * f64::from(sequence)),
            })
            .unwrap();
        journal
            .append_position(PositionWrite {
                bot_id: "aapl".to_owned(),
                symbol: "AAPL".to_owned(),
                trace_id: Some(trace_id.clone()),
                bar_timestamp: Some(bar_timestamp.clone()),
                has_position: sequence < 3,
                quantity: f64::from(sequence),
                entry_price: Some(100.0 + f64::from(sequence)),
                realized_pnl_usd: f64::from(sequence),
                reason: format!("position-{sequence}"),
            })
            .unwrap();
        journal
            .append_reconciliation(ReconciliationWrite {
                bot_id: "aapl".to_owned(),
                source: "manual".to_owned(),
                symbol: "AAPL".to_owned(),
                safe_to_trade: sequence != 3,
                local_open_orders: i64::from(sequence),
                connector_open_orders: i64::from(sequence),
                local_has_position: sequence < 3,
                connector_has_position: sequence < 3,
                reason: format!("recon-{sequence}"),
            })
            .unwrap();
        journal
            .append_cycle_trace(CycleTraceWrite {
                trace_id,
                bot_id: "aapl".to_owned(),
                symbol: "AAPL".to_owned(),
                bar_timestamp,
                phase: "confirmed".to_owned(),
                trigger_kind: "market_bar".to_owned(),
                signal: "buy_confirmed".to_owned(),
                intent: "open_long".to_owned(),
                risk_decision: if sequence == 3 { "blocked" } else { "allowed" }.to_owned(),
                outcome: outcome.to_owned(),
                payload_json: format!(r#"{{"summary":{{"sequence":{sequence}}}}}"#),
            })
            .unwrap();
    }

    journal
        .append_order(OrderWrite {
            bot_id: "msft".to_owned(),
            symbol: "MSFT".to_owned(),
            trace_id: Some("trace-msft-1".to_owned()),
            bar_timestamp: Some("2026-01-01T00:10:00Z".to_owned()),
            client_order_id: "msft-1-open_long".to_owned(),
            intent: "open_long".to_owned(),
            status: "submitted".to_owned(),
            price: 210.0,
            quantity: 1.0,
        })
        .unwrap();

    let orders = journal.recent_orders_for_bot("aapl", 10).unwrap();
    assert_eq!(orders.len(), 3);
    assert_eq!(
        orders
            .iter()
            .map(|record| record.client_order_id.as_str())
            .collect::<Vec<_>>(),
        vec!["aapl-1-open_long", "aapl-2-open_long", "aapl-3-open_long"]
    );

    let fills = journal.recent_fills_for_bot("aapl", 10).unwrap();
    assert_eq!(fills.len(), 3);
    assert_eq!(
        fills
            .iter()
            .map(|record| record.client_order_id.as_str())
            .collect::<Vec<_>>(),
        vec!["aapl-1-open_long", "aapl-2-open_long", "aapl-3-open_long"]
    );

    let positions = journal.recent_positions_for_bot("aapl", 10).unwrap();
    assert_eq!(positions.len(), 3);
    assert_eq!(
        positions
            .iter()
            .map(|record| record.reason.as_str())
            .collect::<Vec<_>>(),
        vec!["position-1", "position-2", "position-3"]
    );

    let reconciliations = journal.recent_reconciliations_for_bot("aapl", 10).unwrap();
    assert_eq!(reconciliations.len(), 3);
    assert_eq!(
        reconciliations
            .iter()
            .map(|record| record.reason.as_str())
            .collect::<Vec<_>>(),
        vec!["recon-1", "recon-2", "recon-3"]
    );

    let cycle_traces = journal
        .recent_cycle_traces_for_bot("aapl", None, None, None, None, 10)
        .unwrap();
    assert_eq!(cycle_traces.len(), 3);
    assert_eq!(
        cycle_traces
            .iter()
            .map(|record| record.trace_id.as_str())
            .collect::<Vec<_>>(),
        vec!["trace-aapl-1", "trace-aapl-2", "trace-aapl-3"]
    );

    let accepted_cycle_traces = journal
        .recent_cycle_traces_for_bot(
            "aapl",
            Some("AAPL"),
            Some("confirmed"),
            Some("accepted_filled"),
            None,
            10,
        )
        .unwrap();
    assert_eq!(accepted_cycle_traces.len(), 2);
    assert_eq!(
        accepted_cycle_traces
            .iter()
            .map(|record| record.trace_id.as_str())
            .collect::<Vec<_>>(),
        vec!["trace-aapl-1", "trace-aapl-2"]
    );

    let specific_cycle_trace = journal
        .recent_cycle_traces_for_bot(
            "aapl",
            Some("AAPL"),
            Some("confirmed"),
            Some("accepted_filled"),
            Some("2026-01-01T00:02:00Z"),
            10,
        )
        .unwrap();
    assert_eq!(specific_cycle_trace.len(), 1);
    assert_eq!(specific_cycle_trace[0].trace_id, "trace-aapl-2");
}

#[test]
fn sqlite_journal_query_plans_use_bot_scoped_indexes() {
    let path = create_temp_db_path("bot-query-plan");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    seed_sqlite_journal(&journal).unwrap();

    let connection = Connection::open(&path).unwrap();

    assert_plan_uses_index_and_avoids_temp_sort(
        &connection,
        "SELECT id FROM orders WHERE bot_id = 'aapl' ORDER BY id DESC LIMIT 20",
        "idx_orders_bot_id_id_desc",
    );
    assert_plan_uses_index_and_avoids_temp_sort(
        &connection,
        "SELECT id FROM fills WHERE bot_id = 'aapl' ORDER BY id DESC LIMIT 20",
        "idx_fills_bot_id_id_desc",
    );
    assert_plan_uses_index_and_avoids_temp_sort(
        &connection,
        "SELECT id FROM positions WHERE bot_id = 'aapl' ORDER BY id DESC LIMIT 20",
        "idx_positions_bot_id_id_desc",
    );
    assert_plan_uses_index_and_avoids_temp_sort(
        &connection,
        "SELECT id FROM positions WHERE bot_id = 'aapl' AND symbol = 'AAPL' ORDER BY id DESC LIMIT 1",
        "idx_positions_bot_id_symbol_id_desc",
    );
    assert_plan_uses_index_and_avoids_temp_sort(
        &connection,
        "SELECT id FROM cycle_traces WHERE bot_id = 'aapl' ORDER BY id DESC LIMIT 20",
        "idx_cycle_traces_bot_id_id_desc",
    );
    assert_plan_uses_index_and_avoids_temp_sort(
        &connection,
        "SELECT id FROM cycle_traces WHERE bot_id = 'aapl' AND symbol = 'AAPL' AND phase = 'confirmed' AND outcome = 'accepted_filled' ORDER BY id DESC LIMIT 20",
        "idx_cycle_traces_bot_id_symbol_id_desc",
    );
    assert_plan_uses_index_and_avoids_temp_sort(
        &connection,
        "SELECT id FROM reconciliations WHERE bot_id = 'aapl' ORDER BY id DESC LIMIT 20",
        "idx_reconciliations_bot_id_id_desc",
    );
    assert_plan_uses_index_and_avoids_temp_sort(
        &connection,
        "SELECT id FROM reconciliations WHERE bot_id = 'aapl' AND symbol = 'AAPL' ORDER BY id DESC LIMIT 1",
        "idx_reconciliations_bot_id_symbol_id_desc",
    );
}

fn assert_table_has_indexes(connection: &Connection, table: &str, expected: &[&str]) {
    let mut statement = connection
        .prepare(&format!("PRAGMA index_list({table})"))
        .unwrap();
    let indexes = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    for expected_index in expected {
        assert!(
            indexes
                .iter()
                .any(|index_name| index_name == expected_index),
            "missing index `{expected_index}` on `{table}`; available indexes: {indexes:?}"
        );
    }
}

fn assert_plan_uses_index_and_avoids_temp_sort(
    connection: &Connection,
    sql: &str,
    index_name: &str,
) {
    let details = explain_query_plan_details(connection, sql);
    assert!(
        details.iter().any(|detail| detail.contains(index_name)),
        "query plan did not use expected index `{index_name}` for `{sql}`; plan details: {details:?}"
    );
    assert!(
        details
            .iter()
            .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")),
        "query plan used temp sort for `{sql}`; plan details: {details:?}"
    );
}

fn explain_query_plan_details(connection: &Connection, sql: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap();
    statement
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn create_temp_db_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-storage-{prefix}-{nanos}.db"))
}
