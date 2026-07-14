//! End-to-end tests for configuration loading and dotenv handling.

use super::support::{create_fixture_dir, write_file};
use crate::{ConfigError, load_from_dir};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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
fn malformed_dotenv_in_config_dir_fails_loudly() {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let config_dir =
        std::env::temp_dir().join(format!("openticker-config-dotenv-malformed-{timestamp}"));
    fs::create_dir_all(&config_dir).expect("config dir should be created");

    // A line with no `=` is a parse error for dotenvy (not a NotFound IO error),
    // so loading must surface it rather than silently leaving secrets unset.
    write_file(config_dir.join(".env"), "THIS LINE HAS NO EQUALS SIGN\n");

    let result = load_from_dir(&config_dir);
    let error = result.expect_err("malformed .env should fail to load");
    assert!(
        matches!(error, ConfigError::Dotenv { .. }),
        "expected ConfigError::Dotenv, got: {error:?}"
    );

    fs::remove_dir_all(&config_dir).ok();
}

#[test]
fn classify_dotenv_result_treats_not_found_as_benign() {
    let not_found = dotenvy::Error::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
    assert!(
        crate::loading::classify_dotenv_result(Path::new("/tmp/.env"), Err(not_found)).is_ok(),
        "a NotFound dotenv error should be treated as benign (optional file)"
    );
}

#[test]
fn classify_dotenv_result_propagates_other_errors() {
    let permission_denied =
        dotenvy::Error::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    let result =
        crate::loading::classify_dotenv_result(Path::new("/tmp/.env"), Err(permission_denied));
    let error = result.expect_err("a non-NotFound dotenv error must propagate");
    assert!(matches!(error, ConfigError::Dotenv { .. }));
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
