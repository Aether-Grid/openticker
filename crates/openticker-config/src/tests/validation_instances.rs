//! Validation tests for instance-level rules.

use super::support::{
    connector_validation_bundle, create_fixture_dir, default_validation_account,
    default_validation_instance, intrabar_validation_account, intrabar_validation_instance,
    write_file,
};
use crate::{
    AccountConfig, BudgetConfig, ConfigError, ExecutionConstraintsConfig, IndicatorInstanceConfig,
    InstanceConfig, InstanceRiskConfig, RiskOverrides, SignalMode, load_from_dir,
};
use openticker_core::{ExecutionMode, IndicatorSignalMetadataFilters, MarketType, Timeframe};
use toml::Table;

#[test]
fn rejects_execution_connector_that_does_not_match_account_kind() {
    let fixture_dir = create_fixture_dir("execution-connector-mismatch");

    write_file(
        fixture_dir.join("openticker.toml"),
        r#"
[service]
environment = "dev"
data_dir = "./var"
bot_dir = "./bots"

[http]
enabled = true
bind = "127.0.0.1:8080"
request_log = true
openapi_enabled = true
openapi_path = "/openapi.json"

[storage]
kind = "sqlite"
path = "./var/openticker.db"
busy_timeout_ms = 5000

[observability]
log_level = "info"
metrics_enabled = true
metrics_path = "/metrics"

[safety]
require_explicit_live_enable = true
default_start_paused_if_recovery_uncertain = true
"#,
    );

    write_file(
        fixture_dir.join("accounts").join("alpaca-paper.toml"),
        r#"
id = "alpaca-paper"
kind = "alpaca"
mode = "paper"
api_key_env = "PATH"
api_secret_env = "PATH"
total_budget_usd = 20000.0
"#,
    );

    write_file(
        fixture_dir.join("risk").join("equities-default.toml"),
        r#"
id = "equities-default"
max_daily_loss_pct = 2.0
max_open_positions = 5
max_order_notional_usd = 2500.0
max_spread_bps = 20
max_slippage_bps = 30
stale_data_ms = 3000
cooldown_after_reject_ms = 15000
"#,
    );

    write_file(
        fixture_dir.join("bots").join("aapl-paper.toml"),
        r#"
id = "aapl-paper"
enabled = true
market = "equities"
symbols = ["AAPL"]
timeframe = "1m"
account = "alpaca-paper"
data_connector = "alpaca"
execution_connector = "binance"
strategy = "single_indicator_signal"
signal_mode = "intrabar"

[[indicators]]
id = "trend-1"
type = "sma_crossover"

[indicators.params]
fast_length = 10
slow_length = 30

[budget]
pct = 100.0

[risk]
profile = "equities-default"
"#,
    );

    let result = load_from_dir(&fixture_dir);
    assert!(matches!(result, Err(ConfigError::Validation { .. })));
}

#[test]
fn rejects_unknown_execution_connector() {
    let bundle = connector_validation_bundle(
        AccountConfig {
            id: "alpaca-live".to_owned(),
            kind: "alpaca".to_owned(),
            mode: ExecutionMode::Live,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: Some("PATH".to_owned()),
            passphrase_env: None,
            use_demo_mode: false,
            reconciliation_remote_snapshot: false,
            execution_remote_submission: None,
            reconciliation_base_url: None,
            cash_balance_assets: Vec::new(),
            total_budget_usd: 20_000.0,
        },
        InstanceConfig {
            id: "aapl-live".to_owned(),
            enabled: true,
            market: MarketType::Equities,
            symbols: vec!["AAPL".to_owned()],
            timeframe: Timeframe::M1,
            account: "alpaca-live".to_owned(),
            data_connector: "alpaca".to_owned(),
            execution_connector: "kraken".to_owned(),
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
            allow_live: true,
        },
    );

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { .. })
    ));
}

#[test]
fn rejects_intrabar_signal_mode_for_connector_without_preview_market_stream_support() {
    let mut instance = default_validation_instance();
    instance.signal_mode = SignalMode::Intrabar;

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("requires data_connector `alpaca` to support preview market-stream bars")
    ));
}

#[test]
fn rejects_live_intrabar_signal_mode_even_for_preview_capable_connector() {
    let mut account = intrabar_validation_account();
    account.mode = ExecutionMode::Live;
    account.use_demo_mode = false;
    account.id = "binance-live".to_owned();

    let mut instance = intrabar_validation_instance();
    instance.account = "binance-live".to_owned();
    instance.allow_live = true;

    let bundle = connector_validation_bundle(account, instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("live intrabar trading is hard-rejected until parity is proven")
    ));
}

#[test]
fn rejects_execution_connector_for_unsupported_market() {
    let bundle = connector_validation_bundle(
        AccountConfig {
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
        },
        InstanceConfig {
            id: "btcusdt-paper".to_owned(),
            enabled: true,
            market: MarketType::Crypto,
            symbols: vec!["BTCUSDT".to_owned()],
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
        },
    );

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { .. })
    ));
}

