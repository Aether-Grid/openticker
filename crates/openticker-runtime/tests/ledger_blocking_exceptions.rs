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
use openticker_runtime::{LaneRuntimeState, LedgerExceptionKind, Runtime};
use openticker_storage::{BotSnapshotWrite, RuntimeJournal, SqliteRuntimeJournal};
use std::path::PathBuf;
use toml::Table;

mod common;

use common::{MockAlpacaResponses, MockAlpacaServer, create_temp_db_path};

const EPSILON: f64 = 1e-6;

#[test]
fn external_remote_position_surplus_is_advisory() {
    let responses = MockAlpacaResponses::new(
        "[]",
        r#"[{"symbol":"AAPL","qty":"1"}]"#,
        r#"{"cash":"1000.0","equity":"1000.0"}"#,
    );
    let server = MockAlpacaServer::spawn(responses);
    let config = fixture_bundle_with_reconciliation(
        create_temp_db_path("runtime-unmatched-blocking"),
        server.base_url.clone(),
    );
    seed_running_snapshot(
        &config.global.storage.path,
        config.global.storage.busy_timeout_ms,
        "aapl",
    );

    let mut runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert_eq!(summary.state, LaneRuntimeState::Running);
    assert!(!summary.reconciliation_blocked);
    assert_eq!(summary.reconciliation_by_symbol.len(), 1);
    assert_eq!(summary.reconciliation_by_symbol[0].symbol, "AAPL");
    assert_eq!(
        summary.reconciliation_by_symbol[0].remote_net_qty,
        Some(1.0)
    );
    assert!((summary.reconciliation_by_symbol[0].aggregate_managed_qty - 0.0).abs() < EPSILON);
    assert_eq!(
        summary.reconciliation_by_symbol[0].external_delta_qty,
        Some(1.0)
    );

    let ledger = runtime.ledger_snapshot();
    let account = ledger
        .accounts
        .iter()
        .find(|account| account.id == "alpaca-paper")
        .expect("account ledger row should exist");
    assert!((account.tradeable_open_room_usd - account.effective_cap_usd).abs() < EPSILON);
    assert!(account.blocked_open_room_usd.abs() < EPSILON);
    assert!(account.exceptions.is_empty());

    let orders_before = runtime.recent_orders(20).expect("orders should load").len();
    let fills_before = runtime.recent_fills(20).expect("fills should load").len();
    let timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .expect("timestamp should parse")
        .with_timezone(&chrono::Utc);
    let outcome = runtime
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 100.0, timestamp)
        .expect("manual signal should process");
    assert!(matches!(
        outcome.intent,
        TradeIntent::OpenLong | TradeIntent::AddLong
    ));
    assert!(runtime.recent_orders(20).expect("orders should load").len() > orders_before);
    assert!(runtime.recent_fills(20).expect("fills should load").len() > fills_before);

    let paths = server.shutdown();
    assert!(paths.iter().any(|path| path.starts_with("/v2/orders")));
    assert!(paths.iter().any(|path| path == "/v2/positions"));
    assert!(paths.iter().any(|path| path == "/v2/account"));
}

