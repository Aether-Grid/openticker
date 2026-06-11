//! Regression tests for connector rate-limit handling.
//!
//! When a connector answers 418/429 (e.g. Binance `-1003` IP bans), the
//! gateway must throttle the account until the ban window passes instead of
//! hammering the provider with further requests.

use openticker_config::AccountConfig;
use openticker_core::{ExecutionMode, Timeframe};
use openticker_gateway::{Gateway, GatewayError, build_connector_registry};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn unix_now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_millis(),
    )
    .expect("timestamp should fit in i64")
}

/// Starts a minimal HTTP server that answers every request with a Binance
/// `-1003` IP-ban payload (HTTP 418), reporting the ban expiry `banned_until_ms`.
fn spawn_banning_server(banned_until_ms: i64) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
    let address = listener.local_addr().expect("local addr");
    let hits = Arc::new(AtomicUsize::new(0));
    let server_hits = Arc::clone(&hits);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            server_hits.fetch_add(1, Ordering::Relaxed);
            let mut reader = BufReader::new(stream.try_clone().expect("clone test server stream"));
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if line == "\r\n" || line == "\n" {
                    break;
                }
            }

            let body = format!(
                "{{\"code\":-1003,\"msg\":\"Way too much request weight used; IP banned until {banned_until_ms}. Please use WebSocket Streams for live updates to avoid bans.\"}}"
            );
            let response = format!(
                "HTTP/1.1 418 I'm a teapot\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    (format!("http://{address}"), hits)
}

fn banning_gateway(banned_until_ms: i64) -> (Gateway, Arc<AtomicUsize>) {
    let (base_url, hits) = spawn_banning_server(banned_until_ms);
    let registry = build_connector_registry(&[AccountConfig {
        id: "binance-demo".to_owned(),
        kind: "binance".to_owned(),
        mode: ExecutionMode::Paper,
        api_key_env: Some("PATH".to_owned()),
        api_secret_env: Some("PATH".to_owned()),
        passphrase_env: None,
        use_demo_mode: true,
        reconciliation_remote_snapshot: true,
        execution_remote_submission: None,
        reconciliation_base_url: Some(base_url),
        cash_balance_assets: Vec::new(),
        total_budget_usd: 10_000.0,
    }])
    .expect("registry should build");
    (Gateway::new(Arc::new(Mutex::new(registry))), hits)
}

#[test]
fn rate_limited_response_throttles_account_and_gates_further_fetches() {
    let banned_until_ms = unix_now_ms() + 60_000;
    let (gateway, hits) = banning_gateway(banned_until_ms);

    // First fetch hits the provider and gets banned.
    let error = gateway
        .fetch_latest_bar("binance-demo", "BTCUSDT", Timeframe::M1)
        .expect_err("banned fetch should fail");
    assert!(
        error.to_string().contains("rate limited"),
        "expected a rate-limited error, got: {error}"
    );
    assert_eq!(hits.load(Ordering::Relaxed), 1);

    // The account must now report the throttle window from the ban payload.
    let statuses = gateway.statuses().expect("statuses should succeed");
    let throttled_until_ms = statuses[0]
        .resilience_state
        .throttled_until_ms
        .expect("account should be throttled after a rate-limit response");
    assert!(
        (throttled_until_ms - banned_until_ms).abs() < 5_000,
        "throttle window should match the ban expiry: \
         throttled_until_ms={throttled_until_ms} banned_until_ms={banned_until_ms}"
    );

    // Subsequent fetches must fail fast without hitting the provider again.
    let error = gateway
        .fetch_latest_bar("binance-demo", "ETHUSDT", Timeframe::M1)
        .expect_err("throttled fetch should fail");
    assert!(
        matches!(error, GatewayError::ConnectorNotReady { .. }),
        "throttled fetch should fail readiness, got: {error}"
    );
    assert_eq!(
        hits.load(Ordering::Relaxed),
        1,
        "throttled account must not issue further provider requests"
    );
}

#[test]
fn expired_throttle_window_allows_fetches_again() {
    let (base_url, hits) = spawn_banning_server(unix_now_ms() + 60_000);
    let registry = build_connector_registry(&[AccountConfig {
        id: "binance-demo".to_owned(),
        kind: "binance".to_owned(),
        mode: ExecutionMode::Paper,
        api_key_env: Some("PATH".to_owned()),
        api_secret_env: Some("PATH".to_owned()),
        passphrase_env: None,
        use_demo_mode: true,
        reconciliation_remote_snapshot: true,
        execution_remote_submission: None,
        reconciliation_base_url: Some(base_url),
        cash_balance_assets: Vec::new(),
        total_budget_usd: 10_000.0,
    }])
    .expect("registry should build");

    // Simulate a throttle window that has already expired.
    registry
        .note_account_rate_limit("binance-demo", unix_now_ms() - 10_000, 1_000)
        .expect("note rate limit should succeed");
    let gateway = Gateway::new(Arc::new(Mutex::new(registry)));

    // The expired window must not gate the fetch: the request reaches the
    // provider again.
    let _ = gateway.fetch_latest_bar("binance-demo", "BTCUSDT", Timeframe::M1);
    assert_eq!(
        hits.load(Ordering::Relaxed),
        1,
        "an expired throttle window must not gate provider requests"
    );
}
