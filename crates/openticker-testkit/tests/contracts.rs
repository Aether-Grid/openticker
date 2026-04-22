use openticker_core::Timeframe;
use openticker_testkit::{
    close_only_bar, close_only_symbol_bar, shared_fixture_bundle, shared_fixture_bundle_for_symbol,
    spawn_fake_reconciliation_server,
};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[test]
fn close_only_bar_is_deterministic() {
    let first = close_only_bar("2030-01-01T00:00:00Z", 101.25);
    let second = close_only_bar("2030-01-01T00:00:00Z", 101.25);
    let (symbol, symbol_bar) = close_only_symbol_bar("AAPL", "2030-01-01T00:00:00Z", 101.25);

    assert_eq!(first.timestamp, second.timestamp);
    assert_f64_eq(first.open, second.open);
    assert_f64_eq(first.high, second.high);
    assert_f64_eq(first.low, second.low);
    assert_f64_eq(first.close, second.close);
    assert_f64_eq(first.volume, second.volume);
    assert_eq!(symbol, "AAPL");
    assert_eq!(symbol_bar.timestamp, first.timestamp);
    assert_f64_eq(symbol_bar.close, first.close);
}

#[test]
fn shared_fixture_bundle_is_deterministic() {
    let first = shared_fixture_bundle();
    let second = shared_fixture_bundle();

    assert_eq!(
        first.global.service.environment,
        second.global.service.environment
    );
    assert_eq!(first.accounts[0].id, second.accounts[0].id);
    assert_eq!(first.accounts[0].mode, second.accounts[0].mode);
    assert_eq!(first.risk_profiles[0].id, second.risk_profiles[0].id);
    assert_eq!(first.instances[0].id, second.instances[0].id);
    assert_eq!(first.instances[0].symbols, second.instances[0].symbols);
    assert_eq!(first.instances[0].timeframe, second.instances[0].timeframe);

    let custom = shared_fixture_bundle_for_symbol("msft", "MSFT", Timeframe::M5);
    assert_eq!(custom.instances[0].id, "msft");
    assert_eq!(custom.instances[0].symbols, vec!["MSFT".to_owned()]);
    assert_eq!(custom.instances[0].timeframe, Timeframe::M5);
}

#[test]
fn fake_reconciliation_server_serves_expected_snapshot_shape() {
    let orders = r#"[{"id":"order-1","symbol":"AAPL"}]"#;
    let positions = r#"[{"symbol":"AAPL","qty":"2"}]"#;
    let account = r#"{"cash":"10000","equity":"12000"}"#;

    let (base_url, handle) = spawn_fake_reconciliation_server(orders, positions, account, 3);

    let orders_body = send_http_get(&base_url, "/v2/orders");
    let positions_body = send_http_get(&base_url, "/v2/positions");
    let account_body = send_http_get(&base_url, "/v2/account");

    assert_eq!(orders_body, orders);
    assert_eq!(positions_body, positions);
    assert_eq!(account_body, account);

    let requests = handle
        .join()
        .expect("fake reconciliation server should stop cleanly");
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .any(|line| line.starts_with("GET /v2/orders"))
    );
    assert!(
        requests
            .iter()
            .any(|line| line.starts_with("GET /v2/positions"))
    );
    assert!(
        requests
            .iter()
            .any(|line| line.starts_with("GET /v2/account"))
    );
}

fn send_http_get(base_url: &str, path: &str) -> String {
    let host = base_url
        .strip_prefix("http://")
        .expect("fake server base url should start with http://");
    let mut stream = TcpStream::connect(host).expect("test request should connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("test request should set read timeout");

    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .expect("test request should write");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("test request should read response");

    if let Some((_, body)) = response.split_once("\r\n\r\n") {
        return body.to_owned();
    }
    if let Some((_, body)) = response.split_once("\n\n") {
        return body.to_owned();
    }

    panic!("http response should include headers and body: {response}");
}

fn assert_f64_eq(left: f64, right: f64) {
    assert!(
        (left - right).abs() < f64::EPSILON,
        "expected {left} to equal {right}"
    );
}
