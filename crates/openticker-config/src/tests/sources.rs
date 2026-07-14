//! Source-file discovery and parsed-id mapping tests.

use super::support::{create_fixture_dir, write_file};
use crate::load_sources_from_dir;

#[test]
#[allow(clippy::too_many_lines)]
fn load_sources_from_dir_maps_entities_by_parsed_id() {
    let fixture_dir = create_fixture_dir("sources-by-id");

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
        fixture_dir.join("accounts").join("weird-account-name.toml"),
        r#"
id = "real-account-id"
kind = "binance"
mode = "paper"
use_demo_mode = true
api_key_env = "PATH"
api_secret_env = "PATH"
total_budget_usd = 20000.0
"#,
    );

    write_file(
        fixture_dir.join("risk").join("weird-risk-name.toml"),
        r#"
id = "real-risk-id"
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
        fixture_dir.join("bots").join("weird-name.toml"),
        r#"
id = "real-id"
enabled = true
market = "crypto"
symbols = ["BTCUSDT"]
timeframe = "5m"
account = "real-account-id"
data_connector = "binance"
execution_connector = "binance"
strategy = "single_indicator_signal"
signal_mode = "confirmed_only"

[[indicators]]
id = "trend-1"
type = "sma_crossover"

[indicators.params]
fast_length = 10
slow_length = 30

[budget]
pct = 100.0

[risk]
profile = "real-risk-id"
"#,
    );

    let sources = load_sources_from_dir(&fixture_dir).expect("sources should load");

    assert_eq!(sources.config_dir, fixture_dir);
    assert_eq!(sources.accounts_dir, fixture_dir.join("accounts"));
    assert_eq!(sources.risk_dir, fixture_dir.join("risk"));
    assert_eq!(sources.bots_dir, fixture_dir.join("bots"));

    let instance = sources
        .instance_by_id("real-id")
        .expect("instance should be located by parsed id");
    assert!(instance.path.ends_with("weird-name.toml"));
    assert!(instance.raw.contains(r#"id = "real-id""#));
    assert!(sources.instance_by_id("weird-name").is_none());

    let account = sources
        .account_by_id("real-account-id")
        .expect("account should be located by parsed id");
    assert!(account.path.ends_with("weird-account-name.toml"));
    assert!(sources.account_by_id("weird-account-name").is_none());

    let risk = sources
        .risk_profile_by_id("real-risk-id")
        .expect("risk profile should be located by parsed id");
    assert!(risk.path.ends_with("weird-risk-name.toml"));
    assert!(sources.risk_profile_by_id("weird-risk-name").is_none());

    let bundle = sources.to_bundle();
    assert_eq!(bundle.instances.len(), 1);
    assert_eq!(bundle.instances[0].id, "real-id");
    assert_eq!(bundle.accounts.len(), 1);
    assert_eq!(bundle.accounts[0].id, "real-account-id");
    assert_eq!(bundle.risk_profiles.len(), 1);
    assert_eq!(bundle.risk_profiles[0].id, "real-risk-id");
}
