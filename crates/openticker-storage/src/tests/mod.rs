mod in_memory;
mod sqlite;

use crate::support::{monotonic_timestamp_ms, now_timestamp_ms};
use crate::{
    BotEventWrite, BotSnapshotWrite, CycleTraceWrite, EventWrite, FillWrite,
    InMemoryRuntimeJournal, IntentWrite, OrderWrite, PositionWrite, ReconciliationWrite,
    RiskDecisionWrite, RuntimeJournal, ServiceEventWrite, SignalWrite, SqliteRuntimeJournal,
    StorageError,
};
use std::path::PathBuf;
use std::sync::atomic::AtomicI64;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn monotonic_timestamp_floor_handles_clock_regression() {
    let floor = AtomicI64::new(0);

    assert_eq!(monotonic_timestamp_ms(&floor, Some(1_000)), 1_000);
    // An advancing clock moves the floor forward.
    assert_eq!(monotonic_timestamp_ms(&floor, Some(2_000)), 2_000);
    // A clock regression returns the last known good timestamp.
    assert_eq!(monotonic_timestamp_ms(&floor, Some(1_500)), 2_000);
    // An unavailable clock (e.g. before the UNIX epoch) returns the floor.
    assert_eq!(monotonic_timestamp_ms(&floor, None), 2_000);
    // Regressed readings did not lower the floor.
    assert_eq!(monotonic_timestamp_ms(&floor, Some(2_500)), 2_500);
}

#[test]
fn now_timestamp_ms_is_positive_and_non_decreasing() {
    let first = now_timestamp_ms();
    let second = now_timestamp_ms();
    assert!(first > 0);
    assert!(second >= first);
}

#[test]
fn cycle_trace_filter_combinations_match_across_backends() {
    assert_cycle_trace_filter_combinations(&InMemoryRuntimeJournal::default()).unwrap();

    let path = create_temp_db_path("cycle-trace-filters");
    let journal = SqliteRuntimeJournal::open(&path, 1_000).unwrap();
    assert_cycle_trace_filter_combinations(&journal).unwrap();
}

struct CycleTraceFilterCase {
    symbol: Option<&'static str>,
    phase: Option<&'static str>,
    outcome: Option<&'static str>,
    bar_timestamp: Option<&'static str>,
    expected_trace_ids: &'static [&'static str],
}

#[allow(clippy::too_many_lines)]
fn assert_cycle_trace_filter_combinations(
    journal: &dyn RuntimeJournal,
) -> Result<(), StorageError> {
    journal.append_cycle_trace(filter_test_cycle_trace_write(
        "trace-1",
        "aapl",
        "AAPL",
        "2026-01-01T00:01:00Z",
        "confirmed",
        "accepted_filled",
    ))?;
    journal.append_cycle_trace(filter_test_cycle_trace_write(
        "trace-2",
        "aapl",
        "MSFT",
        "2026-01-01T00:02:00Z",
        "confirmed",
        "blocked_risk",
    ))?;
    journal.append_cycle_trace(filter_test_cycle_trace_write(
        "trace-3",
        "aapl",
        "AAPL",
        "2026-01-01T00:02:00Z",
        "preview",
        "accepted_filled",
    ))?;
    journal.append_cycle_trace(filter_test_cycle_trace_write(
        "trace-other-bot",
        "msft",
        "MSFT",
        "2026-01-01T00:01:00Z",
        "confirmed",
        "accepted_filled",
    ))?;

    let cases = [
        CycleTraceFilterCase {
            symbol: None,
            phase: None,
            outcome: None,
            bar_timestamp: None,
            expected_trace_ids: &["trace-1", "trace-2", "trace-3"],
        },
        CycleTraceFilterCase {
            symbol: Some("AAPL"),
            phase: None,
            outcome: None,
            bar_timestamp: None,
            expected_trace_ids: &["trace-1", "trace-3"],
        },
        CycleTraceFilterCase {
            symbol: None,
            phase: Some("confirmed"),
            outcome: None,
            bar_timestamp: None,
            expected_trace_ids: &["trace-1", "trace-2"],
        },
        CycleTraceFilterCase {
            symbol: None,
            phase: None,
            outcome: Some("accepted_filled"),
            bar_timestamp: None,
            expected_trace_ids: &["trace-1", "trace-3"],
        },
        CycleTraceFilterCase {
            symbol: None,
            phase: None,
            outcome: None,
            bar_timestamp: Some("2026-01-01T00:02:00Z"),
            expected_trace_ids: &["trace-2", "trace-3"],
        },
        CycleTraceFilterCase {
            symbol: Some("AAPL"),
            phase: Some("preview"),
            outcome: None,
            bar_timestamp: None,
            expected_trace_ids: &["trace-3"],
        },
        CycleTraceFilterCase {
            symbol: Some("AAPL"),
            phase: Some("confirmed"),
            outcome: Some("accepted_filled"),
            bar_timestamp: Some("2026-01-01T00:01:00Z"),
            expected_trace_ids: &["trace-1"],
        },
        CycleTraceFilterCase {
            symbol: Some("TSLA"),
            phase: None,
            outcome: None,
            bar_timestamp: None,
            expected_trace_ids: &[],
        },
    ];

    for case in cases {
        let records = journal.recent_cycle_traces_for_bot(
            "aapl",
            case.symbol,
            case.phase,
            case.outcome,
            case.bar_timestamp,
            10,
        )?;
        let trace_ids = records
            .iter()
            .map(|record| record.trace_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            trace_ids, case.expected_trace_ids,
            "unexpected traces for symbol={:?} phase={:?} outcome={:?} bar_timestamp={:?}",
            case.symbol, case.phase, case.outcome, case.bar_timestamp
        );
    }

    // The limit keeps the most recent matches.
    let limited = journal.recent_cycle_traces_for_bot("aapl", None, None, None, None, 2)?;
    assert_eq!(
        limited
            .iter()
            .map(|record| record.trace_id.as_str())
            .collect::<Vec<_>>(),
        vec!["trace-2", "trace-3"]
    );

    Ok(())
}

fn filter_test_cycle_trace_write(
    trace_id: &str,
    bot_id: &str,
    symbol: &str,
    bar_timestamp: &str,
    phase: &str,
    outcome: &str,
) -> CycleTraceWrite {
    CycleTraceWrite {
        trace_id: trace_id.to_owned(),
        bot_id: bot_id.to_owned(),
        symbol: symbol.to_owned(),
        bar_timestamp: bar_timestamp.to_owned(),
        phase: phase.to_owned(),
        trigger_kind: "market_bar".to_owned(),
        signal: "buy_confirmed".to_owned(),
        intent: "open_long".to_owned(),
        risk_decision: "allowed".to_owned(),
        outcome: outcome.to_owned(),
        payload_json: "{}".to_owned(),
    }
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

fn create_temp_db_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-storage-{prefix}-{nanos}.db"))
}
