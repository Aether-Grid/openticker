use super::account::{
    AlpacaAccountPayload, AlpacaAssetPayload, AlpacaOrderPayload, AlpacaPositionPayload,
    normalize_account_balances, normalize_orders, normalize_positions,
    symbol_constraints_from_asset,
};
use super::bars::{
    ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS, AlpacaBarPayload, AlpacaBarsPayload,
    AlpacaHistoricalBarsPayload, alpaca_recent_bars_lookback_start, alpaca_timeframe_label,
    historical_alpaca_bars_for_symbol, latest_confirmed_alpaca_bar, normalize_recent_alpaca_bars,
};
use super::connector::{ALPACA_LIVE_BASE_URL, ALPACA_PAPER_BASE_URL, AlpacaConnector};
use crate::{ConnectorAccount, ConnectorAccountSnapshot, ConnectorExecution, ConnectorKind};
use chrono::{DateTime, Utc};
use openticker_core::{ExecutionMode, Timeframe, TradeIntent};
use openticker_execution::{ExecutionRequest, OrderSide, OrderType};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn assert_f64_eq(left: f64, right: f64) {
    assert!((left - right).abs() < 1e-9, "left={left}, right={right}");
}

fn account(mode: ExecutionMode) -> ConnectorAccount {
    ConnectorAccount {
        account_id: "alpaca-paper".to_owned(),
        kind: ConnectorKind::Alpaca,
        mode,
        use_demo_mode: false,
        api_key_env: Some("PATH".to_owned()),
        api_secret_env: Some("PATH".to_owned()),
        passphrase_env: None,
        reconciliation_remote_snapshot: false,
        execution_remote_submission: false,
        reconciliation_base_url: None,
    }
}

fn remote_submission_account(base_url: String) -> ConnectorAccount {
    let mut configured = account(ExecutionMode::Paper);
    configured.reconciliation_remote_snapshot = true;
    configured.execution_remote_submission = true;
    configured.reconciliation_base_url = Some(base_url);
    configured.api_key_env = Some("PATH".to_owned());
    configured.api_secret_env = Some("PATH".to_owned());
    configured
}

#[test]
fn resolves_default_reconciliation_base_url_from_mode() {
    let paper = AlpacaConnector::new(&account(ExecutionMode::Paper));
    assert_eq!(paper.reconciliation_base_url(), ALPACA_PAPER_BASE_URL);

    let live = AlpacaConnector::new(&account(ExecutionMode::Live));
    assert_eq!(live.reconciliation_base_url(), ALPACA_LIVE_BASE_URL);
}

#[test]
fn normalizes_alpaca_order_and_position_payloads() {
    let orders = serde_json::from_str::<Vec<AlpacaOrderPayload>>(
        r#"[
            {
                "client_order_id": "abc-123",
                "symbol": "AAPL",
                "status": "new",
                "qty": "2"
            }
        ]"#,
    )
    .unwrap();
    let positions = serde_json::from_str::<Vec<AlpacaPositionPayload>>(
        r#"[
            {
                "symbol": "AAPL",
                "qty": "1.5"
            }
        ]"#,
    )
    .unwrap();

    let snapshot = ConnectorAccountSnapshot {
        open_orders: normalize_orders(orders),
        positions: normalize_positions(positions),
        balances: normalize_account_balances(&AlpacaAccountPayload {
            cash: 750.0,
            equity: 1_000.0,
        }),
    };

    assert_eq!(snapshot.open_orders.len(), 1);
    assert_eq!(snapshot.open_orders[0].client_order_id, "abc-123");
    assert_f64_eq(snapshot.open_orders[0].quantity, 2.0);
    assert_eq!(snapshot.positions.len(), 1);
    assert_eq!(snapshot.positions[0].symbol, "AAPL");
    assert_f64_eq(snapshot.positions[0].quantity, 1.5);
    assert_eq!(snapshot.balances.len(), 2);
    assert_eq!(snapshot.balances[0].asset, "CASH");
    assert_f64_eq(snapshot.balances[0].free, 750.0);
    assert_eq!(snapshot.balances[1].asset, "EQUITY");
    assert_f64_eq(snapshot.balances[1].free, 1_000.0);
}

#[test]
fn derives_symbol_constraints_from_fractionable_asset_metadata() {
    let fractionable = symbol_constraints_from_asset(&AlpacaAssetPayload {
        tradable: true,
        fractionable: true,
    });
    assert_eq!(fractionable.fractional_entry_supported, Some(true));
    assert!(fractionable.quantity_step.is_none());
    assert!(fractionable.min_quantity.is_none());
    assert!(fractionable.min_notional_usd.is_none());

    let whole_share = symbol_constraints_from_asset(&AlpacaAssetPayload {
        tradable: true,
        fractionable: false,
    });
    assert_eq!(whole_share.fractional_entry_supported, Some(false));
    assert_eq!(whole_share.quantity_step, Some(1.0));
    assert_eq!(whole_share.min_quantity, Some(1.0));
    assert!(whole_share.min_notional_usd.is_none());
}

