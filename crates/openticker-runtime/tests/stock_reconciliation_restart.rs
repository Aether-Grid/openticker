use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
    GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
    ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode,
    StorageConfig,
};
use openticker_core::{ExecutionMode, IndicatorSignalMetadataFilters, MarketType, Timeframe};
use openticker_runtime::{LaneRuntimeState, Runtime, ServiceError};
use openticker_storage::{BotSnapshotWrite, PositionWrite, RuntimeJournal, SqliteRuntimeJournal};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Table;

#[test]
fn startup_reconciliation_uses_remote_stock_connector_snapshot() {
    let (base_url, handle) = spawn_mock_alpaca_snapshot_server();
    let db_path = create_temp_db_path("reconcile-remote-ok");
    let config = fixture_bundle_with_reconciliation(db_path.clone(), true, Some(base_url));
    seed_running_snapshot(
        &db_path,
        config.global.storage.busy_timeout_ms,
        ExecutionMode::Paper,
    );

    let runtime = Runtime::from_config_with_storage(&config).unwrap();
    let summary = runtime.get_instance("aapl").unwrap();
    let report = runtime.reconciliation_report("aapl").unwrap();
    let latest = report
        .latest
        .clone()
        .expect("startup reconciliation should emit a latest record");
    assert_eq!(summary.state, LaneRuntimeState::Running);
    assert!(
        !summary.reconciliation_blocked,
        "blocked startup reconciliation: reason={}, differences={:?}",
        latest.reason, latest.differences
    );

    assert_eq!(latest.source, "startup");
    assert!(latest.safe_to_trade);
    assert!(latest.differences.is_empty());

    let paths = handle.join().unwrap();
    assert!(paths.iter().any(|path| path.starts_with("/v2/orders")));
    assert!(paths.iter().any(|path| path.starts_with("/v2/positions")));
    assert!(paths.iter().any(|path| path == "/v2/account"));
}

#[test]
fn startup_reconciliation_blocks_when_remote_stock_snapshot_fails() {
    let db_path = create_temp_db_path("reconcile-remote-fail");
    let config = fixture_bundle_with_reconciliation(
        db_path.clone(),
        true,
        Some("http://127.0.0.1:9".to_owned()),
    );
    seed_running_snapshot(
        &db_path,
        config.global.storage.busy_timeout_ms,
        ExecutionMode::Paper,
    );

    let mut runtime = Runtime::from_config_with_storage(&config).unwrap();
    let summary = runtime.get_instance("aapl").unwrap();
    assert_eq!(summary.state, LaneRuntimeState::Paused);
    assert!(summary.reconciliation_blocked);

    let report = runtime.reconciliation_report("aapl").unwrap();
    let latest = report.latest.unwrap();
    assert_eq!(latest.source, "startup");
    assert!(!latest.safe_to_trade);
    assert!(
        latest
            .differences
            .iter()
            .any(|difference| difference.starts_with("connector_snapshot_unavailable("))
    );

    let start_result = runtime.start_instance("aapl");
    assert!(matches!(
        start_result,
        Err(ServiceError::ReconciliationRequired { .. })
    ));
}

#[test]
fn startup_reconciliation_does_not_backfill_external_remote_stock_open_orders() {
    let (base_url, handle) = spawn_mock_alpaca_snapshot_server_with_bodies(
        r#"[{"client_order_id":"remote-open-1","symbol":"AAPL","status":"new","qty":"2"}]"#,
        "[]",
        // Startup budget refresh plus reconciliation.
        6,
    );
    let db_path = create_temp_db_path("reconcile-remote-open-order-backfill");
    let config = fixture_bundle_with_reconciliation(db_path.clone(), true, Some(base_url));
    seed_running_snapshot(
        &db_path,
        config.global.storage.busy_timeout_ms,
        ExecutionMode::Paper,
    );

    let runtime = Runtime::from_config_with_storage(&config).unwrap();
    let summary = runtime.get_instance("aapl").unwrap();
    let report = runtime.reconciliation_report("aapl").unwrap();
    let latest = report
        .latest
        .clone()
        .expect("startup reconciliation should emit a latest record");
    assert_eq!(summary.state, LaneRuntimeState::Running);
    assert!(
        !summary.reconciliation_blocked,
        "blocked startup reconciliation: reason={}, differences={:?}",
        latest.reason, latest.differences
    );

    assert_eq!(latest.source, "startup");
    assert!(latest.safe_to_trade);
    assert!(
        latest.reason.contains("external_remote_position_surplus")
            || latest.reason == "state_aligned"
            || latest.reason.contains("external_position_surplus")
    );

    let orders = runtime.recent_orders(20).unwrap();
    assert!(
        !orders
            .iter()
            .any(|order| order.client_order_id == "remote-open-1")
    );

    let paths = handle.join().unwrap();
    assert!(paths.iter().any(|path| path.starts_with("/v2/orders")));
    assert!(paths.iter().any(|path| path.starts_with("/v2/positions")));
    assert!(paths.iter().any(|path| path == "/v2/account"));
}

