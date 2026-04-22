use openticker_connectors::{
    ConnectorAccount, ConnectorError, ConnectorKind, ConnectorPrivateStreamEvent, ConnectorRegistry,
};
use openticker_core::ExecutionMode;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

#[test]
fn alpaca_position_payload_normalizes_expected_fields() {
    let (base_url, server_handle) = spawn_mock_alpaca_snapshot_server();
    let registry = ConnectorRegistry::from_accounts(vec![ConnectorAccount {
        account_id: "alpaca-paper".to_owned(),
        kind: ConnectorKind::Alpaca,
        mode: ExecutionMode::Paper,
        use_demo_mode: false,
        api_key_env: Some("PATH".to_owned()),
        api_secret_env: Some("PATH".to_owned()),
        passphrase_env: None,
        reconciliation_remote_snapshot: true,
        execution_remote_submission: false,
        reconciliation_base_url: Some(base_url),
    }])
    .expect("registry should build");

    let snapshot = registry
        .fetch_account_snapshot_unchecked("alpaca-paper")
        .expect("unchecked snapshot should decode remote payload");

    assert_eq!(snapshot.open_orders.len(), 1);
    assert_eq!(snapshot.open_orders[0].client_order_id, "alpaca-open-1");
    assert_eq!(snapshot.open_orders[0].symbol, "AAPL");
    assert_eq!(snapshot.open_orders[0].status, "open");
    assert_f64_eq(snapshot.open_orders[0].quantity, 1.0);

    assert_eq!(snapshot.positions.len(), 1);
    assert_eq!(snapshot.positions[0].symbol, "AAPL");
    assert_f64_eq(snapshot.positions[0].quantity, 1.5);

    assert_eq!(snapshot.balances.len(), 2);
    assert_eq!(snapshot.balances[0].asset, "CASH");
    assert_f64_eq(snapshot.balances[0].free, 750.0);
    assert_eq!(snapshot.balances[1].asset, "EQUITY");
    assert_f64_eq(snapshot.balances[1].free, 1_000.0);

    let request_lines = server_handle
        .join()
        .expect("mock server should finish cleanly");
    assert!(
        request_lines
            .iter()
            .any(|line| line.starts_with("GET /v2/orders?"))
    );
    assert!(
        request_lines
            .iter()
            .any(|line| line.starts_with("GET /v2/positions "))
    );
    assert!(
        request_lines
            .iter()
            .any(|line| line.starts_with("GET /v2/account "))
    );
}

#[test]
fn binance_balance_payload_normalizes_expected_fields() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-demo",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        true,
    )])
    .expect("registry should build");

    let payload = r#"{
        "e": "outboundAccountPosition",
        "B": [
            {"a": "BTC", "f": "0.3", "l": "0.1"},
            {"a": "USDT", "f": "1000", "l": "5.25"}
        ]
    }"#;

    let event = registry
        .normalize_private_stream_event("binance-demo", payload)
        .expect("private stream event should normalize")
        .expect("payload should produce an account update event");

    let ConnectorPrivateStreamEvent::Account(account) = event else {
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
fn malformed_private_payload_returns_explicit_error() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-demo",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        true,
    )])
    .expect("registry should build");

    assert!(matches!(
        registry.normalize_private_stream_event("binance-demo", "not-json"),
        Err(ConnectorError::StreamDecode { kind, detail })
            if kind == ConnectorKind::Binance
                && detail.contains("failed to decode private stream payload JSON")
    ));
}

fn test_account(
    account_id: &str,
    kind: ConnectorKind,
    mode: ExecutionMode,
    use_demo_mode: bool,
) -> ConnectorAccount {
    ConnectorAccount {
        account_id: account_id.to_owned(),
        kind,
        mode,
        use_demo_mode,
        api_key_env: None,
        api_secret_env: None,
        passphrase_env: None,
        reconciliation_remote_snapshot: false,
        execution_remote_submission: false,
        reconciliation_base_url: None,
    }
}

fn spawn_mock_alpaca_snapshot_server() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener
        .local_addr()
        .expect("listener should provide local address");

    let handle = thread::spawn(move || {
        let mut request_lines = Vec::new();
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().expect("request should connect");
            let request = read_http_request(&mut stream);
            let request_line = request.lines().next().unwrap_or_default().to_owned();
            let body = if request_line.starts_with("GET /v2/orders?") {
                r#"[
                    {
                        "client_order_id": "alpaca-open-1",
                        "symbol": "AAPL",
                        "status": "open",
                        "qty": "1"
                    }
                ]"#
            } else if request_line.starts_with("GET /v2/positions ") {
                r#"[
                    {
                        "symbol": "AAPL",
                        "qty": "1.5"
                    }
                ]"#
            } else if request_line.starts_with("GET /v2/account ") {
                r#"{
                    "cash": "750",
                    "equity": "1000"
                }"#
            } else {
                panic!("unexpected request line: {request_line}");
            };

            write_http_response(&mut stream, body);
            request_lines.push(request_line);
        }
        request_lines
    });

    (format!("http://{address}"), handle)
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buffer = [0_u8; 2048];
    let mut data = Vec::new();

    loop {
        let count = stream
            .read(&mut buffer)
            .expect("request bytes should be readable");
        if count == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..count]);

        if data.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    String::from_utf8_lossy(&data).into_owned()
}

fn write_http_response(stream: &mut TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("response bytes should be writable");
}

fn assert_f64_eq(left: f64, right: f64) {
    assert!(
        (left - right).abs() < f64::EPSILON,
        "expected {left} to equal {right}"
    );
}
