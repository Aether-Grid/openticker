use super::*;
use openticker_connectors::{
    ConnectorAccountSnapshot, ConnectorOpenOrder, ConnectorPosition, ConnectorPrivateBalance,
};
use openticker_ledger::{AccountLedger, LedgerException, LedgerExceptionKind, LedgerOwnerPath};
use openticker_storage::{OrderRecord, PositionRecord};
use std::collections::{HashMap, HashSet};

use crate::symbols::symbol_base_asset;

fn lane(id: &str, bot: &str, account: &str, symbol: &str) -> PortfolioLaneView {
    PortfolioLaneView {
        lane_id: id.to_owned(),
        bot_id: bot.to_owned(),
        account_id: account.to_owned(),
        symbol: symbol.to_owned(),
        budget_pct: 25.0,
        effective_position_quantity: 1.0,
        position_notional_usd: 100.0,
        daily_loss_pct_accumulated: 1.5,
    }
}

fn open_position(bot: &str, symbol: &str, reason: &str) -> PositionRecord {
    PositionRecord {
        id: 1,
        bot_id: bot.to_owned(),
        symbol: Some(symbol.to_owned()),
        trace_id: None,
        bar_timestamp: None,
        has_position: true,
        quantity: 1.0,
        entry_price: Some(100.0),
        realized_pnl_usd: 0.0,
        reason: reason.to_owned(),
        created_at_ms: 1,
    }
}

fn position_with_state(
    lane_id: i64,
    bot: &str,
    symbol: &str,
    has_position: bool,
    quantity: f64,
    reason: &str,
) -> PositionRecord {
    PositionRecord {
        id: lane_id,
        bot_id: bot.to_owned(),
        symbol: Some(symbol.to_owned()),
        trace_id: None,
        bar_timestamp: None,
        has_position,
        quantity,
        entry_price: Some(100.0),
        realized_pnl_usd: 0.0,
        reason: reason.to_owned(),
        created_at_ms: lane_id,
    }
}

fn connector_open_order(client_order_id: &str, symbol: &str) -> ConnectorOpenOrder {
    ConnectorOpenOrder {
        client_order_id: client_order_id.to_owned(),
        symbol: symbol.to_owned(),
        status: "open".to_owned(),
        quantity: 1.0,
    }
}

fn order_record(client_order_id: &str, status: &str, symbol: &str) -> OrderRecord {
    OrderRecord {
        id: 1,
        bot_id: "bot-a".to_owned(),
        symbol: Some(symbol.to_owned()),
        trace_id: None,
        bar_timestamp: None,
        client_order_id: client_order_id.to_owned(),
        intent: "buy".to_owned(),
        status: status.to_owned(),
        price: 100.0,
        quantity: 1.0,
        created_at_ms: 1,
    }
}

fn latest_position(
    bot: &str,
    symbol: &str,
    quantity: f64,
    entry_price: Option<f64>,
) -> PositionRecord {
    PositionRecord {
        id: 1,
        bot_id: bot.to_owned(),
        symbol: Some(symbol.to_owned()),
        trace_id: None,
        bar_timestamp: None,
        has_position: quantity > 0.0,
        quantity,
        entry_price,
        realized_pnl_usd: 0.0,
        reason: "entry_fill".to_owned(),
        created_at_ms: 1,
    }
}

const FLOAT_ASSERT_EPSILON: f64 = 1e-9;

fn assert_f64_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < FLOAT_ASSERT_EPSILON,
        "expected {actual} to be within {FLOAT_ASSERT_EPSILON} of {expected}"
    );
}

fn assert_opt_f64_close(actual: Option<f64>, expected: f64) {
    assert!(
        actual.is_some_and(|value| (value - expected).abs() < FLOAT_ASSERT_EPSILON),
        "expected {actual:?} to contain a value within {FLOAT_ASSERT_EPSILON} of {expected}"
    );
}

#[test]
fn classify_remote_open_orders_returns_external_orders_without_local_match() {
    let order = connector_open_order("external-1", "BTCUSDT");
    let classified = classify_remote_open_orders(
        "acct",
        "BTCUSDT",
        std::slice::from_ref(&order),
        &HashMap::new(),
        &HashSet::new(),
    );

    assert_eq!(classified.external_orders, vec![order]);
    assert!(classified.managed_orders.is_empty());
    assert!(classified.unsafe_reasons.is_empty());
}

