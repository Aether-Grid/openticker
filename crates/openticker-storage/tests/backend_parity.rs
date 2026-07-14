use openticker_storage::{
    BotSnapshotWrite, CycleTraceWrite, EventWrite, FillWrite, InMemoryRuntimeJournal, OrderWrite,
    RuntimeJournal, SqliteRuntimeJournal, StorageError,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn orders_are_idempotent_across_backends() {
    assert_order_idempotency(&InMemoryRuntimeJournal::default()).unwrap();

    let path = create_temp_db_path("orders-idempotent");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    assert_order_idempotency(&journal).unwrap();
}

#[test]
fn conflicting_duplicate_orders_keep_first_payload_across_backends() {
    assert_conflicting_duplicate_orders_keep_first_payload(&InMemoryRuntimeJournal::default())
        .unwrap();

    let path = create_temp_db_path("orders-conflict");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    assert_conflicting_duplicate_orders_keep_first_payload(&journal).unwrap();
}

#[test]
fn fills_are_idempotent_across_backends() {
    assert_fill_idempotency(&InMemoryRuntimeJournal::default()).unwrap();

    let path = create_temp_db_path("fills-idempotent");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    assert_fill_idempotency(&journal).unwrap();
}

#[test]
fn conflicting_duplicate_fills_keep_first_payload_across_backends() {
    assert_conflicting_duplicate_fills_keep_first_payload(&InMemoryRuntimeJournal::default())
        .unwrap();

    let path = create_temp_db_path("fills-conflict");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    assert_conflicting_duplicate_fills_keep_first_payload(&journal).unwrap();
}

#[test]
fn prune_bots_except_with_empty_set_clears_all_bot_data_across_backends() {
    assert_empty_prune_clears_all_bot_data(&InMemoryRuntimeJournal::default()).unwrap();

    let path = create_temp_db_path("prune-empty-set");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    assert_empty_prune_clears_all_bot_data(&journal).unwrap();
}

/// Pins the documented semantics of `RuntimeJournal::prune_bots_except`: an
/// empty active set prunes ALL bot-scoped data on both backends, while
/// runtime events without an entity id (global/service events) survive.
fn assert_empty_prune_clears_all_bot_data(
    journal: &dyn RuntimeJournal,
) -> Result<(), StorageError> {
    journal.append_order(order_write(123.45, "submitted"))?;
    journal.append_fill(fill_write(123.45, 1.0))?;
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
        payload_json: "{}".to_owned(),
    })?;
    journal.upsert_bot_snapshot(BotSnapshotWrite {
        bot_id: "aapl".to_owned(),
        state: "running".to_owned(),
        execution_mode: "paper".to_owned(),
        enabled: true,
    })?;
    journal.append_event(EventWrite {
        scope: "instance".to_owned(),
        entity_id: Some("aapl".to_owned()),
        trace_id: None,
        kind: "instance.started".to_owned(),
        payload: "{}".to_owned(),
    })?;
    journal.append_event(EventWrite {
        scope: "service".to_owned(),
        entity_id: None,
        trace_id: None,
        kind: "service.started".to_owned(),
        payload: "{}".to_owned(),
    })?;

    journal.prune_bots_except(&HashSet::new())?;

    assert!(journal.recent_orders(10)?.is_empty());
    assert!(journal.recent_fills(10)?.is_empty());
    assert!(
        journal
            .recent_cycle_traces_for_bot("aapl", None, None, None, None, 10)?
            .is_empty()
    );
    assert!(journal.load_bot_snapshots()?.is_empty());

    let events = journal.recent_events(10)?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "service.started");
    assert!(events[0].entity_id.is_none());
    Ok(())
}

fn assert_order_idempotency(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    let order = order_write(123.45, "submitted");
    journal.append_order(order.clone())?;
    journal.append_order(order)?;

    let orders = journal.recent_orders(10)?;
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].client_order_id, "aapl-1-open_long");
    assert_eq!(orders[0].status, "submitted");
    assert!((orders[0].price - 123.45).abs() < f64::EPSILON);
    Ok(())
}

fn assert_conflicting_duplicate_orders_keep_first_payload(
    journal: &dyn RuntimeJournal,
) -> Result<(), StorageError> {
    journal.append_order(order_write(123.45, "submitted"))?;
    journal.append_order(order_write(999.0, "rejected"))?;

    let orders = journal.recent_orders(10)?;
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].status, "submitted");
    assert!((orders[0].price - 123.45).abs() < f64::EPSILON);
    Ok(())
}

fn assert_fill_idempotency(journal: &dyn RuntimeJournal) -> Result<(), StorageError> {
    let fill = fill_write(123.45, 1.0);
    journal.append_fill(fill.clone())?;
    journal.append_fill(fill)?;

    let fills = journal.recent_fills(10)?;
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].client_order_id, "aapl-1-open_long");
    assert!((fills[0].price - 123.45).abs() < f64::EPSILON);
    assert!((fills[0].quantity - 1.0).abs() < f64::EPSILON);
    Ok(())
}

fn assert_conflicting_duplicate_fills_keep_first_payload(
    journal: &dyn RuntimeJournal,
) -> Result<(), StorageError> {
    journal.append_fill(fill_write(123.45, 1.0))?;
    journal.append_fill(fill_write(999.0, 2.0))?;

    let fills = journal.recent_fills(10)?;
    assert_eq!(fills.len(), 1);
    assert!((fills[0].price - 123.45).abs() < f64::EPSILON);
    assert!((fills[0].quantity - 1.0).abs() < f64::EPSILON);
    Ok(())
}

fn order_write(price: f64, status: &str) -> OrderWrite {
    OrderWrite {
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        trace_id: Some("trace-aapl-1".to_owned()),
        bar_timestamp: Some("2026-01-01T00:00:00Z".to_owned()),
        client_order_id: "aapl-1-open_long".to_owned(),
        intent: "open_long".to_owned(),
        status: status.to_owned(),
        price,
        quantity: 1.0,
    }
}

fn fill_write(price: f64, quantity: f64) -> FillWrite {
    FillWrite {
        bot_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        trace_id: Some("trace-aapl-1".to_owned()),
        bar_timestamp: Some("2026-01-01T00:00:00Z".to_owned()),
        client_order_id: "aapl-1-open_long".to_owned(),
        price,
        quantity,
        fee_asset: Some("USD".to_owned()),
        fee_amount: Some(0.5),
        fee_normalized_usd: Some(0.5),
    }
}

fn create_temp_db_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-storage-{prefix}-{nanos}.db"))
}
