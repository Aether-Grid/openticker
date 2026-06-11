//! Regression tests for connector-registry lock contention.
//!
//! The gateway must not hold the registry lock across network round-trips:
//! doing so serializes every per-ticker fetch and blocks status queries
//! (the dashboard) behind in-flight connector I/O.

use openticker_config::AccountConfig;
use openticker_core::{ExecutionMode, Timeframe};
use openticker_gateway::{Gateway, build_connector_registry};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn unix_now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_millis(),
    )
    .expect("timestamp should fit in i64")
}

/// Starts a minimal HTTP server that answers every request with a single
/// confirmed Binance kline row after `latency` elapses.
fn spawn_slow_kline_server(latency: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
    let address = listener.local_addr().expect("local addr");

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            std::thread::spawn(move || {
                let mut reader =
                    BufReader::new(stream.try_clone().expect("clone test server stream"));
                let mut line = String::new();
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                }

                std::thread::sleep(latency);

                let now_ms = unix_now_ms();
                let timeframe_ms = 60_000;
                let open_time = now_ms - now_ms.rem_euclid(timeframe_ms) - timeframe_ms;
                let close_time = open_time + timeframe_ms - 1;
                let body = format!(
                    "[[{open_time},\"100.0\",\"101.0\",\"99.0\",\"100.5\",\"1000.0\",{close_time}]]"
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });

    format!("http://{address}")
}

fn slow_binance_account(base_url: String) -> AccountConfig {
    AccountConfig {
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
    }
}

fn slow_gateway(latency: Duration) -> Gateway {
    let base_url = spawn_slow_kline_server(latency);
    let registry =
        build_connector_registry(&[slow_binance_account(base_url)]).expect("registry should build");
    Gateway::new(Arc::new(Mutex::new(registry)))
}

#[test]
fn concurrent_fetches_do_not_serialize_on_the_registry_lock() {
    const FETCH_LATENCY: Duration = Duration::from_millis(150);
    const CONCURRENCY: usize = 8;

    let gateway = slow_gateway(FETCH_LATENCY);

    let started_at = Instant::now();
    let handles = (0..CONCURRENCY)
        .map(|worker| {
            let gateway = gateway.clone();
            std::thread::spawn(move || {
                gateway.fetch_latest_bar("binance-demo", &format!("SYM{worker}USDT"), Timeframe::M1)
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle
            .join()
            .expect("fetch thread should not panic")
            .expect("fetch should succeed");
    }
    let elapsed = started_at.elapsed();

    // Serialized fetches would take CONCURRENCY * FETCH_LATENCY (1.2s).
    assert!(
        elapsed < FETCH_LATENCY * 4,
        "concurrent fetches serialized on the registry lock: {elapsed:?}"
    );
}

#[test]
fn statuses_are_not_blocked_by_in_flight_fetches() {
    const FETCH_LATENCY: Duration = Duration::from_millis(500);

    let gateway = slow_gateway(FETCH_LATENCY);

    let fetch_gateway = gateway.clone();
    let fetch = std::thread::spawn(move || {
        fetch_gateway.fetch_latest_bar("binance-demo", "BTCUSDT", Timeframe::M1)
    });

    // Give the fetch time to acquire whatever locks it holds during the
    // network round-trip.
    std::thread::sleep(Duration::from_millis(100));

    let status_started_at = Instant::now();
    let statuses = gateway.statuses().expect("statuses should succeed");
    let status_elapsed = status_started_at.elapsed();

    fetch
        .join()
        .expect("fetch thread should not panic")
        .expect("fetch should succeed");

    assert_eq!(statuses.len(), 1);
    assert!(
        status_elapsed < Duration::from_millis(100),
        "statuses blocked behind an in-flight fetch: {status_elapsed:?}"
    );
}