#[test]
fn open_orders_for_symbol_excludes_terminal_orders() {
    let snapshot = ConnectorAccountSnapshot {
        open_orders: vec![
            connector_open_order("open-1", "BTCUSDT"),
            ConnectorOpenOrder {
                client_order_id: "filled-1".to_owned(),
                symbol: "BTCUSDT".to_owned(),
                status: "filled".to_owned(),
                quantity: 1.0,
            },
            connector_open_order("other-1", "ETHUSDT"),
        ],
        positions: Vec::new(),
        balances: Vec::new(),
    };

    assert_eq!(
        open_orders_for_symbol(&snapshot, "BTCUSDT"),
        vec![connector_open_order("open-1", "BTCUSDT")]
    );
}

#[test]
fn local_open_order_ids_ignore_terminal_and_filled_orders() {
    let orders = vec![
        order_record("open-1", "open", "BTCUSDT"),
        order_record("filled-1", "filled", "BTCUSDT"),
        order_record("filled-by-fill-1", "open", "BTCUSDT"),
    ];
    let filled_client_order_ids = HashSet::from(["filled-by-fill-1".to_owned()]);

    assert_eq!(
        local_open_order_ids(&orders, &filled_client_order_ids),
        vec!["open-1".to_owned()]
    );
}

#[test]
fn classify_remote_open_orders_accepts_exact_managed_match() {
    let order = connector_open_order("managed-1", "BTCUSDT");
    let matches = HashMap::from([(
        "managed-1".to_owned(),
        vec![LocalOpenOrderIdentity {
            bot_id: "bot-a".to_owned(),
            symbol: Some("BTCUSDT".to_owned()),
        }],
    )]);
    let eligible_bot_ids = HashSet::from(["bot-a".to_owned()]);

    let classified = classify_remote_open_orders(
        "acct",
        "BTCUSDT",
        std::slice::from_ref(&order),
        &matches,
        &eligible_bot_ids,
    );

    assert_eq!(
        classified.managed_orders,
        vec![ManagedRemoteOpenOrder {
            bot_id: "bot-a".to_owned(),
            order,
        }]
    );
    assert!(classified.external_orders.is_empty());
    assert!(classified.unsafe_reasons.is_empty());
}

#[test]
fn classify_remote_open_orders_flags_ambiguous_local_matches() {
    let order = connector_open_order("ambiguous-1", "BTCUSDT");
    let matches = HashMap::from([(
        "ambiguous-1".to_owned(),
        vec![
            LocalOpenOrderIdentity {
                bot_id: "bot-a".to_owned(),
                symbol: Some("BTCUSDT".to_owned()),
            },
            LocalOpenOrderIdentity {
                bot_id: "bot-b".to_owned(),
                symbol: Some("BTCUSDT".to_owned()),
            },
        ],
    )]);
    let eligible_bot_ids = HashSet::from(["bot-a".to_owned(), "bot-b".to_owned()]);

    let classified = classify_remote_open_orders(
        "acct",
        "BTCUSDT",
        std::slice::from_ref(&order),
        &matches,
        &eligible_bot_ids,
    );

    assert!(classified.managed_orders.is_empty());
    assert!(classified.external_orders.is_empty());
    assert_eq!(
        classified.unsafe_reasons,
        vec![
            "client_order_id=ambiguous-1 matched multiple local orders (bot-a:BTCUSDT,bot-b:BTCUSDT)"
                .to_owned(),
        ]
    );
}

#[test]
fn classify_remote_open_orders_flags_ineligible_bot_matches() {
    let order = connector_open_order("managed-2", "BTCUSDT");
    let matches = HashMap::from([(
        "managed-2".to_owned(),
        vec![LocalOpenOrderIdentity {
            bot_id: "bot-a".to_owned(),
            symbol: Some("BTCUSDT".to_owned()),
        }],
    )]);
    let eligible_bot_ids = HashSet::from(["bot-b".to_owned()]);

    let classified = classify_remote_open_orders(
        "acct",
        "BTCUSDT",
        std::slice::from_ref(&order),
        &matches,
        &eligible_bot_ids,
    );

    assert!(classified.managed_orders.is_empty());
    assert!(classified.external_orders.is_empty());
    assert_eq!(
        classified.unsafe_reasons,
        vec![
            "client_order_id=managed-2 matched bot bot-a but not account acct symbol BTCUSDT"
                .to_owned(),
        ]
    );
}

