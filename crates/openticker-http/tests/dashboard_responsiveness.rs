//! Regression test: the dashboard must stay responsive while the polling
//! supervisor fetches market data for many tickers from a slow connector.
//!
//! Reproduces the production symptom where adding tickers in config/ made
//! dashboard endpoints slow down superlinearly: per-ticker fetches serialized
//! on the connector-registry lock and blocked tokio workers, so status and
//! snapshot requests queued behind connector network I/O.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use openticker_config::AccountConfig;
use openticker_core::{ExecutionMode, MarketType};
use openticker_http::{HttpState, build_router};
use openticker_runtime::{Runtime, RuntimePollingSupervisor};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tower::util::ServiceExt;

const TICKER_COUNT: usize = 32;
const FETCH_LATENCY: Duration = Duration::from_millis(150);

fn unix_now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_millis(),
    )
    .expect("timestamp should fit in i64")
}

/// Minimal HTTP server answering every request with one confirmed Binance
/// kline row after `latency` elapses.
fn spawn_slow_kline_server(latency: Duration) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
    let address = listener.local_addr().expect("local addr");
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let server_hits = Arc::clone(&hits);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            server_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

    (format!("http://{address}"), hits)
}

fn slow_connector_bundle(base_url: String) -> openticker_config::ConfigBundle {
    let mut bundle = common::fixture_bundle();
    bundle.accounts.push(AccountConfig {
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
        total_budget_usd: 100_000.0,
    });

    let base = bundle.instances[0].clone();
    bundle.instances.clear();
    for idx in 0..TICKER_COUNT {
        let mut instance = base.clone();
        instance.id = format!("crypto-bot-{idx:04}");
        instance.market = MarketType::Crypto;
        instance.symbols = vec![format!("SYM{idx:04}USDT")];
        "binance-demo".clone_into(&mut instance.account);
        "binance".clone_into(&mut instance.data_connector);
        "binance".clone_into(&mut instance.execution_connector);
        instance.polling_interval_ms = 250;
        bundle.instances.push(instance);
    }
    bundle
}

async fn measure_latency(router: axum::Router, path: &str, samples: usize) -> Vec<Duration> {
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let response = router
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .expect("request should complete");
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        durations.push(start.elapsed());
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    durations.sort();
    durations
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dashboard_stays_responsive_while_polling_many_slow_tickers() {
    let (base_url, fetch_hits) = spawn_slow_kline_server(FETCH_LATENCY);
    let bundle = slow_connector_bundle(base_url);

    let runtime = Arc::new(RwLock::new(Runtime::from_config(&bundle)));
    let streams = runtime.read().await.effective_streams_for_dataplane();
    let data_plane = Arc::new(openticker_dataplane::DataPlane::new(streams));
    let state = HttpState::new_from_parts(Arc::clone(&runtime), Arc::clone(&data_plane));
    let router = build_router(state);
    let supervisor = RuntimePollingSupervisor::start(&runtime, data_plane);

    // Let the polling supervisor run a few cycles so per-ticker fetches are
    // continuously in flight.
    tokio::time::sleep(Duration::from_millis(1_200)).await;

    let status = measure_latency(router.clone(), "/v1/service/status", 10).await;
    let snapshot = measure_latency(router.clone(), "/v1/dashboard/snapshot?limit=25", 10).await;

    supervisor.shutdown().await;

    let status_p95 = status[status.len() - 1];
    let snapshot_p95 = snapshot[snapshot.len() - 1];
    let total_fetch_hits = fetch_hits.load(std::sync::atomic::Ordering::Relaxed);
    println!(
        "status worst={status_p95:?} snapshot worst={snapshot_p95:?} fetch_hits={total_fetch_hits}"
    );

    assert!(
        total_fetch_hits >= TICKER_COUNT,
        "polling supervisor produced no per-ticker connector traffic (hits={total_fetch_hits}); \
         the regression scenario did not reproduce"
    );

    // With {TICKER_COUNT} tickers and {FETCH_LATENCY} connector latency, the
    // pre-fix behavior serialized fetches on the registry lock and blocked
    // tokio workers, pushing these endpoints to multi-second latencies.
    assert!(
        status_p95 < Duration::from_millis(250),
        "service status blocked behind connector polling: worst={status_p95:?}"
    );
    assert!(
        snapshot_p95 < Duration::from_millis(250),
        "dashboard snapshot blocked behind connector polling: worst={snapshot_p95:?}"
    );
}