#[test]
fn rejects_data_connector_for_unsupported_market() {
    let bundle = connector_validation_bundle(
        AccountConfig {
            id: "binance-demo".to_owned(),
            kind: "binance".to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: Some("PATH".to_owned()),
            passphrase_env: None,
            use_demo_mode: true,
            reconciliation_remote_snapshot: false,
            execution_remote_submission: None,
            reconciliation_base_url: None,
            cash_balance_assets: Vec::new(),
            total_budget_usd: 20_000.0,
        },
        InstanceConfig {
            id: "btcusdt-paper".to_owned(),
            enabled: true,
            market: MarketType::Crypto,
            symbols: vec!["BTCUSDT".to_owned()],
            timeframe: Timeframe::M1,
            account: "binance-demo".to_owned(),
            data_connector: "alpaca".to_owned(),
            execution_connector: "binance".to_owned(),
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
        },
    );

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { .. })
    ));
}

#[test]
fn rejects_zero_polling_interval() {
    let bundle = connector_validation_bundle(
        AccountConfig {
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
        },
        InstanceConfig {
            id: "aapl-paper".to_owned(),
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
            polling_interval_ms: 0,
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
        },
    );

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("polling_interval_ms")
    ));
}

#[test]
fn rejects_invalid_instance_risk_override_target_order_notional() {
    let mut instance = default_validation_instance();
    instance.risk.overrides.target_order_notional_usd = Some(0.0);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("risk.overrides.target_order_notional_usd")
    ));
}

#[test]
fn rejects_invalid_instance_budget_pct() {
    let mut instance = default_validation_instance();
    instance.budget.pct = 0.0;

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("budget.pct")
    ));
}

#[test]
fn rejects_enabled_bot_budget_sum_over_100() {
    let account = default_validation_account();
    let mut first = default_validation_instance();
    first.id = "aapl-one".to_owned();
    first.budget.pct = 60.0;

    let mut second = default_validation_instance();
    second.id = "aapl-two".to_owned();
    second.symbols = vec!["MSFT".to_owned()];
    second.budget.pct = 50.0;

    let mut bundle = connector_validation_bundle(account, first);
    bundle.instances.push(second);

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("sums to") && message.contains("aapl-one") && message.contains("aapl-two")
    ));
}

#[test]
fn excludes_disabled_bots_from_budget_sum() {
    let account = default_validation_account();
    let mut first = default_validation_instance();
    first.id = "aapl-one".to_owned();
    first.budget.pct = 60.0;

    let mut second = default_validation_instance();
    second.id = "aapl-two".to_owned();
    second.symbols = vec!["MSFT".to_owned()];
    second.enabled = false;
    second.budget.pct = 50.0;

    let mut bundle = connector_validation_bundle(account, first);
    bundle.instances.push(second);

    assert!(bundle.validate().is_ok());
}

#[test]
fn allows_enabled_account_with_multiple_normalized_symbols() {
    let account = default_validation_account();
    let first = default_validation_instance();

    let mut second = default_validation_instance();
    second.id = "aapl-two".to_owned();
    second.symbols = vec!["MSFT".to_owned()];
    second.budget.pct = 50.0;

    let mut bundle = connector_validation_bundle(account, first);
    bundle.instances[0].budget.pct = 50.0;
    bundle.instances.push(second);

    assert!(bundle.validate().is_ok());
}

#[test]
fn allows_single_enabled_bot_with_multiple_symbols_on_one_account() {
    let mut instance = default_validation_instance();
    instance.symbols = vec!["AAPL".to_owned(), "MSFT".to_owned()];

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(bundle.validate().is_ok());
}

#[test]
fn ignores_disabled_account_symbol_mismatch() {
    let account = default_validation_account();
    let first = default_validation_instance();

    let mut second = default_validation_instance();
    second.id = "aapl-two".to_owned();
    second.symbols = vec!["MSFT".to_owned()];
    second.enabled = false;

    let mut bundle = connector_validation_bundle(account, first);
    bundle.instances.push(second);

    assert!(bundle.validate().is_ok());
}

#[test]
fn rejects_blank_symbol_entries() {
    let mut instance = default_validation_instance();
    instance.symbols = vec!["AAPL".to_owned(), "   ".to_owned()];

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("blank symbol entry")
    ));
}

#[test]
fn rejects_duplicate_symbols_after_normalization() {
    let mut instance = default_validation_instance();
    instance.symbols = vec!["AAPL".to_owned(), " aapl ".to_owned()];

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("duplicate symbol")
    ));
}

#[test]
fn rejects_invalid_execution_constraints_quantity_step() {
    let mut instance = default_validation_instance();
    instance.execution_constraints.quantity_step = Some(0.0);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("execution_constraints.quantity_step")
    ));
}

#[test]
fn rejects_invalid_execution_constraints_min_notional() {
    let mut instance = default_validation_instance();
    instance.execution_constraints.min_notional_usd = Some(-5.0);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("execution_constraints.min_notional_usd")
    ));
}
