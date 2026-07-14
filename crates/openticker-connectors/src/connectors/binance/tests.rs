use super::connector::{BINANCE_DEMO_BASE_URL, BINANCE_LIVE_BASE_URL, BinanceConnector};
use super::http::sign_query;
use super::klines::{
    binance_interval_label, latest_confirmed_binance_bar, normalize_recent_binance_klines,
    parse_kline_row, parse_kline_row_with_close_time,
};
use super::orders::{
    BinanceSubmittedOrderPayload, accepted_order_from_binance_payload, format_binance_quantity,
};
use super::snapshot::{
    BinanceAccountPayload, BinanceExchangeInfoPayload, BinanceOpenOrderPayload,
    extract_symbol_constraints, normalize_balances, normalize_orders, normalize_positions,
};
use super::stream::{normalize_market_data_event, normalize_private_event};
use crate::{
    ConnectorAccount, ConnectorAccountSnapshot, ConnectorError, ConnectorExecution, ConnectorKind,
    ConnectorPrivateStreamEvent,
};
use chrono::{DateTime, Utc};
use openticker_core::{ExecutionMode, SignalPhase, Timeframe, TradeIntent};
use openticker_execution::{ExecutionRequest, OrderSide, OrderType};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn assert_f64_eq(left: f64, right: f64) {
    assert!((left - right).abs() < 1e-9, "left={left}, right={right}");
}

fn account(mode: ExecutionMode, use_demo_mode: bool) -> ConnectorAccount {
    ConnectorAccount {
        account_id: "binance-demo".to_owned(),
        kind: ConnectorKind::Binance,
        mode,
        use_demo_mode,
        api_key_env: Some("PATH".to_owned()),
        api_secret_env: Some("PATH".to_owned()),
        passphrase_env: None,
        reconciliation_remote_snapshot: false,
        execution_remote_submission: false,
        reconciliation_base_url: None,
    }
}

fn remote_submission_account(base_url: String) -> ConnectorAccount {
    let mut configured = account(ExecutionMode::Paper, true);
    configured.reconciliation_remote_snapshot = true;
    configured.execution_remote_submission = true;
    configured.reconciliation_base_url = Some(base_url);
    configured.api_key_env = Some("PATH".to_owned());
    configured.api_secret_env = Some("PATH".to_owned());
    configured
}

#[test]
fn resolves_demo_base_url_for_paper_or_demo_modes() {
    let paper = BinanceConnector::new(&account(ExecutionMode::Paper, true));
    assert_eq!(paper.reconciliation_base_url(), BINANCE_DEMO_BASE_URL);

    let live_demo = BinanceConnector::new(&account(ExecutionMode::Live, true));
    assert_eq!(live_demo.reconciliation_base_url(), BINANCE_DEMO_BASE_URL);

    let live = BinanceConnector::new(&account(ExecutionMode::Live, false));
    assert_eq!(live.reconciliation_base_url(), BINANCE_LIVE_BASE_URL);
}

#[test]
fn signature_is_stable_for_identical_query() {
    let query = "recvWindow=5000&timestamp=1700000000000";
    let first = sign_query("secret", query).unwrap();
    let second = sign_query("secret", query).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
}

#[test]
fn normalizes_open_orders_and_non_zero_balances() {
    let orders = serde_json::from_str::<Vec<BinanceOpenOrderPayload>>(
        r#"[
            {
                "clientOrderId": "order-1",
                "symbol": "BTCUSDT",
                "status": "NEW",
                "origQty": "0.01"
            }
        ]"#,
    )
    .unwrap();
    let account = serde_json::from_str::<BinanceAccountPayload>(
        r#"{
            "balances": [
                {"asset": "BTC", "free": "0.5", "locked": "0.1"},
                {"asset": "USDT", "free": "0", "locked": "0"}
            ]
        }"#,
    )
    .unwrap();

    let snapshot = ConnectorAccountSnapshot {
        open_orders: normalize_orders(orders),
        positions: normalize_positions(account.balances.clone()),
        balances: normalize_balances(account.balances),
    };

    assert_eq!(snapshot.open_orders.len(), 1);
    assert_eq!(snapshot.open_orders[0].symbol, "BTCUSDT");
    assert_f64_eq(snapshot.open_orders[0].quantity, 0.01);
    assert_eq!(snapshot.positions.len(), 1);
    assert_eq!(snapshot.positions[0].symbol, "BTC");
    assert_f64_eq(snapshot.positions[0].quantity, 0.6);
    assert_eq!(snapshot.balances.len(), 2);
    assert_eq!(snapshot.balances[0].asset, "BTC");
    assert_f64_eq(snapshot.balances[0].free, 0.5);
    assert_eq!(snapshot.balances[1].asset, "USDT");
}

