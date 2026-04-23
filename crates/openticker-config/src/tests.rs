use super::*;
use openticker_core::{
    ExecutionMode, IndicatorRole, IndicatorSignalMetadataFilters, IndicatorSignalPolicy,
    MarketType, Timeframe,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Table;

#[test]
fn loads_and_validates_configuration_bundle() {
    let fixture_dir = create_fixture_dir("valid");

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
        fixture_dir
            .join("bots")
            .join("aapl-strong-buy-strong-sell-paper.toml"),
        r#"
id = "aapl-strong-buy-strong-sell-paper"
enabled = true
market = "equities"
symbols = ["AAPL"]
timeframe = "1m"
account = "alpaca-paper"
data_connector = "alpaca"
execution_connector = "alpaca"
strategy = "single_indicator_signal"
signal_mode = "confirmed_only"

[[indicators]]
id = "trend-1"
type = "sma_crossover"
signal_policy = "confirmed_required"

[indicators.params]
fast_length = 10
slow_length = 30

[budget]
pct = 100.0

[risk]
profile = "equities-default"
"#,
    );

    let bundle = load_from_dir(&fixture_dir).expect("config should load");
    let effective = bundle.effective_config();
    assert_eq!(effective.instances.len(), 1);
    assert_eq!(effective.accounts.len(), 1);
    assert!(effective.accounts[0].secret_status.api_key_present);
    assert!(effective.accounts[0].secret_status.api_secret_present);
    assert!(!effective.accounts[0].reconciliation_remote_snapshot);
    assert!(!effective.accounts[0].execution_remote_submission);
    assert!(effective.accounts[0].reconciliation_base_url.is_none());
}

#[test]
fn loads_env_vars_from_project_root_dotenv() {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let project_root =
        std::env::temp_dir().join(format!("openticker-config-dotenv-parent-{timestamp}"));
    let config_dir = project_root.join("config");

    fs::create_dir_all(config_dir.join("accounts")).expect("accounts dir should be created");
    fs::create_dir_all(config_dir.join("risk")).expect("risk dir should be created");
    fs::create_dir_all(config_dir.join("bots")).expect("instances dir should be created");

    let api_key_var = format!("OPENTICKER_TEST_API_KEY_{timestamp}");
    let api_secret_var = format!("OPENTICKER_TEST_API_SECRET_{timestamp}");
    assert!(std::env::var(&api_key_var).is_err());
    assert!(std::env::var(&api_secret_var).is_err());

    write_file(
        project_root.join(".env"),
        &format!("{api_key_var}=paper_key\n{api_secret_var}=paper_secret\n"),
    );

    write_file(
        config_dir.join("openticker.toml"),
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
        config_dir.join("accounts").join("alpaca-paper.toml"),
        &format!(
            r#"
id = "alpaca-paper"
kind = "alpaca"
mode = "paper"
api_key_env = "{api_key_var}"
api_secret_env = "{api_secret_var}"
total_budget_usd = 20000.0
"#
        ),
    );

    write_file(
        config_dir.join("risk").join("equities-default.toml"),
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
        config_dir.join("bots").join("aapl-paper.toml"),
        r#"
id = "aapl-paper"
enabled = true
market = "equities"
symbols = ["AAPL"]
timeframe = "1m"
account = "alpaca-paper"
data_connector = "alpaca"
execution_connector = "alpaca"
strategy = "single_indicator_signal"
signal_mode = "confirmed_only"

[[indicators]]
id = "trend-1"
type = "sma_crossover"
signal_policy = "confirmed_required"

[indicators.params]
fast_length = 10
slow_length = 30

[budget]
pct = 100.0

[risk]
profile = "equities-default"
"#,
    );

    let bundle = load_from_dir(&config_dir).expect("config should load with dotenv secrets");
    let effective = bundle.effective_config();
    assert!(effective.accounts[0].secret_status.api_key_present);
    assert!(effective.accounts[0].secret_status.api_secret_present);
}

