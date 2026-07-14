//! Round-trip and atomic-write tests for configuration documents.

use crate::{
    ConfigError, DataPlaneWatchlistEntry, GlobalConfig, InstanceConfig, render_new_document,
    render_updated_document, write_atomic,
};
use openticker_core::Timeframe;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const WRITING_INSTANCE_FIXTURE: &str = r#"# Strategy bot definition
id = "bot-writing"
enabled = true # toggled by ops
market = "crypto"
symbols = ["BTCUSDT", "ETHUSDT"]
timeframe = "5m"
account = "binance-demo"
data_connector = "binance"
execution_connector = "binance"
strategy = "single_indicator_signal"
signal_mode = "confirmed_only"
polling_enabled = true
polling_interval_ms = 1000

[[indicators]]
id = "trend-1"
type = "sma_crossover"
signal_policy = "confirmed_required"

[indicators.params]
fast_length = 10
slow_length = 30

[budget]
pct = 10.0

[risk]
profile = "crypto-default"

[risk.overrides]
stale_data_ms = 30000
"#;

// Modeled on config/openticker.toml at the repo root.

const WRITING_GLOBAL_FIXTURE: &str = r#"# Service-wide settings
[service]
environment = "dev"
data_dir = "./var"
bot_dir = "./config/bots"

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
prune_removed_bots_on_startup = true

[observability]
log_level = "info"
metrics_enabled = true
metrics_path = "/metrics"

[safety]
require_explicit_live_enable = true
default_start_paused_if_recovery_uncertain = true

[data_plane]
default_polling_interval_ms = 5000
default_retention = 500
"#;