#[test]
fn reconciliation_assessment_summary_prefers_runtime_quantity_and_scopes_bot_orders() {
    let classified_orders = ClassifiedRemoteOpenOrders {
        managed_orders: vec![
            ManagedRemoteOpenOrder {
                bot_id: "bot-a".to_owned(),
                order: connector_open_order("managed-a", "BTCUSDT"),
            },
            ManagedRemoteOpenOrder {
                bot_id: "bot-b".to_owned(),
                order: connector_open_order("managed-b", "BTCUSDT"),
            },
        ],
        external_orders: vec![connector_open_order("external-1", "BTCUSDT")],
        unsafe_reasons: Vec::new(),
    };

    let summary = reconciliation_assessment_summary(
        "bot-a",
        2.0,
        Some(&latest_position("bot-a", "BTCUSDT", 1.0, Some(100.0))),
        vec!["local-1".to_owned(), "local-2".to_owned()],
        &classified_orders,
        None,
        None,
        3.0,
    );

    assert_eq!(summary.local_open_orders, 2);
    assert_eq!(
        summary.local_open_order_ids,
        vec!["local-1".to_owned(), "local-2".to_owned()]
    );
    assert_eq!(summary.connector_open_orders, 1);
    assert_eq!(
        summary.connector_open_orders_detail,
        vec![connector_open_order("managed-a", "BTCUSDT")]
    );
    assert_eq!(summary.managed_remote_open_orders, 2);
    assert_eq!(summary.external_remote_open_orders, 1);
    assert!(summary.positions.local_has_position);
    assert!(summary.positions.connector_has_position);
    assert!(summary.positions.resolved_has_position);
    assert!((summary.positions.resolved_position_quantity - 2.0).abs() < 1e-6);
    assert_opt_f64_close(summary.positions.resolved_entry_price, 100.0);
    assert_f64_close(summary.aggregate_managed_qty, 3.0);
    assert_eq!(summary.remote_net_qty, None);
    assert_eq!(summary.external_delta_qty, None);
    assert!(summary.safe_to_trade);
    assert_eq!(summary.reason, "state_aligned");
}

#[test]
fn reconciliation_assessment_summary_keeps_blockers_and_warnings() {
    let classified_orders = ClassifiedRemoteOpenOrders {
        managed_orders: vec![ManagedRemoteOpenOrder {
            bot_id: "bot-a".to_owned(),
            order: connector_open_order("managed-a", "BTCUSDT"),
        }],
        external_orders: Vec::new(),
        unsafe_reasons: vec!["ambiguous_remote_order".to_owned()],
    };
    let exposure = AccountSymbolExposure {
        symbol: "BTCUSDT".to_owned(),
        remote_net_qty: 0.5,
        aggregate_managed_qty: 1.0,
        external_delta_qty: 0.5,
    };

    let summary = reconciliation_assessment_summary(
        "bot-a",
        0.0,
        None,
        Vec::new(),
        &classified_orders,
        Some("snapshot_unavailable"),
        Some(&exposure),
        1.0,
    );

    assert_eq!(summary.connector_open_orders, 1);
    assert!(summary.positions.connector_has_position);
    assert!(!summary.positions.local_has_position);
    assert!(!summary.positions.resolved_has_position);
    assert!(!summary.safe_to_trade);
    assert_eq!(
        summary.reason,
        "snapshot_unavailable;ambiguous_remote_order;managed_position_deficit(remote_net_qty=0.5,aggregate_managed_qty=1,deficit_qty=0.5)"
    );
    assert_opt_f64_close(summary.remote_net_qty, 0.5);
    assert_f64_close(summary.aggregate_managed_qty, 1.0);
    assert_opt_f64_close(summary.external_delta_qty, 0.5);
}