#[test]
#[allow(clippy::too_many_lines)]
fn effective_config_never_exposes_secret_values() {
    let fixture_dir = create_fixture_dir("effective-config-redaction");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let api_key_var = format!("OPENTICKER_REDACT_KEY_{timestamp}");
    let api_secret_var = format!("OPENTICKER_REDACT_SECRET_{timestamp}");
    let api_key_value = format!("paper_key_value_{timestamp}");
    let api_secret_value = format!("paper_secret_value_{timestamp}");

    write_file(
        fixture_dir.join(".env"),
        &format!("{api_key_var}={api_key_value}\n{api_secret_var}={api_secret_value}\n"),
    );

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
        &format!(
            r#"
id = "alpaca-paper"
kind = "alpaca"
mode = "paper"
api_key_env = "{api_key_var}"
api_secret_env = "{api_secret_var}"
total_budget_usd = 20000.0
"#
        ),
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
execution_connector = "alpaca"
strategy = "single_indicator_signal"
signal_mode = "confirmed_only"

[[indicators]]
id = "trend-1"
type = "sma_crossover"
signal_policy = "confirmed_required"

[indicators.params]
fast_length = 10
slow_length = 30

[budget]
pct = 100.0

[risk]
profile = "equities-default"
"#,
    );

    let bundle = load_from_dir(&fixture_dir).expect("config should load with dotenv secrets");
    let serialized = serde_json::to_string(&bundle.effective_config())
        .expect("effective config should serialize");

    assert!(serialized.contains("secret_status"));
    assert!(!serialized.contains("api_key_env"));
    assert!(!serialized.contains("api_secret_env"));
    assert!(!serialized.contains(&api_key_var));
    assert!(!serialized.contains(&api_secret_var));
    assert!(!serialized.contains(&api_key_value));
    assert!(!serialized.contains(&api_secret_value));
}

#[test]
fn loads_instances_from_configured_bot_dir() {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let project_root =
        std::env::temp_dir().join(format!("openticker-config-custom-bot-dir-{timestamp}"));
    let config_dir = project_root.join("config");
    let bots_dir = project_root.join("config").join("custom-bots");

    fs::create_dir_all(config_dir.join("accounts")).expect("accounts dir should be created");
    fs::create_dir_all(config_dir.join("risk")).expect("risk dir should be created");
    fs::create_dir_all(&bots_dir).expect("custom bots dir should be created");

    write_file(
        config_dir.join("openticker.toml"),
        r#"
[service]
environment = "dev"
data_dir = "./var"
bot_dir = "./config/custom-bots"

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
        config_dir.join("accounts").join("alpaca-paper.toml"),
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
        config_dir.join("risk").join("equities-default.toml"),
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
        bots_dir.join("aapl-paper.toml"),
        r#"
id = "aapl-paper"
enabled = true
market = "equities"
symbols = ["AAPL"]
timeframe = "1m"
account = "alpaca-paper"
data_connector = "alpaca"
execution_connector = "alpaca"
strategy = "single_indicator_signal"
signal_mode = "confirmed_only"

[[indicators]]
id = "trend-1"
type = "sma_crossover"
signal_policy = "confirmed_required"

[indicators.params]
fast_length = 10
slow_length = 30

[budget]
pct = 100.0

[risk]
profile = "equities-default"
"#,
    );

    let bundle = load_from_dir(&config_dir).expect("config should load from configured bot_dir");
    assert_eq!(bundle.instances.len(), 1);
    assert_eq!(bundle.instances[0].id, "aapl-paper");
}

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
fn accepts_registered_indicator_type_from_manifest() {
    let mut instance = intrabar_validation_instance();
    instance.indicators[0].indicator_type = "rsi_threshold".to_owned();

    let bundle = connector_validation_bundle(intrabar_validation_account(), instance);
    assert!(bundle.validate().is_ok());
}

#[test]
fn rejects_unknown_indicator_type_from_manifest_registry() {
    let mut instance = default_validation_instance();
    instance.indicators[0].indicator_type = "unknown_indicator".to_owned();

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("unsupported indicator type")
    ));
}

#[test]
fn rejects_disallowed_role_override() {
    let mut instance = default_validation_instance();
    instance.indicators[0].role = Some(IndicatorRole::Filter);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("does not allow configured role")
    ));
}

#[test]
fn rejects_preview_policy_when_signal_mode_is_confirmed_only() {
    let mut instance = default_validation_instance();
    instance.indicators[0].indicator_type = "sma_crossover".to_owned();
    instance.indicators[0].signal_policy = Some(IndicatorSignalPolicy::PreviewAllowed);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("signal_mode is `confirmed_only`")
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
fn rejects_weight_when_strategy_is_not_consensus() {
    let mut instance = default_validation_instance();
    instance.indicators[0].weight = Some(1.0);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("sets weight but strategy")
    ));
}

