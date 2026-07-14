use super::{
    append_cycle_trace_records, append_order_flow_records, append_reconciliation_records,
    append_removed_instance_records, append_signal_flow_records, assert_pruned_journal_contents,
};
use crate::{
    EventWrite, FillWrite, InMemoryRuntimeJournal, OrderWrite, ReconciliationWrite, RuntimeJournal,
};
use std::collections::HashSet;

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