#[test]
fn excludes_cash_balances_from_binance_positions() {
    let account = serde_json::from_str::<BinanceAccountPayload>(
        r#"{
            "balances": [
                {"asset": "BTC", "free": "0.01", "locked": "0"},
                {"asset": "USDT", "free": "9997.48", "locked": "0"},
                {"asset": "USDC", "free": "12.5", "locked": "0"}
            ]
        }"#,
    )
    .unwrap();

    let positions = normalize_positions(account.balances);
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTC");
    assert_f64_eq(positions[0].quantity, 0.01);
}

#[test]
fn extracts_symbol_constraints_from_exchange_info() {
    let payload = serde_json::from_str::<BinanceExchangeInfoPayload>(
        r#"{
            "symbols": [
                {
                    "symbol": "BTCUSDT",
                    "status": "TRADING",
                    "filters": [
                        {
                            "filterType": "LOT_SIZE",
                            "minQty": "0.00001000",
                            "stepSize": "0.00001000"
                        },
                        {
                            "filterType": "MIN_NOTIONAL",
                            "minNotional": "5.00000000"
                        }
                    ]
                }
            ]
        }"#,
    )
    .unwrap();

    let constraints = extract_symbol_constraints(payload, "BTCUSDT").unwrap();
    assert_eq!(constraints.fractional_entry_supported, None);
    assert_f64_eq(constraints.quantity_step.unwrap(), 0.00001);
    assert_f64_eq(constraints.min_quantity.unwrap(), 0.00001);
    assert_f64_eq(constraints.min_notional_usd.unwrap(), 5.0);
    assert_eq!(constraints.source.as_deref(), Some("binance_exchange_info"));
}

#[test]
fn formats_binance_quantity_without_excess_precision() {
    assert_eq!(
        format_binance_quantity(0.000_140_000_000_000_000_01).unwrap(),
        "0.00014"
    );
    assert_eq!(
        format_binance_quantity(2_499_999.999_990_000_4).unwrap(),
        "2499999.99999"
    );
}

#[test]
fn rejects_non_finite_binance_quantity() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = format_binance_quantity(value)
            .expect_err("non-finite quantity must be rejected, not formatted as 0");
        assert!(
            matches!(error, ConnectorError::OrderSubmission { .. }),
            "expected OrderSubmission error, got {error:?}"
        );
    }
}

#[test]
fn formats_non_positive_binance_quantity_as_zero() {
    assert_eq!(format_binance_quantity(0.0).unwrap(), "0");
    assert_eq!(format_binance_quantity(-1.0).unwrap(), "0");
}

#[test]
fn rejects_symbol_constraints_when_symbol_is_not_trading() {
    let payload = serde_json::from_str::<BinanceExchangeInfoPayload>(
        r#"{
            "symbols": [
                {
                    "symbol": "BTCUSDT",
                    "status": "BREAK",
                    "filters": []
                }
            ]
        }"#,
    )
    .unwrap();

    assert!(matches!(
        extract_symbol_constraints(payload, "BTCUSDT"),
        Err(ConnectorError::RemoteSnapshot { .. })
    ));
}

#[test]
fn maps_core_timeframes_to_binance_intervals() {
    assert_eq!(binance_interval_label(Timeframe::M1), "1m");
    assert_eq!(binance_interval_label(Timeframe::M5), "5m");
    assert_eq!(binance_interval_label(Timeframe::M15), "15m");
    assert_eq!(binance_interval_label(Timeframe::M30), "30m");
    assert_eq!(binance_interval_label(Timeframe::H1), "1h");
    assert_eq!(binance_interval_label(Timeframe::H4), "4h");
    assert_eq!(binance_interval_label(Timeframe::D1), "1d");
}