#[test]
fn build_reconciliation_assessment_wraps_summary_with_snapshot_metadata() {
    let summary = ReconciliationAssessmentSummary {
        local_open_orders: 2,
        local_open_order_ids: vec!["local-1".to_owned(), "local-2".to_owned()],
        connector_open_orders: 1,
        connector_open_orders_detail: vec![connector_open_order("managed-a", "BTCUSDT")],
        managed_remote_open_orders: 3,
        external_remote_open_orders: 1,
        positions: ReconciliationPositions {
            local_has_position: true,
            connector_has_position: true,
            resolved_has_position: true,
            resolved_position_quantity: 2.0,
            resolved_entry_price: Some(100.0),
        },
        remote_net_qty: Some(1.5),
        aggregate_managed_qty: 2.0,
        external_delta_qty: Some(0.5),
        safe_to_trade: false,
        reason: "snapshot_unavailable".to_owned(),
    };
    let snapshot = ConnectorAccountSnapshot {
        open_orders: vec![connector_open_order("managed-a", "BTCUSDT")],
        positions: vec![ConnectorPosition {
            symbol: "BTC".to_owned(),
            quantity: 1.5,
        }],
        balances: Vec::new(),
    };

    let assessment = build_reconciliation_assessment(
        "BTCUSDT".to_owned(),
        true,
        Some(snapshot.clone()),
        summary,
    );

    assert_eq!(assessment.symbol, "BTCUSDT");
    assert_eq!(assessment.local_open_orders, 2);
    assert_eq!(assessment.connector_open_orders, 1);
    assert_eq!(assessment.managed_remote_open_orders, 3);
    assert_eq!(assessment.external_remote_open_orders, 1);
    assert!(assessment.connector_snapshot_available);
    assert_eq!(assessment.connector_snapshot, Some(snapshot));
    assert!(assessment.positions.local_has_position);
    assert_opt_f64_close(assessment.remote_net_qty, 1.5);
    assert_f64_close(assessment.aggregate_managed_qty, 2.0);
    assert_opt_f64_close(assessment.external_delta_qty, 0.5);
    assert!(!assessment.safe_to_trade);
    assert_eq!(assessment.reason, "snapshot_unavailable");
}

#[test]
fn reconciliation_differences_splits_reason_payload() {
    assert_eq!(
        reconciliation_differences("blocked_a;warning_b"),
        vec!["blocked_a".to_owned(), "warning_b".to_owned()]
    );
    assert!(reconciliation_differences("state_aligned").is_empty());
}

#[test]
fn unmapped_managed_open_order_exceptions_wrap_unsafe_reasons() {
    let classified_orders = ClassifiedRemoteOpenOrders {
        managed_orders: Vec::new(),
        external_orders: Vec::new(),
        unsafe_reasons: vec![
            "client_order_id=order-1 matched multiple local orders".to_owned(),
            "client_order_id=order-2 matched wrong symbol".to_owned(),
        ],
    };

    let exceptions = unmapped_managed_open_order_exceptions("BTCUSDT", &classified_orders);

    assert_eq!(exceptions.len(), 2);
    assert_eq!(
        exceptions[0].kind,
        LedgerExceptionKind::UnmappedManagedOpenOrder
    );
    assert_eq!(exceptions[0].symbol.as_deref(), Some("BTCUSDT"));
    assert!(exceptions[0].blocks_new_opens);
    assert_eq!(
        exceptions[0].detail,
        "client_order_id=order-1 matched multiple local orders"
    );
    assert_eq!(
        exceptions[1].kind,
        LedgerExceptionKind::UnmappedManagedOpenOrder
    );
    assert_eq!(
        exceptions[1].detail,
        "client_order_id=order-2 matched wrong symbol"
    );
}

