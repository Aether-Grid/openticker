use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
    GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
    ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode,
    StorageConfig,
};
use openticker_core::{
    ExecutionMode, IndicatorSignal, IndicatorSignalMetadataFilters, MarketType, OhlcvBar,
    SignalPhase, Timeframe, TradeIntent,
};
use openticker_runtime::Runtime;
use openticker_testkit::close_only_bar;
use std::path::PathBuf;
use toml::Table;

#[test]
fn reprocessing_same_confirmed_bar_does_not_duplicate_submission() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("aapl").unwrap();

    let mut triggering_bar = None;
    for (index, close) in replay_closes().into_iter().enumerate() {
        let bar = test_bar(index, close);
        let _ = runtime
            .process_bar("aapl", &bar, SignalPhase::Confirmed)
            .unwrap();
        if !runtime.recent_orders(20).unwrap().is_empty() {
            triggering_bar = Some(bar);
            break;
        }
    }

    let triggering_bar = triggering_bar.expect("fixture replay should produce an order");
    let orders_before = runtime.recent_orders(20).unwrap().len();
    let fills_before = runtime.recent_fills(20).unwrap().len();

    let outcome = runtime
        .process_bar("aapl", &triggering_bar, SignalPhase::Confirmed)
        .unwrap();

    assert_eq!(outcome.intent, TradeIntent::NoOp);
    assert_eq!(runtime.recent_orders(20).unwrap().len(), orders_before);
    assert_eq!(runtime.recent_fills(20).unwrap().len(), fills_before);
}

#[test]
fn reprocessing_same_signal_after_fill_does_not_duplicate_submission() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("aapl").unwrap();

    let timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let first = runtime
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 123.45, timestamp)
        .unwrap();
    assert_eq!(first.intent, TradeIntent::OpenLong);

    let orders_before = runtime.recent_orders(20).unwrap().len();
    let fills_before = runtime.recent_fills(20).unwrap().len();

    let duplicate = runtime
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 123.45, timestamp)
        .unwrap();

    assert_eq!(duplicate.intent, TradeIntent::NoOp);
    assert_eq!(runtime.recent_orders(20).unwrap().len(), orders_before);
    assert_eq!(runtime.recent_fills(20).unwrap().len(), fills_before);
}

fn fixture_bundle() -> ConfigBundle {
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
                path: PathBuf::from("./var/openticker-test.db"),
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
            signal_mode: SignalMode::Intrabar,
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

fn replay_closes() -> Vec<f64> {
    let mut closes = Vec::new();
    let mut close = 125.0;
    for _ in 0..20 {
        close -= 1.4;
        closes.push(close);
    }
    for _ in 0..20 {
        close += 2.3;
        closes.push(close);
    }
    for _ in 0..20 {
        close -= 2.6;
        closes.push(close);
    }
    closes
}

fn test_bar(index: usize, close: f64) -> OhlcvBar {
    close_only_bar(&format!("2030-01-01T00:{:02}:00Z", index.min(59)), close)
}