#[test]
fn parses_binance_kline_row_into_bar() {
    let row = vec![
        serde_json::json!(1_704_067_200_000_i64),
        serde_json::json!("42000.1"),
        serde_json::json!("42100.0"),
        serde_json::json!("41950.2"),
        serde_json::json!("42080.4"),
        serde_json::json!("12.5"),
    ];

    let bar = parse_kline_row(&row).unwrap();
    assert_f64_eq(bar.open, 42_000.1);
    assert_f64_eq(bar.high, 42_100.0);
    assert_f64_eq(bar.low, 41_950.2);
    assert_f64_eq(bar.close, 42_080.4);
    assert_f64_eq(bar.volume, 12.5);
}

#[test]
fn recent_binance_klines_are_oldest_first_and_confirmed_only() {
    let rows = vec![
        vec![
            serde_json::json!(1_704_067_200_000_i64),
            serde_json::json!("42000.1"),
            serde_json::json!("42100.0"),
            serde_json::json!("41950.2"),
            serde_json::json!("42080.4"),
            serde_json::json!("12.5"),
            serde_json::json!(1_704_067_259_999_i64),
        ],
        vec![
            serde_json::json!(1_704_067_260_000_i64),
            serde_json::json!("42080.4"),
            serde_json::json!("42150.0"),
            serde_json::json!("42000.0"),
            serde_json::json!("42120.0"),
            serde_json::json!("10.0"),
            serde_json::json!(1_704_067_319_999_i64),
        ],
        vec![
            serde_json::json!(1_704_067_320_000_i64),
            serde_json::json!("42120.0"),
            serde_json::json!("42200.0"),
            serde_json::json!("42050.0"),
            serde_json::json!("42180.0"),
            serde_json::json!("11.0"),
            serde_json::json!(1_704_067_379_999_i64),
        ],
    ];

    let normalized = normalize_recent_binance_klines(rows, 2, 1_704_067_350_000).unwrap();

    assert_eq!(normalized.len(), 2);
    assert_eq!(
        normalized[0].timestamp.timestamp_millis(),
        1_704_067_200_000
    );
    assert_eq!(
        normalized[1].timestamp.timestamp_millis(),
        1_704_067_260_000
    );
    assert_f64_eq(normalized[0].close, 42_080.4);
    assert_f64_eq(normalized[1].close, 42_120.0);
}

#[test]
fn latest_binance_bar_ignores_open_kline() {
    let rows = vec![
        vec![
            serde_json::json!(1_704_067_260_000_i64),
            serde_json::json!("42080.4"),
            serde_json::json!("42150.0"),
            serde_json::json!("42000.0"),
            serde_json::json!("42120.0"),
            serde_json::json!("10.0"),
            serde_json::json!(1_704_067_319_999_i64),
        ],
        vec![
            serde_json::json!(1_704_067_320_000_i64),
            serde_json::json!("42120.0"),
            serde_json::json!("42200.0"),
            serde_json::json!("42050.0"),
            serde_json::json!("42180.0"),
            serde_json::json!("11.0"),
            serde_json::json!(1_704_067_379_999_i64),
        ],
    ];

    let latest = latest_confirmed_binance_bar(rows, 1_704_067_350_000)
        .unwrap()
        .expect("expected a confirmed kline");

    assert_eq!(latest.timestamp.timestamp_millis(), 1_704_067_260_000);
    assert_f64_eq(latest.close, 42_120.0);
}

#[test]
fn parse_kline_row_uses_close_time_when_present() {
    let row = vec![
        serde_json::json!(1_704_067_200_000_i64),
        serde_json::json!("42000.1"),
        serde_json::json!("42100.0"),
        serde_json::json!("41950.2"),
        serde_json::json!("42080.4"),
        serde_json::json!("12.5"),
        serde_json::json!(1_704_067_259_999_i64),
    ];

    let (bar, close_time_ms) = parse_kline_row_with_close_time(&row).unwrap();
    assert_eq!(bar.timestamp.timestamp_millis(), 1_704_067_200_000);
    assert_eq!(close_time_ms, 1_704_067_259_999);
}

#[test]
fn parse_kline_row_falls_back_to_open_time_when_close_time_missing() {
    // Six-field row (no close time at index 6): the parser falls back to
    // using the open time as the close time.
    let row = vec![
        serde_json::json!(1_704_067_200_000_i64),
        serde_json::json!("42000.1"),
        serde_json::json!("42100.0"),
        serde_json::json!("41950.2"),
        serde_json::json!("42080.4"),
        serde_json::json!("12.5"),
    ];

    let (bar, close_time_ms) = parse_kline_row_with_close_time(&row).unwrap();
    assert_eq!(bar.timestamp.timestamp_millis(), 1_704_067_200_000);
    assert_eq!(close_time_ms, 1_704_067_200_000);
}