#[test]
#[allow(clippy::too_many_lines)]
fn managed_position_deficit_blocks_new_open_orders_without_pausing_shared_symbol_bots() {
    let responses = MockAlpacaResponses::new(
        "[]",
        r#"[{"symbol":"AAPL","qty":"1"}]"#,
        r#"{"cash":"1000.0","equity":"1000.0"}"#,
    );
    let server = MockAlpacaServer::spawn(responses);
    let config = fixture_bundle_with_shared_symbol_reconciliation(
        create_temp_db_path("runtime-ambiguous-blocking"),
        server.base_url.clone(),
    );
    seed_running_snapshot(
        &config.global.storage.path,
        config.global.storage.busy_timeout_ms,
        "aapl",
    );
    seed_running_snapshot(
        &config.global.storage.path,
        config.global.storage.busy_timeout_ms,
        "aapl-5m",
    );
    let journal = SqliteRuntimeJournal::open(
        &config.global.storage.path,
        config.global.storage.busy_timeout_ms,
    )
    .expect("sqlite journal should open");
    for bot_id in ["aapl", "aapl-5m"] {
        journal
            .append_position(openticker_storage::PositionWrite {
                bot_id: bot_id.to_owned(),
                symbol: "AAPL".to_owned(),
                trace_id: None,
                bar_timestamp: None,
                has_position: true,
                quantity: 1.0,
                entry_price: Some(123.45),
                realized_pnl_usd: 0.0,
                reason: "order_filled".to_owned(),
            })
            .expect("seeded managed position should persist");
    }

    let mut runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
    for instance_id in ["aapl", "aapl-5m"] {
        let summary = runtime
            .get_instance(instance_id)
            .expect("instance summary should exist");
        assert_eq!(summary.state, LaneRuntimeState::Running);
        assert!(!summary.reconciliation_blocked);
        assert_eq!(summary.reconciliation_by_symbol.len(), 1);
        assert_eq!(summary.reconciliation_by_symbol[0].symbol, "AAPL");
        assert_eq!(
            summary.reconciliation_by_symbol[0].remote_net_qty,
            Some(1.0)
        );
        assert!((summary.reconciliation_by_symbol[0].aggregate_managed_qty - 2.0).abs() < EPSILON);
        assert_eq!(
            summary.reconciliation_by_symbol[0].external_delta_qty,
            Some(-1.0)
        );

        let report = runtime
            .reconciliation_report(instance_id)
            .expect("reconciliation report should load");
        let latest = report
            .latest
            .expect("startup reconciliation should produce a latest record");
        assert!(latest.safe_to_trade);
        assert!(latest.reason.contains("managed_position_deficit"));
    }

    let ledger = runtime.ledger_snapshot();
    let account = ledger
        .accounts
        .iter()
        .find(|account| account.id == "alpaca-paper")
        .expect("account ledger row should exist");
    assert!(account.tradeable_open_room_usd.abs() < EPSILON);
    assert!(account.blocked_open_room_usd > EPSILON);
    assert!(account.blocked_open_room_usd < account.effective_cap_usd);

    let exception = account
        .exceptions
        .iter()
        .find(|exception| exception.symbol.as_deref() == Some("AAPL"))
        .expect("blocking exception for managed AAPL deficit should exist");
    assert_eq!(exception.kind, LedgerExceptionKind::ManagedPositionDeficit);
    assert!(exception.detail.contains("deficit_qty=1"));
    assert!(exception.blocks_new_opens);

    let orders_before = runtime.recent_orders(20).expect("orders should load").len();
    let fills_before = runtime.recent_fills(20).expect("fills should load").len();
    let timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .expect("timestamp should parse")
        .with_timezone(&chrono::Utc);
    let outcome = runtime
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 100.0, timestamp)
        .expect("manual signal should process");
    assert!(matches!(
        outcome.intent,
        TradeIntent::OpenLong | TradeIntent::AddLong
    ));
    assert_eq!(
        runtime.recent_orders(20).expect("orders should load").len(),
        orders_before
    );
    assert_eq!(
        runtime.recent_fills(20).expect("fills should load").len(),
        fills_before
    );

    let paths = server.shutdown();
    assert!(paths.iter().any(|path| path.starts_with("/v2/orders")));
    assert!(paths.iter().any(|path| path == "/v2/positions"));
    assert!(paths.iter().any(|path| path == "/v2/account"));
}

fn fixture_bundle_with_reconciliation(db_path: PathBuf, base_url: String) -> ConfigBundle {
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
            execution_constraints: ExecutionConstraintsConfig {
                quantity_step: Some(1.0),
                min_quantity: Some(1.0),
                min_notional_usd: Some(1.0),
            },
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

fn fixture_bundle_with_shared_symbol_reconciliation(
    db_path: PathBuf,
    base_url: String,
) -> ConfigBundle {
    let mut bundle = fixture_bundle_with_reconciliation(db_path, base_url);
    bundle.instances[0].budget.pct = 50.0;
    let mut second = bundle.instances[0].clone();
    "aapl-5m".clone_into(&mut second.id);
    second.timeframe = Timeframe::M5;
    "trend-2".clone_into(&mut second.indicators[0].id);
    second.budget.pct = 50.0;
    bundle.instances.push(second);
    bundle
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