#[test]
fn render_updated_document_preserves_comments_and_unrelated_keys() {
    let mut next: InstanceConfig =
        toml::from_str(WRITING_INSTANCE_FIXTURE).expect("fixture should parse");
    next.budget.pct = 25.0;

    let rendered =
        render_updated_document(WRITING_INSTANCE_FIXTURE, &next).expect("document should render");

    assert!(rendered.contains("# Strategy bot definition"));
    assert!(rendered.contains("enabled = true # toggled by ops"));
    assert!(rendered.contains(r#"symbols = ["BTCUSDT", "ETHUSDT"]"#));
    assert!(rendered.contains("polling_interval_ms = 1000"));
    assert!(rendered.contains("pct = 25.0"));
    assert!(!rendered.contains("pct = 10.0"));
}

#[test]
fn render_updated_document_removes_dropped_keys() {
    let mut next: InstanceConfig =
        toml::from_str(WRITING_INSTANCE_FIXTURE).expect("fixture should parse");
    assert_eq!(next.risk.overrides.stale_data_ms, Some(30_000));
    next.risk.overrides.stale_data_ms = None;

    let rendered =
        render_updated_document(WRITING_INSTANCE_FIXTURE, &next).expect("document should render");

    assert!(!rendered.contains("stale_data_ms"));
    let reparsed: InstanceConfig = toml::from_str(&rendered).expect("rendered should parse");
    assert_eq!(reparsed.risk.overrides.stale_data_ms, None);
}

#[test]
fn render_updated_document_keeps_absent_default_keys_absent() {
    let raw = WRITING_INSTANCE_FIXTURE
        .replace("polling_enabled = true\n", "")
        .replace("polling_interval_ms = 1000\n", "");
    let mut next: InstanceConfig = toml::from_str(&raw).expect("fixture should parse");
    assert!(next.polling_enabled);
    assert_eq!(next.polling_interval_ms, 1_000);
    next.budget.pct = 42.5;

    let rendered = render_updated_document(&raw, &next).expect("document should render");

    assert!(!rendered.contains("polling_enabled"));
    assert!(!rendered.contains("polling_interval_ms"));
    assert!(rendered.contains("pct = 42.5"));
}

#[test]
fn render_updated_document_preserves_unknown_keys_and_their_comments() {
    let raw = WRITING_INSTANCE_FIXTURE.replace(
        "polling_interval_ms = 1000\n",
        "polling_interval_ms = 1000\n\n# Reserved for phase-3 tooling\nfuture_flag = \"keep-me\"\n",
    );
    let mut next: InstanceConfig = toml::from_str(&raw).expect("fixture should parse");
    next.budget.pct = 33.0;

    let rendered = render_updated_document(&raw, &next).expect("document should render");

    assert!(rendered.contains("# Reserved for phase-3 tooling"));
    assert!(rendered.contains(r#"future_flag = "keep-me""#));
    assert!(rendered.contains("pct = 33.0"));
    assert!(!rendered.contains("pct = 10.0"));
}

#[test]
fn render_updated_document_preserves_dotted_and_inline_table_styles() {
    let raw = WRITING_INSTANCE_FIXTURE
        .replace(
            "polling_interval_ms = 1000\n",
            "polling_interval_ms = 1000\nexecution_constraints = { min_quantity = 1.0 }\nrisk.profile = \"crypto-default\"\n",
        )
        .replace("\n[risk]\nprofile = \"crypto-default\"\n", "")
        .replace("\n[risk.overrides]\nstale_data_ms = 30000\n", "");
    let mut next: InstanceConfig = toml::from_str(&raw).expect("fixture should parse");
    next.risk.profile = "crypto-aggressive".to_owned();
    next.execution_constraints.min_quantity = Some(2.5);

    let rendered = render_updated_document(&raw, &next).expect("document should render");

    assert!(rendered.contains(r#"risk.profile = "crypto-aggressive""#));
    assert!(rendered.contains("execution_constraints = { min_quantity = 2.5 }"));
    let reparsed: InstanceConfig = toml::from_str(&rendered).expect("rendered should parse");
    assert_eq!(reparsed.risk.profile, "crypto-aggressive");
    assert_eq!(reparsed.execution_constraints.min_quantity, Some(2.5));
}

#[test]
fn render_updated_document_rejects_invalid_existing_toml() {
    let next: InstanceConfig =
        toml::from_str(WRITING_INSTANCE_FIXTURE).expect("fixture should parse");

    let result = render_updated_document("id = \"unterminated", &next);

    assert!(matches!(result, Err(ConfigError::Render { .. })));
}

#[test]
fn render_updated_document_does_not_materialize_absent_table_defaults() {
    let raw = WRITING_GLOBAL_FIXTURE.replace(
        "\n[data_plane]\ndefault_polling_interval_ms = 5000\ndefault_retention = 500\n",
        "",
    );
    assert!(!raw.contains("data_plane"));
    let mut next: GlobalConfig = toml::from_str(&raw).expect("fixture should parse");
    next.data_plane.default_retention = 750;

    let rendered = render_updated_document(&raw, &next).expect("document should render");

    assert!(rendered.contains("default_retention = 750"));
    assert!(!rendered.contains("default_polling_interval_ms"));
    assert!(!rendered.contains("watchlist"));
    let reparsed: GlobalConfig = toml::from_str(&rendered).expect("rendered should parse");
    assert_eq!(reparsed.data_plane.default_retention, 750);
    assert_eq!(reparsed.data_plane.default_polling_interval_ms, 5_000);
    assert!(reparsed.data_plane.watchlist.is_empty());
}

#[test]
fn render_updated_document_replaces_changed_indicators() {
    let mut next: InstanceConfig =
        toml::from_str(WRITING_INSTANCE_FIXTURE).expect("fixture should parse");
    next.indicators[0]
        .params
        .insert("fast_length".to_owned(), toml::Value::Integer(12));
    let mut second = next.indicators[0].clone();
    second.id = "trend-2".to_owned();
    second.indicator_type = "rsi_threshold".to_owned();
    next.indicators.push(second);

    let rendered =
        render_updated_document(WRITING_INSTANCE_FIXTURE, &next).expect("document should render");

    assert_eq!(rendered.matches("[[indicators]]").count(), 2);
    let reparsed: InstanceConfig = toml::from_str(&rendered).expect("rendered should parse");
    assert_eq!(reparsed.indicators.len(), 2);
    assert_eq!(
        reparsed.indicators[0].params.get("fast_length"),
        Some(&toml::Value::Integer(12))
    );
    assert_eq!(reparsed.indicators[1].id, "trend-2");
    assert_eq!(reparsed.indicators[1].indicator_type, "rsi_threshold");
}

#[test]
fn render_updated_document_round_trips_realistic_configs() {
    let mut next_instance: InstanceConfig =
        toml::from_str(WRITING_INSTANCE_FIXTURE).expect("instance fixture should parse");
    next_instance.symbols = vec!["BTCUSDT".to_owned(), "SOLUSDT".to_owned()];
    next_instance.polling_interval_ms = 2_000;
    next_instance.warmup_target_bars = Some(120);
    next_instance.risk.overrides.max_open_positions = Some(3);

    let rendered = render_updated_document(WRITING_INSTANCE_FIXTURE, &next_instance)
        .expect("instance document should render");
    let reparsed: InstanceConfig = toml::from_str(&rendered).expect("rendered should parse");
    assert_eq!(
        serde_json::to_value(&reparsed).expect("reparsed should serialize"),
        serde_json::to_value(&next_instance).expect("next should serialize")
    );

    let mut next_global: GlobalConfig =
        toml::from_str(WRITING_GLOBAL_FIXTURE).expect("global fixture should parse");
    next_global.http.request_log = false;
    next_global.storage.busy_timeout_ms = 7_500;
    next_global
        .data_plane
        .watchlist
        .push(DataPlaneWatchlistEntry {
            account: "binance-demo".to_owned(),
            symbol: "BTCUSDT".to_owned(),
            timeframe: Timeframe::M5,
            polling_interval_ms: Some(10_000),
            retention: None,
        });

    let rendered = render_updated_document(WRITING_GLOBAL_FIXTURE, &next_global)
        .expect("global document should render");
    assert!(rendered.contains("# Service-wide settings"));
    let reparsed: GlobalConfig = toml::from_str(&rendered).expect("rendered should parse");
    assert_eq!(
        serde_json::to_value(&reparsed).expect("reparsed should serialize"),
        serde_json::to_value(&next_global).expect("next should serialize")
    );
}

#[test]
fn render_new_document_round_trips() {
    let value: InstanceConfig =
        toml::from_str(WRITING_INSTANCE_FIXTURE).expect("fixture should parse");

    let rendered = render_new_document(&value).expect("document should render");

    let reparsed: InstanceConfig = toml::from_str(&rendered).expect("rendered should parse");
    assert_eq!(
        serde_json::to_value(&reparsed).expect("reparsed should serialize"),
        serde_json::to_value(&value).expect("value should serialize")
    );
}

#[test]
fn write_atomic_leaves_only_target_file() {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("openticker-config-write-atomic-{timestamp}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");

    let target = dir.join("bot-writing.toml");
    write_atomic(&target, WRITING_INSTANCE_FIXTURE).expect("write should succeed");

    let raw = fs::read_to_string(&target).expect("target should be readable");
    let reparsed: InstanceConfig = toml::from_str(&raw).expect("target should parse");
    assert_eq!(reparsed.id, "bot-writing");

    // Overwriting an existing file must also work and leave no temp file behind.
    write_atomic(
        &target,
        &WRITING_INSTANCE_FIXTURE.replace("pct = 10.0", "pct = 20.0"),
    )
    .expect("overwrite should succeed");
    let raw = fs::read_to_string(&target).expect("target should be readable");
    let reparsed: InstanceConfig = toml::from_str(&raw).expect("target should parse");
    assert!((reparsed.budget.pct - 20.0).abs() < f64::EPSILON);

    let entries: Vec<String> = fs::read_dir(&dir)
        .expect("temp dir should be listable")
        .map(|entry| {
            entry
                .expect("entry should be readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(entries, vec!["bot-writing.toml".to_owned()]);
}
