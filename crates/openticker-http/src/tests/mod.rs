mod activity;
mod bots;
mod config;
mod data_streams;
mod ledger_connectors;
mod platform;
mod security;
mod snapshots;
mod web_ui;

use crate::openapi::HTTP_SURFACE_ROUTES;
use crate::*;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, ExecutionConstraintsConfig, GlobalConfig,
    HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig, ObservabilityConfig,
    RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode, StorageConfig,
    load_from_dir,
};
use openticker_core::{ExecutionMode, MarketType, OhlcvBar, Timeframe};
use openticker_dataplane::StreamKey;
use openticker_runtime::Runtime;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tower::util::ServiceExt;

/// Upper bound when collecting response bodies in tests (16 MB). Kept
/// explicit instead of `usize::MAX` so that response-size discipline is
/// enforced if streaming or otherwise unbounded endpoints are ever added:
/// a runaway body fails the test rather than exhausting memory.
const MAX_TEST_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

async fn start_instance(app: &axum::Router, instance_id: &str) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/bots/{instance_id}/start"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn replay_bars_for_instance(app: &axum::Router, instance_id: &str) {
    for close in replay_closes() {
        let body = serde_json::to_vec(&json!({
            "bar": {
                "timestamp": "2030-01-01T00:00:00Z",
                "open": close,
                "high": close + 0.9,
                "low": close - 0.9,
                "close": close,
                "volume": 1000.0
            },
            "phase": "confirmed"
        }))
        .unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/bots/{instance_id}/simulate-bar"))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}

async fn cancel_all_orders_for_instance(app: &axum::Router, instance_id: &str) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/bots/{instance_id}/cancel-open-orders"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn close_all_positions_for_instance(app: &axum::Router, instance_id: &str) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/bots/{instance_id}/close-positions"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn get_json(app: &axum::Router, path: &str) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn fixture_state() -> HttpState {
    let bundle = fixture_bundle();
    HttpState::new(Runtime::from_config(&bundle))
}

fn fixture_state_with_cycle_trace() -> HttpState {
    let bundle = fixture_bundle();
    let mut runtime = Runtime::from_config(&bundle);
    runtime
        .start_instance("aapl")
        .expect("fixture instance should start");
    runtime
        .process_manual_signal(
            "aapl",
            openticker_core::IndicatorSignal::BuyConfirmed,
            123.45,
            chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .expect("timestamp should parse")
                .with_timezone(&chrono::Utc),
        )
        .expect("fixture signal should persist a cycle trace");
    HttpState::new(runtime)
}

fn fixture_state_with_config() -> HttpState {
    let bundle = fixture_bundle();
    HttpState::with_config(
        Runtime::from_config(&bundle),
        PathBuf::from("./config"),
        bundle,
    )
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
            data_plane: openticker_config::DataPlaneConfig::default(),
        },
        accounts: vec![AccountConfig {
            id: "alpaca-paper".to_owned(),
            kind: "alpaca".to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: None,
            api_secret_env: None,
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
                metadata_filters: openticker_core::IndicatorSignalMetadataFilters::default(),
                params: toml::Table::new(),
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

fn create_managed_config_dir(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("openticker-http-{prefix}-{timestamp}"));
    fs::create_dir_all(root.join("accounts")).expect("accounts dir should be created");
    fs::create_dir_all(root.join("risk")).expect("risk dir should be created");
    fs::create_dir_all(root.join("bots")).expect("instances dir should be created");
    root
}

fn write_managed_global(config_dir: &Path, storage_path: &Path, bot_dir: &str) {
    fs::write(
        config_dir.join("openticker.toml"),
        format!(
            "[service]\nenvironment = \"test\"\ndata_dir = \"./var\"\nbot_dir = \"{bot_dir}\"\n\n[http]\nenabled = true\nbind = \"127.0.0.1:8080\"\nrequest_log = true\nopenapi_enabled = true\nopenapi_path = \"/openapi.json\"\n\n[storage]\nkind = \"sqlite\"\npath = \"{}\"\nbusy_timeout_ms = 5000\n\n[observability]\nlog_level = \"info\"\nmetrics_enabled = true\nmetrics_path = \"/metrics\"\n\n[safety]\nrequire_explicit_live_enable = true\ndefault_start_paused_if_recovery_uncertain = true\n",
            storage_path.display()
        ),
    )
    .expect("global config should be written");
}

fn write_managed_config(
    config_dir: &Path,
    storage_path: &Path,
    execution_connector: &str,
    timeframe: &str,
) {
    write_managed_global(config_dir, storage_path, "./bots");

    fs::write(
        config_dir.join("accounts").join("alpaca-paper.toml"),
        "id = \"alpaca-paper\"\nkind = \"alpaca\"\nmode = \"paper\"\napi_key_env = \"PATH\"\napi_secret_env = \"PATH\"\ntotal_budget_usd = 20000.0\n",
    )
    .expect("account config should be written");

    fs::write(
        config_dir.join("risk").join("equities-default.toml"),
        "id = \"equities-default\"\nmax_daily_loss_pct = 2.0\nmax_open_positions = 2\ntarget_order_notional_usd = 1000.0\nmax_order_notional_usd = 1000.0\nmax_spread_bps = 20\nmax_slippage_bps = 30\nstale_data_ms = 3000\ncooldown_after_reject_ms = 1000\n",
    )
    .expect("risk config should be written");

    write_managed_instance(config_dir, execution_connector, timeframe, None, None);
}

fn write_managed_instance(
    config_dir: &Path,
    execution_connector: &str,
    timeframe: &str,
    polling_enabled: Option<bool>,
    polling_interval_ms: Option<u64>,
) {
    write_managed_instance_with_symbol(
        config_dir,
        "AAPL",
        execution_connector,
        timeframe,
        polling_enabled,
        polling_interval_ms,
    );
}

fn write_managed_instance_with_symbol(
    config_dir: &Path,
    symbol: &str,
    execution_connector: &str,
    timeframe: &str,
    polling_enabled: Option<bool>,
    polling_interval_ms: Option<u64>,
) {
    let polling_enabled_line = polling_enabled
        .map(|enabled| format!("polling_enabled = {enabled}\n"))
        .unwrap_or_default();
    let polling_interval_line = polling_interval_ms
        .map(|interval| format!("polling_interval_ms = {interval}\n"))
        .unwrap_or_default();
    fs::write(
        config_dir.join("bots").join("aapl.toml"),
        format!(
            "id = \"aapl\"\nenabled = true\nmarket = \"equities\"\nsymbols = [\"{symbol}\"]\ntimeframe = \"{timeframe}\"\naccount = \"alpaca-paper\"\ndata_connector = \"alpaca\"\nexecution_connector = \"{execution_connector}\"\nstrategy = \"single_indicator_signal\"\nsignal_mode = \"confirmed_only\"\n{polling_enabled_line}{polling_interval_line}\n[[indicators]]\nid = \"trend-1\"\ntype = \"sma_crossover\"\nsignal_policy = \"confirmed_required\"\n\n[indicators.params]\nfast_length = 10\nslow_length = 30\n\n[budget]\npct = 100.0\n\n[risk]\nprofile = \"equities-default\"\n"
        ),
    )
    .expect("instance config should be written");
}

fn write_managed_account(config_dir: &Path, api_key_env: &str, api_secret_env: &str) {
    write_managed_account_with_reconciliation(config_dir, api_key_env, api_secret_env, false, None);
}

fn write_managed_account_with_reconciliation(
    config_dir: &Path,
    api_key_env: &str,
    api_secret_env: &str,
    reconciliation_remote_snapshot: bool,
    reconciliation_base_url: Option<&str>,
) {
    let remote_line = if reconciliation_remote_snapshot {
        "reconciliation_remote_snapshot = true\n"
    } else {
        ""
    };
    let base_url_line = reconciliation_base_url
        .map(|base_url| format!("reconciliation_base_url = \"{base_url}\"\n"))
        .unwrap_or_default();
    fs::write(
        config_dir.join("accounts").join("alpaca-paper.toml"),
        format!(
            "id = \"alpaca-paper\"\nkind = \"alpaca\"\nmode = \"paper\"\napi_key_env = \"{api_key_env}\"\napi_secret_env = \"{api_secret_env}\"\ntotal_budget_usd = 20000.0\n{remote_line}{base_url_line}"
        ),
    )
    .expect("account config should be written");
}

fn test_bar_at(timestamp: &str, close: f64) -> OhlcvBar {
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

fn existing_env_var_name_except(exclude: &str) -> &'static str {
    ["HOME", "USER", "SHELL", "TMPDIR", "PATH"]
        .into_iter()
        .find(|candidate| *candidate != exclude && std::env::var(candidate).is_ok())
        .expect("test environment should expose at least one alternate env var")
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