#[test]
fn startup_reconciliation_auto_resumes_recovered_live_stock_instance() {
    let (base_url, handle) = spawn_mock_alpaca_snapshot_server();
    let db_path = create_temp_db_path("reconcile-remote-live-ok");
    let mut config = fixture_bundle_with_reconciliation(db_path.clone(), true, Some(base_url));
    config.accounts[0].mode = ExecutionMode::Live;
    config.instances[0].allow_live = true;
    seed_running_snapshot(
        &db_path,
        config.global.storage.busy_timeout_ms,
        ExecutionMode::Live,
    );

    let runtime = Runtime::from_config_with_storage(&config).unwrap();
    let summary = runtime.get_instance("aapl").unwrap();
    let report = runtime.reconciliation_report("aapl").unwrap();
    let latest = report
        .latest
        .clone()
        .expect("startup reconciliation should emit a latest record");
    assert_eq!(summary.state, LaneRuntimeState::Running);
    assert!(
        !summary.reconciliation_blocked,
        "blocked startup reconciliation: reason={}, differences={:?}",
        latest.reason, latest.differences
    );

    assert_eq!(latest.source, "startup");
    assert!(latest.safe_to_trade);
    assert!(latest.differences.is_empty());

    let paths = handle.join().unwrap();
    assert!(paths.iter().any(|path| path.starts_with("/v2/orders")));
    assert!(paths.iter().any(|path| path.starts_with("/v2/positions")));
    assert!(paths.iter().any(|path| path == "/v2/account"));
}

#[test]
fn startup_reconciliation_preserves_unique_owner_for_shared_symbol_bots() {
    let (base_url, handle) =
        spawn_mock_alpaca_snapshot_server_with_bodies("[]", r#"[{"symbol":"AAPL","qty":"1"}]"#, 12);
    let db_path = create_temp_db_path("reconcile-shared-symbol-owner");
    let mut config = fixture_bundle_with_reconciliation(db_path.clone(), true, Some(base_url));
    config.instances[0].budget.pct = 50.0;
    let mut second = config.instances[0].clone();
    second.id = "aapl-5m".to_owned();
    second.timeframe = Timeframe::M5;
    second.indicators[0].id = "trend-2".to_owned();
    second.budget.pct = 50.0;
    config.instances.push(second);

    seed_running_snapshot_for(
        &db_path,
        config.global.storage.busy_timeout_ms,
        "aapl",
        ExecutionMode::Paper,
    );
    seed_running_snapshot_for(
        &db_path,
        config.global.storage.busy_timeout_ms,
        "aapl-5m",
        ExecutionMode::Paper,
    );

    let journal =
        SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms).unwrap();
    journal
        .append_position(PositionWrite {
            bot_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: None,
            has_position: true,
            quantity: 1.0,
            entry_price: Some(123.45),
            realized_pnl_usd: 15.25,
            reason: "order_filled".to_owned(),
        })
        .unwrap();
    journal
        .append_position(PositionWrite {
            bot_id: "aapl-5m".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: None,
            has_position: true,
            quantity: 1.0,
            entry_price: None,
            realized_pnl_usd: 0.0,
            reason: "startup_reconciliation_sync".to_owned(),
        })
        .unwrap();

    let runtime = Runtime::from_config_with_storage(&config).unwrap();

    let owner = runtime.get_instance("aapl").unwrap();
    assert_eq!(owner.state, LaneRuntimeState::Running);
    assert!(!owner.reconciliation_blocked);
    assert!(owner.position.has_position);
    assert!((owner.position.quantity - 1.0).abs() < f64::EPSILON);
    assert_eq!(owner.position.entry_price, Some(123.45));

    let other = runtime.get_instance("aapl-5m").unwrap();
    assert_eq!(other.state, LaneRuntimeState::Running);
    assert!(!other.reconciliation_blocked);
    assert!(!other.position.has_position);
    assert!(other.position.quantity.abs() < f64::EPSILON);
    assert!(other.position.entry_price.is_none());

    let other_latest_position = journal.latest_position_for_bot("aapl-5m").unwrap().unwrap();
    assert!(!other_latest_position.has_position);
    assert!(other_latest_position.quantity.abs() < f64::EPSILON);
    assert!(other_latest_position.entry_price.is_none());
    assert_eq!(
        other_latest_position.reason,
        "startup_managed_state_refresh"
    );

    let ledger = runtime.ledger_snapshot();
    let account_ledger = ledger
        .accounts
        .iter()
        .find(|account| account.id == "alpaca-paper")
        .unwrap();
    assert!(account_ledger.unattributed_open_notional_usd.abs() < f64::EPSILON);

    let owner_ledger = ledger.bots.iter().find(|bot| bot.id == "aapl").unwrap();
    assert!(owner_ledger.attributed_open_notional_usd > 100.0);

    let other_ledger = ledger.bots.iter().find(|bot| bot.id == "aapl-5m").unwrap();
    assert!(other_ledger.attributed_open_notional_usd.abs() < f64::EPSILON);

    let paths = handle.join().unwrap();
    assert!(paths.iter().any(|path| path.starts_with("/v2/orders")));
    assert!(paths.iter().any(|path| path.starts_with("/v2/positions")));
    assert!(paths.iter().any(|path| path == "/v2/account"));
}