#[test]
fn parse_kline_row_rejects_short_row() {
    let row = vec![
        serde_json::json!(1_704_067_200_000_i64),
        serde_json::json!("42000.1"),
    ];

    let error = parse_kline_row_with_close_time(&row)
        .expect_err("rows with fewer than 6 fields must be rejected");
    assert!(matches!(error, ConnectorError::RemoteSnapshot { .. }));
}

#[test]
fn parse_kline_row_rejects_invalid_number() {
    // A non-numeric open price must surface as an error rather than being
    // silently coerced.
    let row = vec![
        serde_json::json!(1_704_067_200_000_i64),
        serde_json::json!("not-a-number"),
        serde_json::json!("42100.0"),
        serde_json::json!("41950.2"),
        serde_json::json!("42080.4"),
        serde_json::json!("12.5"),
        serde_json::json!(1_704_067_259_999_i64),
    ];

    let error =
        parse_kline_row_with_close_time(&row).expect_err("invalid JSON number must be rejected");
    assert!(matches!(error, ConnectorError::RemoteSnapshot { .. }));
}

#[test]
fn parse_kline_row_rejects_invalid_close_time_number() {
    let row = vec![
        serde_json::json!(1_704_067_200_000_i64),
        serde_json::json!("42000.1"),
        serde_json::json!("42100.0"),
        serde_json::json!("41950.2"),
        serde_json::json!("42080.4"),
        serde_json::json!("12.5"),
        serde_json::json!("not-a-number"),
    ];

    let error = parse_kline_row_with_close_time(&row)
        .expect_err("invalid close-time number must be rejected");
    assert!(matches!(error, ConnectorError::RemoteSnapshot { .. }));
}

#[test]
fn bar_closing_exactly_at_now_is_not_confirmed_on_either_path() {
    // Confirmation boundary: a kline whose close time equals `now_ms` is
    // still forming and must NOT be treated as confirmed. Both the
    // recent-klines path and the confirmed-range path must agree, using a
    // strict `<` comparison.
    let open_time_ms = 1_704_067_200_000_i64;
    let close_time_ms = 1_704_067_259_999_i64;
    let now_ms = close_time_ms; // exactly at close time

    let row = vec![
        serde_json::json!(open_time_ms),
        serde_json::json!("42000.1"),
        serde_json::json!("42100.0"),
        serde_json::json!("41950.2"),
        serde_json::json!("42080.4"),
        serde_json::json!("12.5"),
        serde_json::json!(close_time_ms),
    ];

    // Recent-klines path: a bar closing exactly at now is excluded.
    let recent = normalize_recent_binance_klines(vec![row.clone()], 10, now_ms).unwrap();
    assert!(
        recent.is_empty(),
        "bar closing exactly at now must not be confirmed on the recent-klines path"
    );

    // Confirmed-range path (shares `parse_kline_row_with_close_time`): the
    // same `close_time_ms < now_ms` rule rejects the boundary bar, and
    // accepts it one millisecond later.
    let (_, parsed_close_time_ms) = parse_kline_row_with_close_time(&row).unwrap();
    assert!(
        parsed_close_time_ms >= now_ms,
        "boundary bar must be unconfirmed at now == close_time"
    );
    assert!(
        parsed_close_time_ms < now_ms + 1,
        "the same bar must be confirmed one millisecond after its close time"
    );
}

#[test]
fn normalizes_binance_kline_websocket_payloads() {
    let combined = r#"{
        "stream": "btcusdt@kline_1m",
        "data": {
            "e": "kline",
            "s": "BTCUSDT",
            "k": {
                "t": 1704067200000,
                "o": "42000.0",
                "h": "42100.0",
                "l": "41900.0",
                "c": "42050.0",
                "v": "15.5",
                "x": false
            }
        }
    }"#;

    let preview = normalize_market_data_event(combined).unwrap().unwrap();
    assert_eq!(preview.symbol, "BTCUSDT");
    assert_eq!(preview.phase, SignalPhase::Preview);
    assert_f64_eq(preview.bar.close, 42_050.0);

    let raw = r#"{
        "e": "kline",
        "s": "BTCUSDT",
        "k": {
            "t": 1704067260000,
            "o": "42050.0",
            "h": "42200.0",
            "l": "42000.0",
            "c": "42180.0",
            "v": "20.0",
            "x": true
        }
    }"#;

    let confirmed = normalize_market_data_event(raw).unwrap().unwrap();
    assert_eq!(confirmed.phase, SignalPhase::Confirmed);
    assert_f64_eq(confirmed.bar.close, 42_180.0);
}