#[test]
fn apply_account_ledger_refresh_state_updates_ledger_snapshot_fields() {
    let mut ledger = AccountLedger::new(1_000.0);
    ledger.set_unattributed_open_notional_usd(125.0);

    let owner = LedgerOwnerPath::new("acct", "bot-a", "BTCUSDT");
    let refresh_state = AccountLedgerRefreshState {
        lane_open_notionals: vec![(owner.clone(), 250.0)],
        live_balance_usd: Some(750.0),
        exceptions: vec![LedgerException {
            kind: LedgerExceptionKind::ManagedPositionDeficit,
            owner: None,
            symbol: Some("BTCUSDT".to_owned()),
            detail: "deficit_qty=0.5".to_owned(),
            blocks_new_opens: true,
        }],
    };
    let exceptions = vec![LedgerException {
        kind: LedgerExceptionKind::UnmappedManagedOpenOrder,
        owner: None,
        symbol: Some("BTCUSDT".to_owned()),
        detail: "client_order_id=order-1 matched multiple local orders".to_owned(),
        blocks_new_opens: true,
    }];

    apply_account_ledger_refresh_state(&mut ledger, refresh_state, exceptions);

    let snapshot = ledger.account_snapshot("acct");
    assert_opt_f64_close(snapshot.live_balance_usd, 750.0);
    assert!(snapshot.unattributed_open_notional_usd.abs() < 1e-6);
    assert_eq!(snapshot.exceptions.len(), 1);
    assert_eq!(
        snapshot.exceptions[0].kind,
        LedgerExceptionKind::UnmappedManagedOpenOrder
    );
    let lane_snapshot = ledger.lane_snapshots();
    assert_eq!(lane_snapshot.len(), 1);
    assert_eq!(lane_snapshot[0].owner, owner);
    assert!((lane_snapshot[0].attributed_open_notional_usd - 250.0).abs() < 1e-6);
}

#[test]
fn sync_account_ledger_from_lanes_replaces_lane_open_notionals() {
    let mut ledger = AccountLedger::new(1_000.0);
    let existing_owner = LedgerOwnerPath::new("acct", "bot-a", "BTCUSDT");
    ledger.replace_lane_open_notional(vec![(existing_owner, 50.0)]);

    let lanes = vec![PortfolioLaneView {
        lane_id: "lane-eth".to_owned(),
        bot_id: "bot-b".to_owned(),
        account_id: "acct".to_owned(),
        symbol: "ETHUSDT".to_owned(),
        budget_pct: 25.0,
        effective_position_quantity: 1.0,
        position_notional_usd: 300.0,
        daily_loss_pct_accumulated: 0.0,
    }];

    sync_account_ledger_from_lanes(&mut ledger, &lanes);

    let lane_snapshots = ledger.lane_snapshots();
    assert_eq!(lane_snapshots.len(), 1);
    assert_eq!(
        lane_snapshots[0].owner,
        LedgerOwnerPath::new("acct", "bot-b", "ETHUSDT")
    );
    assert!((lane_snapshots[0].attributed_open_notional_usd - 300.0).abs() < 1e-6);
}

#[test]
fn connector_position_owner_prefers_strong_local_holder() {
    let lanes = vec![
        lane("lane-a", "bot-a", "acct", "BTCUSDT"),
        lane("lane-b", "bot-b", "acct", "BTCUSDT"),
    ];
    let latest_positions = vec![
        LatestLanePosition {
            lane_id: "lane-a".to_owned(),
            position: Some(open_position("bot-a", "BTCUSDT", "entry_fill")),
        },
        LatestLanePosition {
            lane_id: "lane-b".to_owned(),
            position: Some(open_position(
                "bot-b",
                "BTCUSDT",
                "position_reconciliation_sync",
            )),
        },
    ];

    match connector_position_owner("acct", "BTC", &lanes, &latest_positions) {
        ConnectorPositionOwner::Unique(owner) => assert_eq!(owner, "lane-a"),
        other => panic!("unexpected owner resolution: {other:?}"),
    }
}

#[test]
fn latest_authoritative_position_skips_close_requested_rows() {
    let positions = vec![
        PositionRecord {
            id: 1,
            bot_id: "bot".to_owned(),
            symbol: Some("BTCUSDT".to_owned()),
            trace_id: None,
            bar_timestamp: None,
            has_position: false,
            quantity: 0.0,
            entry_price: None,
            realized_pnl_usd: 0.0,
            reason: "close_requested".to_owned(),
            created_at_ms: 1,
        },
        open_position("bot", "BTCUSDT", "entry_fill"),
    ];

    let latest = latest_authoritative_position("BTCUSDT", &positions).unwrap();
    assert_eq!(latest.reason, "entry_fill");
}

#[test]
fn latest_authoritative_position_skips_reconciliation_sync_rows() {
    let positions = vec![
        PositionRecord {
            id: 1,
            bot_id: "bot".to_owned(),
            symbol: Some("BTCUSDT".to_owned()),
            trace_id: None,
            bar_timestamp: None,
            has_position: true,
            quantity: 1.0,
            entry_price: Some(100.0),
            realized_pnl_usd: 0.0,
            reason: "startup_reconciliation_sync".to_owned(),
            created_at_ms: 1,
        },
        open_position("bot", "BTCUSDT", "entry_fill"),
    ];

    let latest = latest_authoritative_position("BTCUSDT", &positions).unwrap();
    assert_eq!(latest.reason, "entry_fill");
}

