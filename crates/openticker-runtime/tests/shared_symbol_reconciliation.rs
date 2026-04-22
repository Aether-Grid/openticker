use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
    GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
    ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode,
    StorageConfig,
};
use openticker_core::{ExecutionMode, IndicatorSignalMetadataFilters, MarketType, Timeframe};
use openticker_runtime::{LaneRuntimeState, Runtime};
use openticker_storage::{
    BotSnapshotWrite, OrderWrite, PositionWrite, RuntimeJournal, SqliteRuntimeJournal,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use toml::Table;

const EPSILON: f64 = 1e-6;

#[test]
fn shared_symbol_reconciliation_keeps_single_owner_after_restart() {
    let responses = MockAlpacaResponses::new(
        "[]",
        r#"[{"symbol":"AAPL","qty":"1"}]"#,
        r#"{"cash":"1000.0","equity":"1000.0"}"#,
    );
    let server = MockAlpacaServer::spawn(responses);
    let config = fixture_bundle_with_shared_symbol_reconciliation(
        create_temp_db_path("runtime-shared-symbol-restart"),
        server.base_url.clone(),
    );
    let db_path = config.global.storage.path.clone();
    let busy_timeout_ms = config.global.storage.busy_timeout_ms;

    seed_running_snapshot(&db_path, busy_timeout_ms, "aapl");
    seed_running_snapshot(&db_path, busy_timeout_ms, "aapl-5m");
    seed_shared_symbol_positions(&db_path, busy_timeout_ms);

    {
        let runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
        assert_shared_symbol_single_owner(&runtime);
    }
    let counts_after_first_boot = position_row_counts(&db_path, busy_timeout_ms);

    {
        let restarted =
            Runtime::from_config_with_storage(&config).expect("runtime should restart cleanly");
        assert_shared_symbol_single_owner(&restarted);
    }
    let counts_after_second_boot = position_row_counts(&db_path, busy_timeout_ms);

    assert_eq!(counts_after_second_boot, counts_after_first_boot);

    let paths = server.shutdown();
    assert!(paths.iter().any(|path| path.starts_with("/v2/orders")));
    assert!(paths.iter().any(|path| path == "/v2/positions"));
    assert!(paths.iter().any(|path| path == "/v2/account"));
}

#[test]
fn shared_symbol_reconciliation_maps_managed_remote_order_to_exactly_one_bot() {
    let responses = MockAlpacaResponses::new(
        r#"[{"client_order_id":"alpaca-managed-order","symbol":"AAPL","status":"new","qty":"2"}]"#,
        "[]",
        r#"{"cash":"1000.0","equity":"1000.0"}"#,
    );
    let server = MockAlpacaServer::spawn(responses);
    let config = fixture_bundle_with_shared_symbol_reconciliation(
        create_temp_db_path("runtime-shared-symbol-order-attribution"),
        server.base_url.clone(),
    );
    let db_path = config.global.storage.path.clone();
    let busy_timeout_ms = config.global.storage.busy_timeout_ms;

    seed_running_snapshot(&db_path, busy_timeout_ms, "aapl");
    seed_running_snapshot(&db_path, busy_timeout_ms, "aapl-5m");
    seed_managed_open_order(&db_path, busy_timeout_ms, "aapl", "alpaca-managed-order");

    let runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");

    let owner_report = runtime
        .reconciliation_report("aapl")
        .expect("owner report should load");
    let owner_latest = owner_report
        .latest
        .expect("owner latest reconciliation should exist");
    assert_eq!(owner_latest.connector_open_orders, 1);
    assert!(owner_latest.safe_to_trade);

    let secondary_report = runtime
        .reconciliation_report("aapl-5m")
        .expect("secondary report should load");
    let secondary_latest = secondary_report
        .latest
        .expect("secondary latest reconciliation should exist");
    assert_eq!(secondary_latest.connector_open_orders, 0);
    assert!(secondary_latest.safe_to_trade);

    let journal =
        SqliteRuntimeJournal::open(&db_path, busy_timeout_ms).expect("journal should open");
    let matching_orders = journal
        .orders_by_client_order_id("alpaca-managed-order")
        .expect("orders by client order id should load");
    assert_eq!(matching_orders.len(), 1);
    assert_eq!(matching_orders[0].bot_id, "aapl");

    let secondary_orders = journal
        .recent_orders_for_bot("aapl-5m", 20)
        .expect("secondary orders should load");
    assert!(
        !secondary_orders
            .iter()
            .any(|order| order.client_order_id == "alpaca-managed-order")
    );

    let paths = server.shutdown();
    assert!(paths.iter().any(|path| path.starts_with("/v2/orders")));
    assert!(paths.iter().any(|path| path == "/v2/positions"));
    assert!(paths.iter().any(|path| path == "/v2/account"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PositionRowCounts {
    owner_rows: usize,
    secondary_rows: usize,
}

fn assert_shared_symbol_single_owner(runtime: &Runtime) {
    let owner = runtime.get_instance("aapl").expect("aapl should exist");
    assert_eq!(owner.state, LaneRuntimeState::Running);
    assert!(!owner.reconciliation_blocked);
    assert!(owner.position.has_position);
    assert!((owner.position.quantity - 1.0).abs() < EPSILON);
    assert_eq!(owner.position.entry_price, Some(123.45));

    let secondary = runtime
        .get_instance("aapl-5m")
        .expect("aapl-5m should exist");
    assert_eq!(secondary.state, LaneRuntimeState::Running);
    assert!(!secondary.reconciliation_blocked);
    assert!(!secondary.position.has_position);
    assert!(secondary.position.quantity.abs() < EPSILON);
    assert!(secondary.position.entry_price.is_none());

    let ledger = runtime.ledger_snapshot();
    let account = ledger
        .accounts
        .iter()
        .find(|account| account.id == "alpaca-paper")
        .expect("account ledger row should exist");
    assert!(
        !account
            .exceptions
            .iter()
            .any(|exception| exception.symbol.as_deref() == Some("AAPL"))
    );

    let owner_ledger = ledger
        .bots
        .iter()
        .find(|bot| bot.id == "aapl")
        .expect("owner bot ledger row should exist");
    assert!(owner_ledger.attributed_open_notional_usd > 100.0);

    let secondary_ledger = ledger
        .bots
        .iter()
        .find(|bot| bot.id == "aapl-5m")
        .expect("secondary bot ledger row should exist");
    assert!(secondary_ledger.attributed_open_notional_usd.abs() < EPSILON);
}

fn position_row_counts(db_path: &PathBuf, busy_timeout_ms: u64) -> PositionRowCounts {
    let journal =
        SqliteRuntimeJournal::open(db_path, busy_timeout_ms).expect("sqlite journal should open");

    let owner_rows = journal
        .recent_positions_for_bot("aapl", 20)
        .expect("owner positions should load")
        .len();
    let secondary_rows = journal
        .recent_positions_for_bot("aapl-5m", 20)
        .expect("secondary positions should load")
        .len();

    let latest_secondary = journal
        .latest_position_for_bot("aapl-5m")
        .expect("latest secondary position should load")
        .expect("secondary position should exist");
    assert!(!latest_secondary.has_position);
    assert!(latest_secondary.quantity.abs() < EPSILON);
    assert!(latest_secondary.entry_price.is_none());
    assert_eq!(latest_secondary.reason, "startup_managed_state_refresh");

    PositionRowCounts {
        owner_rows,
        secondary_rows,
    }
}

fn seed_running_snapshot(db_path: &PathBuf, busy_timeout_ms: u64, bot_id: &str) {
    let journal = SqliteRuntimeJournal::open(db_path, busy_timeout_ms)
        .expect("sqlite journal should open for seeding");
    journal
        .upsert_bot_snapshot(BotSnapshotWrite {
            bot_id: bot_id.to_owned(),
            state: "running".to_owned(),
            execution_mode: "paper".to_owned(),
            enabled: true,
        })
        .expect("running snapshot should seed");
}

fn seed_shared_symbol_positions(db_path: &PathBuf, busy_timeout_ms: u64) {
    let journal = SqliteRuntimeJournal::open(db_path, busy_timeout_ms)
        .expect("sqlite journal should open for position seeding");
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
        .expect("owner position should seed");
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
        .expect("secondary position should seed");
}

fn seed_managed_open_order(
    db_path: &PathBuf,
    busy_timeout_ms: u64,
    bot_id: &str,
    client_order_id: &str,
) {
    let journal = SqliteRuntimeJournal::open(db_path, busy_timeout_ms)
        .expect("sqlite journal should open for order seeding");
    journal
        .append_order(OrderWrite {
            bot_id: bot_id.to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: None,
            client_order_id: client_order_id.to_owned(),
            intent: "open_long".to_owned(),
            status: "submitted".to_owned(),
            price: 123.45,
            quantity: 2.0,
        })
        .expect("managed open order should seed");
}

#[derive(Clone)]
struct MockAlpacaResponses {
    orders: Arc<Mutex<String>>,
    positions: Arc<Mutex<String>>,
    account: Arc<Mutex<String>>,
}

impl MockAlpacaResponses {
    fn new(orders_body: &str, positions_body: &str, account_body: &str) -> Self {
        Self {
            orders: Arc::new(Mutex::new(orders_body.to_owned())),
            positions: Arc::new(Mutex::new(positions_body.to_owned())),
            account: Arc::new(Mutex::new(account_body.to_owned())),
        }
    }

    fn orders_body(&self) -> String {
        self.orders.lock().expect("orders body lock").clone()
    }

    fn positions_body(&self) -> String {
        self.positions.lock().expect("positions body lock").clone()
    }

    fn account_body(&self) -> String {
        self.account.lock().expect("account body lock").clone()
    }
}

struct MockAlpacaServer {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<Vec<String>>,
}

impl MockAlpacaServer {
    fn spawn(responses: MockAlpacaResponses) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock server should bind");
        listener
            .set_nonblocking(true)
            .expect("mock server should be non-blocking");
        let address = listener
            .local_addr()
            .expect("mock server local addr should resolve");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let mut paths = Vec::new();
            while !stop_signal.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_http_request(&mut stream);
                        if request.is_empty() {
                            continue;
                        }
                        let path = request_path(&request);
                        paths.push(path.clone());

                        let (status, body) = if path.starts_with("/v2/orders") {
                            ("HTTP/1.1 200 OK", responses.orders_body())
                        } else if path == "/v2/positions" {
                            ("HTTP/1.1 200 OK", responses.positions_body())
                        } else if path == "/v2/account" {
                            ("HTTP/1.1 200 OK", responses.account_body())
                        } else {
                            (
                                "HTTP/1.1 404 Not Found",
                                "{\"error\":\"not found\"}".to_owned(),
                            )
                        };
                        write_http_response(&mut stream, status, body.as_str());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
            paths
        });

        Self {
            base_url: format!("http://{address}"),
            stop,
            handle,
        }
    }

    fn shutdown(self) -> Vec<String> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle
            .join()
            .expect("mock server should shut down cleanly")
    }
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buffer = [0_u8; 1024];
    let mut data = Vec::new();
    for _ in 0..200 {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                data.extend_from_slice(&buffer[..count]);
                if data.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(_) => break,
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
    stream
        .write_all(response.as_bytes())
        .expect("response write should succeed");
}

fn fixture_bundle_with_shared_symbol_reconciliation(
    db_path: PathBuf,
    base_url: String,
) -> ConfigBundle {
    let primary = InstanceConfig {
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
        execution_constraints: ExecutionConstraintsConfig {
            quantity_step: Some(1.0),
            min_quantity: Some(1.0),
            min_notional_usd: Some(1.0),
        },
        budget: BudgetConfig { pct: 50.0 },
        risk: InstanceRiskConfig {
            profile: "equities-default".to_owned(),
            overrides: RiskOverrides::default(),
        },
        warmup_target_bars: None,
        allow_live: false,
    };

    let mut secondary = primary.clone();
    "aapl-5m".clone_into(&mut secondary.id);
    secondary.timeframe = Timeframe::M5;
    "trend-2".clone_into(&mut secondary.indicators[0].id);

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
            reconciliation_remote_snapshot: true,
            execution_remote_submission: Some(false),
            reconciliation_base_url: Some(base_url),
            cash_balance_assets: Vec::new(),
            total_budget_usd: 1_000.0,
        }],
        risk_profiles: vec![RiskProfileConfig {
            id: "equities-default".to_owned(),
            max_daily_loss_pct: 2.0,
            max_open_positions: 2,
            target_order_notional_usd: Some(500.0),
            max_order_notional_usd: 500.0,
            max_spread_bps: 20,
            max_slippage_bps: 30,
            stale_data_ms: 3_000,
            cooldown_after_reject_ms: 1_000,
        }],
        instances: vec![primary, secondary],
    }
}

fn create_temp_db_path(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-runtime-{prefix}-{timestamp}.db"))
}