#[test]
fn maps_core_timeframes_to_alpaca_labels() {
    assert_eq!(alpaca_timeframe_label(Timeframe::M1), "1Min");
    assert_eq!(alpaca_timeframe_label(Timeframe::M5), "5Min");
    assert_eq!(alpaca_timeframe_label(Timeframe::M15), "15Min");
    assert_eq!(alpaca_timeframe_label(Timeframe::M30), "30Min");
    assert_eq!(alpaca_timeframe_label(Timeframe::H1), "1Hour");
    assert_eq!(alpaca_timeframe_label(Timeframe::H4), "4Hour");
    assert_eq!(alpaca_timeframe_label(Timeframe::D1), "1Day");
}

#[test]
fn decodes_alpaca_bar_payload() {
    let payload = serde_json::from_str::<AlpacaBarsPayload>(
        r#"{
            "bars": [
                {
                    "t": "2026-01-01T00:00:00Z",
                    "o": 100.0,
                    "h": 102.0,
                    "l": 99.5,
                    "c": 101.5,
                    "v": 1200
                }
            ]
        }"#,
    )
    .unwrap();

    let bars = payload.bars.expect("bars should be present");

    assert_eq!(bars.len(), 1);
    assert_f64_eq(bars[0].open, 100.0);
    assert_f64_eq(bars[0].close, 101.5);
}

#[test]
fn decodes_null_alpaca_bars_payload() {
    let payload = serde_json::from_str::<AlpacaBarsPayload>(
        r#"{
            "bars": null
        }"#,
    )
    .unwrap();

    assert!(payload.bars.is_none());
}

#[test]
fn decodes_paginated_alpaca_historical_bars_payload() {
    let payload = serde_json::from_str::<AlpacaHistoricalBarsPayload>(
        r#"{
            "bars": {
                "TSLA": [
                    {
                        "t": "2026-01-01T00:00:00Z",
                        "o": 100.0,
                        "h": 102.0,
                        "l": 99.5,
                        "c": 101.5,
                        "v": 1200
                    }
                ]
            },
            "next_page_token": "page-2"
        }"#,
    )
    .unwrap();

    let bars = historical_alpaca_bars_for_symbol(payload.bars, "TSLA");
    assert_eq!(bars.len(), 1);
    assert_eq!(payload.next_page_token.as_deref(), Some("page-2"));
    assert_f64_eq(bars[0].close, 101.5);
}

#[test]
fn paginated_alpaca_recent_bars_stay_oldest_first_and_confirmed_only() {
    let page_one = serde_json::from_str::<AlpacaHistoricalBarsPayload>(
        r#"{
            "bars": {
                "TSLA": [
                    {
                        "t": "2026-01-01T16:00:00Z",
                        "o": 104.0,
                        "h": 105.0,
                        "l": 103.0,
                        "c": 104.5,
                        "v": 1400
                    },
                    {
                        "t": "2026-01-01T12:00:00Z",
                        "o": 103.0,
                        "h": 104.0,
                        "l": 102.0,
                        "c": 103.5,
                        "v": 1300
                    }
                ]
            },
            "next_page_token": "page-2"
        }"#,
    )
    .unwrap();
    let page_two = serde_json::from_str::<AlpacaHistoricalBarsPayload>(
        r#"{
            "bars": {
                "TSLA": [
                    {
                        "t": "2026-01-01T08:00:00Z",
                        "o": 102.0,
                        "h": 103.0,
                        "l": 101.0,
                        "c": 102.5,
                        "v": 1200
                    },
                    {
                        "t": "2026-01-01T04:00:00Z",
                        "o": 101.0,
                        "h": 102.0,
                        "l": 100.0,
                        "c": 101.5,
                        "v": 1100
                    }
                ]
            },
            "next_page_token": null
        }"#,
    )
    .unwrap();

    let mut collected = historical_alpaca_bars_for_symbol(page_one.bars, "TSLA");
    collected.extend(historical_alpaca_bars_for_symbol(page_two.bars, "TSLA"));

    let now = DateTime::parse_from_rfc3339("2026-01-01T17:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let normalized = normalize_recent_alpaca_bars(collected, Timeframe::H4, now, 3);

    assert_eq!(normalized.len(), 3);
    assert_eq!(
        normalized[0].timestamp.to_rfc3339(),
        "2026-01-01T04:00:00+00:00"
    );
    assert_eq!(
        normalized[1].timestamp.to_rfc3339(),
        "2026-01-01T08:00:00+00:00"
    );
    assert_eq!(
        normalized[2].timestamp.to_rfc3339(),
        "2026-01-01T12:00:00+00:00"
    );
}

