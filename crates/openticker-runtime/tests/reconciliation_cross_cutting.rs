use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
    GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
    ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode,
    StorageConfig,
};
use openticker_core::{ExecutionMode, IndicatorSignalMetadataFilters, MarketType, Timeframe};
use openticker_runtime::{LaneRuntimeState, Runtime, ServiceError};
use openticker_storage::{BotSnapshotWrite, PositionWrite, RuntimeJournal, SqliteRuntimeJournal};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Table;

const POSITION_TOLERANCE: f64 = 1e-9;

#[test]
fn live_mode_auto_resumes_recovered_running_snapshot_after_safe_startup_reconciliation() {
    let db_path = create_temp_db_path("live-startup-reconcile");
    let mut config = fixture_live_bundle_with_db_path(db_path.clone());
    config.instances[0].signal_mode = SignalMode::ConfirmedOnly;

    let journal = SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms)
        .expect("sqlite journal should open");
    journal
        .upsert_bot_snapshot(BotSnapshotWrite {
            bot_id: "aapl".to_owned(),
            state: "running".to_owned(),
            execution_mode: "live".to_owned(),
            enabled: true,
        })
        .expect("snapshot should persist");

    let runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert_eq!(summary.state, LaneRuntimeState::Running);
    assert!(!summary.reconciliation_blocked);

    let report = runtime
        .reconciliation_report("aapl")
        .expect("reconciliation report should exist");
    let latest = report.latest.expect("startup reconciliation should exist");
    assert_eq!(latest.source, "startup");
    assert!(latest.safe_to_trade);
}

#[test]
fn live_mode_blocks_start_and_resume_after_unresolved_manual_reconciliation() {
    let mut config = fixture_bundle();
    config.accounts[0].mode = ExecutionMode::Live;
    config.accounts[0].reconciliation_remote_snapshot = true;
    config.accounts[0].reconciliation_base_url = Some("http://127.0.0.1:9".to_owned());
    config.instances[0].allow_live = true;
    let mut runtime = Runtime::from_config(&config);

    let reconciled = runtime
        .reconcile_instance("aapl")
        .expect("manual reconciliation call should succeed");
    assert_eq!(reconciled.state, LaneRuntimeState::Paused);
    assert!(reconciled.reconciliation_blocked);

    let report = runtime
        .reconciliation_report("aapl")
        .expect("reconciliation report should exist");
    let latest = report
        .latest
        .expect("manual reconciliation should be recorded");
    assert_eq!(latest.source, "manual");
    assert!(!latest.safe_to_trade);
    assert!(latest.reason.contains("connector_snapshot_unavailable"));

    let start = runtime.start_instance("aapl");
    assert!(matches!(
        start,
        Err(ServiceError::ReconciliationRequired { reason, .. })
            if reason.contains("startup reconciliation has unresolved differences")
    ));

    let resume = runtime.resume_instance("aapl");
    assert!(matches!(
        resume,
        Err(ServiceError::ReconciliationRequired { reason, .. })
            if reason.contains("instance must reconcile successfully before resume")
    ));
}

#[test]
fn startup_reconciliation_moves_recovered_running_snapshot_to_running() {
    let db_path = create_temp_db_path("recover");
    let mut config = fixture_bundle_with_db_path(db_path.clone());
    config.instances[0].signal_mode = SignalMode::ConfirmedOnly;

    let journal = SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms)
        .expect("sqlite journal should open");
    journal
        .upsert_bot_snapshot(BotSnapshotWrite {
            bot_id: "aapl".to_owned(),
            state: "running".to_owned(),
            execution_mode: "paper".to_owned(),
            enabled: true,
        })
        .expect("snapshot should persist");

    let runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert_eq!(summary.state, LaneRuntimeState::Running);
    assert!(!summary.reconciliation_blocked);
}

#[test]
fn startup_reconciliation_preserves_journal_authoritative_position_state() {
    let db_path = create_temp_db_path("reconcile-diff");
    let mut config = fixture_bundle_with_db_path(db_path.clone());
    config.instances[0].signal_mode = SignalMode::ConfirmedOnly;

    let journal = SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms)
        .expect("sqlite journal should open");
    journal
        .upsert_bot_snapshot(BotSnapshotWrite {
            bot_id: "aapl".to_owned(),
            state: "running".to_owned(),
            execution_mode: "paper".to_owned(),
            enabled: true,
        })
        .expect("snapshot should persist");
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
            reason: "test_seed".to_owned(),
        })
        .expect("position should persist");

    let mut runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert_eq!(summary.state, LaneRuntimeState::Running);
    assert!(!summary.reconciliation_blocked);
    assert!(summary.position.has_position);
    assert!((summary.position.quantity - 1.0).abs() < POSITION_TOLERANCE);
    assert_eq!(summary.position.entry_price, Some(123.45));

    let startup_report = runtime
        .reconciliation_report("aapl")
        .expect("report should load");
    let startup_latest = startup_report
        .lanes
        .into_iter()
        .next()
        .expect("lane report");
    assert_eq!(startup_latest.source, "startup");
    assert_eq!(startup_latest.symbol, "AAPL");
    assert!(startup_latest.safe_to_trade);
    assert!(startup_latest.differences.is_empty());

    let startup_position = journal
        .latest_position_for_bot("aapl")
        .expect("positions should load")
        .expect("position should exist");
    assert!(startup_position.has_position);
    assert!((startup_position.quantity - 1.0).abs() < POSITION_TOLERANCE);
    assert_eq!(startup_position.entry_price, Some(123.45));
    assert!((startup_position.realized_pnl_usd - 15.25).abs() < f64::EPSILON);
    assert_eq!(startup_position.reason, "test_seed");

    journal
        .append_position(PositionWrite {
            bot_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: None,
            has_position: true,
            quantity: 1.0,
            entry_price: Some(123.45),
            realized_pnl_usd: 22.75,
            reason: "manual_drift".to_owned(),
        })
        .expect("position should persist");

    let reconciled = runtime
        .reconcile_instance("aapl")
        .expect("manual reconciliation should succeed");
    assert_eq!(reconciled.state, LaneRuntimeState::Paused);
    assert!(!reconciled.reconciliation_blocked);

    let preserved_position = journal
        .latest_position_for_bot("aapl")
        .expect("positions should load")
        .expect("position should exist");
    assert!(preserved_position.has_position);
    assert!((preserved_position.quantity - 1.0).abs() < POSITION_TOLERANCE);
    assert_eq!(preserved_position.entry_price, Some(123.45));
    assert!((preserved_position.realized_pnl_usd - 22.75).abs() < f64::EPSILON);
    assert_eq!(preserved_position.reason, "manual_drift");

    let manual_report = runtime
        .reconciliation_report("aapl")
        .expect("report should load");
    let manual_latest = manual_report.lanes.into_iter().next().expect("lane report");
    assert_eq!(manual_latest.source, "manual");
    assert!(manual_latest.safe_to_trade);
    assert!(manual_latest.differences.is_empty());
}