fn seed_running_snapshot(db_path: &PathBuf, busy_timeout_ms: u64, execution_mode: ExecutionMode) {
    seed_running_snapshot_for(db_path, busy_timeout_ms, "aapl", execution_mode);
}

fn seed_running_snapshot_for(
    db_path: &PathBuf,
    busy_timeout_ms: u64,
    instance_id: &str,
    execution_mode: ExecutionMode,
) {
    let journal = SqliteRuntimeJournal::open(db_path, busy_timeout_ms).unwrap();
    journal
        .upsert_bot_snapshot(BotSnapshotWrite {
            bot_id: instance_id.to_owned(),
            state: "running".to_owned(),
            execution_mode: match execution_mode {
                ExecutionMode::Paper => "paper".to_owned(),
                ExecutionMode::Live => "live".to_owned(),
            },
            enabled: true,
        })
        .unwrap();
}

fn spawn_mock_alpaca_snapshot_server() -> (String, thread::JoinHandle<Vec<String>>) {
    // Startup now fetches one remote snapshot before reconciliation.
    spawn_mock_alpaca_snapshot_server_with_bodies("[]", "[]", 6)
}

fn spawn_mock_alpaca_snapshot_server_with_bodies(
    orders_body: &str,
    positions_body: &str,
    request_count: usize,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let orders_body = orders_body.to_owned();
    let positions_body = positions_body.to_owned();

    let handle = thread::spawn(move || {
        let mut paths = Vec::new();
        for _ in 0..request_count {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            let path = request_path(&request);
            paths.push(path.clone());

            let (status, body) = if path.starts_with("/v2/orders") {
                ("HTTP/1.1 200 OK", orders_body.as_str())
            } else if path == "/v2/positions" {
                ("HTTP/1.1 200 OK", positions_body.as_str())
            } else if path == "/v2/account" {
                ("HTTP/1.1 200 OK", r#"{"cash":"1000.0","equity":"1000.0"}"#)
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

fn request_path(request: &str) -> String {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned()
}

fn write_http_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "{status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}

fn fixture_bundle_with_reconciliation(
    db_path: PathBuf,
    reconciliation_remote_snapshot: bool,
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
                path: db_path,
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
            reconciliation_remote_snapshot,
            execution_remote_submission: None,
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

fn create_temp_db_path(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-runtime-{prefix}-{timestamp}.db"))
}
