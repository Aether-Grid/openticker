use openticker_storage::{FillWrite, OrderWrite, RuntimeJournal, SqliteRuntimeJournal};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn orders_are_idempotent_across_reopen() {
    let path = create_temp_db_path("order-reopen");
    let first = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    first
        .append_order(order_write(123.45, "submitted"))
        .unwrap();
    drop(first);

    let reopened = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    reopened
        .append_order(order_write(999.0, "rejected"))
        .unwrap();

    let orders = reopened.recent_orders(10).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].status, "submitted");
    assert!((orders[0].price - 123.45).abs() < f64::EPSILON);
}

#[test]
fn fills_are_idempotent_across_reopen() {
    let path = create_temp_db_path("fill-reopen");
    let first = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    first.append_fill(fill_write(123.45, 1.0)).unwrap();
    drop(first);

    let reopened = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    reopened.append_fill(fill_write(999.0, 2.0)).unwrap();

    let fills = reopened.recent_fills(10).unwrap();
    assert_eq!(fills.len(), 1);
    assert!((fills[0].price - 123.45).abs() < f64::EPSILON);
    assert!((fills[0].quantity - 1.0).abs() < f64::EPSILON);
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