#[test]
fn recent_bars_lookback_expands_for_slow_timeframes() {
    let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let minute_start = alpaca_recent_bars_lookback_start(now, Timeframe::M1, 200);
    let hour_start = alpaca_recent_bars_lookback_start(now, Timeframe::H4, 200);

    let minute_start = DateTime::parse_from_rfc3339(&minute_start)
        .unwrap()
        .with_timezone(&Utc);
    let hour_start = DateTime::parse_from_rfc3339(&hour_start)
        .unwrap()
        .with_timezone(&Utc);

    assert_eq!(
        (now - minute_start).num_days(),
        ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS
    );
    assert!((now - hour_start).num_days() > ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS);
}

#[test]
fn recent_alpaca_bars_are_oldest_first_and_confirmed_only() {
    let bars = serde_json::from_str::<Vec<AlpacaBarPayload>>(
        r#"[
            {
                "t": "2026-01-01T00:03:00Z",
                "o": 103.0,
                "h": 104.0,
                "l": 102.0,
                "c": 103.5,
                "v": 1300
            },
            {
                "t": "2026-01-01T00:02:00Z",
                "o": 102.0,
                "h": 103.0,
                "l": 101.0,
                "c": 102.5,
                "v": 1200
            },
            {
                "t": "2026-01-01T00:01:00Z",
                "o": 101.0,
                "h": 102.0,
                "l": 100.0,
                "c": 101.5,
                "v": 1100
            }
        ]"#,
    )
    .unwrap();

    let now = DateTime::parse_from_rfc3339("2026-01-01T00:03:30Z")
        .unwrap()
        .with_timezone(&Utc);
    let normalized = normalize_recent_alpaca_bars(bars, Timeframe::M1, now, 2);

    assert_eq!(normalized.len(), 2);
    assert_eq!(
        normalized[0].timestamp.to_rfc3339(),
        "2026-01-01T00:01:00+00:00"
    );
    assert_eq!(
        normalized[1].timestamp.to_rfc3339(),
        "2026-01-01T00:02:00+00:00"
    );
    assert_f64_eq(normalized[0].close, 101.5);
    assert_f64_eq(normalized[1].close, 102.5);
}

#[test]
fn latest_alpaca_bar_ignores_open_bar() {
    let bars = serde_json::from_str::<Vec<AlpacaBarPayload>>(
        r#"[
            {
                "t": "2026-01-01T00:03:00Z",
                "o": 103.0,
                "h": 104.0,
                "l": 102.0,
                "c": 103.5,
                "v": 1300
            },
            {
                "t": "2026-01-01T00:02:00Z",
                "o": 102.0,
                "h": 103.0,
                "l": 101.0,
                "c": 102.5,
                "v": 1200
            }
        ]"#,
    )
    .unwrap();

    let now = DateTime::parse_from_rfc3339("2026-01-01T00:03:30Z")
        .unwrap()
        .with_timezone(&Utc);
    let latest =
        latest_confirmed_alpaca_bar(bars, Timeframe::M1, now).expect("expected a confirmed bar");

    assert_eq!(latest.timestamp.to_rfc3339(), "2026-01-01T00:02:00+00:00");
    assert_f64_eq(latest.close, 102.5);
}

#[test]
fn submits_remote_market_order_when_remote_snapshot_mode_enabled() {
    let (base_url, handle) = spawn_mock_alpaca_order_server();
    let connector = AlpacaConnector::new(&remote_submission_account(base_url));
    let timestamp = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let request = ExecutionRequest {
        instance_id: "aapl-instance".to_owned(),
        symbol: "AAPL".to_owned(),
        timestamp,
        intent: TradeIntent::OpenLong,
        price: 101.25,
        quantity: 2.0,
    };

    let accepted = connector.submit_order(&request).unwrap();
    assert_eq!(accepted.side, OrderSide::Buy);
    assert_eq!(accepted.order_type, OrderType::Market);
    assert_f64_eq(accepted.quantity, 2.0);
    assert_f64_eq(accepted.price, 101.25);
    assert!(accepted.fee_asset.is_none());
    assert!(accepted.fee_amount.is_none());
    assert!(accepted.fee_normalized_usd.is_none());

    let request_lines = handle.join().unwrap();
    assert!(
        request_lines
            .iter()
            .any(|line| line.starts_with("POST /v2/orders "))
    );
}

fn spawn_mock_alpaca_order_server() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request_with_body(&mut stream);
        let request_lines = request.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

        let response_body = r#"{
            "id": "order-1",
            "client_order_id": "alpaca-test-order-1",
            "status": "filled",
            "filled_qty": "2",
            "filled_avg_price": "101.25"
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
