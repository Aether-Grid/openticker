use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
    GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
    ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode,
    StorageConfig,
};
use openticker_core::{ExecutionMode, IndicatorSignalMetadataFilters, MarketType, Timeframe};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Table;

/// Upper bound when collecting response bodies in tests (16 MB). Kept
/// explicit instead of `usize::MAX` so that response-size discipline is
/// enforced if streaming or otherwise unbounded endpoints are ever added.
// Not every test binary that includes `common` reads response bodies.
#[allow(dead_code)]
pub(crate) const MAX_TEST_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn fixture_bundle() -> ConfigBundle {
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
                path: temp_db_path("http-tests"),
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

#[allow(dead_code)]
pub(crate) fn synthetic_bundle(bot_count: usize) -> ConfigBundle {
    let mut bundle = fixture_bundle();
    let base = bundle.instances[0].clone();
    for idx in 1..bot_count {
        let mut twin = base.clone();
        twin.id = format!("bot-{idx:04}");
        bundle.instances.push(twin);
    }
    bundle
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-{prefix}-{timestamp}.db"))
}

/// Removes the materialized config directory (including its sqlite storage)
/// when dropped.
#[allow(dead_code)]
pub(crate) struct ConfigDirGuard {
    path: PathBuf,
}

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Materializes a managed config directory equivalent to [`fixture_bundle`]
/// (openticker.toml + accounts/ + risk/ + bots/), written through
/// `render_new_document` so the writer participates in the fixtures too.
/// Storage points inside the directory to keep tests isolated.
#[allow(dead_code)]
pub(crate) fn fixture_config_dir(prefix: &str) -> (ConfigDirGuard, PathBuf) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("openticker-http-cfgwrite-{prefix}-{timestamp}"));
    std::fs::create_dir_all(dir.join("accounts")).expect("accounts dir should be created");
    std::fs::create_dir_all(dir.join("risk")).expect("risk dir should be created");
    std::fs::create_dir_all(dir.join("bots")).expect("bots dir should be created");

    let mut bundle = fixture_bundle();
    bundle.global.service.bot_dir = "./bots".into();
    bundle.global.storage.path = dir.join("runtime.db");

    write_rendered(&dir.join("openticker.toml"), &bundle.global);
    write_rendered(
        &dir.join("accounts").join("alpaca-paper.toml"),
        &bundle.accounts[0],
    );
    write_rendered(
        &dir.join("risk").join("equities-default.toml"),
        &bundle.risk_profiles[0],
    );
    write_rendered(&dir.join("bots").join("aapl.toml"), &bundle.instances[0]);

    (ConfigDirGuard { path: dir.clone() }, dir)
}

#[allow(dead_code)]
fn write_rendered<T: serde::Serialize>(path: &Path, value: &T) {
    let rendered =
        openticker_config::render_new_document(value).expect("fixture entity should render");
    std::fs::write(path, rendered).expect("fixture file should be written");
}
