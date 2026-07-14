use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
    GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
    ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode,
    StorageConfig,
};
use openticker_core::{
    ExecutionMode, IndicatorSignal, IndicatorSignalMetadataFilters, MarketType, Timeframe,
    TradeIntent,
};
use openticker_runtime::Runtime;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use toml::Table;

mod common;

use common::{create_temp_db_path, request_path};

#[test]
fn restart_does_not_duplicate_existing_order_chain() {
    let db_path = create_temp_db_path("restart-order-chain");
    let (base_url, handle) = spawn_mock_alpaca_snapshot_server_with_position();
    let config = fixture_bundle_with_storage(db_path, Some(base_url));
    let timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let mut runtime = Runtime::from_config_with_storage(&config).unwrap();
    runtime.start_instance("aapl").unwrap();
    let outcome = runtime
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 123.45, timestamp)
        .unwrap();
    assert_eq!(outcome.intent, TradeIntent::OpenLong);
    assert_eq!(runtime.recent_orders(20).unwrap().len(), 1);
    assert_eq!(runtime.recent_fills(20).unwrap().len(), 1);
    drop(runtime);

    let restarted = Runtime::from_config_with_storage(&config).unwrap();
    assert_eq!(restarted.recent_orders(20).unwrap().len(), 1);
    assert_eq!(restarted.recent_fills(20).unwrap().len(), 1);

    let _paths = handle.join().unwrap();
}

#[test]
fn restart_preserves_position_and_does_not_reopen_existing_exposure() {
    let db_path = create_temp_db_path("restart-position");
    let (base_url, handle) = spawn_mock_alpaca_snapshot_server_with_position();
    let config = fixture_bundle_with_storage(db_path, Some(base_url));
    let timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let mut runtime = Runtime::from_config_with_storage(&config).unwrap();
    runtime.start_instance("aapl").unwrap();
    runtime
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 123.45, timestamp)
        .unwrap();
    let expected_position = runtime
        .recent_positions(20)
        .unwrap()
        .into_iter()
        .rfind(|position| position.bot_id == "aapl")
        .unwrap();
    drop(runtime);

    let mut restarted = Runtime::from_config_with_storage(&config).unwrap();
    let latest_position = restarted
        .recent_positions(20)
        .unwrap()
        .into_iter()
        .rfind(|position| position.bot_id == "aapl")
        .unwrap();
    assert!(latest_position.has_position);
    assert!((latest_position.quantity - expected_position.quantity).abs() < f64::EPSILON);
    assert_eq!(latest_position.entry_price, expected_position.entry_price);

    let orders_before = restarted.recent_orders(20).unwrap().len();
    let fills_before = restarted.recent_fills(20).unwrap().len();
    let duplicate = restarted
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 123.45, timestamp)
        .unwrap();

    assert_eq!(duplicate.intent, TradeIntent::NoOp);
    assert_eq!(restarted.recent_orders(20).unwrap().len(), orders_before);
    assert_eq!(restarted.recent_fills(20).unwrap().len(), fills_before);

    let _paths = handle.join().unwrap();
}

fn spawn_mock_alpaca_snapshot_server_with_position() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let mut paths = Vec::new();
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            let path = request_path(&request);
            paths.push(path.clone());

            let (status, body) = if path.starts_with("/v2/orders") {
                ("HTTP/1.1 200 OK", "[]")
            } else if path == "/v2/positions" {
                ("HTTP/1.1 200 OK", r#"[{"symbol":"AAPL","qty":"1"}]"#)
            } else if path == "/v2/account" {
                ("HTTP/1.1 200 OK", r#"{"cash":"1000.0","equity":"1123.45"}"#)
            } else {
                ("HTTP/1.1 404 Not Found", "{\"error\":\"not found\"}")
            };
            write_http_response(&mut stream, status, body);
        }
        paths
    });

    (format!("http://{address}"), handle)
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buffer = [0_u8; 1024];
    let mut data = Vec::new();
    loop {
        let count = stream.read(&mut buffer).unwrap();
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

fn write_http_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "{status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}

fn fixture_bundle_with_storage(
    path: PathBuf,
    reconciliation_base_url: Option<String>,
) -> ConfigBundle {
    ConfigBundle {
        global: GlobalConfig {
            service: ServiceConfig {
                environment: "test".to_owned(),
                data_dir: "./var".into(),
                bot_dir: "./config/bots".into(),
            },
            http: HttpConfig {
                enabled: true,
                bind: "127.0.0.1:8080".to_owned(),
                request_log: true,
                openapi_enabled: true,
                openapi_path: "/openapi.json".to_owned(),
            },
            storage: StorageConfig {
                kind: "sqlite".to_owned(),
                path,
                busy_timeout_ms: 5_000,
                prune_removed_bots_on_startup: false,
            },
            observability: ObservabilityConfig {
                log_level: "info".to_owned(),
                metrics_enabled: true,
                metrics_path: "/metrics".to_owned(),
            },
            safety: SafetyConfig {
                require_explicit_live_enable: true,
                default_start_paused_if_recovery_uncertain: true,
            },
            data_plane: DataPlaneConfig::default(),
        },
        accounts: vec![AccountConfig {
            id: "alpaca-paper".to_owned(),
            kind: "alpaca".to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: Some("PATH".to_owned()),
            passphrase_env: None,
            use_demo_mode: false,
            reconciliation_remote_snapshot: true,
            execution_remote_submission: Some(false),
            reconciliation_base_url,
            cash_balance_assets: Vec::new(),
            total_budget_usd: 1_000.0,
        }],
        risk_profiles: vec![RiskProfileConfig {
            id: "equities-default".to_owned(),
            max_daily_loss_pct: 2.0,
            max_open_positions: 2,
            target_order_notional_usd: Some(1_000.0),
            max_order_notional_usd: 1_000.0,
            max_spread_bps: 20,
            max_slippage_bps: 30,
            stale_data_ms: 3_000,
            cooldown_after_reject_ms: 1_000,
        }],
        instances: vec![InstanceConfig {
            id: "aapl".to_owned(),
            enabled: true,
            market: MarketType::Equities,
            symbols: vec!["AAPL".to_owned()],
            timeframe: Timeframe::M1,
            account: "alpaca-paper".to_owned(),
            data_connector: "alpaca".to_owned(),
            execution_connector: "alpaca".to_owned(),
            strategy: "single_indicator_signal".to_owned(),
            signal_mode: SignalMode::ConfirmedOnly,
            polling_enabled: true,
            polling_interval_ms: 1_000,
            indicators: vec![IndicatorInstanceConfig {
                id: "trend-1".to_owned(),
                indicator_type: "sma_crossover".to_owned(),
                enabled: true,
                role: None,
                signal_policy: None,
                weight: None,
                metadata_filters: IndicatorSignalMetadataFilters::default(),
                params: Table::new(),
            }],
            execution_constraints: ExecutionConstraintsConfig::default(),
            budget: BudgetConfig { pct: 100.0 },
            risk: InstanceRiskConfig {
                profile: "equities-default".to_owned(),
                overrides: RiskOverrides::default(),
            },
            warmup_target_bars: None,
            allow_live: false,
        }],
    }
}