#[test]
fn rejects_malformed_market_stream_payload() {
    assert!(matches!(
        normalize_market_data_event("not-json"),
        Err(ConnectorError::StreamDecode { .. })
    ));
}

#[test]
fn normalizes_binance_private_order_and_account_payloads() {
    let order_payload = r#"{
        "stream": "btcusdt@executionReport",
        "data": {
            "e": "executionReport",
            "s": "BTCUSDT",
            "c": "cli-123",
            "S": "BUY",
            "X": "PARTIALLY_FILLED",
            "q": "0.50",
            "z": "0.20",
            "L": "42123.40"
        }
    }"#;

    let order_event = normalize_private_event(order_payload).unwrap().unwrap();
    let ConnectorPrivateStreamEvent::Order(order) = order_event else {
        panic!("expected normalized order event");
    };
    assert_eq!(order.symbol, "BTCUSDT");
    assert_eq!(order.client_order_id, "cli-123");
    assert_eq!(order.status, "PARTIALLY_FILLED");
    assert_eq!(order.side, "BUY");
    assert_f64_eq(order.order_quantity, 0.5);
    assert_f64_eq(order.cumulative_filled_quantity, 0.2);
    assert_eq!(order.last_fill_price, Some(42_123.4));

    let account_payload = r#"{
        "e": "outboundAccountPosition",
        "B": [
            {"a": "BTC", "f": "0.3", "l": "0.1"},
            {"a": "USDT", "f": "1000", "l": "5.25"}
        ]
    }"#;

    let account_event = normalize_private_event(account_payload).unwrap().unwrap();
    let ConnectorPrivateStreamEvent::Account(account) = account_event else {
        panic!("expected normalized account event");
    };
    assert_eq!(account.balances.len(), 2);
    assert_eq!(account.balances[0].asset, "BTC");
    assert_f64_eq(account.balances[0].free, 0.3);
    assert_f64_eq(account.balances[0].locked, 0.1);
    assert_eq!(account.balances[1].asset, "USDT");
    assert_f64_eq(account.balances[1].free, 1_000.0);
    assert_f64_eq(account.balances[1].locked, 5.25);
}

#[test]
fn ignores_unhandled_private_stream_event_types() {
    let payload = r#"{
        "e": "listenKeyExpired"
    }"#;

    assert!(normalize_private_event(payload).unwrap().is_none());
}

#[test]
fn rejects_malformed_private_stream_payload() {
    assert!(matches!(
        normalize_private_event("not-json"),
        Err(ConnectorError::StreamDecode { .. })
    ));
}

#[test]
fn submits_remote_market_order_when_remote_snapshot_mode_enabled() {
    let (base_url, handle) = spawn_mock_binance_order_server();
    let connector = BinanceConnector::new(&remote_submission_account(base_url));
    let timestamp = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let request = ExecutionRequest {
        instance_id: "btcusdt-instance".to_owned(),
        symbol: "BTCUSDT".to_owned(),
        timestamp,
        intent: TradeIntent::OpenLong,
        price: 42_050.0,
        quantity: 0.01,
    };

    let accepted = connector.submit_order(&request).unwrap();
    assert_eq!(accepted.side, OrderSide::Buy);
    assert_eq!(accepted.order_type, OrderType::Market);
    assert_f64_eq(accepted.quantity, 0.00999);
    assert_f64_eq(accepted.price, 42_050.0);
    assert_eq!(accepted.fee_asset.as_deref(), Some("BTC"));
    assert_f64_eq(accepted.fee_amount.unwrap(), 0.00001);
    assert_f64_eq(accepted.fee_normalized_usd.unwrap(), 0.4205);

    let request_lines = handle.join().unwrap();
    assert!(
        request_lines
            .iter()
            .any(|line| line.starts_with("POST /api/v3/order?"))
    );
}

