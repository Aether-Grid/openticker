use crate::{
    ConfigBundle, DataPlaneConfig, ExecutionMode, InstanceConfig, LaneRuntime, LaneRuntimeState,
    MarketType, NormalizedTrade, OhlcvBar, Runtime, Timeframe,
};
use openticker_config::{
    AccountConfig, ExecutionConstraintsConfig, GlobalConfig, HttpConfig, IndicatorInstanceConfig,
    InstanceRiskConfig, ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig,
    ServiceConfig, SignalMode, StorageConfig,
};
use openticker_core::IndicatorSignalMetadataFilters;
use openticker_lane::build_lane_runtime;
use serde_json::json;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Table;

pub(crate) fn fixture_bundle() -> ConfigBundle {
    fixture_bundle_with_db_path(PathBuf::from("./var/openticker-test.db"))
}

pub(crate) fn fixture_bundle_with_timeframe(timeframe: Timeframe) -> ConfigBundle {
    let mut bundle = fixture_bundle();
    bundle.instances[0].timeframe = timeframe;
    bundle
}

pub(crate) fn fixture_live_bundle() -> ConfigBundle {
    fixture_live_bundle_with_db_path(PathBuf::from("./var/openticker-test-live.db"))
}

pub(crate) fn fixture_live_bundle_with_db_path(db_path: PathBuf) -> ConfigBundle {
    let mut bundle = fixture_bundle_with_db_path(db_path);
    bundle.accounts[0].mode = ExecutionMode::Live;
    bundle.instances[0].allow_live = true;
    bundle
}

pub(crate) fn fixture_bundle_with_db_path(db_path: PathBuf) -> ConfigBundle {
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
            budget: openticker_config::BudgetConfig { pct: 100.0 },
            risk: InstanceRiskConfig {
                profile: "equities-default".to_owned(),
                overrides: RiskOverrides::default(),
            },
            warmup_target_bars: None,
            allow_live: false,
        }],
    }
}

pub(crate) fn fixture_bundle_with_missing_account_instance(db_path: PathBuf) -> ConfigBundle {
    let mut bundle = fixture_bundle_with_db_path(db_path);
    bundle.instances.push(InstanceConfig {
        id: "msft".to_owned(),
        enabled: true,
        market: MarketType::Equities,
        symbols: vec!["MSFT".to_owned()],
        timeframe: Timeframe::M1,
        account: "missing-account".to_owned(),
        data_connector: "alpaca".to_owned(),
        execution_connector: "alpaca".to_owned(),
        strategy: "single_indicator_signal".to_owned(),
        signal_mode: SignalMode::ConfirmedOnly,
        polling_enabled: true,
        polling_interval_ms: 1_000,
        indicators: vec![IndicatorInstanceConfig {
            id: "trend-2".to_owned(),
            indicator_type: "sma_crossover".to_owned(),
            enabled: true,
            role: None,
            signal_policy: None,
            weight: None,
            metadata_filters: IndicatorSignalMetadataFilters::default(),
            params: Table::new(),
        }],
        execution_constraints: ExecutionConstraintsConfig::default(),
        budget: openticker_config::BudgetConfig { pct: 100.0 },
        risk: InstanceRiskConfig {
            profile: "equities-default".to_owned(),
            overrides: RiskOverrides::default(),
        },
        warmup_target_bars: None,
        allow_live: false,
    });
    bundle
}

pub(crate) fn replay_closes() -> Vec<f64> {
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

#[allow(dead_code)]
pub(crate) fn build_test_instance_runtime(indicator_type: &str) -> LaneRuntime {
    let mut bundle = fixture_bundle();
    bundle.instances[0].indicators = vec![IndicatorInstanceConfig {
        id: "test-indicator".to_owned(),
        indicator_type: indicator_type.to_owned(),
        enabled: true,
        role: None,
        signal_policy: None,
        weight: None,
        metadata_filters: IndicatorSignalMetadataFilters::default(),
        params: Table::new(),
    }];

    let account_modes = Runtime::account_modes(&bundle);
    let risk_profiles = Runtime::risk_profiles_by_id(&bundle);
    let execution_mode = account_modes
        .get(&bundle.instances[0].account)
        .copied()
        .unwrap_or(ExecutionMode::Paper);
    build_lane_runtime(
        &bundle.instances[0],
        &bundle.instances[0].symbols[0],
        LaneRuntimeState::Stopped,
        false,
        execution_mode,
        &risk_profiles,
        0.0,
    )
    .unwrap()
}

pub(crate) fn test_bar(close: f64) -> OhlcvBar {
    test_bar_at("2030-01-01T00:00:00Z", close)
}

pub(crate) fn test_bar_at(timestamp: &str, close: f64) -> OhlcvBar {
    serde_json::from_value(json!({
        "timestamp": timestamp,
        "open": close,
        "high": close + 0.9,
        "low": close - 0.9,
        "close": close,
        "volume": 1000.0
    }))
    .unwrap()
}

pub(crate) fn test_trade(
    symbol: &str,
    timestamp: &str,
    price: f64,
    quantity: f64,
) -> NormalizedTrade {
    serde_json::from_value(json!({
        "symbol": symbol,
        "timestamp": timestamp,
        "price": price,
        "quantity": quantity
    }))
    .unwrap()
}

pub(crate) fn create_temp_db_path(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-runtime-{prefix}-{timestamp}.db"))
}
