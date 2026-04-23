use openticker_core::{ExecutionMode, MarketType};
use openticker_dataplane::DataPlane;
use openticker_runtime::{Runtime, RuntimePollingSupervisor};
use std::convert::TryFrom;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reader_not_starved_during_polling_cycle() {
    let (base_url, kline_started) = spawn_slow_binance_server(Duration::from_millis(150));
    let mut bundle = openticker_testkit::shared_fixture_bundle_for_symbol(
        "btcusdt",
        "BTCUSDT",
        openticker_core::Timeframe::M1,
    );
    bundle.accounts[0].id = "binance-paper".to_owned();
    bundle.accounts[0].kind = "binance".to_owned();
    bundle.accounts[0].mode = ExecutionMode::Paper;
    bundle.accounts[0].api_key_env = Some("PATH".to_owned());
    bundle.accounts[0].api_secret_env = Some("PATH".to_owned());
    bundle.accounts[0].use_demo_mode = true;
    bundle.accounts[0].reconciliation_remote_snapshot = true;
    bundle.accounts[0].reconciliation_base_url = Some(base_url);
    bundle.accounts[0].cash_balance_assets = vec!["USDT".to_owned()];
    bundle.instances[0].market = MarketType::Crypto;
    bundle.instances[0].account = "binance-paper".to_owned();
    bundle.instances[0].data_connector = "binance".to_owned();
    bundle.instances[0].execution_connector = "binance".to_owned();
    bundle.instances[0].polling_interval_ms = 1_000;

    let runtime = Arc::new(RwLock::new(Runtime::from_config(&bundle)));
    runtime
        .write()
        .await
        .start_instance("btcusdt")
        .expect("instance should start");
    let streams = runtime.read().await.effective_streams_for_dataplane();
    let data_plane = Arc::new(DataPlane::new(streams));
    let supervisor = RuntimePollingSupervisor::start(&runtime, data_plane);

    let wait_deadline = Instant::now() + Duration::from_secs(2);
    while !kline_started.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < wait_deadline,
            "slow kline request did not start"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let start = Instant::now();
    let guard = runtime.read().await;
    let elapsed = start.elapsed();
    drop(guard);
    supervisor.shutdown().await;

    assert!(
        elapsed < Duration::from_millis(100),
        "reader waited {elapsed:?}; supervisor is holding the write lock across connector I/O"
    );
}

fn spawn_slow_binance_server(delay: Duration) -> (String, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock server should bind");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let kline_started = Arc::new(AtomicBool::new(false));
    let kline_started_for_thread = Arc::clone(&kline_started);

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                break;
            };
            handle_binance_request(stream, delay, &kline_started_for_thread);
        }
    });

    (base_url, kline_started)
}

fn handle_binance_request(mut stream: TcpStream, delay: Duration, kline_started: &AtomicBool) {
    let mut buffer = [0u8; 4096];
    let Ok(read) = stream.read(&mut buffer) else {
        return;
    };
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let body = if path.starts_with("/api/v3/klines") {
        kline_started.store(true, Ordering::SeqCst);
        thread::sleep(delay);
        kline_rows_json()
    } else if path.starts_with("/api/v3/openOrders") {
        "[]".to_owned()
    } else if path.starts_with("/api/v3/account") {
        r#"{"balances":[{"asset":"USDT","free":"20000","locked":"0"}]}"#.to_owned()
    } else if path.starts_with("/api/v3/exchangeInfo") {
        r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","filters":[{"filterType":"LOT_SIZE","minQty":"0.0001","stepSize":"0.0001"},{"filterType":"MIN_NOTIONAL","minNotional":"10"}]}]}"#.to_owned()
    } else {
        "{}".to_owned()
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn kline_rows_json() -> String {
    let now_ms = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_millis(),
    )
    .expect("clock should fit in i64");
    let first_open = now_ms.saturating_sub(180_000);
    let second_open = now_ms.saturating_sub(120_000);
    format!(
        r#"[[{first_open},"100","101","99","100","1",{}],[{second_open},"101","102","100","101","1",{}]]"#,
        first_open + 59_999,
        second_open + 59_999
    )
}
