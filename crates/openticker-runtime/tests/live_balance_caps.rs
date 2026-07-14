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
use openticker_runtime::{LaneRuntimeState, ProcessBarRisk, Runtime};
use std::path::PathBuf;
use toml::Table;

mod common;

use common::{MockAlpacaResponses, MockAlpacaServer, create_temp_db_path};

const EPSILON: f64 = 1e-6;

#[test]
fn live_balance_drop_reduces_effective_cap_before_next_order() {
    let responses = MockAlpacaResponses::new("[]", "[]", r#"{"cash":"1000.0","equity":"1000.0"}"#);
    let server = MockAlpacaServer::spawn(responses.clone());
    let config = fixture_bundle_with_reconciliation(
        create_temp_db_path("runtime-live-balance-cap"),
        server.base_url.clone(),
    );

    let mut runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
    let started_aapl = runtime.start_instance("aapl").expect("aapl should start");
    assert_eq!(started_aapl.state, LaneRuntimeState::Running);
    assert!(!started_aapl.reconciliation_blocked);
    let started_msft = runtime.start_instance("msft").expect("msft should start");
    assert_eq!(started_msft.state, LaneRuntimeState::Running);
    assert!(!started_msft.reconciliation_blocked);

    let first_timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .expect("timestamp should parse")
        .with_timezone(&chrono::Utc);
    let first = runtime
        .process_manual_signal(
            "aapl",
            IndicatorSignal::BuyConfirmed,
            100.0,
            first_timestamp,
        )
        .expect("first manual signal should process");
    assert!(matches!(first.risk, ProcessBarRisk::Allowed));
    assert_eq!(first.intent, TradeIntent::OpenLong);
    assert_eq!(
        runtime.recent_orders(20).expect("orders should load").len(),
        1
    );
    assert_eq!(
        runtime.recent_fills(20).expect("fills should load").len(),
        1
    );

    responses.set_account_body(r#"{"cash":"200.0","equity":"200.0"}"#);

    runtime.pause_instance("msft").expect("msft should pause");
    let resumed = runtime.resume_instance("msft").expect("msft should resume");
    assert_eq!(resumed.state, LaneRuntimeState::Running);
    assert!(!resumed.reconciliation_blocked);

    let ledger = runtime.ledger_snapshot();
    let account = ledger
        .accounts
        .iter()
        .find(|account| account.id == "alpaca-paper")
        .expect("account ledger row should exist");
    assert!((account.effective_cap_usd - 200.0).abs() < EPSILON);
    assert!(account.total_committed_notional_usd > account.effective_cap_usd + EPSILON);
    assert!(account.tradeable_open_room_usd.abs() < EPSILON);

    let orders_before = runtime.recent_orders(20).expect("orders should load").len();
    let fills_before = runtime.recent_fills(20).expect("fills should load").len();

    let second_timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:01:00Z")
        .expect("timestamp should parse")
        .with_timezone(&chrono::Utc);
    let second = runtime
        .process_manual_signal(
            "msft",
            IndicatorSignal::BuyConfirmed,
            100.0,
            second_timestamp,
        )
        .expect("second manual signal should process");
    assert_eq!(second.intent, TradeIntent::OpenLong);
    assert!(matches!(second.risk, ProcessBarRisk::Rejected { .. }));

    assert_eq!(
        runtime.recent_orders(20).expect("orders should load").len(),
        orders_before
    );
    assert_eq!(
        runtime.recent_fills(20).expect("fills should load").len(),
        fills_before
    );

    let paths = server.shutdown();
    let account_requests = paths
        .iter()
        .filter(|path| path.as_str() == "/v2/account")
        .count();
    assert!(account_requests >= 4);
}

fn fixture_bundle_with_reconciliation(db_path: PathBuf, base_url: String) -> ConfigBundle {
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
    "msft".clone_into(&mut secondary.id);
    secondary.symbols = vec!["MSFT".to_owned()];
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
