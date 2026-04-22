use openticker_storage::{
    PositionWrite, ReconciliationWrite, RuntimeJournal, SqliteRuntimeJournal,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn latest_position_returns_newest_row_after_reopen() {
    let path = create_temp_db_path("latest-position");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    journal
        .append_position(PositionWrite {
            bot_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: Some("2026-01-01T00:00:00Z".to_owned()),
            has_position: true,
            quantity: 1.0,
            entry_price: Some(123.45),
            realized_pnl_usd: 0.0,
            reason: "initial_fill".to_owned(),
        })
        .unwrap();
    journal
        .append_position(PositionWrite {
            bot_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: Some("2026-01-01T00:01:00Z".to_owned()),
            has_position: false,
            quantity: 0.0,
            entry_price: None,
            realized_pnl_usd: 12.34,
            reason: "close_fill".to_owned(),
        })
        .unwrap();
    drop(journal);

    let reopened = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    let latest = reopened.latest_position_for_bot("aapl").unwrap().unwrap();
    assert!(!latest.has_position);
    assert!(latest.quantity.abs() < f64::EPSILON);
    assert_eq!(latest.entry_price, None);
    assert!((latest.realized_pnl_usd - 12.34).abs() < f64::EPSILON);
    assert_eq!(latest.reason, "close_fill");

    let latest_lane = reopened.latest_position_for_lane("aapl", "AAPL").unwrap();
    assert!(latest_lane.is_some());
}

#[test]
fn latest_reconciliation_returns_newest_row_after_reopen() {
    let path = create_temp_db_path("latest-reconciliation");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
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
            reason: "position_mismatch".to_owned(),
        })
        .unwrap();
    journal
        .append_reconciliation(ReconciliationWrite {
            bot_id: "aapl".to_owned(),
            source: "manual".to_owned(),
            symbol: "AAPL".to_owned(),
            safe_to_trade: true,
            local_open_orders: 0,
            connector_open_orders: 0,
            local_has_position: false,
            connector_has_position: false,
            reason: "state_aligned".to_owned(),
        })
        .unwrap();
    drop(journal);

    let reopened = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    let latest = reopened
        .latest_reconciliation_for_bot("aapl")
        .unwrap()
        .unwrap();
    assert_eq!(latest.source, "manual");
    assert!(latest.safe_to_trade);
    assert_eq!(latest.reason, "state_aligned");

    let latest_lane = reopened
        .latest_reconciliation_for_lane("aapl", "AAPL")
        .unwrap();
    assert!(latest_lane.is_some());
}

fn create_temp_db_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-storage-{prefix}-{nanos}.db"))
}
