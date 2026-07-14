//! Tests that the serialized effective config never exposes secret values.

use super::support::{create_fixture_dir, write_file};
use crate::load_from_dir;
use std::time::{SystemTime, UNIX_EPOCH};

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