#[test]
fn managed_position_deficit_exceptions_only_block_true_deficits() {
    let snapshot = ConnectorAccountSnapshot {
        open_orders: Vec::new(),
        positions: vec![ConnectorPosition {
            symbol: "BTC".to_owned(),
            quantity: 0.5,
        }],
        balances: Vec::new(),
    };
    let lanes = vec![lane("lane-a", "bot-a", "acct", "BTCUSDT")];
    let latest_positions = vec![LatestLanePosition {
        lane_id: "lane-a".to_owned(),
        position: Some(position_with_state(
            1,
            "bot-a",
            "BTCUSDT",
            true,
            1.0,
            "entry_fill",
        )),
    }];

    let exceptions =
        managed_position_deficit_exceptions("acct", &snapshot, &lanes, &latest_positions);

    assert_eq!(exceptions.len(), 1);
    assert_eq!(
        exceptions[0].kind,
        LedgerExceptionKind::ManagedPositionDeficit
    );
    assert_eq!(exceptions[0].symbol.as_deref(), Some("BTCUSDT"));
    assert!(exceptions[0].detail.contains("deficit_qty=0.5"));
    assert!(exceptions[0].blocks_new_opens);
}

#[test]
fn managed_position_deficit_exceptions_ignore_external_surplus() {
    let snapshot = ConnectorAccountSnapshot {
        open_orders: Vec::new(),
        positions: vec![ConnectorPosition {
            symbol: "BTC".to_owned(),
            quantity: 1.5,
        }],
        balances: Vec::new(),
    };
    let lanes = vec![lane("lane-a", "bot-a", "acct", "BTCUSDT")];
    let latest_positions = vec![LatestLanePosition {
        lane_id: "lane-a".to_owned(),
        position: Some(position_with_state(
            1,
            "bot-a",
            "BTCUSDT",
            true,
            1.0,
            "entry_fill",
        )),
    }];

    let exceptions =
        managed_position_deficit_exceptions("acct", &snapshot, &lanes, &latest_positions);

    assert!(exceptions.is_empty());
}

#[test]
fn close_requested_and_reconciliation_sync_rows_are_not_authoritative_owners() {
    let lanes = vec![
        lane("lane-a", "bot-a", "acct", "BTCUSDT"),
        lane("lane-b", "bot-b", "acct", "BTCUSDT"),
    ];
    let latest_positions = vec![
        LatestLanePosition {
            lane_id: "lane-a".to_owned(),
            position: Some(position_with_state(
                1,
                "bot-a",
                "BTCUSDT",
                true,
                1.0,
                "close_requested",
            )),
        },
        LatestLanePosition {
            lane_id: "lane-b".to_owned(),
            position: Some(position_with_state(
                2,
                "bot-b",
                "BTCUSDT",
                true,
                1.0,
                "position_reconciliation_sync",
            )),
        },
    ];

    match connector_position_owner("acct", "BTC", &lanes, &latest_positions) {
        ConnectorPositionOwner::Ambiguous(owners) => {
            assert_eq!(owners, vec!["lane-a".to_owned(), "lane-b".to_owned()]);
        }
        other => panic!("unexpected owner resolution: {other:?}"),
    }
}

#[test]
fn symbol_suffix_matching_does_not_alias_unrelated_assets() {
    match connector_position_owner(
        "acct",
        "ETH",
        &[lane("lane-a", "bot-a", "acct", "BETHUSDT")],
        &[],
    ) {
        ConnectorPositionOwner::None => {}
        other => panic!("unexpected owner resolution: {other:?}"),
    }
}

#[test]
fn position_quantity_for_symbol_treats_base_asset_positions_as_symbol_positions() {
    let snapshot = ConnectorAccountSnapshot {
        open_orders: Vec::new(),
        positions: vec![ConnectorPosition {
            symbol: "BTC".to_owned(),
            quantity: 0.42,
        }],
        balances: Vec::new(),
    };

    assert!(position_quantity_for_symbol(&snapshot, "BTCUSDT") > POSITION_QUANTITY_TOLERANCE);
    assert!(position_quantity_for_symbol(&snapshot, "ETHUSDT") <= POSITION_QUANTITY_TOLERANCE);
}