#[test]
fn accepts_weight_for_consensus_strategy() {
    let mut instance = default_validation_instance();
    instance.strategy = "consensus".to_owned();
    instance.indicators[0].weight = Some(1.5);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(bundle.validate().is_ok());
}

#[test]
fn accepts_indicator_metadata_filters_without_capability_requirements() {
    let mut instance = default_validation_instance();
    instance.indicators[0]
        .metadata_filters
        .entry
        .allowed_strengths = vec![openticker_core::SignalStrength::Strong];
    instance.indicators[0]
        .metadata_filters
        .entry
        .allowed_reason_codes = vec!["supertrend_cross_up_sma_filter".to_owned()];

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(bundle.validate().is_ok());
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
fn rejects_warmup_target_bars_below_indicator_minimum() {
    let mut instance = default_validation_instance();
    instance.warmup_target_bars = Some(10);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("warmup_target_bars")
    ));
}

#[test]
fn accepts_warmup_target_bars_at_or_above_indicator_minimum() {
    let mut instance = default_validation_instance();
    instance.warmup_target_bars = Some(50);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(bundle.validate().is_ok());
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

#[test]
fn accepts_data_plane_watchlist_using_defaults() {
    let mut bundle =
        connector_validation_bundle(default_validation_account(), default_validation_instance());
    bundle.global.data_plane = DataPlaneConfig {
        default_polling_interval_ms: 5_000,
        default_retention: 500,
        watchlist: vec![DataPlaneWatchlistEntry {
            account: "alpaca-paper".to_owned(),
            symbol: "SPY".to_owned(),
            timeframe: Timeframe::M1,
            polling_interval_ms: None,
            retention: None,
        }],
    };

    assert!(bundle.validate().is_ok());
}

#[test]
fn rejects_data_plane_watchlist_with_unknown_account() {
    let mut bundle =
        connector_validation_bundle(default_validation_account(), default_validation_instance());
    bundle
        .global
        .data_plane
        .watchlist
        .push(DataPlaneWatchlistEntry {
            account: "missing-account".to_owned(),
            symbol: "SPY".to_owned(),
            timeframe: Timeframe::M1,
            polling_interval_ms: None,
            retention: None,
        });

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("unknown account")
    ));
}

#[test]
fn rejects_data_plane_watchlist_with_invalid_interval_or_retention() {
    let mut bundle =
        connector_validation_bundle(default_validation_account(), default_validation_instance());
    bundle
        .global
        .data_plane
        .watchlist
        .push(DataPlaneWatchlistEntry {
            account: "alpaca-paper".to_owned(),
            symbol: "SPY".to_owned(),
            timeframe: Timeframe::M1,
            polling_interval_ms: Some(500),
            retention: Some(5),
        });

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("polling_interval_ms")
    ));

    bundle.global.data_plane.watchlist[0].polling_interval_ms = Some(1_000);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("retention")
    ));
}

fn connector_validation_bundle(account: AccountConfig, instance: InstanceConfig) -> ConfigBundle {
    ConfigBundle {
        global: GlobalConfig {
            service: ServiceConfig {
                environment: "test".to_owned(),
                data_dir: "./var".into(),
                bot_dir: "./bots".into(),
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
                path: "./var/openticker.db".into(),
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
        accounts: vec![account],
        risk_profiles: vec![RiskProfileConfig {
            id: "equities-default".to_owned(),
            max_daily_loss_pct: 2.0,
            max_open_positions: 5,
            target_order_notional_usd: Some(2_500.0),
            max_order_notional_usd: 2_500.0,
            max_spread_bps: 20,
            max_slippage_bps: 30,
            stale_data_ms: 3_000,
            cooldown_after_reject_ms: 15_000,
        }],
        instances: vec![instance],
    }
}

fn default_validation_account() -> AccountConfig {
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
    }
}

fn default_validation_instance() -> InstanceConfig {
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
    }
}

fn intrabar_validation_account() -> AccountConfig {
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
    }
}

fn intrabar_validation_instance() -> InstanceConfig {
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
    }
}

fn create_fixture_dir(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("openticker-config-{prefix}-{timestamp}"));
    fs::create_dir_all(root.join("accounts")).expect("accounts dir should be created");
    fs::create_dir_all(root.join("risk")).expect("risk dir should be created");
    fs::create_dir_all(root.join("bots")).expect("instances dir should be created");
    root
}

fn write_file(path: PathBuf, content: &str) {
    fs::write(path, content).expect("fixture file should be written");
}
