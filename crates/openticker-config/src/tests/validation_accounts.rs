//! Validation tests for account-level rules.

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
fn live_account_requires_explicit_live_enable() {
    let fixture_dir = create_fixture_dir("live-flag");

    write_file(
        fixture_dir.join("openticker.toml"),
        r#"
[service]
environment = "prod"
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
        fixture_dir.join("accounts").join("alpaca-live.toml"),
        r#"
id = "alpaca-live"
kind = "alpaca"
mode = "live"
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
        fixture_dir.join("bots").join("aapl-live.toml"),
        r#"
id = "aapl-live"
enabled = true
market = "equities"
symbols = ["AAPL"]
timeframe = "1m"
account = "alpaca-live"
data_connector = "alpaca"
execution_connector = "alpaca"
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
fn rejects_binance_paper_without_demo_mode() {
    let fixture_dir = create_fixture_dir("binance-paper-no-demo");

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
        fixture_dir.join("accounts").join("binance-paper.toml"),
        r#"
id = "binance-paper"
kind = "binance"
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
market = "crypto"
symbols = ["BTCUSDT"]
timeframe = "1m"
account = "binance-paper"
data_connector = "binance"
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
fn accepts_binance_paper_with_demo_mode() {
    let fixture_dir = create_fixture_dir("binance-paper-demo");

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
        fixture_dir.join("accounts").join("binance-paper.toml"),
        r#"
id = "binance-paper"
kind = "binance"
mode = "paper"
use_demo_mode = true
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
market = "crypto"
symbols = ["BTCUSDT"]
timeframe = "1m"
account = "binance-paper"
data_connector = "binance"
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
    assert!(result.is_ok());
}

#[test]
fn rejects_account_missing_required_api_key_reference() {
    let bundle = connector_validation_bundle(
        AccountConfig {
            id: "alpaca-paper".to_owned(),
            kind: "alpaca".to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: None,
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
        Err(ConfigError::Validation { message }) if message.contains("requires `api_key_env`")
    ));
}

#[test]
fn rejects_binance_account_missing_required_api_secret_reference() {
    let bundle = connector_validation_bundle(
        AccountConfig {
            id: "binance-demo".to_owned(),
            kind: "binance".to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: None,
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
            data_connector: "binance".to_owned(),
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
        Err(ConfigError::Validation { message }) if message.contains("requires `api_secret_env`")
    ));
}

#[test]
fn rejects_reconciliation_base_url_without_remote_snapshot_mode() {
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
            reconciliation_base_url: Some("https://paper-api.alpaca.markets".to_owned()),
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
        Err(ConfigError::Validation { message })
            if message.contains("reconciliation_base_url")
    ));
}

#[test]
fn accepts_reconciliation_base_url_with_remote_execution_submission() {
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
            execution_remote_submission: Some(true),
            reconciliation_base_url: Some("https://paper-api.alpaca.markets".to_owned()),
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
        },
    );

    assert!(bundle.validate().is_ok());
}

#[test]
fn rejects_invalid_reconciliation_base_url_scheme() {
    let bundle = connector_validation_bundle(
        AccountConfig {
            id: "alpaca-paper".to_owned(),
            kind: "alpaca".to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: Some("PATH".to_owned()),
            passphrase_env: None,
            use_demo_mode: false,
            reconciliation_remote_snapshot: true,
            execution_remote_submission: None,
            reconciliation_base_url: Some("ftp://paper-api.alpaca.markets".to_owned()),
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
        Err(ConfigError::Validation { message })
            if message.contains("invalid `reconciliation_base_url`")
    ));
}

#[test]
fn rejects_invalid_account_total_budget() {
    let mut account = default_validation_account();
    account.total_budget_usd = 0.0;

    let bundle = connector_validation_bundle(account, default_validation_instance());
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("total_budget_usd")
    ));
}

#[test]
fn rejects_cash_balance_assets_for_non_binance_account() {
    let mut account = default_validation_account();
    account.cash_balance_assets = vec!["USDT".to_owned()];

    let bundle = connector_validation_bundle(account, default_validation_instance());
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("does not support `cash_balance_assets`")
    ));
}

#[test]
fn accepts_supported_binance_cash_balance_assets() {
    let mut account = intrabar_validation_account();
    account.cash_balance_assets = vec!["usdt".to_owned(), "FDUSD".to_owned()];

    let bundle = connector_validation_bundle(account, intrabar_validation_instance());
    assert!(bundle.validate().is_ok());
}

#[test]
fn rejects_unsupported_binance_cash_balance_assets() {
    let mut account = intrabar_validation_account();
    account.cash_balance_assets = vec!["BTC".to_owned()];

    let bundle = connector_validation_bundle(account, intrabar_validation_instance());
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("unsupported `cash_balance_assets`")
    ));
}