#[test]
fn keeps_binance_buy_quantity_when_fee_is_not_in_base_asset() {
    let payload = serde_json::from_value::<BinanceSubmittedOrderPayload>(serde_json::json!({
        "symbol": "BTCUSDT",
        "clientOrderId": "binance-test-order-2",
        "status": "FILLED",
        "executedQty": "0.01000000",
        "cummulativeQuoteQty": "420.50000000",
        "fills": [
            {
                "price": "42050.0",
                "qty": "0.01",
                "commission": "0.00002000",
                "commissionAsset": "BNB"
            }
        ]
    }))
    .unwrap();

    let accepted = accepted_order_from_binance_payload(&payload, OrderSide::Buy, 42_050.0).unwrap();

    assert_f64_eq(accepted.quantity, 0.01);
    assert_f64_eq(accepted.price, 42_050.0);
    assert_eq!(accepted.fee_asset.as_deref(), Some("BNB"));
    assert_f64_eq(accepted.fee_amount.unwrap(), 0.00002);
    assert!(accepted.fee_normalized_usd.is_none());
}

#[test]
fn keeps_binance_sell_quantity_when_fee_is_in_quote_asset() {
    let payload = serde_json::from_value::<BinanceSubmittedOrderPayload>(serde_json::json!({
        "symbol": "ETHBTC",
        "clientOrderId": "binance-test-order-3",
        "status": "FILLED",
        "executedQty": "1.25000000",
        "cummulativeQuoteQty": "0.08000000",
        "fills": [
            {
                "price": "0.064",
                "qty": "1.25",
                "commission": "0.00008000",
                "commissionAsset": "BTC"
            }
        ]
    }))
    .unwrap();

    let accepted = accepted_order_from_binance_payload(&payload, OrderSide::Sell, 0.064).unwrap();

    assert_f64_eq(accepted.quantity, 1.25);
    assert_f64_eq(accepted.price, 0.064);
    assert_eq!(accepted.fee_asset.as_deref(), Some("BTC"));
    assert_f64_eq(accepted.fee_amount.unwrap(), 0.00008);
    assert!(accepted.fee_normalized_usd.is_none());
}

#[test]
fn falls_back_to_executed_qty_when_binance_fee_details_are_missing() {
    let payload = serde_json::from_value::<BinanceSubmittedOrderPayload>(serde_json::json!({
        "symbol": "BTCUSDT",
        "clientOrderId": "binance-test-order-4",
        "status": "FILLED",
        "executedQty": "0.01000000",
        "cummulativeQuoteQty": "420.50000000",
        "fills": [
            {
                "price": "42050.0",
                "qty": "0.01"
            }
        ]
    }))
    .unwrap();

    let accepted = accepted_order_from_binance_payload(&payload, OrderSide::Buy, 42_050.0).unwrap();

    assert_f64_eq(accepted.quantity, 0.01);
    assert_f64_eq(accepted.price, 42_050.0);
    assert!(accepted.fee_asset.is_none());
    assert!(accepted.fee_amount.is_none());
    assert!(accepted.fee_normalized_usd.is_none());
}

fn spawn_mock_binance_order_server() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request_with_body(&mut stream);
        let request_lines = request.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

        let response_body = r#"{
            "symbol": "BTCUSDT",
            "clientOrderId": "binance-test-order-1",
            "status": "FILLED",
            "executedQty": "0.01000000",
            "cummulativeQuoteQty": "420.50000000",
            "fills": [
                {
                    "price": "42050.0",
                    "qty": "0.01",
                    "commission": "0.00001000",
                    "commissionAsset": "BTC"
                }
            ]
        }"#;
        write_http_response(&mut stream, "HTTP/1.1 200 OK", response_body);
        request_lines
    });

    (format!("http://{address}"), handle)
}

fn read_http_request_with_body(stream: &mut TcpStream) -> String {
    let mut buffer = [0_u8; 2048];
    let mut data = Vec::new();
    let mut header_len = None;
    let mut content_len = 0_usize;

    loop {
        let count = stream.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..count]);

        if header_len.is_none()
            && let Some(index) = data.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let end = index + 4;
            header_len = Some(end);
            let headers = String::from_utf8_lossy(&data[..end]);
            content_len = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    if name.eq_ignore_ascii_case("content-length") {
                        value.trim().parse::<usize>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
        }

        if let Some(header_len) = header_len
            && data.len() >= header_len + content_len
        {
            break;
        }
    }

    String::from_utf8_lossy(&data).into_owned()
}

fn write_http_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "{status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}