#[test]
fn account_risk_snapshot_sums_open_positions_and_daily_loss() {
    let mut lanes = vec![lane("lane-a", "bot-a", "acct", "BTCUSDT")];
    let mut flat = lane("lane-b", "bot-b", "acct", "ETHUSDT");
    flat.effective_position_quantity = 0.0;
    flat.daily_loss_pct_accumulated = 2.0;
    lanes.push(flat);

    let snapshot = account_risk_snapshot(&lanes);
    assert_eq!(snapshot.open_positions, 1);
    assert!((snapshot.daily_loss_pct - 3.5).abs() < 1e-6);
}

#[test]
fn live_balance_from_binance_snapshot_includes_known_open_notional() {
    let snapshot = ConnectorAccountSnapshot {
        open_orders: Vec::new(),
        positions: vec![ConnectorPosition {
            symbol: "BTC".to_owned(),
            quantity: 0.5,
        }],
        balances: vec![ConnectorPrivateBalance {
            asset: "USDT".to_owned(),
            free: 400.0,
            locked: 100.0,
        }],
    };

    assert_eq!(
        live_balance_from_snapshot("binance", &snapshot, 250.0, &[]),
        Some(750.0)
    );
}

#[test]
fn live_balance_from_binance_snapshot_respects_configured_cash_assets() {
    let snapshot = ConnectorAccountSnapshot {
        open_orders: Vec::new(),
        positions: vec![ConnectorPosition {
            symbol: "BTC".to_owned(),
            quantity: 0.5,
        }],
        balances: vec![
            ConnectorPrivateBalance {
                asset: "USDT".to_owned(),
                free: 400.0,
                locked: 100.0,
            },
            ConnectorPrivateBalance {
                asset: "USDC".to_owned(),
                free: 80.0,
                locked: 20.0,
            },
        ],
    };
    let cash_balance_assets = vec!["USDC".to_owned()];

    assert_eq!(
        live_balance_from_snapshot("binance", &snapshot, 250.0, &cash_balance_assets),
        Some(350.0)
    );
}

#[test]
fn live_balance_from_snapshot_returns_none_when_binance_cash_is_effectively_zero() {
    let snapshot = ConnectorAccountSnapshot {
        open_orders: Vec::new(),
        positions: Vec::new(),
        balances: vec![ConnectorPrivateBalance {
            asset: "USDT".to_owned(),
            free: 5e-10,
            locked: 0.0,
        }],
    };

    assert_eq!(
        live_balance_from_snapshot("binance", &snapshot, 0.0, &[]),
        None
    );
}

#[test]
fn symbol_base_asset_handles_fdusd_pairs() {
    assert_eq!(symbol_base_asset("BTCFDUSD"), Some("BTC"));
}

#[test]
fn ledger_rooms_use_bot_and_account_tradeable_room() {
    let mut ledger = AccountLedger::new(1_000.0);
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    ledger.try_reserve_open(&owner, 250.0, 40.0).unwrap();

    let rooms = ledger_rooms(&ledger, "bot-a", 40.0);
    assert!((rooms.remaining_bot_usd - 150.0).abs() < 1e-6);
    assert!((rooms.remaining_account_usd - 750.0).abs() < 1e-6);
}

#[test]
fn bot_ledger_rejection_payload_exposes_bot_snapshot_fields() {
    let mut ledger = AccountLedger::new(1_000.0);
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    ledger.try_reserve_open(&owner, 250.0, 40.0).unwrap();

    let payload = bot_ledger_rejection_payload(
        &ledger,
        "acct",
        "bot-a",
        40.0,
        "open_long",
        "bot_ledger_exhausted",
    );

    assert_eq!(payload.intent, "open_long");
    assert_eq!(payload.decision, "rejected");
    assert_eq!(payload.reason, "bot_ledger_exhausted");
    assert_opt_f64_close(payload.committed_usd, 250.0);
    assert_opt_f64_close(payload.allocated_usd, 400.0);
    assert_opt_f64_close(payload.tradeable_room_usd, 150.0);
}