#[test]
fn startup_reconciliation_leaves_local_open_order_records_without_remote_snapshot() {
    let db_path = create_temp_db_path("reconcile-open-order");
    let mut config = fixture_bundle_with_db_path(db_path.clone());
    config.instances[0].signal_mode = SignalMode::ConfirmedOnly;

    let journal = SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms)
        .expect("sqlite journal should open");
    journal
        .upsert_bot_snapshot(BotSnapshotWrite {
            bot_id: "aapl".to_owned(),
            state: "running".to_owned(),
            execution_mode: "paper".to_owned(),
            enabled: true,
        })
        .expect("snapshot should persist");
    journal
        .append_order(openticker_storage::OrderWrite {
            bot_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: None,
            client_order_id: "aapl-1-open_long".to_owned(),
            intent: "open_long".to_owned(),
            status: "submitted".to_owned(),
            price: 123.45,
            quantity: 1.0,
        })
        .expect("order should persist");

    let runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert_eq!(summary.state, LaneRuntimeState::Running);
    assert!(!summary.reconciliation_blocked);

    let report = runtime
        .reconciliation_report("aapl")
        .expect("report should load");
    let latest = report.lanes.into_iter().next().expect("lane report");
    assert_eq!(latest.source, "startup");
    assert!(latest.safe_to_trade);
    assert!(latest.differences.is_empty());

    let fills = journal
        .recent_fills_for_bot("aapl", 10)
        .expect("fills should load");
    assert!(fills.is_empty());
}

#[test]
fn startup_reconciliation_uses_persisted_position_quantity_when_connector_unavailable() {
    let db_path = create_temp_db_path("reconcile-local-quantity");
    let mut config = fixture_bundle_with_db_path(db_path.clone());
    config.instances[0].signal_mode = SignalMode::ConfirmedOnly;
    config.accounts[0].reconciliation_remote_snapshot = true;
    config.accounts[0].reconciliation_base_url = Some("http://127.0.0.1:9".to_owned());

    let journal = SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms)
        .expect("sqlite journal should open");
    journal
        .upsert_bot_snapshot(BotSnapshotWrite {
            bot_id: "aapl".to_owned(),
            state: "running".to_owned(),
            execution_mode: "paper".to_owned(),
            enabled: true,
        })
        .expect("snapshot should persist");
    journal
        .append_position(PositionWrite {
            bot_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            trace_id: None,
            bar_timestamp: None,
            has_position: true,
            quantity: 3.5,
            entry_price: Some(200.0),
            realized_pnl_usd: 88.0,
            reason: "persisted_seed".to_owned(),
        })
        .expect("position should persist");

    let runtime = Runtime::from_config_with_storage(&config).expect("runtime should boot");
    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert_eq!(summary.state, LaneRuntimeState::Paused);
    assert!(summary.reconciliation_blocked);
    assert!(summary.position.has_position);
    assert!((summary.position.quantity - 3.5).abs() < POSITION_TOLERANCE);
    assert_eq!(summary.position.entry_price, Some(200.0));

    let latest_position = journal
        .latest_position_for_bot("aapl")
        .expect("positions should load")
        .expect("position should exist");
    assert!((latest_position.realized_pnl_usd - 88.0).abs() < f64::EPSILON);
}

fn fixture_bundle() -> ConfigBundle {
    fixture_bundle_with_db_path(PathBuf::from("./var/openticker-test.db"))
}

fn fixture_live_bundle_with_db_path(db_path: PathBuf) -> ConfigBundle {
    let mut bundle = fixture_bundle_with_db_path(db_path);
    bundle.accounts[0].mode = ExecutionMode::Live;
    bundle.instances[0].allow_live = true;
    bundle
}

fn fixture_bundle_with_db_path(db_path: PathBuf) -> ConfigBundle {
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
            reconciliation_remote_snapshot: false,
            execution_remote_submission: None,
            reconciliation_base_url: None,
            cash_balance_assets: Vec::new(),
            total_budget_usd: 20_000.0,
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
